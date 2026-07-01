//! `codeforge ship` — produce the L2 daily ledger and POST it to Mnemos.
//!
//! Spec: `doc/specs/codeforge-ship.md`. Flags (§3): --date / --dry-run / --no-hook
//! / --resend. Endpoint resolution §8, retry/queue §7, digest §5–6.

use anyhow::Result;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::db;
use crate::mnemos::config::MnemosConfig;
use crate::mnemos::health;
use crate::mnemos::ledger::{LedgerEnvelope, LedgerLesson, LedgerPayload};
use crate::mnemos::transport::{self, SendResult};
use crate::mnemos::{autocite, digest, evidence::SourceEvidence, new_ulid, state};

/// Haiku model for ship digest（provenance + API 呼叫共用,抽常數避免 drift,review Minor）。
const HAIKU_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Clone, Default)]
pub struct ShipOpts {
    pub date: Option<String>,
    pub dry_run: bool,
    pub no_hook: bool,
    pub resend: bool,
}

pub fn run(ctx: &db::Context, opts: ShipOpts) -> Result<()> {
    // §3.1: --dry-run and --resend are mutually exclusive.
    if opts.dry_run && opts.resend {
        anyhow::bail!("--dry-run 與 --resend 互斥");
    }
    ctx.ensure_initialized()?;

    // per-repo opt-out (A′): `<repo>/.codeforge/no-ship` present → never POST this repo's
    // ledger. Hard no-op regardless of mode (even interactive `codeforge ship`): the repo
    // is explicitly marked private. Checked before any envelope build / state read.
    if ctx.no_ship() {
        println!("ℹ {}", rust_i18n::t!("ship.no_ship_optout"));
        return Ok(());
    }

    // §3.3 opt-in gate: in hook mode (--no-hook, the SessionEnd path), only ship
    // when Mnemos is explicitly opted-in. codeforge-only users (no Mnemos) still
    // get dream distillation; ship becomes a clean no-op — never POSTs, never
    // queues junk to ship-failed/. Interactive `codeforge ship` (no --no-hook) is
    // a deliberate user action and always attempts, opt-in or not.
    if opts.no_hook && !MnemosConfig::opted_in() {
        return Ok(());
    }

    let cfg = MnemosConfig::load()?;
    let root = state::ship_root();
    let rt = tokio::runtime::Runtime::new()?;

    // --resend: only flush the failed queue, no fresh digest.
    if opts.resend {
        let n = rt.block_on(flush_failed_queue(&cfg, &root, false));
        if n > 0 {
            // spec §8: any real flush success proves readiness; update the cache
            // so statusline reflects current state. record_ship_health is gated on
            // opted_in() internally, so codeforge-only users are unaffected.
            record_ship_health(true);
        }
        println!("✓ {} ({n})", rust_i18n::t!("ship.resend_done"));
        return Ok(());
    }

    let ledger_date = opts
        .date
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    // Build the envelope from today's L1 + git.
    let repo = repo_name(&ctx.project_dir);
    let lessons = build_lessons(ctx, &ledger_date, &rt)?;
    let provenance = build_provenance(ctx, &lessons);
    let ship_id = new_ulid();
    let envelope = LedgerEnvelope {
        ship_id: ship_id.clone(),
        source: "codeforge_ledger".to_string(),
        ship_at: chrono::Utc::now().to_rfc3339(),
        machine_id: cfg.machine_id.clone(),
        payload: LedgerPayload {
            ledger_date: ledger_date.clone(),
            repo: repo.clone(),
            lessons,
            provenance,
        },
    };

    // --dry-run: pretty-print envelope, exit 0, NO side effects (§3.1).
    if opts.dry_run {
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    // ship-state guard (§7.6): skip digest+POST if already shipped today,
    // but still flush the failed queue (unless --no-hook).
    let mut sstate = state::load_state(&root);
    if state::already_shipped(&sstate, &repo, &ledger_date) {
        println!(
            "ℹ {} ({repo} {ledger_date})",
            rust_i18n::t!("ship.already_shipped")
        );
        if !opts.no_hook {
            let flushed = rt.block_on(flush_failed_queue(&cfg, &root, false));
            if flushed > 0 {
                record_ship_health(true);
            }
        }
        // B18: still cite atoms this session referenced (citation is about usage,
        // independent of whether a fresh ledger was shipped today).
        maybe_auto_cite(ctx, &cfg, &rt, &ledger_date);
        return Ok(());
    }

    // §5.1 rule 2: empty lessons → don't POST (avoid atom-less document).
    if envelope.payload.lessons.is_empty() {
        eprintln!("⚠ {} ({ledger_date})", rust_i18n::t!("ship.empty_ledger"));
        // B18: an empty ledger still had a session — cite what it referenced.
        maybe_auto_cite(ctx, &cfg, &rt, &ledger_date);
        return Ok(());
    }

    // Default mode (not --no-hook): piggyback-flush failed queue first (§7.4).
    if !opts.no_hook {
        let flushed = rt.block_on(flush_failed_queue(&cfg, &root, false));
        if flushed > 0 {
            record_ship_health(true);
        }
    }

    // POST with the appropriate retry profile.
    let result = rt.block_on(send_envelope(&cfg, &envelope, opts.no_hook));
    match result {
        SendResult::Ok => {
            record_ship_health(true);
            state::mark_shipped(&mut sstate, &repo, &ledger_date, &ship_id);
            state::save_state(&root, &sstate)?;
            let n = envelope.payload.lessons.len();
            println!(
                "✓ {} ({repo} {ledger_date} — {n} lessons, ship_id {ship_id})",
                rust_i18n::t!("ship.shipped")
            );
            // B18: auto-cite atoms this session referenced (best-effort, §9).
            maybe_auto_cite(ctx, &cfg, &rt, &ledger_date);
            Ok(())
        }
        SendResult::Exhausted(msg) => {
            record_ship_health(false);
            let path = state::write_failed(&root, &envelope)?;
            eprintln!(
                "⚠ {} ({msg}) — {}",
                rust_i18n::t!("ship.failed_queued"),
                path.display()
            );
            if opts.no_hook {
                Ok(()) // hook 永不阻塞 SessionEnd
            } else {
                anyhow::bail!("ship 失敗，已 queue 待重送")
            }
        }
        SendResult::BadRequest(msg) => {
            // 4xx：壞 payload，不 queue（§7.2）。
            record_ship_health(false);
            eprintln!("✗ {} ({msg})", rust_i18n::t!("ship.rejected"));
            if opts.no_hook {
                Ok(())
            } else {
                anyhow::bail!("ship payload 被拒：{msg}")
            }
        }
    }
}

/// Build lessons via Haiku digest or rule-based passthrough (§6).
fn build_lessons(
    ctx: &db::Context,
    ledger_date: &str,
    rt: &tokio::runtime::Runtime,
) -> Result<Vec<LedgerLesson>> {
    let store_dir = ctx.project_dir.join("store");
    let project_parent = ctx.project_dir.parent().unwrap_or(Path::new(""));
    let all = digest::scan_l1(&store_dir).unwrap_or_default();
    let selected = digest::select_l1_for_date(&all, ledger_date);
    if selected.is_empty() {
        return Ok(vec![]);
    }

    // digest prompt(各 backend 共用)。
    let repo = repo_name(&ctx.project_dir);
    let (head, branch) = git_head_branch(&ctx.project_dir);
    let git_log = git_log_oneline(&ctx.project_dir, ledger_date);
    let l1_block = digest::render_l1_block(&selected, project_parent);
    let prompt = digest::build_prompt(&repo, ledger_date, &head, &branch, &git_log, &l1_block);
    let pass = || digest::passthrough_lessons(&selected, project_parent);

    // backend fallback 鏈:headless 三引擎(claude -p → agy → codex,見 llm::headless_digest)
    //   → ANTHROPIC_API_KEY(Haiku)→ passthrough。
    let lessons = match crate::llm::headless_digest(&prompt, &crate::llm::digest_model()) {
        Ok(json) => digest::parse_digest_json(&json).unwrap_or_else(|e| {
            eprintln!("⚠ headless digest parse 失敗（{e}）— fallback passthrough");
            pass()
        }),
        Err(e) => {
            eprintln!("ℹ headless 三引擎不可用（{e}）— 試 ANTHROPIC_API_KEY / passthrough");
            match std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
            {
                Some(key) => match rt.block_on(call_haiku(&key, &prompt)) {
                    Ok(json) => digest::parse_digest_json(&json).unwrap_or_else(|e| {
                        eprintln!("⚠ Haiku digest parse 失敗（{e}）— passthrough");
                        pass()
                    }),
                    Err(e) => {
                        eprintln!("⚠ Haiku digest 失敗（{e}）— passthrough");
                        pass()
                    }
                },
                None => {
                    eprintln!("ℹ {}", rust_i18n::t!("ship.no_api_key"));
                    pass()
                }
            }
        }
    };

    // Enrich evidence with today's commits (§5.1 rule 4) + merge same-title (rule 5).
    let mut lessons = digest::merge_lessons(lessons);
    if let Some(head) = git_head_branch(&ctx.project_dir)
        .0
        .split_whitespace()
        .next()
    {
        if !head.is_empty() {
            for l in &mut lessons {
                let has_commit = l.source_evidence.iter().any(|e| e.kind == "git_commit");
                if !has_commit {
                    l.source_evidence.push(SourceEvidence::git_commit(head));
                }
            }
        }
    }
    Ok(lessons)
}

/// B18 auto-cite-on-ship (spec §9): after a ship, scan today's session transcripts
/// for this repo and cite any surfaced Mnemos atom. Best-effort — never fails ship,
/// only prints when at least one cite was detected. Runs regardless of `--no-hook`
/// (SessionEnd is exactly when we want it) but only when Mnemos is opted-in.
fn maybe_auto_cite(
    ctx: &db::Context,
    cfg: &MnemosConfig,
    rt: &tokio::runtime::Runtime,
    ledger_date: &str,
) {
    if !MnemosConfig::opted_in() {
        return;
    }
    let Some(repo_root) = ctx.project_dir.parent() else {
        return;
    };
    let report = autocite::run(cfg, rt, repo_root, ledger_date);
    if report.hits > 0 {
        println!(
            "✓ {} ({}/{} — scanned {} transcript)",
            rust_i18n::t!("ship.auto_cite_done"),
            report.cited_ok,
            report.hits,
            report.transcripts
        );
    }
}

fn build_provenance(ctx: &db::Context, lessons: &[LedgerLesson]) -> serde_json::Value {
    let (head, branch) = git_head_branch(&ctx.project_dir);
    let l1_files: Vec<String> = lessons
        .iter()
        .flat_map(|l| &l.source_evidence)
        .filter(|e| e.kind == "l1_concept")
        .map(|e| e.value.clone())
        .collect();
    serde_json::json!({
        "lesson_count": lessons.len(),
        "l1_concept_files": l1_files,
        "git_head_sha": head,
        "git_branch": branch,
        "haiku_model": HAIKU_MODEL,
    })
}

/// POST the envelope with retry (default) or single attempt (--no-hook).
async fn send_envelope(cfg: &MnemosConfig, env: &LedgerEnvelope, no_hook: bool) -> SendResult {
    let client = transport::http_client();
    let url = cfg.ledger_url();
    let token = cfg.token.clone();
    if no_hook {
        let outcome = transport::post_attempt(&client, &url, token.as_deref(), env).await;
        transport::run_single(|_| outcome.clone())
    } else {
        // Default: §7.2 backoff retry, driven by the shared async policy.
        transport::run_with_retry_async(transport::BACKOFF_SCHEDULE, |_| {
            transport::post_attempt(&client, &url, token.as_deref(), env)
        })
        .await
    }
}

/// Flush ship-failed/ queue, deleting files on success (§7.4). Returns count resent OK.
async fn flush_failed_queue(cfg: &MnemosConfig, root: &Path, _single: bool) -> usize {
    let client = transport::http_client();
    let url = cfg.ledger_url();
    let token = cfg.token.clone();
    let mut ok = 0;
    for (path, env) in state::load_failed(root) {
        // each retried with full backoff, via the shared async policy
        match transport::run_with_retry_async(transport::BACKOFF_SCHEDULE, |_| {
            transport::post_attempt(&client, &url, token.as_deref(), &env)
        })
        .await
        {
            SendResult::Ok => {
                let _ = std::fs::remove_file(&path);
                ok += 1;
            }
            SendResult::BadRequest(msg) => {
                // 4xx:payload 被拒,重送無用。但別靜默刪(silent-data-loss 紅線,review Major)
                // —— 移到 ship-rejected/ 隔離 + log,供事後追(暫時性 4xx 如版本不一致可救)。
                let fname = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let rejected_dir = root.join("ship-rejected");
                let isolated = std::fs::create_dir_all(&rejected_dir).is_ok()
                    && std::fs::rename(&path, rejected_dir.join(&fname)).is_ok();
                if !isolated {
                    let _ = std::fs::remove_file(&path);
                }
                eprintln!(
                    "ship: ledger {fname} 遭 Mnemos 拒絕({msg});{}(未進腦,請查 contract)",
                    if isolated {
                        "已隔離至 ship-rejected/"
                    } else {
                        "隔離失敗已刪除"
                    }
                );
                // 不 break:此 file 隔離,繼續 flush 下一個。
            }
            SendResult::Exhausted(_) => {
                // backoff 全耗盡仍非 Success/BadRequest = Mnemos 持續不可達。
                // 停止 flush 剩餘 queue,避免手動 ship 卡 N×36s(review Minor);留待下次。
                eprintln!("ship: Mnemos 持續不可達,暫停 flush 剩餘 queue(下次 ship 再試)");
                break;
            }
        }
    }
    ok
}

async fn call_haiku(api_key: &str, prompt: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": HAIKU_MODEL,
            "max_tokens": 2048,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        let st = resp.status();
        if st.as_u16() == 401 || st.as_u16() == 403 {
            anyhow::bail!("Haiku API 認證失敗（{st}）：ANTHROPIC_API_KEY 無效或無權限");
        }
        anyhow::bail!("Haiku API 錯誤：{st}");
    }
    let body: serde_json::Value = resp.json().await?;
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("{}")
        .to_string())
}

