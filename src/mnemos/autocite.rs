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
use super::{new_ulid, transport};

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

    // Detect + cite. Dedup across transcripts so the same atom is cited at most once
    // per ship (cite_dedup on the server side is keyed by cite_id, which is fresh
    // each ship; the client-side dedup keeps one ship = one cite per atom).
    let mut cited: HashSet<String> = HashSet::new();
    for path in &transcripts {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for hit in cite_detect::detect(&text, &atoms) {
            if !cited.insert(hit.atom_id.clone()) {
                continue;
            }
            report.hits += 1;
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
        let in_window = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs() as i64;
                secs >= start && secs < end
            })
            .unwrap_or(false);
        if in_window {
            out.push(path);
        }
    }
    out
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
}
