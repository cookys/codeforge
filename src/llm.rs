//! LLM backend for codeforge digest / compile。
//!
//! codeforge 是 Claude Code 套件,LLM 子任務**優先走 `claude -p` headless**:免 API key、
//! 用既有訂閱(零 per-token 成本),品質遠勝 rule-based passthrough。bake-off(2026-06-17,
//! ship digest prompt × haiku/sonnet/opus)實測:Opus 最銳(title/detail/source_evidence
//! 最正規),Haiku 會丟教訓 + 編假 evidence。故預設 `opus`,由 `CODEFORGE_DIGEST_MODEL` 可調。
//!
//! 呼叫端 fallback 鏈:`claude -p`(本模組)→ `ANTHROPIC_API_KEY`(Haiku API)→ rule-based。

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// digest/compile 用的 model alias。env `CODEFORGE_DIGEST_MODEL`,預設 `opus`(bake-off 最佳)。
pub fn digest_model() -> String {
    std::env::var("CODEFORGE_DIGEST_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "opus".to_string())
}

/// 跑 `claude -p --model <model>`,prompt 走 stdin,回 stdout 文字。
///
/// 用 `timeout`(env `CODEFORGE_LLM_TIMEOUT_SECS`,預設 180)包住避免 hang(cron 友善)。
/// claude CLI 不在 / 非零退出 / 空輸出 → Err(呼叫端據此 fallback)。
pub fn claude_p(prompt: &str, model: &str) -> Result<String> {
    let secs = std::env::var("CODEFORGE_LLM_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(180);

    let mut child = Command::new("timeout")
        .arg(secs.to_string())
        .arg("claude")
        .arg("-p")
        .arg("--model")
        .arg(model)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn `claude -p` 失敗(claude CLI 不在 PATH?)")?;

    {
        // prompt 走 stdin(避免 ARG_MAX);寫完 drop → EOF,claude 才開始處理。
        let mut stdin = child.stdin.take().context("claude -p 無 stdin")?;
        stdin.write_all(prompt.as_bytes())?;
    }

    let out = child.wait_with_output().context("等 claude -p 結束失敗")?;
    if !out.status.success() {
        anyhow::bail!(
            "claude -p 失敗(code {:?}):{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        anyhow::bail!("claude -p 回空輸出");
    }
    Ok(text)
}
