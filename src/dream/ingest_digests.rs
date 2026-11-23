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
use crate::memory::l0::{Signal, SignalMeta, SignalSource, SignalWriter};
use anyhow::Result;
use std::path::{Path, PathBuf};

const SEAT_ENV_KEYS: &[&str] = &[
    "CODEFORGE_SEAT",
    "FLEET_ADDRESS",
    "FLEET_SEAT",
    "AUTOPILOT_SEAT",
    "AUTOPILOT_FLEET_ADDRESS",
];

fn resolve_seat_with<F>(lookup: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    for key in SEAT_ENV_KEYS {
        if let Some(val) = lookup(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn non_empty_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn rfc3339_field(v: &serde_json::Value, key: &str) -> Option<String> {
    non_empty_field(v, key).filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
}

fn extract_digest_meta_with<F>(v: &serde_json::Value, env_lookup: F) -> SignalMeta
where
    F: Fn(&str) -> Option<String>,
{
    let session_id = non_empty_field(v, "session_id").filter(|s| s != "unknown");
    let time_start = rfc3339_field(v, "time_start");
    let time_end = rfc3339_field(v, "time_end");
    let seat = non_empty_field(v, "seat").or_else(|| resolve_seat_with(env_lookup));
    let model = non_empty_field(v, "model").filter(|s| s != "unknown" && s != "<synthetic>");
    SignalMeta {
        session_id,
        time_start,
        time_end,
        seat,
        model,
    }
}

fn extract_digest_meta(v: &serde_json::Value) -> SignalMeta {
    extract_digest_meta_with(v, |k| std::env::var(k).ok())
}

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

    // per-repo opt-out(A′):`<repo>/.codeforge/no-ship` 存在 → 此 repo dev signal 不進腦,
    // 連 ingest 都跳過(digest 檔留原處,等 session-digest cleanup 30 天過期清)。
    if ctx.no_ship() {
        return Ok(IngestResult {
            ingested,
            digests_processed,
        });
    }

    let writer = SignalWriter::new(ctx);

    // 1) per-repo digests(A′):`<repo>/.codeforge/digests/`。session-digest.js 已只把
    //    本 repo 的 session 落在這裡 → 天然隔離,無 cwd filter。
    let repo_digest_dir = ctx.project_dir.join("digests");
    ingest_from_dir(
        &repo_digest_dir,
        None,
        &writer,
        &mut ingested,
        &mut digests_processed,
    );

    // 2) 過渡:舊全域 `~/.claude/session-digests/`(收嚴前 hook 寫的,全機共用 → 帶 cwd,
    //    仍套 cwd filter)。讀完即刪;30 天舊 digest 自然過期後可移除此段。
    if let Some(home) = dirs::home_dir() {
        let legacy_dir = home.join(".claude").join("session-digests");
        // ctx.project_dir = <repo>/.codeforge → repo root = parent;舊 digest 的 cwd 是 repo root。
        let repo_root = ctx.project_dir.parent().map(|p| p.to_path_buf());
        let repo_root_canon = repo_root
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok());
        let filter = CwdFilter {
            repo_root,
            repo_root_canon,
        };
        ingest_from_dir(
            &legacy_dir,
            Some(&filter),
            &writer,
            &mut ingested,
            &mut digests_processed,
        );
    }

    Ok(IngestResult {
        ingested,
        digests_processed,
    })
}

/// Dry-run 預覽:掃同樣的 digest 目錄,回「會被 ingest 的 signal」,
/// 但**不 append L0、不刪 digest、不改任何檔**。給 `dream --dry-run` 用。
pub fn preview_signals(ctx: &db::Context) -> Result<Vec<Signal>> {
    let mut out = Vec::new();
    if ctx.no_ship() {
        return Ok(out);
    }
    preview_from_dir(&ctx.project_dir.join("digests"), None, &mut out);
    if let Some(home) = dirs::home_dir() {
        let legacy_dir = home.join(".claude").join("session-digests");
        let repo_root = ctx.project_dir.parent().map(|p| p.to_path_buf());
        let repo_root_canon = repo_root
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok());
        let filter = CwdFilter {
            repo_root,
            repo_root_canon,
        };
        preview_from_dir(&legacy_dir, Some(&filter), &mut out);
    }
    Ok(out)
}

