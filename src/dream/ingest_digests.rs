//! Dream Ingest-Digests:把 session-digest hook 萃出的 high-confidence dev signal
//! 接進 codeforge L0 signals(取代 absorb 當 ledger 的真料來源)。
//!
//! session-digest.js(SessionEnd/PreCompact hook)讀本 repo transcript → 萃
//! error-recovery / user-correction / self-correction → 寫
//! `~/.claude/session-digests/<date>-<sid8>.json`(schema:{cwd, date, signals[], processed})。
//! 這些是**第一人稱、本 repo coding 經驗**,正是 ledger 該收的料。
//!
//! 本步驟:掃 digest 檔 → 只收 cwd 對應本 repo 且未處理的 → 每個 signal 轉成
//! 可讀 content → 以 `SignalSource::SessionDigest` append 進 signals jsonl
//! (下游 compile 會標 origin="session",ship 收;不像 absorb 被排除)→ 標記
//! digest `processed:true`(冪等,不重複吸)。

use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
use anyhow::Result;
use std::path::Path;

pub struct IngestResult {
    pub ingested: usize,
    pub digests_processed: usize,
}

pub fn run(ctx: &db::Context) -> Result<IngestResult> {
    let mut ingested = 0;
    let mut digests_processed = 0;

    let digest_dir = dirs::home_dir()
        .map(|h| h.join(".claude").join("session-digests"))
        .unwrap_or_default();
    if !digest_dir.exists() {
        return Ok(IngestResult { ingested, digests_processed });
    }

    // ctx.project_dir = <repo>/.codeforge → repo root = parent。session-digest 的 cwd 是 repo root。
    let repo_root = ctx.project_dir.parent().map(|p| p.to_path_buf());
    let repo_root_canon = repo_root.as_ref().and_then(|p| std::fs::canonicalize(p).ok());

    let writer = SignalWriter::new(ctx);

    let entries = match std::fs::read_dir(&digest_dir) {
        Ok(e) => e,
        Err(_) => return Ok(IngestResult { ingested, digests_processed }),
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // 冪等:已處理過的 digest 跳過。
        if v.get("processed").and_then(|p| p.as_bool()).unwrap_or(false) {
            continue;
        }
        // 只收 cwd 對應本 repo 的 digest(session-digests 是全機共用目錄)。
        let cwd = v.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
        if !cwd_matches_repo(cwd, repo_root.as_deref(), repo_root_canon.as_deref()) {
            continue;
        }

        let empty = Vec::new();
        let signals = v.get("signals").and_then(|s| s.as_array()).unwrap_or(&empty);
        for sig in signals {
            // 只收 high-confidence(對齊海馬 ripple 選擇性 +「high-confidence」承諾;
            // self-correction 等 medium 略過,寧缺勿濫)。
            if sig.get("confidence").and_then(|c| c.as_str()) != Some("high") {
                continue;
            }
            if let Some(text) = format_signal(sig) {
                let signal = Signal::new(text, SignalSource::SessionDigest);
                if writer.append(&signal).is_ok() {
                    ingested += 1;
                }
            }
        }

        // 標記 processed 回寫(冪等鍵)。回寫失敗不吞錯:出聲告警(下次可能重收,
        // 靠 Mnemos 端離峰 dedup --scan 收斂)。
        v["processed"] = serde_json::Value::Bool(true);
        match serde_json::to_string_pretty(&v) {
            Ok(s) => {
                if let Err(e) = std::fs::write(&path, &s) {
                    eprintln!(
                        "⚠ ingest-digests: 標記 {} processed 失敗(下次可能重收,靠 dedup 收斂):{e}",
                        path.display()
                    );
                }
            }
            Err(e) => eprintln!("⚠ ingest-digests: 序列化 digest 失敗 {}:{e}", path.display()),
        }
        digests_processed += 1;
    }

    Ok(IngestResult { ingested, digests_processed })
}

/// cwd 字串是否指向本 repo root(raw 比對 + canonical 比對,容 symlink)。
fn cwd_matches_repo(cwd: &str, repo_root: Option<&Path>, repo_root_canon: Option<&Path>) -> bool {
    if cwd.is_empty() {
        return false;
    }
    let Some(root) = repo_root else {
        return false;
    };
    let cwd_path = Path::new(cwd);
    if cwd_path == root {
        return true;
    }
    match (std::fs::canonicalize(cwd_path).ok(), repo_root_canon) {
        (Some(c), Some(rc)) => c == rc,
        _ => false,
    }
}