/// 把真實 ingest 成敗寫進 per-machine 快取 `mnemos-ship.json`（spec §8）。
/// 受 `MnemosConfig::opted_in()` gate — 未 opt-in 不寫。
/// 寫入失敗時以 `eprintln!` 輸出到 stderr，絕不 propagate（保持 SessionEnd 的 Ok(()) 不中斷）。
fn record_ship_health(ok: bool) {
    if !MnemosConfig::opted_in() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if let Err(e) = health::write_atomic(
        &health::ship_path(),
        &health::ShipCache {
            last_ship_at: now,
            last_ship_ok: ok,
        },
    ) {
        eprintln!("ship: 無法寫 mnemos-ship.json（{e}）");
    }
}

fn repo_name(project_dir: &Path) -> String {
    project_dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn git_head_branch(repo_dir: &Path) -> (String, String) {
    let parent = repo_dir.parent().unwrap_or(repo_dir);
    let head = git(parent, &["rev-parse", "HEAD"]).unwrap_or_default();
    let branch = git(parent, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    (head.trim().to_string(), branch.trim().to_string())
}

fn git_log_oneline(repo_dir: &Path, date: &str) -> String {
    let parent = repo_dir.parent().unwrap_or(repo_dir);
    let since = format!("{date} 00:00");
    let until = format!("{date} 23:59");
    git(
        parent,
        &[
            "log",
            &format!("--since={since}"),
            &format!("--until={until}"),
            "--pretty=format:%h %s",
        ],
    )
    .unwrap_or_default()
}

fn git(repo_dir: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}