/// preview_signals 的單目錄掃描:同 ingest_from_dir 的過濾(processed / cwd / high-confidence)
/// 但只收集、不寫不刪。
/// ⚠ 過濾邏輯必須與 `ingest_from_dir` 一致(改一邊要改另一邊),否則 dry-run 會誤導
/// (預覽出來的跟實際 ingest 的不符)。`preview_signals_is_read_only` 測試守 read-only,
/// 但過濾平價沒有測試守 —— 改 filter 時人工確認兩處同步。
fn preview_from_dir(dir: &Path, cwd_filter: Option<&CwdFilter>, out: &mut Vec<Signal>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
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
        if v.get("processed")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(f) = cwd_filter {
            let cwd = v.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
            if !cwd_matches_repo(cwd, f.repo_root.as_deref(), f.repo_root_canon.as_deref()) {
                continue;
            }
        }
        let meta = extract_digest_meta(&v);
        let empty = Vec::new();
        let signals = v
            .get("signals")
            .and_then(|s| s.as_array())
            .unwrap_or(&empty);
        for sig in signals {
            if sig.get("confidence").and_then(|c| c.as_str()) != Some("high") {
                continue;
            }
            if let Some(text) = format_signal(sig) {
                let signal =
                    Signal::new(text, SignalSource::SessionDigest).with_session_meta(meta.clone());
                out.push(signal);
            }
        }
    }
}

/// 掃一個 digest 目錄:吸 high-confidence signals → L0 讀回驗證通過後才刪檔。
/// `cwd_filter` = Some 時只收 cwd 對應本 repo 的檔(過渡期舊全域目錄用);None = per-repo
/// 目錄天然隔離全收。
fn ingest_from_dir(
    dir: &Path,
    cwd_filter: Option<&CwdFilter>,
    writer: &SignalWriter,
    ingested: &mut usize,
    digests_processed: &mut usize,
) {
    ingest_from_dir_with_verifier(
        dir,
        cwd_filter,
        writer,
        ingested,
        digests_processed,
        |ids| writer.verify_signals_persisted(ids),
    );
}

