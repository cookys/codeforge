//! Local recall — rank active L1 entries and render a lean, budgeted index for
//! SessionStart injection (the no-mnemos READ path, symmetric to
//! `mnemos-cli context`).
//!
//! Design (per `doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`):
//! we inject a **lean ranked index, never a dump** (claude-mem v3→v4 context
//! pollution lesson). Detail is pulled on demand via `codeforge memory search`,
//! so each line carries a citation (the L1 `topic`) to pull by. Budget is
//! enforced both as an approximate token target and a hard character cap (the
//! SessionStart `additionalContext` channel caps at 10k chars).

use crate::memory::l1::L1Entry;
use chrono::NaiveDate;

/// Default lean-index token budget (approx). Kept well under what would pollute
/// context — we surface an index, not the corpus.
pub const DEFAULT_MAX_TOKENS: usize = 1500;

/// Recency half-life in days (T2.3): an entry this many days staler than the
/// freshest active entry keeps half its recency weight. ~one quarter — keeps
/// `strength` (importance) dominant across months while recency still reorders
/// near-ties and gently demotes stale knowledge.
const RECENCY_HALF_LIFE_DAYS: f64 = 90.0;

/// Hard character ceiling — stays clear of the 10k `additionalContext` limit
/// regardless of the token estimate (CJK-heavy content trends higher tok/char).
pub const HARD_CHAR_CAP: usize = 8000;

/// Per-entry body snippet length (chars). Index, not full body.
const SNIPPET_CHARS: usize = 140;

/// Char allowance reserved for the trailing "…+N more" overflow note, so
/// appending it after the budget loop can't push the total past [`HARD_CHAR_CAP`].
const OVERFLOW_NOTE_RESERVE: usize = 120;

/// Rough token estimate. Approximate by design — `/3` over-counts ASCII a little
/// and under-counts CJK a little; combined with [`HARD_CHAR_CAP`] it keeps the
/// injected block lean without pulling in a tokenizer dependency.
pub fn estimate_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(3)
}

/// Parse a `YYYY-MM-DD` frontmatter date, ignoring anything unparseable.
fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Per-entry recency anchor: the later of `updated` and `last_ref` — a recently
/// *cited* old note counts as fresh for recall. `None` if neither date parses.
fn effective_date(e: &L1Entry) -> Option<NaiveDate> {
    let updated = parse_date(&e.frontmatter.updated);
    let last_ref = e.frontmatter.last_ref.as_deref().and_then(parse_date);
    match (updated, last_ref) {
        (Some(u), Some(l)) => Some(u.max(l)),
        (only, None) | (None, only) => only,
    }
}

/// Unified recall score (T2.3) = importance × recency × citation. Multiplicative
/// (Generative-Agents-style, adapted to local signals): a weakness in any one
/// dimension pulls the entry down rather than being averaged away.
/// - **importance** = ACT-R `strength`
/// - **recency** = `0.5^(age_days / half_life)`, age measured against the freshest
///   active entry (`reference`) so the function stays pure / deterministic — no clock
/// - **citation** = `1 + ln(1+refs)/2`, log-damped so a few local cites help but a
///   runaway count can't dominate
fn score(e: &L1Entry, reference: Option<NaiveDate>) -> f64 {
    let importance = e.frontmatter.strength as f64;
    let recency = match (reference, effective_date(e)) {
        (Some(r), Some(d)) => {
            let age_days = (r - d).num_days().max(0) as f64;
            0.5_f64.powf(age_days / RECENCY_HALF_LIFE_DAYS)
        }
        // Unparseable date or empty set → neutral, don't bury the entry.
        _ => 1.0,
    };
    let citation = 1.0 + (e.frontmatter.refs as f64).ln_1p() / 2.0;
    importance * recency * citation
}

/// Rank active entries by the unified `score` (importance × recency × citation),
/// descending, with most-recently-updated as a deterministic tiebreak. Non-active
/// dropped. The recency reference is the freshest effective date in the active set.
pub fn rank(entries: Vec<L1Entry>) -> Vec<L1Entry> {
    let mut active: Vec<L1Entry> = entries
        .into_iter()
        .filter(|e| e.frontmatter.status == "active")
        .collect();
    let reference = active.iter().filter_map(effective_date).max();
    active.sort_by(|a, b| {
        let (sa, sb) = (score(a, reference), score(b, reference));
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.frontmatter.updated.cmp(&a.frontmatter.updated))
    });
    active
}

