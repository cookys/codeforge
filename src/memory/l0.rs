use crate::db;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl Signal {
    pub fn new(content: String, source: SignalSource) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            source,
            timestamp: Utc::now().to_rfc3339(),
            compiled: false,
        }
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

        // 更新 log.md（人類可讀摘要）
        self.append_log_md(signal)?;

        Ok(signal.id.clone())
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
}
