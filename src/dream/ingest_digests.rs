//! Dream Ingest-Digests:把 session-digest hook 萃出的 high-confidence dev signal
//! 接進 codeforge L0 signals(取代 absorb 當 ledger 的真料來源)。
//!
//! session-digest.js(SessionEnd/PreCompact hook)讀本 repo transcript → 萃
//! error-recovery / user-correction / self-correction → 寫(A′ 2026-06-17)
//! `<repo>/.codeforge/digests/<date>-<sid8>.json`(schema:{cwd, date, signals[], processed})。
//! 這些是**第一人稱、本 repo coding 經驗**,正是 ledger 該收的料。
//!
//! 本步驟:掃 **per-repo** digest 檔(`ctx.project_dir/digests/`,天然只本 repo,
//! 無需 cwd filter)→ 每個 high-confidence signal 轉可讀 content → 以
//! `SignalSource::SessionDigest` append 進 signals jsonl(下游 compile 標
//! origin="session",ship 收;不像 absorb 被排除)→ **ingest 完刪 digest 檔**
//! (明文不長存)。
//!
//! 過渡(A′ point 5):同時掃舊全域 `~/.claude/session-digests/`(收嚴前 hook 寫的,
//! 帶 cwd → 仍套 cwd filter),讀完即刪;舊 digest 30 天自然過期後可移除此段。

use crate::db;
use crate::memory::l0::{Signal, SignalSource, SignalWriter};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct IngestResult {
    pub ingested: usize,
    pub digests_processed: usize,
}

/// cwd filter（僅過渡期掃舊全域目錄需要;per-repo 目錄天然隔離,不傳）。
struct CwdFilter {
    repo_root: Option<PathBuf>,
    repo_root_canon: Option<PathBuf>,
}

pub fn run(ctx: &db::Context) -> Result<IngestResult> {
    let mut ingested = 0;
    let mut digests_processed = 0;
    let writer = SignalWriter::new(ctx);

    // 1) per-repo digests(A′):`<repo>/.codeforge/digests/`。session-digest.js 已只把
    //    本 repo 的 session 落在這裡 → 天然隔離,無 cwd filter。
    let repo_digest_dir = ctx.project_dir.join("digests");
    ingest_from_dir(&repo_digest_dir, None, &writer, &mut ingested, &mut digests_processed);

    // 2) 過渡:舊全域 `~/.claude/session-digests/`(收嚴前 hook 寫的,全機共用 → 帶 cwd,
    //    仍套 cwd filter)。讀完即刪;30 天舊 digest 自然過期後可移除此段。
    if let Some(home) = dirs::home_dir() {
        let legacy_dir = home.join(".claude").join("session-digests");
        // ctx.project_dir = <repo>/.codeforge → repo root = parent;舊 digest 的 cwd 是 repo root。
        let repo_root = ctx.project_dir.parent().map(|p| p.to_path_buf());
        let repo_root_canon = repo_root.as_ref().and_then(|p| std::fs::canonicalize(p).ok());
        let filter = CwdFilter { repo_root, repo_root_canon };
        ingest_from_dir(
            &legacy_dir,
            Some(&filter),
            &writer,
            &mut ingested,
            &mut digests_processed,
        );
    }

    Ok(IngestResult { ingested, digests_processed })
}

