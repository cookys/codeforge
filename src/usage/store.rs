//! `<data_dir>/usage/usage.db` (G36 plan, phase 2).
//!
//! Modes are set before SQLite opens the file, in this order: the dir is created 0700 or
//! tightened; the db is created 0600 or tightened; existing `-wal` / `-shm` are tightened;
//! only then the connection opens (the unix VFS gives new sidecars the db file's mode).
//! No umask change. Each fix of a wider mode is returned as a note for one stderr line.

use anyhow::{Context as _, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// `CODEFORGE_DATA_DIR`, else `<data_local_dir>/codeforge` — the same root as `src/db`.
/// The env value is a parameter (env-var test seams race under `cargo test`).
pub fn data_dir(env_data_dir: Option<&str>) -> PathBuf {
    match env_data_dir {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("codeforge"),
    }
}

/// The G36-owned dir under the data dir.
pub fn usage_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("usage")
}

#[cfg(unix)]
fn tighten(path: &Path, mode: u32, notes: &mut Vec<String>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    let have = meta.permissions().mode() & 0o777;
    if have & !mode != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .with_context(|| format!("無法收緊權限：{}", path.display()))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        notes.push(format!("{name}: {have:o} → {mode:o}"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten(_path: &Path, _mode: u32, _notes: &mut Vec<String>) -> Result<()> {
    Ok(())
}

/// Prepare modes in order, then open. Returns the connection and the mode notes.
pub fn open(dir: &Path) -> Result<(Connection, Vec<String>)> {
    let mut notes = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("無法建立用量目錄：{}", dir.display()))?;
        tighten(dir, 0o700, &mut notes)?;
        let db = dir.join("usage.db");
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&db)
            .with_context(|| format!("無法建立用量資料庫：{}", db.display()))?;
        tighten(&db, 0o600, &mut notes)?;
        tighten(&dir.join("usage.db-wal"), 0o600, &mut notes)?;
        tighten(&dir.join("usage.db-shm"), 0o600, &mut notes)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)?;
    let conn = Connection::open(dir.join("usage.db"))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         CREATE TABLE IF NOT EXISTS sources (
             source_id TEXT PRIMARY KEY, inode INTEGER NOT NULL, size INTEGER NOT NULL,
             offset INTEGER NOT NULL, head_len INTEGER NOT NULL, head TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS requests (
             request_id TEXT PRIMARY KEY, day TEXT NOT NULL, source TEXT NOT NULL,
             account TEXT NOT NULL, model TEXT NOT NULL, project TEXT NOT NULL, speed TEXT NOT NULL,
             input INTEGER NOT NULL, output INTEGER NOT NULL, cw5m INTEGER NOT NULL,
             cw1h INTEGER NOT NULL, read INTEGER NOT NULL, seen_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS codex_sessions (
             session_id TEXT PRIMARY KEY, input INTEGER NOT NULL, cached INTEGER NOT NULL,
             cache_write INTEGER NOT NULL, output INTEGER NOT NULL, seen_ms INTEGER NOT NULL,
             model TEXT NOT NULL DEFAULT 'unknown', project TEXT NOT NULL DEFAULT '');
         CREATE TABLE IF NOT EXISTS buckets (
             day TEXT NOT NULL, source TEXT NOT NULL, account TEXT NOT NULL, model TEXT NOT NULL,
             project TEXT NOT NULL, speed TEXT NOT NULL,
             input INTEGER NOT NULL DEFAULT 0, output INTEGER NOT NULL DEFAULT 0,
             cw5m INTEGER NOT NULL DEFAULT 0, cw1h INTEGER NOT NULL DEFAULT 0,
             read INTEGER NOT NULL DEFAULT 0, requests INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (day, source, account, model, project, speed));
         CREATE TABLE IF NOT EXISTS roots (
             root_hash TEXT PRIMARY KEY, alias TEXT NOT NULL, seen_ms INTEGER NOT NULL);",
    )?;
    // the first write creates -wal/-shm: they take the db's 0600; check once more
    #[cfg(unix)]
    {
        tighten(&dir.join("usage.db-wal"), 0o600, &mut notes)?;
        tighten(&dir.join("usage.db-shm"), 0o600, &mut notes)?;
    }
    Ok((conn, notes))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn fresh_store_is_0600_in_a_0700_dir() {
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        let (conn, notes) = open(&dir).unwrap();
        conn.execute("INSERT INTO roots VALUES ('x', 'acct-a', 1)", [])
            .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(mode(&dir), 0o700);
        for f in ["usage.db", "usage.db-wal", "usage.db-shm"] {
            assert_eq!(mode(&dir.join(f)), 0o600, "{f}");
        }
    }

    #[test]
    fn widened_db_dir_and_stale_wal_are_tightened_before_open() {
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        {
            let (conn, _) = open(&dir).unwrap();
            conn.execute("INSERT INTO roots VALUES ('x', 'acct-a', 1)", [])
                .unwrap();
        }
        let set = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        set(&dir, 0o755);
        set(&dir.join("usage.db"), 0o644);
        std::fs::write(dir.join("usage.db-wal"), b"").unwrap();
        set(&dir.join("usage.db-wal"), 0o644);
        let (conn, notes) = open(&dir).unwrap();
        conn.execute("INSERT INTO roots VALUES ('y', 'acct-b', 2)", [])
            .unwrap();
        assert_eq!(mode(&dir), 0o700);
        for f in ["usage.db", "usage.db-wal", "usage.db-shm"] {
            assert_eq!(mode(&dir.join(f)), 0o600, "{f}");
        }
        assert!(notes.len() >= 3, "{notes:?}");
    }

    #[test]
    fn data_dir_override() {
        assert_eq!(data_dir(Some("/x/d")), PathBuf::from("/x/d"));
        assert!(data_dir(None).ends_with("codeforge"));
    }
}
