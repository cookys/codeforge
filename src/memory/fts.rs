use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug)]
pub struct SearchResult {
    pub file_path: String,
    pub title: String,
    pub snippet: Option<String>,
    pub rank: f64,
}

pub struct FtsIndex<'a> {
    conn: &'a Connection,
}

impl<'a> FtsIndex<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 索引或更新一個 L1 entry
    pub fn upsert(&self, file_path: &str, title: &str, body: &str, tags: &str) -> Result<()> {
        // 先刪除舊紀錄
        self.conn.execute(
            "DELETE FROM memory_fts WHERE file_path = ?1",
            rusqlite::params![file_path],
        )?;
        // 插入新紀錄
        self.conn.execute(
            "INSERT INTO memory_fts (file_path, title, body, tags) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![file_path, title, body, tags],
        )?;
        Ok(())
    }

    /// 全文搜尋，回傳最多 limit 筆結果
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // 清理 query（避免 FTS5 語法錯誤）
        let safe_query = sanitize_query(query);

        let mut stmt = self.conn.prepare(
            "SELECT file_path, title, snippet(memory_fts, 2, '<', '>', '…', 20), rank
             FROM memory_fts
             WHERE memory_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        )?;

        let results = stmt.query_map(
            rusqlite::params![safe_query, limit as i64],
            |row| {
                Ok(SearchResult {
                    file_path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get(2)?,
                    rank: row.get(3)?,
                })
            },
        )?
        .filter_map(|r| r.ok())
        .collect();

        Ok(results)
    }

    /// 重建整個 FTS index（從 L1 store 目錄掃描）
    pub fn rebuild(&self, store_dir: &std::path::Path) -> Result<usize> {
        // 清空舊 index
        self.conn.execute("DELETE FROM memory_fts", [])?;

        let entries = crate::memory::l1::scan_all(store_dir)?;
        let count = entries.len();

        for entry in &entries {
            let rel_path = entry.file_path
                .strip_prefix(store_dir.parent().unwrap_or(store_dir))
                .unwrap_or(&entry.file_path)
                .to_string_lossy()
                .to_string();

            let tags = entry.frontmatter.links.join(" ");
            self.upsert(&rel_path, &entry.title, &entry.body, &tags)?;
        }

        Ok(count)
    }
}

fn sanitize_query(q: &str) -> String {
    // 移除 FTS5 特殊字元，保留字詞
    q.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
