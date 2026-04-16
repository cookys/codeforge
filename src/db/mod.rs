use anyhow::{Context as AnyhowContext, Result};
use rusqlite::Connection;
use std::path::PathBuf;

pub struct Context {
    /// 專案記憶目錄（.codeforge/）
    pub project_dir: PathBuf,
    /// 全域 brain 目錄（~/.codeforge/brain/）
    pub brain_dir: PathBuf,
    /// SQLite game state DB 路徑
    pub db_path: PathBuf,
}

impl Context {
    pub fn load() -> Result<Self> {
        let project_dir = std::env::var("CODEFORGE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".codeforge")
            });

        let data_dir = std::env::var("CODEFORGE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_local_dir()
                    .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                    .join("codeforge")
            });

        let brain_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".codeforge")
            .join("brain");

        let db_path = data_dir.join("state.db");

        Ok(Self { project_dir, brain_dir, db_path })
    }

    /// 開啟（或建立）game state SQLite DB（WAL mode）
    pub fn open_db(&self) -> Result<Connection> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建立 DB 目錄失敗：{}", parent.display()))?;
        }
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("開啟 DB 失敗：{}", self.db_path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
        Ok(conn)
    }

    /// 確認 .codeforge/ 目錄結構已初始化
    pub fn ensure_initialized(&self) -> Result<()> {
        if !self.project_dir.exists() {
            anyhow::bail!(
                ".codeforge/ 目錄不存在，請先執行 `codeforge init`"
            );
        }
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.project_dir.exists()
    }
}

pub mod migrations {
    use anyhow::Result;
    use rusqlite::Connection;

    pub fn run(conn: &Connection) -> Result<()> {
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_migrated_memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        migrations::run(&conn).expect("migrations");
        conn
    }

    #[test]
    fn migrations_run_clean() {
        let conn = open_migrated_memory_db();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
             AND name IN ('pet','xp_events','badges','dream_runs','signal_cursors','settings')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn schema_version_seeded() {
        let conn = open_migrated_memory_db();
        let v: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE version=1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn default_settings_seeded() {
        let conn = open_migrated_memory_db();
        let theme: String = conn.query_row(
            "SELECT value FROM settings WHERE key='theme'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(theme, "amber");

        let width: String = conn.query_row(
            "SELECT value FROM settings WHERE key='statusline_width'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(width, "100");
    }

    #[test]
    fn db_opens_with_wal_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = Connection::open(tmp.path().join("test.db")).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        ).unwrap();
        let mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(mode, "wal");
    }
}