/// First meaningful body line as a CJK-safe snippet (skips the `# title` line
/// and blanks). Truncation uses `.chars()` — never byte slicing (CJK safe).
fn snippet(entry: &L1Entry) -> String {
    let line = entry
        .body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("");
    let truncated: String = line.chars().take(SNIPPET_CHARS).collect();
    if line.chars().count() > SNIPPET_CHARS {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Render one entry as a compact index bullet: title + citation topic + strength,
/// and a one-line snippet. Citation `topic` is what `memory search` pulls by.
fn render_entry(entry: &L1Entry) -> String {
    let title = {
        let t = entry.title.trim();
        if t.is_empty() { "(untitled)" } else { t }
    };
    let topic = &entry.frontmatter.topic;
    let mut s = format!(
        "- **{title}**  <sub>{topic} · s{:.2}</sub>\n",
        entry.frontmatter.strength
    );
    let snip = snippet(entry);
    if !snip.is_empty() {
        s.push_str(&format!("  > {snip}\n"));
    }
    s
}

/// Build the lean markdown index, ranked and budgeted. Stops adding entries once
/// either the token budget or the hard char cap would be exceeded — the index is
/// deliberately partial under pressure rather than a dump.
pub fn render_index(ranked: &[L1Entry], topic: &str, max_tokens: usize) -> String {
    let mut out = String::from("## CodeForge — 本地相關記憶\n\n");
    if topic.is_empty() {
        out.push_str("_topic: (whole store, by strength)_\n\n");
    } else {
        out.push_str(&format!("_topic: {topic}_\n\n"));
    }

    if ranked.is_empty() {
        out.push_str("_尚無 active L1 知識。先 `codeforge learn` + `codeforge dream`。_\n");
        return out;
    }

    let mut shown = 0usize;
    for entry in ranked {
        let bullet = render_entry(entry);
        let projected = out.chars().count() + bullet.chars().count();
        // Reserve room for the overflow note appended after the loop.
        if projected > HARD_CHAR_CAP - OVERFLOW_NOTE_RESERVE
            || estimate_tokens(&out) + estimate_tokens(&bullet) > max_tokens
        {
            break;
        }
        out.push_str(&bullet);
        shown += 1;
    }

    if shown < ranked.len() {
        out.push_str(&format!(
            "\n_…+{} more — `codeforge memory search <topic>` 拉詳情_\n",
            ranked.len() - shown
        ));
    }
    out
}

/// Wrap a markdown block as a Claude Code SessionStart hook payload
/// (`hookSpecificOutput.additionalContext`). Serialized via serde_json so the
/// markdown is escaped correctly.
pub fn wrap_session_start_json(markdown: &str) -> String {
    let v = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": markdown,
        }
    });
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::l1::{L1Entry, L1Frontmatter, L1Type};
    use std::path::PathBuf;

    fn entry(topic: &str, strength: f32, updated: &str, status: &str, body: &str) -> L1Entry {
        L1Entry {
            frontmatter: L1Frontmatter {
                kind: L1Type::Concept,
                topic: topic.to_string(),
                created: "2026-01-01".to_string(),
                updated: updated.to_string(),
                sources: vec![],
                links: vec![],
                refs: 0,
                last_ref: None,
                strength,
                status: status.to_string(),
            },
            title: format!("title-{topic}"),
            body: body.to_string(),
            file_path: PathBuf::from(format!("{topic}.md")),
        }
    }

    /// Build an entry with explicit `refs` / `last_ref` for the T2.3 score tests.
    fn entry_cited(
        topic: &str,
        strength: f32,
        updated: &str,
        refs: usize,
        last_ref: Option<&str>,
    ) -> L1Entry {
        let mut e = entry(topic, strength, updated, "active", "b");
        e.frontmatter.refs = refs;
        e.frontmatter.last_ref = last_ref.map(str::to_string);
        e
    }

    fn ranked_topics(entries: Vec<L1Entry>) -> Vec<String> {
        rank(entries)
            .iter()
            .map(|e| e.frontmatter.topic.clone())
            .collect()
    }

    #[test]
    fn rank_drops_inactive_and_orders_by_strength_then_recency() {
        let entries = vec![
            entry("low", 0.2, "2026-06-01", "active", "b"),
            entry("super", 0.9, "2026-01-01", "superseded", "b"),
            entry("hi-old", 0.8, "2026-01-01", "active", "b"),
            entry("hi-new", 0.8, "2026-06-10", "active", "b"),
        ];
        let topics = ranked_topics(entries);
        // superseded dropped; 0.8 before 0.2; within 0.8, newer (hi-new) first.
        // (90-day half-life keeps importance dominant across the ~5-month gap.)
        assert_eq!(topics, vec!["hi-new", "hi-old", "low"]);
    }

    #[test]
    fn rank_breaks_ties_by_local_citation_count() {
        // Identical strength + date → higher local `refs` wins (T2.3 citation factor).
        let entries = vec![
            entry_cited("uncited", 0.5, "2026-06-01", 0, None),
            entry_cited("cited", 0.5, "2026-06-01", 5, None),
        ];
        assert_eq!(ranked_topics(entries), vec!["cited", "uncited"]);
    }

    #[test]
    fn rank_prefers_recent_at_equal_strength_and_citation() {
        // Identical strength + refs → more recently updated wins (T2.3 recency factor).
        let entries = vec![
            entry_cited("stale", 0.5, "2026-03-10", 0, None),
            entry_cited("fresh", 0.5, "2026-06-10", 0, None),
        ];
        assert_eq!(ranked_topics(entries), vec!["fresh", "stale"]);
    }

    #[test]
    fn rank_last_ref_revives_an_old_note() {
        // An old note cited recently (last_ref newer) beats one of equal strength
        // never re-cited — effective_date takes the later of updated / last_ref.
        let entries = vec![
            entry_cited("dormant", 0.5, "2026-03-01", 0, None),
            entry_cited("revived", 0.5, "2026-02-01", 0, Some("2026-06-15")),
        ];
        assert_eq!(ranked_topics(entries), vec!["revived", "dormant"]);
    }

    #[test]
    fn render_index_includes_citation_and_snippet() {
        let r = rank(vec![entry("notify-shutdown", 0.9, "2026-06-01", "active",
            "# Notify drops wakeup\n\n用 mpsc::channel(1) 而非 Notify::notify_waiters")]);
        let md = render_index(&r, "shutdown", DEFAULT_MAX_TOKENS);
        assert!(md.contains("_topic: shutdown_"));
        assert!(md.contains("notify-shutdown"), "citation topic present: {md}");
        assert!(md.contains("mpsc::channel(1)"), "snippet present: {md}");
        // snippet skips the `# title` line
        assert!(!md.contains("> # Notify"), "must skip the markdown title line");
    }

    #[test]
    fn render_index_budget_truncates_and_notes_overflow() {
        // many fat entries → budget forces a partial index + overflow note.
        let big_body = "x".repeat(2000);
        let entries: Vec<L1Entry> = (0..50)
            .map(|i| entry(&format!("t{i}"), 0.5, "2026-06-01", "active", &big_body))
            .collect();
        let r = rank(entries);
        let md = render_index(&r, "", 300); // tight budget
        assert!(md.chars().count() <= HARD_CHAR_CAP);
        assert!(estimate_tokens(&md) <= 300 + 200, "roughly within budget: {}", estimate_tokens(&md));
        assert!(md.contains("more — `codeforge memory search"), "overflow note present");
    }

    #[test]
    fn render_index_char_cap_binds_with_overflow_note_within_cap() {
        // Token budget effectively unlimited → the HARD_CHAR_CAP is the binding
        // constraint. Final string (incl. the appended overflow note) must stay
        // within HARD_CHAR_CAP.
        let body = "x".repeat(300);
        let entries: Vec<L1Entry> = (0..200)
            .map(|i| entry(&format!("topic{i:03}"), 0.5, "2026-06-01", "active", &body))
            .collect();
        let r = rank(entries);
        let md = render_index(&r, "", 1_000_000); // token budget huge → char cap binds
        assert!(
            md.chars().count() <= HARD_CHAR_CAP,
            "must not exceed HARD_CHAR_CAP even with overflow note: got {}",
            md.chars().count()
        );
        assert!(md.contains("more — `codeforge memory search"), "overflow note present");
    }

    #[test]
    fn render_index_empty_is_valid() {
        let md = render_index(&[], "", DEFAULT_MAX_TOKENS);
        assert!(md.contains("尚無 active L1"));
    }

    #[test]
    fn wrap_session_start_json_shape() {
        let json = wrap_session_start_json("## hi\n- a");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "## hi\n- a");
    }
}
