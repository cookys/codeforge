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
        upgrade_mobs_origin_path(conn)?;
        upgrade_pet_snapshot_mood(conn)?;
        upgrade_pet_snapshot_strategy(conn)?;
        Ok(())
    }

    /// Phase 2c upgrade: existing Phase 2b DBs have a `mobs` table without
    /// `origin_path`. Greenfield installs already include the column via
    /// CREATE TABLE (schema.sql), so check before ALTERing. SQLite has no
    /// `ALTER TABLE ADD COLUMN IF NOT EXISTS` — we inspect `PRAGMA
    /// table_info` ourselves.
    fn upgrade_mobs_origin_path(conn: &Connection) -> Result<()> {
        let has_column = column_exists(conn, "mobs", "origin_path")?;
        if !has_column {
            conn.execute("ALTER TABLE mobs ADD COLUMN origin_path TEXT", [])?;
        }
        Ok(())
    }

    /// Phase 3d upgrade: existing DBs lack `pet_snapshot.mood` and
    /// `mood_tick_stamp`. Greenfield installs already have them via
    /// CREATE TABLE. Same ALTER-guard pattern as phase2c.
    ///
    /// NOTE: SQLite's ALTER TABLE ADD COLUMN forbids non-constant DEFAULTs,
    /// but literal integers are fine, so the upgrade and CREATE TABLE
    /// produce identical column state.
    fn upgrade_pet_snapshot_mood(conn: &Connection) -> Result<()> {
        if !column_exists(conn, "pet_snapshot", "mood")? {
            conn.execute(
                "ALTER TABLE pet_snapshot ADD COLUMN mood INTEGER NOT NULL DEFAULT 60",
                [],
            )?;
        }
        if !column_exists(conn, "pet_snapshot", "mood_tick_stamp")? {
            conn.execute(
                "ALTER TABLE pet_snapshot ADD COLUMN mood_tick_stamp INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    /// Phase 3b upgrade: existing DBs lack `pet_snapshot.strategy`. Greenfield
    /// installs already have it via CREATE TABLE. Default 'explorer' keeps
    /// prior combat behaviour (1.0x/1.0x, no MOB priority bias).
    fn upgrade_pet_snapshot_strategy(conn: &Connection) -> Result<()> {
        if !column_exists(conn, "pet_snapshot", "strategy")? {
            conn.execute(
                "ALTER TABLE pet_snapshot ADD COLUMN strategy TEXT NOT NULL DEFAULT 'explorer'",
                [],
            )?;
        }
        Ok(())
    }

    fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
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

    // ─── Phase 2c migration tests ─────────────────────────────────

    #[test]
    fn phase2c_origin_path_on_fresh_install() {
        let conn = open_migrated_memory_db();
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(mobs)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"origin_path".to_string()));
    }

    #[test]
    fn phase2c_origin_path_nullable() {
        // Fresh install: INSERT without origin_path must succeed.
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, spawned_at)
             VALUES ('rust', 'ghost', 'x', 1, 1, 1, 1, 100)",
            [],
        )
        .unwrap();
        let path: Option<String> = conn
            .query_row("SELECT origin_path FROM mobs WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(path.is_none(), "origin_path must default to NULL");
    }

    #[test]
    fn phase2c_origin_path_round_trips() {
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO mobs
                (zone_id, kind, name, hp, hp_max, atk, def, spawned_at, origin_path)
             VALUES ('rust', 'zombie', 'TODO@src/lib.rs', 30, 30, 5, 5, 100, ?1)",
            rusqlite::params!["src/lib.rs"],
        )
        .unwrap();
        let path: String = conn
            .query_row("SELECT origin_path FROM mobs WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path, "src/lib.rs");
    }

    #[test]
    fn phase2c_upgrade_path_adds_origin_path_column() {
        // Simulate an existing Phase 2b DB: the mobs table lacks origin_path.
        // Running migrations must not drop data and must produce the column.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                zone_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                hp INTEGER NOT NULL,
                hp_max INTEGER NOT NULL,
                atk INTEGER NOT NULL,
                def INTEGER NOT NULL,
                difficulty INTEGER NOT NULL DEFAULT 1,
                spawned_at INTEGER NOT NULL,
                defeated_at INTEGER
             );
             INSERT INTO mobs
                (zone_id, kind, name, hp, hp_max, atk, def, spawned_at)
                VALUES ('rust', 'boss', 'legacy', 100, 100, 10, 10, 100);",
        )
        .unwrap();

        migrations::run(&conn).unwrap();

        // Column now exists
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(mobs)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"origin_path".to_string()));

        // Legacy row preserved, origin_path = NULL
        let (name, path): (String, Option<String>) = conn
            .query_row(
                "SELECT name, origin_path FROM mobs WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "legacy");
        assert!(path.is_none());

        // Second run must be idempotent (already has column → no error)
        migrations::run(&conn).unwrap();
    }

    #[test]
    fn phase2c_version_seeded() {
        let conn = open_migrated_memory_db();
        let v4: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=4",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v4, 4);
    }

    // ─── Phase 3d migration tests ─────────────────────────────────

    #[test]
    fn phase3d_version_seeded() {
        let conn = open_migrated_memory_db();
        let v5: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=5",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v5, 5);
    }

    #[test]
    fn phase3d_first_events_table_created() {
        let conn = open_migrated_memory_db();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='first_events'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn phase3d_first_events_pk_enforces_idempotency() {
        // event_id is PK — INSERT OR IGNORE means duplicate triggers are
        // silently no-ops. This is the core invariant for §3.8.
        let conn = open_migrated_memory_db();
        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO first_events (event_id, triggered_at, tick_count)
                 VALUES ('first_boss_kill', '2026-04-17 12:00:00', 100)",
                [],
            )
            .unwrap();
        assert_eq!(inserted, 1);
        // Second call returns 0 rows affected and does NOT error.
        let inserted_again = conn
            .execute(
                "INSERT OR IGNORE INTO first_events (event_id, triggered_at, tick_count)
                 VALUES ('first_boss_kill', '2099-99-99', 9999)",
                [],
            )
            .unwrap();
        assert_eq!(inserted_again, 0, "duplicate event_id must be ignored");
        let (ts, tick): (String, i64) = conn
            .query_row(
                "SELECT triggered_at, tick_count FROM first_events WHERE event_id='first_boss_kill'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(ts, "2026-04-17 12:00:00");
        assert_eq!(tick, 100);
    }

    #[test]
    fn phase3d_mood_columns_on_fresh_install() {
        let conn = open_migrated_memory_db();
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(pet_snapshot)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"mood".to_string()));
        assert!(cols.contains(&"mood_tick_stamp".to_string()));
    }

    #[test]
    fn phase3d_mood_default_60_on_insert() {
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver)
             VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10)",
            [],
        )
        .unwrap();
        let (mood, ts): (i64, i64) = conn
            .query_row(
                "SELECT mood, mood_tick_stamp FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(mood, 60);
        assert_eq!(ts, 0);
    }

    #[test]
    fn phase3d_mood_check_constraint_rejects_out_of_range() {
        let conn = open_migrated_memory_db();
        let err = conn
            .execute(
                "INSERT INTO pet_snapshot
                     (id, village, level, hp, hp_max, xp, xp_to_next,
                      atk, def, sup, ver, mood)
                 VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10, 150)",
                [],
            )
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("check"),
            "expected CHECK constraint, got: {err}"
        );
    }

    #[test]
    fn phase3d_upgrade_path_adds_mood_columns_to_existing_snapshot() {
        // Simulate an existing Phase 2c DB: pet_snapshot table without
        // mood / mood_tick_stamp. Migration must ALTER them in without
        // dropping data, and default existing rows to mood=60, ts=0.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL, applied_at TEXT);
             CREATE UNIQUE INDEX idx_schema_version_version ON schema_version(version);
             INSERT INTO schema_version (version, applied_at) VALUES (4, '2026-04-17');

             -- Phase 2c-era pet_snapshot (no mood columns)
             CREATE TABLE pet_snapshot (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 village TEXT NOT NULL,
                 level INTEGER NOT NULL,
                 hp INTEGER NOT NULL,
                 hp_max INTEGER NOT NULL,
                 xp INTEGER NOT NULL,
                 xp_to_next INTEGER NOT NULL,
                 atk INTEGER NOT NULL,
                 def INTEGER NOT NULL,
                 sup INTEGER NOT NULL,
                 ver INTEGER NOT NULL,
                 last_message TEXT,
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, last_message)
             VALUES (1, 'python', 5, 50, 50, 200, 500,
                     18, 14, 12, 16, 'legacy');",
        )
        .unwrap();

        migrations::run(&conn).unwrap();

        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(pet_snapshot)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"mood".to_string()));
        assert!(cols.contains(&"mood_tick_stamp".to_string()));

        // Existing row preserved + defaults applied
        let (village, mood, ts): (String, i64, i64) = conn
            .query_row(
                "SELECT village, mood, mood_tick_stamp FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(village, "python");
        assert_eq!(mood, 60);
        assert_eq!(ts, 0);

        // Second run stays idempotent
        migrations::run(&conn).unwrap();
    }

    // ─── Phase 3b migration tests ─────────────────────────────────

    #[test]
    fn phase3b_version_seeded() {
        let conn = open_migrated_memory_db();
        let v6: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=6",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v6, 6);
    }

    #[test]
    fn phase3b_strategy_column_on_fresh_install() {
        let conn = open_migrated_memory_db();
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(pet_snapshot)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"strategy".to_string()));
    }

    #[test]
    fn phase3b_strategy_default_explorer_on_insert() {
        let conn = open_migrated_memory_db();
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver)
             VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10)",
            [],
        )
        .unwrap();
        let strategy: String = conn
            .query_row(
                "SELECT strategy FROM pet_snapshot WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(strategy, "explorer");
    }

    #[test]
    fn phase3b_upgrade_path_adds_strategy_column_to_existing_snapshot() {
        // Simulate an existing Phase 3d DB: pet_snapshot with mood columns
        // but no strategy. Migration must add the column defaulting to
        // 'explorer' for preserved rows.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL, applied_at TEXT);
             CREATE UNIQUE INDEX idx_schema_version_version ON schema_version(version);
             INSERT INTO schema_version (version, applied_at) VALUES (5, '2026-04-17');

             -- Phase 3d-era pet_snapshot (with mood, no strategy)
             CREATE TABLE pet_snapshot (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 village TEXT NOT NULL,
                 level INTEGER NOT NULL,
                 hp INTEGER NOT NULL,
                 hp_max INTEGER NOT NULL,
                 xp INTEGER NOT NULL,
                 xp_to_next INTEGER NOT NULL,
                 atk INTEGER NOT NULL,
                 def INTEGER NOT NULL,
                 sup INTEGER NOT NULL,
                 ver INTEGER NOT NULL,
                 last_message TEXT,
                 mood INTEGER NOT NULL DEFAULT 60,
                 mood_tick_stamp INTEGER NOT NULL DEFAULT 0,
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, mood)
             VALUES (1, 'python', 5, 50, 50, 200, 500,
                     18, 14, 12, 16, 45);",
        )
        .unwrap();

        migrations::run(&conn).unwrap();

        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(pet_snapshot)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"strategy".to_string()));

        // Preserved row gets default 'explorer', prior mood intact.
        let (village, mood, strategy): (String, i64, String) = conn
            .query_row(
                "SELECT village, mood, strategy FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(village, "python");
        assert_eq!(mood, 45);
        assert_eq!(strategy, "explorer");

        // schema_version must advance to 6 — the insertion happens in
        // schema.sql's INSERT OR IGNORE path inside execute_batch, but
        // assert it here so future refactors don't regress silently.
        let v6: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=6",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v6, 6);

        // Second run stays idempotent.
        migrations::run(&conn).unwrap();
    }

    // ─── Phase 3c migration tests ─────────────────────────────────

    #[test]
    fn phase3c_version_seeded() {
        let conn = open_migrated_memory_db();
        let v7: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v7, 7);
    }

    #[test]
    fn phase3c_commentary_tables_created() {
        let conn = open_migrated_memory_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
                 AND name IN ('commentary_feed','commentary_history','commentary_budget')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn phase3c_budget_single_row_seeded() {
        let conn = open_migrated_memory_db();
        let (id, last): (i64, Option<i64>) = conn
            .query_row(
                "SELECT id, last_emit_at FROM commentary_budget",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(id, 1);
        assert_eq!(last, None);
    }

    #[test]
    fn phase3c_commentary_opt_in_default_off() {
        let conn = open_migrated_memory_db();
        let v: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='commentary_opt_in'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "0");
    }

    #[test]
    fn phase3c_upgrade_path_from_v6_seeds_new_tables() {
        // Simulate a Phase 3b DB on v6. The migration should create the
        // three v7 tables without touching existing rows, then seed the
        // budget row + opt-in flag.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL, applied_at TEXT);
             CREATE UNIQUE INDEX idx_schema_version_version ON schema_version(version);
             INSERT INTO schema_version (version, applied_at) VALUES (6, '2026-04-18');

             CREATE TABLE settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL,
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO settings (key, value) VALUES ('theme','amber');",
        )
        .unwrap();

        migrations::run(&conn).unwrap();

        // v7 row lands in schema_version
        let v7: i64 = conn
            .query_row(
                "SELECT version FROM schema_version WHERE version=7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v7, 7);

        // Three new tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
                 AND name IN ('commentary_feed','commentary_history','commentary_budget')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);

        // Budget single-row seeded, opt-in default off
        let last: Option<i64> = conn
            .query_row(
                "SELECT last_emit_at FROM commentary_budget WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(last, None);

        let flag: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='commentary_opt_in'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(flag, "0");

        // Idempotent second run
        migrations::run(&conn).unwrap();
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
