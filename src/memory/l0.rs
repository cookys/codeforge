use crate::db;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignalMeta {
    pub session_id: Option<String>,
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub seat: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalSource {
    Manual,
    ClaudeCodeSession,
    WebChatClaude,
    WebChatChatGpt,
    Ingest {
        path: String,
    },
    Hook {
        event: String,
    },
    /// `dream absorb` 收編的跨專案 ~/.claude/projects/*/memory（別人/別專案的記憶）。
    /// 給專屬 source 以便 ship 排除（不污染 Mnemos 腦)。見 ledger source 收嚴。
    AbsorbedMemory,
    /// session-digest hook 從本 repo transcript 萃出的 high-confidence dev signal
    /// (error-recovery / user-correction / self-correction)。第一人稱本 repo coding 經驗。
    SessionDigest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub content: String,
    pub source: SignalSource,
    pub timestamp: String,
    pub compiled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Signal {
    pub fn new(content: String, source: SignalSource) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            source,
            timestamp: Utc::now().to_rfc3339(),
            compiled: false,
            source_slug: None,
            source_file: None,
            session_id: None,
            time_start: None,
            time_end: None,
            seat: None,
            model: None,
        }
    }

    pub fn with_absorb_source(mut self, slug: String, file: String) -> Self {
        self.source_slug = Some(slug);
        self.source_file = Some(file);
        self
    }

    pub fn with_session_meta(mut self, meta: SignalMeta) -> Self {
        self.session_id = meta.session_id;
        self.time_start = meta.time_start;
        self.time_end = meta.time_end;
        self.seat = meta.seat;
        self.model = meta.model;
        self
    }
}

pub struct SignalWriter<'a> {
    ctx: &'a db::Context,
}

impl<'a> SignalWriter<'a> {
    pub fn new(ctx: &'a db::Context) -> Self {
        Self { ctx }
    }

    /// 取得今日的 JSONL 檔路徑
    fn today_file(&self) -> std::path::PathBuf {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        self.ctx
            .project_dir
            .join("signals")
            .join(format!("{}.jsonl", date))
    }

    /// Append signal 到 JSONL，同步更新人類可讀 log.md，回傳 signal id
    pub fn append(&self, signal: &Signal) -> Result<String> {
        // every source passes the shared secret mask before it touches disk
        let mut masked = signal.clone();
        masked.content = crate::redact::mask_secrets(&signal.content);
        let signal = &masked;
        let jsonl_path = self.today_file();

        // `create(true)` 只建檔、不建父目錄 → 新 store(無 signals/)會 open 失敗。
        // 必須先 create_dir_all,否則 append 靜默失敗、上游若「失敗也刪 digest」就資料遺失。
        if let Some(parent) = jsonl_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建 signals 目錄失敗：{}", parent.display()))?;
        }

