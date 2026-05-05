//! Daemon lifecycle: pidfile, signal handling, graceful shutdown.
//!
//! Design:
//! - Single-instance enforced via atomic `create_new` on pidfile + liveness
//!   probe (kill -0). Stale pidfile from unclean shutdown is auto-cleaned.
//! - SIGTERM / SIGINT trigger graceful shutdown via tokio::sync::Notify.
//! - Pidfile path: $CODEFORGE_DIR/../daemon.pid (next to the DB).

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::Notify;

use crate::db::Context as DbContext;

/// Where the pidfile lives. Next to the DB — same ownership, same lifecycle.
pub fn pidfile_path(db_ctx: &DbContext) -> PathBuf {
    db_ctx
        .db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("daemon.pid")
}

/// Attempt to acquire the pidfile exclusively. Cleans up a stale file
/// (pid no longer alive). Returns Err if another daemon is already running.
pub fn acquire_pidfile(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }

    // Try atomic create
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => write_pid(path),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Pidfile exists — check liveness
            if is_stale_pidfile(path)? {
                fs::remove_file(path).ok();
                // Retry once
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?;
                return write_pid(path);
            }
            let pid = read_pidfile(path)?;
            Err(anyhow!(
                "另一個 daemon 已在執行（pid {pid}）。使用 `codeforge daemon stop` 停止。"
            ))
        }
        Err(e) => Err(e.into()),
    }
}

fn write_pid(path: &Path) -> Result<()> {
    let pid = std::process::id();
    fs::write(path, pid.to_string())?;
    Ok(())
}

pub fn read_pidfile(path: &Path) -> Result<u32> {
    let mut s = String::new();
    fs::File::open(path)
        .with_context(|| format!("pidfile 不存在：{}", path.display()))?
        .read_to_string(&mut s)?;
    s.trim()
        .parse()
        .with_context(|| format!("pidfile 內容不是有效 pid：{}", s.trim()))
}

pub fn release_pidfile(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Is the pidfile stale (process dead or non-existent)?
fn is_stale_pidfile(path: &Path) -> Result<bool> {
    let pid = read_pidfile(path)?;
    Ok(!pid_alive(pid))
}

pub fn pid_alive(pid: u32) -> bool {
    // kill -0: send signal 0, which only tests for existence
    match std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
    {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Send SIGTERM to the given pid; wait up to `timeout` for it to exit.
pub fn send_sigterm(pid: u32, timeout: Duration) -> Result<()> {
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    if !status.success() {
        return Err(anyhow!("kill -TERM {} 失敗（pid 可能已不存在）", pid));
    }

    // Poll for exit (~100ms intervals)
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(anyhow!(
        "daemon 收到 SIGTERM 但 {}s 內未退出",
        timeout.as_secs()
    ))
}

/// Build a shutdown Notify and register tokio signal handlers for SIGTERM + SIGINT.
/// Returns the shutdown handle; signals will call `notify_one` on it.
pub fn install_signal_handlers() -> Arc<Notify> {
    let shutdown = Arc::new(Notify::new());
    let term_trigger = shutdown.clone();
    let int_trigger = shutdown.clone();

    tokio::spawn(async move {
        use signal::unix::{signal as new_signal, SignalKind};
        let mut sigterm = match new_signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => return,
        };
        if sigterm.recv().await.is_some() {
            term_trigger.notify_one();
        }
    });

    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            int_trigger.notify_one();
        }
    });

    shutdown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_release_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");

        acquire_pidfile(&path).unwrap();
        assert!(path.exists());
        let pid = read_pidfile(&path).unwrap();
        assert_eq!(pid, std::process::id());

        release_pidfile(&path);
        assert!(!path.exists());
    }

    #[test]
    fn acquire_fails_when_pid_alive() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");

        acquire_pidfile(&path).unwrap();
        // Second acquire should fail — current process is alive
        let err = acquire_pidfile(&path).unwrap_err();
        assert!(err.to_string().contains("daemon 已在執行"));
        release_pidfile(&path);
    }

    #[test]
    fn acquire_cleans_stale_pidfile() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");

        // Write pidfile with a pid that won't exist (high number)
        fs::write(&path, "999999").unwrap();

        acquire_pidfile(&path).unwrap();
        // Now the file should point to our pid
        let pid = read_pidfile(&path).unwrap();
        assert_eq!(pid, std::process::id());

        release_pidfile(&path);
    }

    #[test]
    fn pid_alive_returns_true_for_self() {
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn pid_alive_returns_false_for_nonexistent() {
        // pid 0 isn't a real process to signal in a non-privileged user context
        // use a high number instead
        assert!(!pid_alive(999_999));
    }
}
