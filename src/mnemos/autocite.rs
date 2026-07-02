//! B18 auto-cite-on-ship (spec `codeforge-ship.md` §9).
//!
//! At SessionEnd (right after `ship`), scan today's Claude session transcripts for
//! this repo and cite any Mnemos atom whose title appears in them. This is the
//! automatic form of the manual `mnemos-cli cite-detect <transcript>` subcommand —
//! wiring it into the ship path is what makes surfaced atoms accrue `citation_count`
//! without a human running cite-detect by hand.
//!
//! Everything here is **best-effort**: it never fails the ship (SessionEnd must not
//! break). Detection reuses the Sprint-1 `fulltext_match` heuristic (`cite_detect`);
//! Sprint 5+ upgrades to Haiku judgement. Cites carry `confidence = 0.5` (heuristic,
//! high false-positive rate accepted) + a `session_jsonl` provenance ref.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;

use super::cite::CiteEnvelope;
use super::cite_detect;
use super::config::MnemosConfig;
use super::context::{self, ContextAtom, ContextResponse};
use super::{new_ulid, state, transport};

/// Summary of one auto-cite pass (for the caller's log line).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AutoCiteReport {
    /// today's transcripts discovered for this repo
    pub transcripts: usize,
    /// distinct atoms whose title matched (deduped across transcripts)
    pub hits: usize,
    /// cites POSTed with a Success outcome
    pub cited_ok: usize,
}

/// Run the auto-cite pass for `repo_root` on `ledger_date` (UTC `YYYY-MM-DD`).
///
/// Never returns Err — a failure at any step degrades to a smaller/empty report
/// (so the SessionEnd `ship` stays `Ok`). The caller decides whether to print.
pub fn run(
    cfg: &MnemosConfig,
    rt: &tokio::runtime::Runtime,
    repo_root: &Path,
    ledger_date: &str,
) -> AutoCiteReport {
    let mut report = AutoCiteReport::default();

    let transcripts = discover_transcripts(repo_root, ledger_date);
    report.transcripts = transcripts.len();
    if transcripts.is_empty() {
        return report;
    }

    // Candidate atoms = what this repo's session would have surfaced (wide net for
    // detection: max 20, work sensitivity — ledger atoms are Work-tier).
    let topic = context::derive_topic(repo_root);
    let atoms = match rt.block_on(fetch_atoms(cfg, &topic, 20)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ℹ auto-cite: 取候選 atom 失敗（{e}）— skip");
            return report;
        }
    };
    if atoms.is_empty() {
        return report;
    }

    // Detect + cite. Dedup is TWO-tier:
    //   * within this pass — one ship never cites the same atom twice; and
    //   * across same-day sessions (C2) — a persistent `{repo:{date:[atom_id]}}`
    //     set seeds the skip list, so a later session's SessionEnd ship re-scanning
    //     the same day's transcripts never re-cites an atom already auto-cited today.
    let ac_root = state::ship_root();
    let mut ac_state = state::load_autocite(&ac_root);
    let repo_key = repo_root.to_string_lossy().to_string();
    let already: HashSet<String> = ac_state
        .get(&repo_key)
        .and_then(|m| m.get(ledger_date))
        .map(|v| v.iter().cloned().collect())
        .unwrap_or_default();
    let mut cited: HashSet<String> = already; // seed → skip cross-session repeats
    let mut newly_cited: Vec<String> = Vec::new();
    for path in &transcripts {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        // C1 (feedback-loop guard, RISKS ③): scan genuine conversation ONLY — strip
        // the SessionStart-hook additionalContext (incl. the P1.2 central-recall
        // downlink) so surfaced atom titles don't self-cite.
        let text = cite_detect::citable_text_from_transcript(&raw);
        for hit in cite_detect::detect(&text, &atoms) {
            if !cited.insert(hit.atom_id.clone()) {
                continue;
            }
            report.hits += 1;
            // At-most-once: record the atom as handled for (repo, date) the moment we
            // commit to citing it — BEFORE the POST — so a client-observed transient
            // failure (a 5xx/timeout the server may nonetheless have committed) is
            // never re-POSTed on a later same-day ship with a fresh cite_id, which
            // would double-bump citation_count and re-pollute the rank signal C1
            // protects. Cost: a cite that genuinely failed to reach the server is not
            // retried today (undercount — fail-safe, never inflates; the atom re-cites
            // the next day it is genuinely used). Exactly-once would need server-side
            // atom+date idempotency (follow-up in the ship spec).
            newly_cited.push(hit.atom_id.clone());
            let env = CiteEnvelope::fulltext_match(
                new_ulid(),
                chrono::Utc::now().to_rfc3339(),
                hit.matched_text.clone(),
                0.5,
                Some(serde_json::json!({
                    "kind": "session_jsonl",
                    "value": path.to_string_lossy(),
                })),
            );
            match rt.block_on(post_cite(cfg, &hit.atom_id, &env)) {
                transport::AttemptOutcome::Success => report.cited_ok += 1,
                other => eprintln!("ℹ auto-cite: cite {} 未成功（{other:?}）", hit.atom_id),
            }
        }
    }
    // Persist atoms successfully cited today so later same-day sessions skip them
    // (best-effort — a persistence failure never breaks the ship).
    if !newly_cited.is_empty() {
        ac_state
            .entry(repo_key)
            .or_default()
            .entry(ledger_date.to_string())
            .or_default()
            .extend(newly_cited);
        let _ = state::save_autocite(&ac_root, &ac_state);
    }
    report
}