        // JSONL append（append-only，不刪除）
        let line = serde_json::to_string(signal)? + "\n";
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)
            .with_context(|| format!("開啟 signal 檔失敗：{}", jsonl_path.display()))?;
        f.write_all(line.as_bytes())?;
        f.flush()?;

        // 更新 log.md（人類可讀摘要）
        self.append_log_md(signal)?;

        Ok(signal.id.clone())
    }

    /// 從磁碟重讀 `signals/*.jsonl`，驗證指定的每個 signal id 皆已落盤且可反序列化。
    /// 供上游（如 `ingest-digests`）在刪除 digest 檔前做下游確認。
    pub fn verify_signals_persisted(&self, ids: &[String]) -> Result<bool> {
        if ids.is_empty() {
            return Ok(true);
        }
        let signal_dir = self.ctx.project_dir.join("signals");
        if !signal_dir.is_dir() {
            return Ok(false);
        }
        let mut remaining: HashSet<&str> = ids.iter().map(String::as_str).collect();
        for entry in std::fs::read_dir(&signal_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(sig) = serde_json::from_str::<Signal>(trimmed) {
                    remaining.remove(sig.id.as_str());
                    if remaining.is_empty() {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(remaining.is_empty())
    }

    fn append_log_md(&self, signal: &Signal) -> Result<()> {
        let log_path = self.ctx.project_dir.join("log.md");
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let source_str = match &signal.source {
            SignalSource::Manual => "manual",
            SignalSource::ClaudeCodeSession => "claude-code",
            SignalSource::WebChatClaude => "claude.ai",
            SignalSource::WebChatChatGpt => "chatgpt",
            SignalSource::Ingest { .. } => "ingest",
            SignalSource::Hook { event } => event.as_str(),
            SignalSource::AbsorbedMemory => "absorbed",
            SignalSource::SessionDigest => "session-digest",
        };

        let preview = truncate_str(&signal.content, 80);

        writeln!(
            f,
            "- `{}` [{}] {} — {}",
            signal.id.chars().take(8).collect::<String>(),
            source_str,
            signal.timestamp,
            preview
        )?;
        Ok(())
    }
}

/// 未編譯 L0 signal 及其來源檔與行尾 byte offset,
/// 供 `dream compile` 依實際處理到的位置增量推進 `signal_cursors`,不越過未處理項目。
#[derive(Debug, Clone)]
pub struct LocatedSignal {
    pub signal: Signal,
    pub source_file: String,
    pub end_offset: i64,
}

/// 讀取所有未編譯的 signals（含來源檔與行尾 byte offset，從上次編譯 cursor 之後）。
pub fn read_uncompiled_located(
    ctx: &db::Context,
    conn: &rusqlite::Connection,
) -> Result<Vec<LocatedSignal>> {
    let signal_dir = ctx.project_dir.join("signals");
    if !signal_dir.exists() {
        return Ok(vec![]);
    }

    let mut signals = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&signal_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        // cursor key 用完整路徑(含 repo),避免跨 repo 同名檔(如各 repo 的 2026-06-17.jsonl)
        // 在全域 state.db 共用同一 cursor → 互相擋編譯。
        let cursor_key = path.to_string_lossy().to_string();

        // 取得上次編譯到的 offset
        let cursor: i64 = conn
            .query_row(
                "SELECT last_offset FROM signal_cursors WHERE source_file = ?1",
                rusqlite::params![&cursor_key],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let content = std::fs::read_to_string(&path)?;
        if cursor as usize >= content.len() {
            continue;
        }

        // 找到 cursor 之後的第一個合法 UTF-8 字元邊界（避免在多 byte 字元中間切割）
        let char_start = content
            .char_indices()
            .find(|(byte_pos, _)| *byte_pos >= cursor as usize)
            .map(|(byte_pos, _)| byte_pos)
            .unwrap_or(content.len());

        let mut pos = char_start;
        for raw_line in content[char_start..].split_inclusive('\n') {
            let line_end = (pos + raw_line.len()) as i64;
            pos += raw_line.len();
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(s) = serde_json::from_str::<Signal>(line) {
                if !s.compiled {
                    signals.push(LocatedSignal {
                        signal: s,
                        source_file: cursor_key.clone(),
                        end_offset: line_end,
                    });
                }
            }
        }
    }

    Ok(signals)
}

/// 讀取所有未編譯的 signals（從上次編譯 cursor 之後）
#[allow(dead_code)]
pub fn read_uncompiled(ctx: &db::Context, conn: &rusqlite::Connection) -> Result<Vec<Signal>> {
    Ok(read_uncompiled_located(ctx, conn)?
        .into_iter()
        .map(|ls| ls.signal)
        .collect())
}

/// 將指定來源檔的編譯 cursor 單調推進至 `offset`（不越過未處理的 signal）。
pub fn advance_cursor(conn: &rusqlite::Connection, source_file: &str, offset: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO signal_cursors (source_file, last_offset, updated_at)
         VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(source_file) DO UPDATE SET
             last_offset = MAX(signal_cursors.last_offset, excluded.last_offset),
             updated_at = datetime('now')",
        rusqlite::params![source_file, offset],
    )?;
    Ok(())
}

/// 標記整日 signal 檔為已編譯（更新 cursor 至檔尾）
#[allow(dead_code)]
pub fn mark_compiled(
    ctx: &db::Context,
    conn: &rusqlite::Connection,
    file_date: &str,
) -> Result<()> {
    let file_name = format!("{}.jsonl", file_date);
    let path = ctx.project_dir.join("signals").join(&file_name);
    let size = std::fs::metadata(&path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    // cursor key = 完整路徑(含 repo),與 read_uncompiled 一致,避免跨 repo 共用 cursor。
    let cursor_key = path.to_string_lossy().to_string();
    advance_cursor(conn, &cursor_key, size)
}

/// 安全截斷字串（不在中文字元邊界截斷）
fn truncate_str(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", chars[..max_chars].iter().collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// R3: every L0 source (learn/absorb/digest) is secret-masked at write time.
    #[test]
    fn append_masks_secrets_for_every_source() {
        let tmp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: tmp.path().to_path_buf(),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        let writer = SignalWriter::new(&ctx);
        for src in [SignalSource::Manual, SignalSource::AbsorbedMemory] {
            let sig = Signal::new("deploy with password=hunter2 now".to_string(), src);
            writer.append(&sig).unwrap();
        }
        let mut all = String::new();
        for e in std::fs::read_dir(tmp.path().join("signals")).unwrap() {
            all.push_str(&std::fs::read_to_string(e.unwrap().path()).unwrap());
        }
        all.push_str(&std::fs::read_to_string(tmp.path().join("log.md")).unwrap());
        assert!(!all.contains("hunter2"), "secret leaked into L0: {all}");
        assert!(all.contains("deploy with"), "{all}");
    }

    /// 回歸:fresh store(無 signals/ 目錄)時 append 必須自建目錄並成功寫入。
    /// 否則 append 靜默失敗,上游 ingest-digests「失敗也刪 digest」會造成
    /// high-confidence signal 永久遺失(2026-06-25 抓到的 bug)。
    #[test]
    fn append_creates_signals_dir_on_fresh_store() {
        let tmp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: tmp.path().to_path_buf(),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        assert!(
            !tmp.path().join("signals").exists(),
            "前提:store 一開始無 signals/ 目錄"
        );
        let writer = SignalWriter::new(&ctx);
        let sig = Signal::new(
            "【錯誤修復】tool=Bash | error=demo signal".to_string(),
            SignalSource::SessionDigest,
        );
        let r = writer.append(&sig);
        assert!(r.is_ok(), "append 應成功(自建 signals/),got: {r:?}");
        assert!(
            tmp.path().join("signals").is_dir(),
            "append 應 create_dir_all 出 signals/"
        );
        let n = std::fs::read_dir(tmp.path().join("signals"))
            .unwrap()
            .count();
        assert_eq!(n, 1, "應寫出一個 jsonl 檔");
    }

    #[test]
    fn advance_cursor_preserves_unprocessed_tail_signals() {
        let tmp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: tmp.path().to_path_buf(),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        let conn = ctx.open_db().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        let writer = SignalWriter::new(&ctx);
        for i in 0..5 {
            writer
                .append(&Signal::new(
                    format!("中文訊號第 {i} 條"),
                    SignalSource::Manual,
                ))
                .unwrap();
        }

        let all = read_uncompiled_located(&ctx, &conn).unwrap();
        assert_eq!(all.len(), 5);

        // 只處理前 2 條，cursor 停在第 2 條行尾。
        advance_cursor(&conn, &all[1].source_file, all[1].end_offset).unwrap();

        let rest = read_uncompiled(&ctx, &conn).unwrap();
        assert_eq!(rest.len(), 3);
        assert!(rest[0].content.contains("第 2 條"));
        assert!(rest[2].content.contains("第 4 條"));
    }

    #[test]
    fn legacy_and_new_signal_jsonl_roundtrip_compat() {
        // 舊格式 JSONL（無 session_id / time_start / time_end / seat / model）仍須可反序列化
        let legacy_line = r#"{"id":"11111111-1111-1111-1111-111111111111","content":"舊版 signal 內容","source":"SessionDigest","timestamp":"2026-06-17T08:00:00Z","compiled":false}"#;
        let legacy: Signal = serde_json::from_str(legacy_line).unwrap();
        assert_eq!(legacy.id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(legacy.session_id, None);
        assert_eq!(legacy.time_start, None);
        assert_eq!(legacy.time_end, None);
        assert_eq!(legacy.seat, None);
        assert_eq!(legacy.model, None);

        // None 欄位序列化時應省略（skip_serializing_if）
        let serialized_none = serde_json::to_string(&legacy).unwrap();
        assert!(!serialized_none.contains("session_id"));
        assert!(!serialized_none.contains("time_start"));
        assert!(!serialized_none.contains("time_end"));
        assert!(!serialized_none.contains("seat"));
        assert!(!serialized_none.contains("model"));

        // 新格式完整欄位 roundtrip
        let meta = SignalMeta {
            session_id: Some("sess-demo-0001".to_string()),
            time_start: Some("2026-11-23T01:00:00Z".to_string()),
            time_end: Some("2026-11-23T01:45:00Z".to_string()),
            seat: Some("seat-alpha.local".to_string()),
            model: Some("claude-sonnet-4-5".to_string()),
        };
        let fresh = Signal::new("新版 signal 內容".to_string(), SignalSource::SessionDigest)
            .with_session_meta(meta);
        let line = serde_json::to_string(&fresh).unwrap();
        let parsed: Signal = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.session_id.as_deref(), Some("sess-demo-0001"));
        assert_eq!(parsed.time_start.as_deref(), Some("2026-11-23T01:00:00Z"));
        assert_eq!(parsed.time_end.as_deref(), Some("2026-11-23T01:45:00Z"));
        assert_eq!(parsed.seat.as_deref(), Some("seat-alpha.local"));
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-5"));

        // 混合舊新兩行在同一 JSONL 檔內皆可讀進
        let tmp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: tmp.path().to_path_buf(),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        let conn = ctx.open_db().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let sig_dir = tmp.path().join("signals");
        std::fs::create_dir_all(&sig_dir).unwrap();
        std::fs::write(
            sig_dir.join("2026-11-23.jsonl"),
            format!("{legacy_line}\n{line}\n"),
        )
        .unwrap();

        let loaded = read_uncompiled(&ctx, &conn).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].session_id, None);
        assert_eq!(loaded[1].session_id.as_deref(), Some("sess-demo-0001"));
        assert_eq!(loaded[1].model.as_deref(), Some("claude-sonnet-4-5"));
    }

    #[test]
    fn verify_signals_persisted_confirms_written_and_rejects_missing() {
        let tmp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: tmp.path().to_path_buf(),
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        let writer = SignalWriter::new(&ctx);
        assert!(writer.verify_signals_persisted(&[]).unwrap());
        let missing_one = writer
            .verify_signals_persisted(&["nonexistent-id".to_string()])
            .unwrap();
        assert!(!missing_one);

        let s1 = Signal::new("第一筆 signal".to_string(), SignalSource::SessionDigest);
        let id1 = writer.append(&s1).unwrap();
        assert!(writer
            .verify_signals_persisted(std::slice::from_ref(&id1))
            .unwrap());
        let partial = writer
            .verify_signals_persisted(&[id1, "missing-id".to_string()])
            .unwrap();
        assert!(!partial);
    }
}