/// 把一個 digest signal 轉成可讀的 L0 content。穩健:不臆測各 type 的精確 schema,
/// 收集已知字串欄位(error-recovery/user-correction/self-correction 皆涵蓋)。回 None=資訊太少跳過。
fn format_signal(sig: &serde_json::Value) -> Option<String> {
    let obj = sig.as_object()?;
    let typ = obj.get("type").and_then(|t| t.as_str()).unwrap_or("signal");
    let label = match typ {
        "error-recovery" => "錯誤修復",
        "user-correction" => "使用者糾正",
        "self-correction" => "自我修正",
        other => other,
    };
    let mut parts: Vec<String> = Vec::new();
    for key in [
        "tool",
        "error",
        "correction",
        "context",
        "assistant_context",
        "file",
        "description",
        "text",
    ] {
        if let Some(val) = obj
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("{key}={}", mask_secrets(val)));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let content = format!("【{label}】{}", parts.join(" | "));
    if content.chars().count() < 15 {
        return None;
    }
    Some(content)
}

/// 粗略遮罩常見 credential/token,避免 secret 隨 dev signal 進腦(codeforge 無共用遮罩
/// 函式;無 regex crate,用 prefix + 高熵長度啟發式,寧可多遮)。逐 token 檢查。
fn mask_secrets(s: &str) -> String {
    s.split(' ').map(mask_word).collect::<Vec<_>>().join(" ")
}

/// 遮一個以空白切出的詞:取 `=`/`:` 之後的值部分當候選(處理 KEY=secret /
/// Authorization:token / 裸 token),候選像 secret 就替成 ***。
fn mask_word(word: &str) -> String {
    let val_start = word.rfind(['=', ':']).map(|i| i + 1).unwrap_or(0);
    let (prefix, val_raw) = word.split_at(val_start);
    let candidate =
        val_raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    if !candidate.is_empty() && looks_secret(candidate) {
        format!("{}{}", prefix, val_raw.replace(candidate, "***"))
    } else {
        word.to_string()
    }
}

/// token 是否像 credential:已知前綴(sk-/ghp_/AKIA/AIza/xox…)或高熵 32+ 字串。
fn looks_secret(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-", "ghp_", "gho_", "ghs_", "github_pat_", "xoxb-", "xoxp-", "xoxa-", "AKIA", "ASIA",
        "AIza",
    ];
    if t.len() >= 12 && PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    // 高熵:32+ 連續 alnum/_/-(不含 '/' 以免遮路徑),且字母+數字混合。
    t.len() >= 32
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_token_prefixes_and_high_entropy() {
        // KEY=token(無空格)前綴遮
        assert_eq!(mask_secrets("export GH=ghp_abcdefABCDEF1234567890"), "export GH=***");
        // 裸高熵 token 遮
        assert!(mask_secrets("got abcd1234efgh5678ijkl9012mnop3456").contains("***"));
        // 一般文字不遮
        assert_eq!(mask_secrets("修了 crontab 的 sed 問題"), "修了 crontab 的 sed 問題");
        // 路徑不遮(含 /)
        assert_eq!(
            mask_secrets("檔案 /home/cookys/projects/mnemos/src"),
            "檔案 /home/cookys/projects/mnemos/src"
        );
    }

    #[test]
    fn format_error_recovery() {
        let sig = serde_json::json!({
            "type": "error-recovery", "confidence": "high",
            "tool": "Bash", "error": "sed 清空 crontab", "context": "改 crontab 用 sed pipe"
        });
        let out = format_signal(&sig).unwrap();
        assert!(out.contains("錯誤修復"));
        assert!(out.contains("Bash"));
        assert!(out.contains("sed 清空 crontab"));
    }

    #[test]
    fn format_user_correction() {
        let sig = serde_json::json!({
            "type": "user-correction", "confidence": "high",
            "correction": "不是叫你以後都用正體中文回答嗎"
        });
        let out = format_signal(&sig).unwrap();
        assert!(out.contains("使用者糾正"));
        assert!(out.contains("正體中文"));
    }

    #[test]
    fn drops_empty_signal() {
        let sig = serde_json::json!({ "type": "error-recovery", "confidence": "high" });
        assert!(format_signal(&sig).is_none());
    }

    #[test]
    fn cwd_match_raw() {
        let root = Path::new("/home/cookys/projects/mnemos");
        assert!(cwd_matches_repo("/home/cookys/projects/mnemos", Some(root), None));
        assert!(!cwd_matches_repo("/home/cookys/projects/hangar", Some(root), None));
        assert!(!cwd_matches_repo("", Some(root), None));
    }
}