/// Discover this repo's Claude session transcripts modified on `ledger_date`.
///
/// Claude Code stores transcripts under `~/.claude/projects/<slug>/<uuid>.jsonl`,
/// where `<slug>` is the repo's absolute path with every non-alphanumeric char
/// replaced by `-`. We keep only `*.jsonl` whose mtime falls in the UTC day window.
fn discover_transcripts(repo_root: &Path, ledger_date: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    let dir = home
        .join(".claude")
        .join("projects")
        .join(claude_project_slug(repo_root));
    let Some((start, end)) = day_window_unix(ledger_date) else {
        return vec![];
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        if transcript_in_window(&entry, &path, start, end) {
            out.push(path);
        }
    }
    out
}

/// True if a transcript belongs to the `[start, end)` UTC day window.
///
/// mtime alone is unreliable — a session opened before midnight UTC but appended
/// through the ledger day, a copied/rsynced file, or a `ship --date` backfill can
/// carry an out-of-window mtime and silently drop real citations. So: take the
/// mtime fast-path when it's already in-window, otherwise fall back to the
/// transcript's OWN event timestamps (Claude Code writes an ISO-8601 `timestamp`
/// per record) and include the file if ANY event lands in the window.
fn transcript_in_window(entry: &std::fs::DirEntry, path: &Path, start: i64, end: i64) -> bool {
    let mtime = entry
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    if let Some(secs) = mtime {
        if secs >= start && secs < end {
            return true;
        }
    }
    // mtime out-of-window (or unreadable) → trust the transcript's own timestamps.
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                let secs = dt.timestamp();
                if secs >= start && secs < end {
                    return true;
                }
            }
        }
    }
    false
}

/// Claude Code's project-dir slug: absolute path, every non-alnum char → `-`.
/// e.g. `/home/cookys/projects/codeforge` → `-home-cookys-projects-codeforge`.
fn claude_project_slug(repo_root: &Path) -> String {
    repo_root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// UTC `[date 00:00:00, date+1 00:00:00)` as unix-second bounds.
fn day_window_unix(ledger_date: &str) -> Option<(i64, i64)> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDate::parse_from_str(ledger_date, "%Y-%m-%d").ok()?;
    let start = chrono::Utc
        .from_utc_datetime(&naive.and_hms_opt(0, 0, 0)?)
        .timestamp();
    Some((start, start + 86_400))
}

async fn fetch_atoms(cfg: &MnemosConfig, topic: &str, max: usize) -> Result<Vec<ContextAtom>> {
    let client = transport::http_client();
    let mut req = client.get(cfg.context_url()).query(&[
        ("topic", topic.to_string()),
        ("max", max.to_string()),
        ("max_sensitivity", "work".to_string()),
    ]);
    if let Some(tok) = &cfg.token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    Ok(resp.json::<ContextResponse>().await?.atoms)
}

