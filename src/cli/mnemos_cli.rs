//! `codeforge mnemos-cli context|cite` — the client read/write-back surface.
//!
//! - `context [--topic][--max]` → GET /v1/atoms/context → markdown for SessionStart.
//! - `cite <atom_id>` → POST /v1/atoms/:id/cite (fulltext_match envelope, §11.1).

use anyhow::Result;

use crate::db;
use crate::mnemos::cite::CiteEnvelope;
use crate::mnemos::cite_detect;
use crate::mnemos::config::MnemosConfig;
use crate::mnemos::context::{self, ContextResponse};
use crate::mnemos::{new_ulid, transport};

#[derive(Debug, Clone)]
pub enum MnemosCliCmd {
    Context {
        topic: Option<String>,
        max: Option<usize>,
        max_sensitivity: Option<String>,
        /// P-E: ask Mnemos for top-3 theme summaries (`include_themes=true`).
        with_themes: bool,
    },
    Cite {
        atom_id: String,
        matched_text: Option<String>,
    },
    /// SessionEnd heuristic: scan a transcript for atom-title hits → cite each (§9).
    CiteDetect {
        transcript: std::path::PathBuf,
        topic: Option<String>,
        max: Option<usize>,
    },
}

pub fn run(ctx: &db::Context, cmd: MnemosCliCmd) -> Result<()> {
    let cfg = MnemosConfig::load()?;
    let rt = tokio::runtime::Runtime::new()?;
    match cmd {
        MnemosCliCmd::Context {
            topic,
            max,
            max_sensitivity,
            with_themes,
        } => run_context(ctx, &cfg, &rt, topic, max, max_sensitivity, with_themes),
        MnemosCliCmd::Cite {
            atom_id,
            matched_text,
        } => run_cite(&cfg, &rt, &atom_id, matched_text),
        MnemosCliCmd::CiteDetect {
            transcript,
            topic,
            max,
        } => run_cite_detect(ctx, &cfg, &rt, &transcript, topic, max),
    }
}

