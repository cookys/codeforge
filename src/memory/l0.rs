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
    Ingest { path: String },
    Hook { event: String },
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

/// 讀取所有未編譯的 signals（從上次編譯 cursor 之後）
pub fn read_uncompiled(ctx: &db::Context, conn: &rusqlite::Connection) -> Result<Vec<Signal>> {
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
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();

        // 取得上次編譯到的 offset
        let cursor: i64 = conn
            .query_row(
                "SELECT last_offset FROM signal_cursors WHERE source_file = ?1",
                rusqlite::params![&file_name],
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

        let slice = &content[char_start..];
        for line in slice.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(s) = serde_json::from_str::<Signal>(line) {
                if !s.compiled {
                    signals.push(s);
                }
            }
        }
    }

    Ok(signals)
}

/// 標記 signal 為已編譯（更新 cursor）
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

    conn.execute(
        "INSERT OR REPLACE INTO signal_cursors (source_file, last_offset, updated_at) VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![&file_name, size],
    )?;
    Ok(())
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