async fn post_cite(
    cfg: &MnemosConfig,
    atom_id: &str,
    env: &CiteEnvelope,
) -> transport::AttemptOutcome {
    let client = transport::http_client();
    transport::post_attempt(&client, &cfg.cite_url(atom_id), cfg.token.as_deref(), env).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_claude_encoding() {
        assert_eq!(
            claude_project_slug(Path::new("/home/cookys/projects/codeforge")),
            "-home-cookys-projects-codeforge"
        );
        // dots / underscores also collapse to '-'
        assert_eq!(claude_project_slug(Path::new("/a/b.c_d/e")), "-a-b-c-d-e");
    }

    #[test]
    fn day_window_parses_utc_midnight() {
        use chrono::{TimeZone, Utc};
        let (start, end) = day_window_unix("2026-07-02").unwrap();
        assert_eq!(end - start, 86_400);
        // start == 2026-07-02T00:00:00Z
        let expected = Utc
            .with_ymd_and_hms(2026, 7, 2, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(start, expected);
    }

    #[test]
    fn day_window_rejects_bad_date() {
        assert!(day_window_unix("not-a-date").is_none());
    }

    #[test]
    fn discover_none_when_no_transcript_dir() {
        // A repo path that certainly has no ~/.claude/projects slug dir.
        let got = discover_transcripts(Path::new("/nonexistent/repo/xyzzy-unlikely"), "2026-07-02");
        assert!(got.is_empty());
    }

    #[test]
    fn discover_uses_event_timestamps_when_mtime_is_out_of_window() {
        // Robustness (hetero-review finding): a transcript whose FILE mtime is set
        // to yesterday (backfill / copy / skew) but whose EVENT timestamps fall in
        // the target day must still be discovered — mtime alone would drop it.
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let repo = Path::new("/some/repo/proj");
        let slug = claude_project_slug(repo);
        let dir = home.join(".claude").join("projects").join(&slug);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("sess.jsonl");
        let mut fh = std::fs::File::create(&f).unwrap();
        // event timestamp is inside 2026-07-02 UTC
        writeln!(
            fh,
            r#"{{"type":"user","timestamp":"2026-07-02T09:00:00Z","message":{{"role":"user","content":"hi"}}}}"#
        )
        .unwrap();
        drop(fh);
        // force the FILE mtime to a day OUTSIDE the window (2026-07-01)
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_751_328_000); // 2026-07-01
        filetime_set(&f, old);

        let (start, end) = day_window_unix("2026-07-02").unwrap();
        let e = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap();
        assert!(
            transcript_in_window(&e, &f, start, end),
            "event-timestamp in-window must be discovered despite out-of-window mtime"
        );
    }

    // Minimal mtime setter for the test above (avoids adding a dep — uses the same
    // UNIX_EPOCH-relative SystemTime the discover path reads).
    fn filetime_set(path: &Path, when: std::time::SystemTime) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(when).unwrap();
    }

    #[test]
    fn autocite_dedup_state_roundtrips_and_seeds_skip() {
        // C2: the persistent (repo, date, atom) set is what stops a later same-day
        // session from re-citing. Prove the state layer: save today's cited atom,
        // reload, and confirm it seeds the skip set for the same (repo, date).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = "/home/cookys/projects/codeforge";
        let date = "2026-07-02";

        let mut st = state::load_autocite(root);
        assert!(st.is_empty(), "fresh dir → empty state");
        st.entry(repo.to_string())
            .or_default()
            .entry(date.to_string())
            .or_default()
            .push("ATOM_X".to_string());
        state::save_autocite(root, &st).unwrap();

        // A later same-day session reloads and seeds its skip set from this.
        let reloaded = state::load_autocite(root);
        let already: std::collections::HashSet<String> = reloaded
            .get(repo)
            .and_then(|m| m.get(date))
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        assert!(
            already.contains("ATOM_X"),
            "same-day cite must be remembered"
        );

        // A different day does NOT skip it (citation is per-day).
        let other_day: std::collections::HashSet<String> = reloaded
            .get(repo)
            .and_then(|m| m.get("2026-07-03"))
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        assert!(!other_day.contains("ATOM_X"), "next day starts fresh");
    }
}
