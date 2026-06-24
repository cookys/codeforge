//! LLM backend for codeforge digest / compile。
//!
//! codeforge 是 Claude Code 套件,LLM 子任務**優先走 `claude -p` headless**:免 API key、
//! 用既有訂閱(零 per-token 成本),品質遠勝 rule-based passthrough。bake-off(2026-06-17,
//! ship digest prompt × haiku/sonnet/opus)實測:Opus 最銳(title/detail/source_evidence
//! 最正規),Haiku 會丟教訓 + 編假 evidence。故預設 `opus`,由 `CODEFORGE_DIGEST_MODEL` 可調。
//!
//! 呼叫端 fallback 鏈(見 [`headless_digest`]):
//!   `claude -p`(Anthropic 訂閱)→ `agy`(Gemini,訂閱)→ `codex`(OpenAI/ChatGPT 訂閱)
//!     → `ANTHROPIC_API_KEY`(Haiku API,呼叫端)→ rule-based(呼叫端)
//!
//! 為什麼三家 headless 引擎:`claude -p` 在大 prompt 下可能空輸出(#7263),無 key 機若直接
//! 掉 rule-based 是品質懸崖。agy/codex 是 **decorrelated**(不同廠)的免 key 中間檔 ——
//! Anthropic 那條失敗時,Google/OpenAI 那條通常不受影響,把下限從 rule-based 拉到 LLM 級。
//! bake-off(2026-06-24,真實 ship prompt)實測三家都產 valid JSON;agy 最快(16s)、codex
//! 最徹底(多抓一條)、claude(Opus)nuance 最全 → Opus 當主、agy→codex 當 fallback。
//!
//! 運維注意:
//! - **cron PATH**:`claude`/`agy`/`codex` 常在 `~/.local/bin` 或 nvm bin,須在 cron 的 PATH 內
//!   (見 `scripts/codeforge_ship.sh`),否則 spawn 失敗 → 沿鏈降級(失去較高品質)。
//! - **#7263**:舊版 `claude -p` 大 stdin(>~7KB)可能 exit 0 但空 stdout;空輸出 → Err 防衛,
//!   讓鏈往 agy/codex 走(見 claude-code issue #7263)。

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

/// agy(Gemini)fallback 用的 model。env `CODEFORGE_AGY_MODEL`,預設快又夠好的 Flash。
/// 預設字串是 `agy models` 的人類可讀 label(非 API id);agy 改版若重命名(如 Gemini 3.6)
/// 此預設會靜默失效 → 鏈落到 codex。要換時對照 `agy models` 輸出更新。
fn agy_model() -> String {
    std::env::var("CODEFORGE_AGY_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Gemini 3.5 Flash (Medium)".to_string())
}

fn timeout_secs() -> u64 {
    std::env::var("CODEFORGE_LLM_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(180)
}

/// 共用 headless CLI 執行器:`timeout <secs> <argv...>`,prompt 走 stdin(避免 ARG_MAX),
/// capture stdout。三引擎(claude/agy/codex)同一把尺,避免各自 drift。
///
/// spawn 失敗 / 非零退出 / 空 stdout → Err,呼叫端據此 fallback。`label` 只用於錯誤訊息。
fn run_cli(label: &str, argv: &[&str], prompt: &str) -> Result<String> {
    let mut child = Command::new("timeout")
        .arg(timeout_secs().to_string())
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn `{label}` 失敗(CLI 不在 PATH?)"))?;

    {
        // prompt 走 stdin;寫完 drop → EOF,子程序才開始處理。
        let mut stdin = child.stdin.take().context("子程序無 stdin")?;
        stdin.write_all(prompt.as_bytes())?;
    }

    let out = child
        .wait_with_output()
        .with_context(|| format!("等 `{label}` 結束失敗"))?;
    if !out.status.success() {
        anyhow::bail!(
            "`{label}` 失敗(code {:?}):{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        // exit 0 但空 stdout:大 prompt 可能觸發 headless empty-output(如 claude #7263)。
        // 回 Err 讓呼叫端 fallback,並標明以便診斷「高品質路徑靜默降級」。
        anyhow::bail!("`{label}` 回空輸出(exit 0、無 stdout;疑大 prompt)");
    }
    Ok(text)
}

/// 跑 `claude -p --model <model>`,prompt 走 stdin,回 stdout 文字。
pub fn claude_p(prompt: &str, model: &str) -> Result<String> {
    run_cli("claude -p", &["claude", "-p", "--model", model], prompt)
}

/// 跑 `agy -p --model <agy_model>`(Gemini headless,免 Anthropic key),prompt 走 stdin。
pub fn agy_p(prompt: &str) -> Result<String> {
    let model = agy_model();
    run_cli("agy", &["agy", "-p", "--model", &model], prompt)
}

/// 跑 `codex exec`(OpenAI/ChatGPT headless),prompt(指令)走 stdin。
///
/// `--sandbox read-only`:純文字 digest 不需執行命令,鎖死避免 agent 亂動。
/// `--skip-git-repo-check`:ship/compile 可能在非 git dir 跑;第三 fallback 最大容忍 CWD。
///
/// `codex exec` 本身非互動(無 TTY、不會卡 approval prompt),故不需 approval 旗標 ——
/// 它也沒有 `--ask-for-approval`;唯一相關的 `--dangerously-bypass…` 會拆掉 sandbox,不用。
pub fn codex_exec(prompt: &str) -> Result<String> {
    run_cli(
        "codex exec",
        &[
            "codex",
            "exec",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
        ],
        prompt,
    )
}

/// headless digest 鏈:`claude -p`(Opus,最佳)→ `agy`(Gemini,快)→ `codex`(最徹底)。
/// 回第一個成功(非空)引擎的原始輸出;三家全失敗才 Err(呼叫端再接 Haiku/rule-based)。
///
/// parse(JSON 解析)留給呼叫端 —— 它對「成功引擎但回垃圾」的處置(passthrough vs 跳過)
/// 各 call site 語義不同,不適合在此統一。本函式只負責「拿到一段非空 LLM 輸出」。
pub fn headless_digest(prompt: &str, model: &str) -> Result<String> {
    match claude_p(prompt, model) {
        Ok(t) => return Ok(t),
        Err(e) => eprintln!("ℹ claude -p 不可用（{e}）— 試 agy"),
    }
    match agy_p(prompt) {
        Ok(t) => return Ok(t),
        Err(e) => eprintln!("ℹ agy 不可用（{e}）— 試 codex"),
    }
    codex_exec(prompt).context("headless 三引擎全失敗(claude -p / agy / codex)")
}

#[cfg(test)]
mod tests {
    use super::*;

    // CODEFORGE_AGY_MODEL 是本檔唯一讀者,測試獨佔此 env key(不與他測爭用)。
    #[test]
    fn agy_model_default_and_override() {
        std::env::remove_var("CODEFORGE_AGY_MODEL");
        assert_eq!(agy_model(), "Gemini 3.5 Flash (Medium)");

        std::env::set_var("CODEFORGE_AGY_MODEL", "Gemini 9 Ultra");
        assert_eq!(agy_model(), "Gemini 9 Ultra");

        // 空字串視同未設 → 回預設(對齊 digest_model 的 filter)。
        std::env::set_var("CODEFORGE_AGY_MODEL", "");
        assert_eq!(agy_model(), "Gemini 3.5 Flash (Medium)");

        std::env::remove_var("CODEFORGE_AGY_MODEL");
    }
}
