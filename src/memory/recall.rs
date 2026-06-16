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

/// Default lean-index token budget (approx). Kept well under what would pollute
/// context — we surface an index, not the corpus.
pub const DEFAULT_MAX_TOKENS: usize = 1500;

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

/// Rank active entries: strength desc (ACT-R activation, primary), then most
/// recently updated first (recency weight / tiebreak). Non-active dropped.
pub fn rank(entries: Vec<L1Entry>) -> Vec<L1Entry> {
    let mut active: Vec<L1Entry> = entries
        .into_iter()
        .filter(|e| e.frontmatter.status == "active")
        .collect();
    active.sort_by(|a, b| {
        b.frontmatter
            .strength
            .partial_cmp(&a.frontmatter.strength)
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

    #[test]
    fn rank_drops_inactive_and_orders_by_strength_then_recency() {
        let entries = vec![
            entry("low", 0.2, "2026-06-01", "active", "b"),
            entry("super", 0.9, "2026-01-01", "superseded", "b"),
            entry("hi-old", 0.8, "2026-01-01", "active", "b"),
            entry("hi-new", 0.8, "2026-06-10", "active", "b"),
        ];
        let r = rank(entries);
        let topics: Vec<&str> = r.iter().map(|e| e.frontmatter.topic.as_str()).collect();
        // superseded dropped; 0.8 before 0.2; within 0.8, newer (hi-new) first.
        assert_eq!(topics, vec!["hi-new", "hi-old", "low"]);
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
