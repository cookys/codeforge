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

/// How an entry of the store is opened: never through a symlink (`O_NOFOLLOW`), never blocking
/// on a FIFO (`O_NONBLOCK`); the handle must then be a real dir / regular file, and a wider mode
/// is tightened on that handle (`fchmod`), so nothing is ever changed through a link.
#[cfg(unix)]
fn open_entry(path: &Path, name: &str, dir: bool, create: bool) -> Result<Option<std::fs::File>> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    let mut flags = libc::O_NOFOLLOW | libc::O_NONBLOCK;
    if dir {
        flags |= libc::O_DIRECTORY;
    } else if create {
        o.write(true).create(true).mode(0o600);
    }
    match o.custom_flags(flags).open(path) {
        Ok(f) => Ok(Some(f)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => Ok(None),
        // O_NOFOLLOW answers ELOOP for a link, or ENOTDIR with O_DIRECTORY: name the link as such
        Err(_) if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) => {
            anyhow::bail!("用量儲存區的 {name} 是 symlink，拒絕使用")
        }
        Err(e) if e.raw_os_error() == Some(libc::ENOTDIR) => {
            anyhow::bail!("用量儲存區的 {name} 不是目錄，拒絕使用")
        }
        Err(e) => anyhow::bail!("無法開啟用量儲存區的 {name}（{:?}）", e.kind()),
    }
}

#[cfg(unix)]
fn check_and_tighten(
    f: &std::fs::File,
    name: &str,
    dir: bool,
    mode: u32,
    notes: &mut Vec<String>,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = f
        .metadata()
        .map_err(|e| anyhow::anyhow!("無法讀取 {name} 的屬性（{:?}）", e.kind()))?;
    let kind_ok = if dir {
        meta.is_dir()
    } else {
        meta.file_type().is_file()
    };
    if !kind_ok {
        let want = if dir { "目錄" } else { "一般檔案" };
        anyhow::bail!("用量儲存區的 {name} 不是{want}，拒絕使用");
    }
    let have = meta.permissions().mode() & 0o777;
    if have & !mode != 0 {
        f.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|e| anyhow::anyhow!("無法收緊 {name} 的權限（{:?}）", e.kind()))?;
        notes.push(format!("{name}: {have:o} → {mode:o}"));
    }
    Ok(())
}

/// The sidecars SQLite makes beside the db: refused when not regular files, tightened when wider.
#[cfg(unix)]
fn sidecars(dir: &Path, notes: &mut Vec<String>) -> Result<()> {
    for name in ["usage.db-wal", "usage.db-shm"] {
        if let Some(f) = open_entry(&dir.join(name), name, false, false)? {
            check_and_tighten(&f, name, false, 0o600, notes)?;
        }
    }
    Ok(())
}