/// 掃一個 digest 目錄:吸 high-confidence signals → ingest 完刪檔。
/// `cwd_filter` = Some 時只收 cwd 對應本 repo 的檔(過渡期舊全域目錄用);None = per-repo
/// 目錄天然隔離全收。
fn ingest_from_dir(
    dir: &Path,
    cwd_filter: Option<&CwdFilter>,
    writer: &SignalWriter,
    ingested: &mut usize,
    digests_processed: &mut usize,
) {
    if !dir.exists() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // 既有殘留 `processed:true`(收嚴前舊行為標記後留檔的)→ 已吸過,直接刪掉收尾,不重收。
        // 與 cwd 無關(旗標只在成功吸入後設),故先於 cwd filter。
        if v.get("processed").and_then(|p| p.as_bool()).unwrap_or(false) {
            let _ = std::fs::remove_file(&path);
            continue;
        }

        // cwd filter(僅過渡期舊全域目錄;per-repo 目錄不傳 → 全收)。
        if let Some(f) = cwd_filter {
            let cwd = v.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
            if !cwd_matches_repo(cwd, f.repo_root.as_deref(), f.repo_root_canon.as_deref()) {
                continue;
            }
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
                    *ingested += 1;
                }
            }
        }

        // ingest 完刪 digest 檔(A′:明文不長存)。刪失敗 → 退回標 `processed:true`
        // 避免下次重收(下次再嘗試刪),不吞錯出聲告警。
        if let Err(e) = std::fs::remove_file(&path) {
            eprintln!(
                "⚠ ingest-digests: 刪 {} 失敗,退回標 processed 避免重收:{e}",
                path.display()
            );
            let mut vv = v;
            vv["processed"] = serde_json::Value::Bool(true);
            if let Ok(s) = serde_json::to_string_pretty(&vv) {
                let _ = std::fs::write(&path, &s);
            }
        }
        *digests_processed += 1;
    }
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

    use tempfile::TempDir;

    /// 建一個可吸的 test ctx:project_dir = <temp>/.codeforge,signals/ 已建。
    fn test_ctx(temp: &TempDir, repo_subdir: &str) -> db::Context {
        let project_dir = temp.path().join(repo_subdir).join(".codeforge");
        std::fs::create_dir_all(project_dir.join("signals")).unwrap();
        db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        }
    }

    fn write_digest(dir: &Path, name: &str, v: serde_json::Value) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, v.to_string()).unwrap();
        p
    }

    #[test]
    fn per_repo_ingest_deletes_digest_after() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let digests = ctx.project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-06-17-deadbeef.json",
            serde_json::json!({
                "cwd": "/whatever", "date": "2026-06-17", "processed": false,
                "signals": [
                    {"type":"user-correction","confidence":"high","correction":"不對,sed 會清空 crontab,要改檔案式"}
                ]
            }),
        );
        let writer = SignalWriter::new(&ctx);
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(&digests, None, &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 1, "應吸 1 個 high-confidence signal");
        assert_eq!(processed, 1);
        assert!(!digest.exists(), "ingest 完應刪 digest 檔(A′ 明文不長存)");
    }

    #[test]
    fn processed_digest_is_deleted_not_reingested() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let digests = ctx.project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-06-17-cafef00d.json",
            serde_json::json!({
                "processed": true,
                "signals": [{"type":"user-correction","confidence":"high","correction":"不對啦這個錯了重來一次"}]
            }),
        );
        let writer = SignalWriter::new(&ctx);
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(&digests, None, &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 0, "processed:true 不該重收");
        assert!(!digest.exists(), "processed:true 殘留檔應被刪除收尾");
    }

    #[test]
    fn legacy_dir_skips_other_repo_and_keeps_file() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let legacy = temp.path().join("legacy");
        let other = write_digest(
            &legacy,
            "2026-06-17-11111111.json",
            serde_json::json!({
                "cwd": "/some/other/repo", "processed": false,
                "signals": [{"type":"user-correction","confidence":"high","correction":"不對重來這個不行"}]
            }),
        );
        let writer = SignalWriter::new(&ctx);
        let repo_root = ctx.project_dir.parent().map(|p| p.to_path_buf());
        let filter = CwdFilter { repo_root, repo_root_canon: None };
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(&legacy, Some(&filter), &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 0);
        assert!(other.exists(), "別的 repo 未處理的 legacy digest 不該被刪(留給其 owning repo)");
    }

    #[test]
    fn legacy_dir_ingests_matching_repo_and_deletes() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let repo = temp.path().join("myrepo");
        let legacy = temp.path().join("legacy");
        let f = write_digest(
            &legacy,
            "2026-06-17-22222222.json",
            serde_json::json!({
                "cwd": repo.to_str().unwrap(), "processed": false,
                "signals": [{"type":"user-correction","confidence":"high","correction":"不對,這要改成檔案式不要用 sed"}]
            }),
        );
        let writer = SignalWriter::new(&ctx);
        let filter = CwdFilter { repo_root: Some(repo), repo_root_canon: None };
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(&legacy, Some(&filter), &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 1, "cwd 對應本 repo 的 legacy digest 應吸入");
        assert!(!f.exists(), "matching legacy digest 吸完應刪");
    }
}