fn ingest_from_dir_with_verifier<F>(
    dir: &Path,
    cwd_filter: Option<&CwdFilter>,
    writer: &SignalWriter,
    ingested: &mut usize,
    digests_processed: &mut usize,
    mut verify: F,
) where
    F: FnMut(&[String]) -> Result<bool>,
{
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
        // 記讀取時 mtime:ingest 完刪檔前會再比對,避免刪掉「讀後被並行 hook 重寫」
        // 的新檔(那會丟其未讀 signals)。session-digest.js 採 atomic write(temp+rename)
        // → 重寫必換 mtime,可靠偵測。
        let mtime_at_read = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // 既有殘留 `processed:true`(收嚴前舊行為標記後留檔的)→ 已吸過,直接刪掉收尾,不重收。
        // 與 cwd 無關(旗標只在成功吸入後設),故先於 cwd filter。
        if v.get("processed")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
        {
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

        let meta = extract_digest_meta(&v);
        let empty = Vec::new();
        let signals = v
            .get("signals")
            .and_then(|s| s.as_array())
            .unwrap_or(&empty);
        let mut append_failed = false;
        let mut written_ids: Vec<String> = Vec::new();
        for sig in signals {
            // 只收 high-confidence(對齊海馬 ripple 選擇性 +「high-confidence」承諾;
            // self-correction 等 medium 略過,寧缺勿濫)。
            if sig.get("confidence").and_then(|c| c.as_str()) != Some("high") {
                continue;
            }
            if let Some(text) = format_signal(sig) {
                let signal =
                    Signal::new(text, SignalSource::SessionDigest).with_session_meta(meta.clone());
                match writer.append(&signal) {
                    Ok(id) => {
                        written_ids.push(id);
                        *ingested += 1;
                    }
                    // append 失敗(如 signals/ 建不出)→ 記下,稍後保留 digest 待重收。
                    // 絕不可吞掉 + 照刪:那會把寫入失敗變成 high-confidence signal 永久遺失。
                    // 第一次失敗即 break:保留的 digest 下次重收時會從頭再跑,若這次已寫入 1..k,
                    // 繼續 append 會讓 1..k 在重收時重複(各帶新 uuid)→ L0 重複行。break 維持
                    // 「保留的 digest 尚無 partial prefix 寫入」,重收時乾淨重來。
                    Err(e) => {
                        eprintln!(
                            "⚠ ingest-digests: append signal 失敗（{e}）— 保留 digest 待重收"
                        );
                        append_failed = true;
                        break;
                    }
                }
            }
        }

        // append 曾失敗 → 不刪此 digest(留待修好後重收),避免靜默遺失未寫入的 signal。
        if append_failed {
            continue;
        }

        // 下游確認（P1）：寫入 L0 後必須從磁碟讀回驗證每個 signal id 確實存在，才允許刪除 digest；
        // 讀回失敗或驗證未通過就保留 digest 檔，留待下次重試。
        if !written_ids.is_empty() {
            match verify(&written_ids) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!(
                        "⚠ ingest-digests: L0 讀回驗證未找到剛寫入的 signal — 保留 {} 待下次重試",
                        path.display()
                    );
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "⚠ ingest-digests: L0 讀回驗證失敗（{e}）— 保留 {} 待下次重試",
                        path.display()
                    );
                    continue;
                }
            }
        }

        // ingest 完刪 digest 檔(A′:明文不長存)。**全 medium(吸 0 顆)也刪**:medium 本
        // 就不入腦(設計如此),且不刪會讓明文多留到 30 天 cleanup → 與隱私目標相悖,故
        // 一律刪,不保留(別採「ingested>0 才刪」)。
        // 競態防護:只在「自讀取後 mtime 未變」時刪。若被並行 hook 重寫(新 mtime),不刪、
        // 留待下次 ingest 重讀新內容——寧靠 Mnemos dedup --scan 重收,也不靜默丟未讀 signal。
        let rewritten = match (
            mtime_at_read,
            std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
        ) {
            (Some(a), Some(b)) => a != b,
            _ => false, // 取不到 mtime → 當作沒變,維持「吸完即刪」舊行為。
        };
        if rewritten {
            eprintln!(
                "ℹ ingest-digests: {} 讀取後被改寫,本輪不刪,留待下次重讀(dedup 收斂)",
                path.display()
            );
        } else if let Err(e) = std::fs::remove_file(&path) {
            // 刪失敗 → 退回標 `processed:true` 避免下次重收(下次再嘗試刪),不吞錯出聲告警。
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
            parts.push(format!("{key}={}", crate::redact::mask_secrets(val)));
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
        let root = Path::new("/home/user/projects/mnemos");
        assert!(cwd_matches_repo(
            "/home/user/projects/mnemos",
            Some(root),
            None
        ));
        assert!(!cwd_matches_repo(
            "/home/user/projects/hangar",
            Some(root),
            None
        ));
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

    /// `dream --dry-run` 用的 preview_signals 必須是純讀:回會吸入的 signal,但
    /// 不刪 digest、不寫 L0/log。對比上面真 ingest 會刪 —— 釘死 read-only 不變式,
    /// 防未來改動(如 compile_signal 長出 side effect)讓 dry-run 默默寫 prod store。
    #[test]
    fn preview_signals_is_read_only() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let digests = ctx.project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-06-25-readonly.json",
            serde_json::json!({
                "cwd": "/whatever", "date": "2026-06-25", "processed": false,
                "signals": [
                    {"type":"error-recovery","confidence":"high","tool":"Bash","error":"demo read-only preview signal 夠長以過字數門檻"}
                ]
            }),
        );
        let sigs = preview_signals(&ctx).unwrap();
        assert!(
            sigs.iter().any(|s| s.content.contains("demo read-only")),
            "preview 應回會吸入的 signal"
        );
        assert!(digest.exists(), "preview 不該刪 digest");
        let jsonl_count = std::fs::read_dir(ctx.project_dir.join("signals"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
            .count();
        assert_eq!(jsonl_count, 0, "preview 不該寫任何 L0 signal 檔");
        assert!(
            !ctx.project_dir.join("log.md").exists(),
            "preview 不該寫 log.md"
        );
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
        let filter = CwdFilter {
            repo_root,
            repo_root_canon: None,
        };
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(
            &legacy,
            Some(&filter),
            &writer,
            &mut ingested,
            &mut processed,
        );

        assert_eq!(ingested, 0);
        assert!(
            other.exists(),
            "別的 repo 未處理的 legacy digest 不該被刪(留給其 owning repo)"
        );
    }

    #[test]
    fn no_ship_marker_skips_ingest() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let digests = ctx.project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-06-17-33333333.json",
            serde_json::json!({
                "processed": false,
                "signals": [{"type":"user-correction","confidence":"high","correction":"不對這個要改掉重來一次"}]
            }),
        );
        std::fs::write(ctx.project_dir.join("no-ship"), "").unwrap();

        // run() 在 no_ship() 為 true 時提早 return,連舊全域目錄都不掃(故此測不碰真 home)。
        let r = run(&ctx).unwrap();
        assert_eq!(r.ingested, 0, "no-ship repo 不該 ingest");
        assert!(digest.exists(), "no-ship 時 digest 檔保留原處(不刪)");
        assert!(ctx.no_ship());
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
        let filter = CwdFilter {
            repo_root: Some(repo),
            repo_root_canon: None,
        };
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(
            &legacy,
            Some(&filter),
            &writer,
            &mut ingested,
            &mut processed,
        );

        assert_eq!(ingested, 1, "cwd 對應本 repo 的 legacy digest 應吸入");
        assert!(!f.exists(), "matching legacy digest 吸完應刪");
    }

    fn read_all_l0_signals(ctx: &db::Context) -> Vec<Signal> {
        let dir = ctx.project_dir.join("signals");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(sig) = serde_json::from_str::<Signal>(trimmed) {
                        out.push(sig);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn extract_digest_meta_populates_all_fields_and_seat_env_fallback() {
        let digest = serde_json::json!({
            "session_id": "sess-demo-0001",
            "time_start": "2026-11-23T01:00:00Z",
            "time_end": "2026-11-23T01:30:00Z",
            "model": "claude-sonnet-4-5",
            "signals": []
        });
        let meta = extract_digest_meta_with(&digest, |k| {
            if k == "CODEFORGE_SEAT" {
                Some("seat-alpha.local".to_string())
            } else {
                None
            }
        });
        assert_eq!(meta.session_id.as_deref(), Some("sess-demo-0001"));
        assert_eq!(meta.time_start.as_deref(), Some("2026-11-23T01:00:00Z"));
        assert_eq!(meta.time_end.as_deref(), Some("2026-11-23T01:30:00Z"));
        assert_eq!(meta.seat.as_deref(), Some("seat-alpha.local"));
        assert_eq!(meta.model.as_deref(), Some("claude-sonnet-4-5"));

        // Fleet env fallback when CODEFORGE_SEAT absent; None when no env set.
        let fleet_meta = extract_digest_meta_with(&digest, |k| {
            if k == "FLEET_ADDRESS" {
                Some("lane-beta.local".to_string())
            } else {
                None
            }
        });
        assert_eq!(fleet_meta.seat.as_deref(), Some("lane-beta.local"));

        let no_env_meta = extract_digest_meta_with(&digest, |_| None);
        assert_eq!(no_env_meta.seat, None);
    }

    #[test]
    fn ingest_keeps_digest_on_write_failure_and_deletes_after_retry_succeeds() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("myrepo").join(".codeforge");
        std::fs::create_dir_all(&project_dir).unwrap();
        // Make `signals` a regular file so SignalWriter::append fails deterministically
        // on every OS (even as root).
        let signals_blocker = project_dir.join("signals");
        std::fs::write(&signals_blocker, "blocker").unwrap();

        let ctx = db::Context {
            project_dir: project_dir.clone(),
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };
        let digests = project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-11-23-sess0001.json",
            serde_json::json!({
                "session_id": "sess-retry-0001",
                "time_start": "2026-11-23T02:00:00Z",
                "time_end": "2026-11-23T02:20:00Z",
                "seat": "seat-alpha.local",
                "model": "claude-opus-4-6",
                "processed": false,
                "signals": [
                    {
                        "type": "error-recovery",
                        "confidence": "high",
                        "tool": "Bash",
                        "error": "write failed before retry demo signal"
                    }
                ]
            }),
        );

        let writer = SignalWriter::new(&ctx);
        let (mut ingested, mut processed) = (0, 0);
        ingest_from_dir(&digests, None, &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 0, "寫入失敗時不應計入 ingested");
        assert_eq!(processed, 0, "寫入失敗時不應計入 processed");
        assert!(digest.exists(), "寫入失敗時 digest 檔必須保留在原處");

        // Remove blocker and retry: L0 write + read-back verification now succeed → digest deleted.
        std::fs::remove_file(&signals_blocker).unwrap();
        ingest_from_dir(&digests, None, &writer, &mut ingested, &mut processed);

        assert_eq!(ingested, 1, "重試後應成功寫入 1 筆 signal");
        assert_eq!(processed, 1, "重試並驗證後應完成 1 份 digest");
        assert!(
            !digest.exists(),
            "寫入成功且 L0 讀回驗證通過後才應刪 digest"
        );

        let saved = read_all_l0_signals(&ctx);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].session_id.as_deref(), Some("sess-retry-0001"));
        assert_eq!(saved[0].time_start.as_deref(), Some("2026-11-23T02:00:00Z"));
        assert_eq!(saved[0].time_end.as_deref(), Some("2026-11-23T02:20:00Z"));
        assert_eq!(saved[0].seat.as_deref(), Some("seat-alpha.local"));
        assert_eq!(saved[0].model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn ingest_keeps_digest_when_l0_readback_verification_fails() {
        let temp = TempDir::new().unwrap();
        let ctx = test_ctx(&temp, "myrepo");
        let digests = ctx.project_dir.join("digests");
        let digest = write_digest(
            &digests,
            "2026-11-23-sess0002.json",
            serde_json::json!({
                "session_id": "sess-verify-0002",
                "time_start": "2026-11-23T03:00:00Z",
                "time_end": "2026-11-23T03:15:00Z",
                "model": "claude-sonnet-4-5",
                "processed": false,
                "signals": [
                    {
                        "type": "user-correction",
                        "confidence": "high",
                        "correction": "請在刪除暫存檔前先從磁碟讀回驗證資料完整"
                    }
                ]
            }),
        );

        let writer = SignalWriter::new(&ctx);
        let (mut ingested, mut processed) = (0, 0);
        // Simulate downstream read-back verification failing.
        ingest_from_dir_with_verifier(
            &digests,
            None,
            &writer,
            &mut ingested,
            &mut processed,
            |_| Ok(false),
        );
        assert!(digest.exists(), "L0 讀回驗證未通過時不得刪除 digest");
        assert_eq!(processed, 0);

        // With real read-back verification, digest is confirmed and deleted.
        ingest_from_dir(&digests, None, &writer, &mut ingested, &mut processed);
        assert!(!digest.exists(), "L0 讀回驗證通過後應刪除 digest");
        assert_eq!(processed, 1);
    }

    #[test]
    fn measure_l0_metadata_and_digest_safety_5_sessions_report() {
        let fixtures = [
            (
                "2026-11-23-sess0001.json",
                "sess-fixture-0001",
                "2026-11-23T01:00:00Z",
                "2026-11-23T01:25:00Z",
                "seat-alpha.local",
                "claude-sonnet-4-5",
                "【錯誤修復】Bash 測試路徑缺失修復流程範例訊號",
            ),
            (
                "2026-11-23-sess0002.json",
                "sess-fixture-0002",
                "2026-11-23T02:10:00Z",
                "2026-11-23T02:45:00Z",
                "seat-beta.local",
                "claude-opus-4-6",
                "【使用者糾正】不要直接覆寫設定檔應改用暫存檔再原子改名",
            ),
            (
                "2026-11-23-sess0003.json",
                "sess-fixture-0003",
                "2026-11-23T03:00:00Z",
                "2026-11-23T03:35:00Z",
                "seat-gamma.local",
                "claude-haiku-4-5",
                "【錯誤修復】SQLite busy_timeout 設定避免多程序鎖衝突",
            ),
            (
                "2026-11-23-sess0004.json",
                "sess-fixture-0004",
                "2026-11-23T04:15:00Z",
                "2026-11-23T04:50:00Z",
                "seat-delta.local",
                "claude-sonnet-4-5",
                "【使用者糾正】中文字串截斷必須用 chars().take(N) 避免切斷 UTF-8",
            ),
            (
                "2026-11-23-sess0005.json",
                "sess-fixture-0005",
                "2026-11-23T05:05:00Z",
                "2026-11-23T05:40:00Z",
                "seat-epsilon.local",
                "claude-opus-4-6",
                "【錯誤修復】pre-push 檢查擋下非 develop 分支推送動作",
            ),
        ];

        // 1) Measure BEFORE baseline (pre-P1 behavior: Signal had no session_id/time_start/time_end,
        //    and digest deletion did not run L0 read-back verification).
        let before_total = fixtures.len();
        let before_with_sid = 0usize;
        let before_with_range = 0usize;

        // 2) Measure AFTER on real ingest_from_dir across all 5 fixture sessions.
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&project_dir).unwrap();
        let blocker = project_dir.join("signals");
        std::fs::write(&blocker, "fail-write").unwrap();

        let ctx = db::Context {
            project_dir: project_dir.clone(),
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };
        let digests_dir = project_dir.join("digests");
        let mut digest_paths = Vec::new();
        for (fname, sid, t0, t1, seat, model, correction) in &fixtures {
            let p = write_digest(
                &digests_dir,
                fname,
                serde_json::json!({
                    "session_id": sid,
                    "time_start": t0,
                    "time_end": t1,
                    "seat": seat,
                    "model": model,
                    "processed": false,
                    "signals": [
                        {
                            "type": "user-correction",
                            "confidence": "high",
                            "correction": correction
                        }
                    ]
                }),
            );
            digest_paths.push(p);
        }

        let writer = SignalWriter::new(&ctx);
        let (mut ingested, mut processed) = (0, 0);
        // Phase A: ingest under write failure — all 5 digests must survive.
        ingest_from_dir(&digests_dir, None, &writer, &mut ingested, &mut processed);
        let survived_after_write_fail = digest_paths.iter().filter(|p| p.exists()).count();
        assert_eq!(survived_after_write_fail, 5);

        // Phase B: unblock write, simulate read-back verification failure — all 5 digests must survive.
        std::fs::remove_file(&blocker).unwrap();
        ingest_from_dir_with_verifier(
            &digests_dir,
            None,
            &writer,
            &mut ingested,
            &mut processed,
            |_| Ok(false),
        );
        let survived_after_verify_fail = digest_paths.iter().filter(|p| p.exists()).count();
        assert_eq!(survived_after_verify_fail, 5);

        // Clean L0 before final clean pass so counts reflect exact 5 fixture signals.
        std::fs::remove_dir_all(project_dir.join("signals")).unwrap();
        let (mut final_ingested, mut final_processed) = (0, 0);
        ingest_from_dir(
            &digests_dir,
            None,
            &writer,
            &mut final_ingested,
            &mut final_processed,
        );
        assert_eq!(final_ingested, 5);
        assert_eq!(final_processed, 5);
        let remaining_after_ok = digest_paths.iter().filter(|p| p.exists()).count();
        assert_eq!(remaining_after_ok, 0);

        let persisted = read_all_l0_signals(&ctx);
        assert_eq!(persisted.len(), 5);
        let after_with_sid = persisted.iter().filter(|s| s.session_id.is_some()).count();
        let after_with_range = persisted
            .iter()
            .filter(|s| s.time_start.is_some() && s.time_end.is_some())
            .count();

        println!();
        println!("| Metric (5 fixture sessions) | Before | After |");
        println!("|-----------------------------|--------|-------|");
        println!(
            "| 有 session_id 的 signal 比例 | {before_with_sid}/{before_total} (0.0%) | {after_with_sid}/{} ({:.1}%) |",
            persisted.len(),
            (after_with_sid as f64 / persisted.len() as f64) * 100.0
        );
        println!(
            "| 有時間範圍 (time_start/time_end) 的比例 | {before_with_range}/{before_total} (0.0%) | {after_with_range}/{} ({:.1}%) |",
            persisted.len(),
            (after_with_range as f64 / persisted.len() as f64) * 100.0
        );
        println!(
            "| digest 在 ingest 失敗後是否還在 (寫入失敗 / 讀回失敗) | 0/5 (無 L0 讀回確認) | {survived_after_write_fail}/5 寫入失敗保留, {survived_after_verify_fail}/5 讀回失敗保留, 成功確認後殘留 {remaining_after_ok}/5 |"
        );

        assert_eq!(after_with_sid, 5);
        assert_eq!(after_with_range, 5);
    }
}
