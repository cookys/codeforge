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
//! - **限流閥(throttle)**:LLM 子程序受三層保護,別讓 dream/ship 壓垮忙碌的機器 ——
//!   (1) 高載退讓:1-min load/核心數 > `CODEFORGE_LLM_MAX_LOAD_PER_CORE`(預設 2.0)→ 直接
//!   退 rule-based、不 spawn;(2) single-flight:全機同時只一個 codeforge LLM 子程序;
//!   (3) `nice -n 19`:子程序永遠讓步給其他 CPU 工作。三者任一觸發 → 沿 fallback 鏈降級。
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

// ─── 限流閥(throttle):別讓 dream/ship 的 LLM 子程序壓垮一台已忙的機器 ──────────
// 三層:(1) load ceiling 高載直接退 rule-based、(2) single-flight 全機同時只一個、
// (3) nice -n 19 永遠讓步給其他 CPU 工作(在 run_cli 的 spawn 處)。

/// 系統 1-min load / 核心數 > `CODEFORGE_LLM_MAX_LOAD_PER_CORE`(預設 2.0)→ true(該退讓)。
/// 讀不到 /proc/loadavg(非 Linux)→ false(不擋,保持原行為)。
fn load_too_high() -> bool {
    let max_per_core = std::env::var("CODEFORGE_LLM_MAX_LOAD_PER_CORE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(2.0);
    let Ok(s) = std::fs::read_to_string("/proc/loadavg") else {
        return false;
    };
    let Some(load1) = s
        .split_whitespace()
        .next()
        .and_then(|x| x.parse::<f64>().ok())
    else {
        return false;
    };
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f64;
    load1 / cores > max_per_core
}

fn lock_path() -> std::path::PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("codeforge")
        .join("llm.lock")
}

/// 全機單飛鎖:同時只允許一個 codeforge LLM 子程序。RAII,drop 時釋放。
/// 拿不到(別人持有且未過期)→ None,呼叫端視為 throttled。std-only(O_EXCL + mtime 防孤兒)。
struct LlmSlot {
    path: std::path::PathBuf,
}

impl LlmSlot {
    fn try_acquire() -> Option<Self> {
        Self::try_acquire_at(lock_path())
    }

    fn try_acquire_at(path: std::path::PathBuf) -> Option<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // 孤兒鎖防護:比 (timeout + 60s) 還舊 → 持有者大概早死了,搶走。
        if let Ok(meta) = std::fs::metadata(&path) {
            let stale = meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .map(|age| age.as_secs() > timeout_secs() + 60)
                .unwrap_or(true);
            if stale {
                let _ = std::fs::remove_file(&path);
            }
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true) // O_EXCL:已存在即失敗 = 別人持有
            .open(&path)
        {
            Ok(_) => Some(Self { path }),
            Err(_) => None,
        }
    }
}

impl Drop for LlmSlot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 共用 headless CLI 執行器:`timeout <secs> <argv...>`,prompt 走 stdin(避免 ARG_MAX),
/// capture stdout。三引擎(claude/agy/codex)同一把尺,避免各自 drift。
///
/// spawn 失敗 / 非零退出 / 空 stdout → Err,呼叫端據此 fallback。`label` 只用於錯誤訊息。
fn run_cli(label: &str, argv: &[&str], prompt: &str) -> Result<String> {
    // 限流閥 1:高載 → 不 spawn,直接 Err 讓呼叫端退 rule-based(不往機器上堆重程序)。
    if load_too_high() {
        anyhow::bail!("`{label}` 限流:系統負載過高(1-min load/core 超過閥值)→ 退 fallback");
    }
    // 限流閥 2:single-flight。全機已有 codeforge LLM 子程序在跑 → 不排隊,直接退 fallback。
    // _slot 持有到本函式結束(子程序跑完)才 drop 釋放。
    let _slot = LlmSlot::try_acquire().ok_or_else(|| {
        anyhow::anyhow!("`{label}` 限流:已有 codeforge LLM 子程序在跑 → 退 fallback")
    })?;

    // 限流閥 3:`nice -n 19` 包住 → 子程序永遠讓步給其他 CPU 工作(如本機長跑運算)。
    let mut child = Command::new("nice")
        .arg("-n")
        .arg("19")
        .arg("timeout")
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

    /// 限流閥 load ceiling:閥值設極高時永不擋(load/core 不可能超過 1e6)。
    #[test]
    fn load_ceiling_huge_threshold_never_throttles() {
        std::env::set_var("CODEFORGE_LLM_MAX_LOAD_PER_CORE", "1000000");
        assert!(!load_too_high(), "閥值極高時不該 throttle");
        std::env::remove_var("CODEFORGE_LLM_MAX_LOAD_PER_CORE");
    }

    /// 限流閥 single-flight:同一鎖路徑,持有時第二次拿不到,釋放後可再拿。
    #[test]
    fn llm_slot_is_single_flight() {
        let tmp = tempfile::TempDir::new().unwrap();
        let p = tmp.path().join("llm.lock");
        let a = LlmSlot::try_acquire_at(p.clone());
        assert!(a.is_some(), "首次應拿到鎖");
        let b = LlmSlot::try_acquire_at(p.clone());
        assert!(b.is_none(), "已持有時第二次應拿不到(single-flight)");
        drop(a);
        let c = LlmSlot::try_acquire_at(p.clone());
        assert!(c.is_some(), "釋放後應能再拿");
    }
}