fn run_cite_detect(
    ctx: &db::Context,
    cfg: &MnemosConfig,
    rt: &tokio::runtime::Runtime,
    transcript_path: &std::path::Path,
    topic: Option<String>,
    max: Option<usize>,
) -> Result<()> {
    let transcript = std::fs::read_to_string(transcript_path)
        .map_err(|e| anyhow::anyhow!("讀取 transcript 失敗 {}: {e}", transcript_path.display()))?;

    // Fetch candidate atoms (the ones surfaced this session), then detect title hits.
    let repo_parent = ctx
        .project_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let topic = topic.unwrap_or_else(|| context::derive_topic(&repo_parent));
    let max = max.or(Some(20)); // wider net for detection than for injection
    let atoms = match rt.block_on(fetch_context(cfg, &topic, max, Some("work"))) {
        Ok(r) => r.atoms,
        Err(e) => {
            eprintln!("⚠ cite-detect: 取 atoms 失敗：{e}");
            return Ok(()); // best-effort, never fail SessionEnd
        }
    };

    let hits = cite_detect::detect(&transcript, &atoms);
    if hits.is_empty() {
        println!("ℹ {}", rust_i18n::t!("mnemos.cite_detect_none"));
        return Ok(());
    }
    let mut ok = 0;
    for hit in &hits {
        let env = CiteEnvelope::fulltext_match(
            new_ulid(),
            chrono::Utc::now().to_rfc3339(),
            hit.matched_text.clone(),
            0.5, // heuristic, lower confidence (high false-positive rate accepted)
            Some(serde_json::json!({
                "kind": "session_jsonl",
                "value": transcript_path.to_string_lossy()
            })),
        );
        match rt.block_on(post_cite(cfg, &hit.atom_id, &env)) {
            transport::AttemptOutcome::Success => ok += 1,
            other => eprintln!("⚠ cite-detect: cite {} 未成功：{other:?}", hit.atom_id),
        }
    }
    println!(
        "✓ {} ({ok}/{})",
        rust_i18n::t!("mnemos.cite_detect_done"),
        hits.len()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_context(
    ctx: &db::Context,
    cfg: &MnemosConfig,
    rt: &tokio::runtime::Runtime,
    topic: Option<String>,
    max: Option<usize>,
    max_sensitivity: Option<String>,
    with_themes: bool,
) -> Result<()> {
    // Derive topic from git branch + recent commits when not provided.
    let repo_parent = ctx
        .project_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let topic = topic.unwrap_or_else(|| context::derive_topic(&repo_parent));

    let header = rust_i18n::t!("mnemos.context_header").to_string();
    let empty = rust_i18n::t!("mnemos.context_empty").to_string();
    let resp = rt.block_on(fetch_context_opts(
        cfg,
        &topic,
        max,
        max_sensitivity.as_deref(),
        with_themes,
    ));
    match resp {
        Ok(r) => {
            // Themes render before atoms (coarse-grained first, P-E). An old server
            // without the `themes` field deserializes to [] → block silently skipped.
            print!(
                "{}",
                context::format_markdown_with(&header, &empty, &topic, &r.atoms, &r.themes)
            );
            Ok(())
        }
        Err(e) => {
            // Don't fail SessionStart hard — emit an empty-but-valid block + warn.
            eprintln!("⚠ {}: {e}", rust_i18n::t!("mnemos.context_failed"));
            print!(
                "{}",
                context::format_markdown_with(&header, &empty, &topic, &[], &[])
            );
            Ok(())
        }
    }
}

/// Build the context query params (pure — unit-testable flag propagation).
/// `with_themes=true` → `include_themes=true` (Mnemos P-E; older servers ignore it).
fn context_query_params(
    topic: &str,
    max: Option<usize>,
    max_sensitivity: Option<&str>,
    with_themes: bool,
) -> Vec<(&'static str, String)> {
    let mut q: Vec<(&'static str, String)> = vec![("topic", topic.to_string())];
    if let Some(m) = max {
        q.push(("max", m.to_string()));
    }
    if let Some(s) = max_sensitivity {
        q.push(("max_sensitivity", s.to_string()));
    }
    if with_themes {
        q.push(("include_themes", "true".to_string()));
    }
    q
}

async fn fetch_context(
    cfg: &MnemosConfig,
    topic: &str,
    max: Option<usize>,
    max_sensitivity: Option<&str>,
) -> Result<ContextResponse> {
    fetch_context_opts(cfg, topic, max, max_sensitivity, false).await
}

async fn fetch_context_opts(
    cfg: &MnemosConfig,
    topic: &str,
    max: Option<usize>,
    max_sensitivity: Option<&str>,
    with_themes: bool,
) -> Result<ContextResponse> {
    let client = transport::http_client();
    let mut req = client.get(cfg.context_url());
    req = req.query(&context_query_params(topic, max, max_sensitivity, with_themes));
    if let Some(tok) = &cfg.token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    Ok(resp.json::<ContextResponse>().await?)
}

fn run_cite(
    cfg: &MnemosConfig,
    rt: &tokio::runtime::Runtime,
    atom_id: &str,
    matched_text: Option<String>,
) -> Result<()> {
    let envelope = CiteEnvelope::fulltext_match(
        new_ulid(),
        chrono::Utc::now().to_rfc3339(),
        matched_text.unwrap_or_else(|| atom_id.to_string()),
        0.7,
        None,
    );
    let outcome = rt.block_on(post_cite(cfg, atom_id, &envelope));
    match outcome {
        transport::AttemptOutcome::Success => {
            println!("✓ {} {atom_id}", rust_i18n::t!("mnemos.cited"));
            Ok(())
        }
        transport::AttemptOutcome::BadRequest(m) => {
            anyhow::bail!("cite 被拒（4xx: {m}）")
        }
        transport::AttemptOutcome::Retryable(m) => {
            // cite is best-effort; warn but don't fail the caller.
            eprintln!("⚠ {} {atom_id}: {m}", rust_i18n::t!("mnemos.cite_failed"));
            Ok(())
        }
    }
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
    use super::context_query_params;

    #[test]
    fn with_themes_flag_adds_include_themes_param() {
        let q = context_query_params("t", Some(5), Some("work"), true);
        assert!(q.contains(&("include_themes", "true".to_string())));
        assert!(q.contains(&("topic", "t".to_string())));
        assert!(q.contains(&("max", "5".to_string())));
        assert!(q.contains(&("max_sensitivity", "work".to_string())));
    }

    #[test]
    fn without_flag_no_include_themes_param() {
        let q = context_query_params("t", None, None, false);
        assert!(q.iter().all(|(k, _)| *k != "include_themes"));
        assert_eq!(q, vec![("topic", "t".to_string())]);
    }
}
