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
    fn phase2a_tables_created() {
        let conn = open_migrated_memory_db();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
             AND name IN ('pet_snapshot','game_world','combat_log','event_inbox','last_tick_at')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn phase2b_tables_created() {
        let conn = open_migrated_memory_db();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
             AND name IN ('mobs','loot_inventory')",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn phase2b_version_seeded() {
        let conn = open_migrated_memory_db();
        let v3: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE version=3",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v3, 3);
    }

    #[test]
    fn mobs_alive_partial_index_exists() {
        let conn = open_migrated_memory_db();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='index' AND name='idx_mobs_alive'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn loot_dedupe_unique_index_enforces_aggregation() {
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('item', 'Rare Item', 1, 1000, 1000)",
            [],
        ).unwrap();
        // Same (kind, name) must collide; caller is expected to UPSERT
        let err = conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('item', 'Rare Item', 1, 2000, 2000)",
            [],
        ).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"), "expected UNIQUE constraint, got: {err}");
    }

    #[test]
    fn mobs_unique_alive_index_blocks_duplicate_spawns() {
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, spawned_at)
             VALUES ('rust', 'boss', 'src/auth.rs', 100, 100, 20, 15, 1000)",
            [],
        ).unwrap();
        // Same (zone, kind, name) while still alive → reject (scanner must UPSERT)
        let err = conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, spawned_at)
             VALUES ('rust', 'boss', 'src/auth.rs', 100, 100, 20, 15, 2000)",
            [],
        ).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));

        // But once defeated, a new instance may re-spawn (the partial index excludes it)
        conn.execute(
            "UPDATE mobs SET defeated_at = 3000 WHERE id = 1",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, spawned_at)
             VALUES ('rust', 'boss', 'src/auth.rs', 100, 100, 20, 15, 4000)",
            [],
        ).unwrap();
    }

    #[test]
    fn loot_quantity_must_be_positive() {
        let conn = open_migrated_memory_db();
        let err = conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('item', 'Broken', 0, 1000, 1000)",
            [],
        ).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));
    }

    #[test]
    fn event_inbox_unseen_index_exists() {
        let conn = open_migrated_memory_db();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='index' AND name='idx_event_inbox_unseen'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn event_inbox_write_shape() {
        // Hook 端：INSERT 只寫 payload + created_at；seen_at 留 NULL
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
            rusqlite::params!["{\"event\":\"git_commit\"}", 1_700_000_000i64],
        ).unwrap();
        let (payload, created_at, seen_at): (String, i64, Option<i64>) = conn.query_row(
            "SELECT payload, created_at, seen_at FROM event_inbox WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(payload, "{\"event\":\"git_commit\"}");
        assert_eq!(created_at, 1_700_000_000);
        assert_eq!(seen_at, None);

        // Daemon 端：UPDATE seen_at，不動 payload 欄位（寫入集不重疊）
        conn.execute(
            "UPDATE event_inbox SET seen_at = ?1 WHERE id = ?2",
            rusqlite::params![1_700_000_001i64, 1],
        ).unwrap();
        let seen_at: Option<i64> = conn.query_row(
            "SELECT seen_at FROM event_inbox WHERE id=1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(seen_at, Some(1_700_000_001));
    }

    #[test]
    fn migrations_dedup_phase1_duplicates() {
        // Simulates an existing Phase-1 DB where schema_version got multiple
        // rows with version=1 (because INSERT OR IGNORE had no UNIQUE to key off).
        // Running the new migration on such a DB must dedup before CREATE INDEX.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE schema_version (
                version INTEGER NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO schema_version (version) VALUES (1);
            INSERT INTO schema_version (version) VALUES (1);
            INSERT INTO schema_version (version) VALUES (1);
        ").unwrap();

        // Should succeed (dedup first, then create UNIQUE index)
        migrations::run(&conn).unwrap();

        let v1_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_version WHERE version=1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v1_rows, 1, "duplicate version=1 rows should be deduped");
    }

    #[test]
    fn migrations_are_idempotent() {
        // 同一個連線跑兩次 migration 不應該錯、也不應該重複 seed
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        migrations::run(&conn).unwrap();

        let theme_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM settings WHERE key='theme'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(theme_rows, 1);

        let v1_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_version WHERE version=1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v1_rows, 1);

        let v2_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_version WHERE version=2",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v2_rows, 1);
    }

    #[test]
    fn schema_version_seeded() {
        let conn = open_migrated_memory_db();
        let v1: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE version=1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v1, 1);

        let v2: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE version=2",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v2, 2);
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
