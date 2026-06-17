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
            if let Some(text) = format_signal(sig) {
                let signal = Signal::new(text, SignalSource::SessionDigest);
                if writer.append(&signal).is_ok() {
                    ingested += 1;
                }
            }
        }

        // 標記 processed 回寫(冪等鍵)。
        v["processed"] = serde_json::Value::Bool(true);
        if let Ok(s) = serde_json::to_string_pretty(&v) {
            let _ = std::fs::write(&path, s);
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
            parts.push(format!("{key}={val}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
