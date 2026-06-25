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
                out.push(Signal::new(text, SignalSource::SessionDigest));
            }
        }
    }
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

        let empty = Vec::new();
        let signals = v
            .get("signals")
            .and_then(|s| s.as_array())
            .unwrap_or(&empty);
        let mut append_failed = false;
        for sig in signals {
            // 只收 high-confidence(對齊海馬 ripple 選擇性 +「high-confidence」承諾;
            // self-correction 等 medium 略過,寧缺勿濫)。
            if sig.get("confidence").and_then(|c| c.as_str()) != Some("high") {
                continue;
            }
            if let Some(text) = format_signal(sig) {
                let signal = Signal::new(text, SignalSource::SessionDigest);
                match writer.append(&signal) {
                    Ok(_) => *ingested += 1,
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
            *digests_processed += 1;
            continue;
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
/// 函式;無 regex crate,用 prefix + 高熵長度啟發式,寧可多遮)。
///
/// 兩道:(1) 先整段遮 PEM / OpenSSH 私鑰 block(多行,逐 token 啟發式抓不到 header /
/// 行長 metadata / key 路徑等周邊);(2) 再逐 token 遮已知前綴 / 高熵 token。
fn mask_secrets(s: &str) -> String {
    let pem_masked = mask_pem_blocks(s);
    pem_masked
        .split(' ')
        .map(mask_word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// 整段遮蔽 PEM / OpenSSH 私鑰 block:任何含 `BEGIN`+`PRIVATE KEY` 的行起,到含
/// `END`+`PRIVATE KEY` 的行止(含),整段(header / body / footer)收成單一 marker。
///
/// 為何需要:私鑰 base64 body 雖是高熵、token 啟發式或可抓,但 header
/// (`-----BEGIN OPENSSH PRIVATE KEY-----`)、`base64 body 行長` 之類結構 metadata、
/// key 路徑等周邊全漏網;且被 `truncate(300)` 截斷的 block 根本沒有 END 行。逐行整段
/// 遮才穩。無 END(截斷)時,從 BEGIN 行起遮到字串尾(保守,寧可多遮)。
/// codeforge 無 regex crate → 逐行字串比對。
fn mask_pem_blocks(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_key = false;
    for line in s.split('\n') {
        let has_pk = line.contains("PRIVATE KEY");
        if in_key {
            // body / footer 全丟,看到 END 行才退出 redact。
            if has_pk && line.contains("END") {
                in_key = false;
            }
            continue;
        }
        if has_pk && line.contains("BEGIN") {
            out.push("[PRIVATE KEY REDACTED]");
            // 純 BEGIN 行 → 進多行 redact;同行 BEGIN+END(單行表示)罕見,只留 marker。
            if !line.contains("END") {
                in_key = true;
            }
            continue;
        }
        out.push(line);
    }
    out.join("\n")
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
        "sk-",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "AKIA",
        "ASIA",
        "AIza",
    ];
    if t.len() >= 12 && PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    // 高熵:32+ 連續 alnum/_/-(不含 '/' 以免遮路徑),且字母+數字混合。
    t.len() >= 32
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_token_prefixes_and_high_entropy() {
        // KEY=token(無空格)前綴遮
        assert_eq!(
            mask_secrets("export GH=ghp_abcdefABCDEF1234567890"),
            "export GH=***"
        );
        // 裸高熵 token 遮
        assert!(mask_secrets("got abcd1234efgh5678ijkl9012mnop3456").contains("***"));
        // 一般文字不遮
        assert_eq!(
            mask_secrets("修了 crontab 的 sed 問題"),
            "修了 crontab 的 sed 問題"
        );
        // 路徑不遮(含 /)
        assert_eq!(
            mask_secrets("檔案 /home/cookys/projects/mnemos/src"),
            "檔案 /home/cookys/projects/mnemos/src"
        );
    }

    #[test]
    fn masks_pem_private_key_blocks() {
        // 完整 block:header / body / footer 全遮,前後文保留。
        let full = "ctx before\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXkAAAAB\nMOREKEYBODY9999\n-----END OPENSSH PRIVATE KEY-----\nctx after";
        let m = mask_secrets(full);
        assert!(m.contains("[PRIVATE KEY REDACTED]"), "需有 marker: {m}");
        assert!(!m.contains("BEGIN OPENSSH"), "header 須遮: {m}");
        assert!(!m.contains("b3BlbnNzaC1rZXk"), "body 須遮: {m}");
        assert!(!m.contains("MOREKEYBODY9999"), "body 須遮: {m}");
        assert!(
            m.contains("ctx before") && m.contains("ctx after"),
            "前後文須保留: {m}"
        );

        // 截斷(被 truncate(300) 砍掉 END):從 BEGIN 遮到字串尾。
        let truncated =
            "行數: 8\n-----BEGIN OPENSSH PRIVATE KEY-----   \nbase64 body 行長:\n2: 70\nLEAKTAIL";
        let mt = mask_secrets(truncated);
        assert!(mt.contains("[PRIVATE KEY REDACTED]"));
        assert!(!mt.contains("BEGIN OPENSSH"));
        assert!(!mt.contains("LEAKTAIL"), "截斷後 BEGIN 之後須全遮: {mt}");
        assert!(mt.contains("行數: 8"), "BEGIN 前文字須保留: {mt}");

        // 無 PEM 的一般多行文字不受影響。
        assert_eq!(mask_secrets("line one\nline two"), "line one\nline two");
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
        assert!(cwd_matches_repo(
            "/home/cookys/projects/mnemos",
            Some(root),
            None
        ));
        assert!(!cwd_matches_repo(
            "/home/cookys/projects/hangar",
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
}