/// Held while a collect runs (`<usage dir>/collect.lock`, flock'd); released on drop. Its own
/// file, so taking or dropping it touches no lock SQLite holds.
pub struct CollectLock(#[allow(dead_code)] Option<std::fs::File>);

/// Take the collect lock. `wait` false: `None` when another collect holds it.
#[cfg(unix)]
pub fn collect_lock(dir: &Path, wait: bool) -> Result<Option<CollectLock>> {
    use std::os::unix::io::AsRawFd;
    let f = open_entry(&dir.join("collect.lock"), "collect.lock", false, true)?
        .context("無法建立 collect.lock")?;
    check_and_tighten(&f, "collect.lock", false, 0o600, &mut Vec::new())?;
    let op = if wait {
        libc::LOCK_EX
    } else {
        libc::LOCK_EX | libc::LOCK_NB
    };
    // SAFETY: a valid open fd; the lock is released when the fd closes.
    if unsafe { libc::flock(f.as_raw_fd(), op) } != 0 {
        if wait {
            anyhow::bail!("無法取得 collect.lock");
        }
        return Ok(None);
    }
    Ok(Some(CollectLock(Some(f))))
}

#[cfg(not(unix))]
pub fn collect_lock(_dir: &Path, _wait: bool) -> Result<Option<CollectLock>> {
    Ok(Some(CollectLock(None)))
}

/// Prepare modes in order, then open. Returns the connection and the mode notes. Error texts
/// name the entry (`usage`, `usage.db`, …), never the local path.
pub fn open(dir: &Path) -> Result<(Connection, Vec<String>)> {
    let mut notes = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| anyhow::anyhow!("無法建立用量目錄（{:?}）", e.kind()))?;
        let d = open_entry(dir, "usage", true, false)?.context("用量目錄不見了")?;
        check_and_tighten(&d, "usage", true, 0o700, &mut notes)?;
        let db = open_entry(&dir.join("usage.db"), "usage.db", false, true)?
            .context("無法建立 usage.db")?;
        check_and_tighten(&db, "usage.db", false, 0o600, &mut notes)?;
        sidecars(dir, &mut notes)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)
        .map_err(|e| anyhow::anyhow!("無法建立用量目錄（{:?}）", e.kind()))?;
    // SQLite opens the path itself: NOFOLLOW so it never follows a link swapped in after the check
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let conn = Connection::open_with_flags(dir.join("usage.db"), flags)
        .map_err(|e| anyhow::anyhow!("無法開啟 usage.db（SQLite {:?}）", e.sqlite_error_code()))?;

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
             root_hash TEXT PRIMARY KEY, alias TEXT NOT NULL, seen_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS codex_limits (
             account TEXT PRIMARY KEY, observed_ms INTEGER NOT NULL,
             five_pct REAL, five_resets INTEGER, week_pct REAL, week_resets INTEGER);",
    )?;
    // No fd on usage.db, -wal or -shm is opened (or closed) from here on: closing any fd of a file
    // drops every POSIX lock this process holds on it, including the connection's DMS lock on
    // -shm. New -wal/-shm need no check: SQLite's unix VFS creates them with the db file's mode.
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

    #[test]
    fn a_fifo_or_link_in_the_store_is_refused_without_blocking_or_chmod_through_it() {
        let base = tempfile::tempdir().unwrap();
        // a FIFO where the db goes: refused at once (O_NONBLOCK), not opened by SQLite
        let dir = usage_dir(base.path());
        std::fs::create_dir_all(&dir).unwrap();
        let fifo =
            std::ffi::CString::new(dir.join("usage.db").as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let t = std::time::Instant::now();
        let err = open(&dir).unwrap_err().to_string();
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        assert!(
            err.contains("usage.db") && err.contains("不是一般檔案"),
            "{err}"
        );
        // a FIFO sidecar is opened read-only, which blocks without O_NONBLOCK
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        drop(open(&dir).unwrap());
        let _ = std::fs::remove_file(dir.join("usage.db-wal"));
        let wal = std::ffi::CString::new(dir.join("usage.db-wal").as_os_str().as_encoded_bytes())
            .unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(wal.as_ptr(), 0o600) }, 0);
        let t = std::time::Instant::now();
        let err = open(&dir).unwrap_err().to_string();
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        assert!(
            err.contains("usage.db-wal") && err.contains("不是一般檔案"),
            "{err}"
        );
        // a symlinked sidecar: refused, and the file it points at keeps its mode
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        drop(open(&dir).unwrap());
        let target = base.path().join("elsewhere");
        std::fs::write(&target, b"").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let _ = std::fs::remove_file(dir.join("usage.db-wal"));
        std::os::unix::fs::symlink(&target, dir.join("usage.db-wal")).unwrap();
        let err = open(&dir).unwrap_err().to_string();
        assert!(
            err.contains("usage.db-wal") && err.contains("symlink"),
            "{err}"
        );
        assert_eq!(mode(&target), 0o644, "chmod went through the link");
        // a symlinked usage dir: refused, its target untouched
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&real, usage_dir(base.path())).unwrap();
        let err = open(&usage_dir(base.path())).unwrap_err().to_string();
        assert!(err.contains("symlink"), "{err}");
        assert_eq!(mode(&real), 0o755);
        // no error, cause chain included, names the local path
        let full = format!("{:?}", open(&usage_dir(base.path())).unwrap_err());
        assert!(!full.contains(base.path().to_str().unwrap()), "{full}");
        let home = dirs::home_dir().unwrap();
        assert!(!full.contains(home.to_str().unwrap()), "{full}");
    }

    /// The connection's shared lock on the -shm DMS byte (offset 128), seen from another
    /// process: true when a write lock there would conflict.
    fn dms_locked_elsewhere(shm: &Path) -> bool {
        let path = std::ffi::CString::new(shm.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: the child only calls open/fcntl/_exit (async-signal-safe) on memory prepared
        // before the fork; the parent waits for it.
        unsafe {
            let pid = libc::fork();
            assert!(pid >= 0);
            if pid == 0 {
                let fd = libc::open(path.as_ptr(), libc::O_RDWR);
                if fd < 0 {
                    libc::_exit(3);
                }
                let mut fl: libc::flock = std::mem::zeroed();
                fl.l_type = libc::F_WRLCK as _;
                fl.l_whence = libc::SEEK_SET as _;
                fl.l_start = 128;
                fl.l_len = 1;
                if libc::fcntl(fd, libc::F_GETLK, &mut fl) != 0 {
                    libc::_exit(4);
                }
                libc::_exit(if fl.l_type as libc::c_int == libc::F_UNLCK {
                    0
                } else {
                    1
                });
            }
            let mut status = 0;
            libc::waitpid(pid, &mut status, 0);
            let code = libc::WEXITSTATUS(status);
            assert!(code <= 1, "child failed: {code}");
            code == 1
        }
    }

    #[test]
    fn the_open_connection_keeps_its_shm_lock() {
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        // twice: the second open finds existing -wal/-shm, the case a post-open check broke
        for _ in 0..2 {
            let (conn, _) = open(&dir).unwrap();
            conn.execute("INSERT OR REPLACE INTO roots VALUES ('x', 'acct-a', 1)", [])
                .unwrap();
            assert!(
                dms_locked_elsewhere(&dir.join("usage.db-shm")),
                "DMS lock lost"
            );
            drop(conn);
        }
    }

    #[test]
    fn a_running_collect_makes_a_quiet_one_skip_and_a_waiting_one_wait() {
        let base = tempfile::tempdir().unwrap();
        let dir = usage_dir(base.path());
        drop(open(&dir).unwrap());
        let held = collect_lock(&dir, false).unwrap().expect("free");
        assert!(collect_lock(&dir, false).unwrap().is_none(), "busy → skip");
        let d = dir.clone();
        let waiter = std::thread::spawn(move || collect_lock(&d, true).unwrap().is_some());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!waiter.is_finished(), "a waiting collect waits");
        drop(held);
        assert!(waiter.join().unwrap());
        assert!(collect_lock(&dir, false).unwrap().is_some(), "free again");
    }
}
