//! Statusline-triggered background collect (PM G36 follow-up): `usage collect` runs on a host
//! without the codeforge SessionEnd hook.
//!
//! Every statusline render calls [`maybe_trigger`]: it takes a non-blocking `flock` on
//! `<live_base>/codeforge/usage/collect.stamp` (0600, opened `O_NOFOLLOW | O_NONBLOCK`), reads the
//! last trigger time written in it, and when that is at least [`EVERY_MS`] ago (or in the future:
//! a clock that went back never locks it out) writes the new time and spawns a detached
//! `codeforge usage collect --quiet`. A render costs one open, one flock and a read; the collect
//! itself takes its own lock and skips when another is still running.
//! `CODEFORGE_USAGE_AUTOCOLLECT=0` turns it off.

use std::path::Path;

/// At most one background collect per this many ms per live base.
pub const EVERY_MS: i64 = 15 * 60_000;
/// `0` turns the statusline trigger off.
// used by the statusline outside test builds only
#[cfg_attr(test, allow(dead_code))]
pub const ENV_OFF: &str = "CODEFORGE_USAGE_AUTOCOLLECT";
pub const STAMP: &str = "collect.stamp";

/// Decide and, when due, call `spawn`. Returns whether `spawn` was called and succeeded.
/// Every failure (no dir, a link, a busy lock) is a quiet no: the statusline must never break.
#[cfg(unix)]
pub fn maybe_trigger(
    dir: &Path,
    env_off: Option<&str>,
    now_ms: i64,
    spawn: impl FnOnce() -> std::io::Result<()>,
) -> bool {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::os::unix::io::AsRawFd;
    if env_off == Some("0") {
        return false;
    }
    if std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .is_err()
    {
        return false;
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(dir.join(STAMP))
    else {
        return false;
    };
    if !f.metadata().is_ok_and(|m| m.file_type().is_file()) {
        return false;
    }
    // SAFETY: a valid open fd; the lock goes with the fd when `f` drops.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return false;
    }
    let mut text = String::new();
    let _ = (&f).take(32).read_to_string(&mut text);
    let last: i64 = text.trim().parse().unwrap_or(0);
    if last <= now_ms && now_ms - last < EVERY_MS {
        return false;
    }
    let wrote = f.set_len(0).is_ok()
        && f.seek(SeekFrom::Start(0)).is_ok()
        && f.write_all(now_ms.to_string().as_bytes()).is_ok();
    wrote && spawn().is_ok()
}

#[cfg(not(unix))]
pub fn maybe_trigger(
    _dir: &Path,
    _env_off: Option<&str>,
    _now_ms: i64,
    _spawn: impl FnOnce() -> std::io::Result<()>,
) -> bool {
    false
}

/// Spawn `<this binary> usage collect --quiet` detached: its own process group, stdio to null,
/// not waited for (the statusline exits right after; init reaps the child).
#[cfg(unix)]
#[cfg_attr(test, allow(dead_code))]
pub fn spawn_collect() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    Command::new(std::env::current_exe()?)
        .args(["usage", "collect", "--quiet"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
}

#[cfg(not(unix))]
#[cfg_attr(test, allow(dead_code))]
pub fn spawn_collect() -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::io::AsRawFd;

    const T: i64 = 1_793_707_200_000;

    fn trig(dir: &Path, env: Option<&str>, now: i64) -> bool {
        let mut called = false;
        let r = maybe_trigger(dir, env, now, || {
            called = true;
            Ok(())
        });
        assert_eq!(r, called);
        r
    }

    #[test]
    fn triggers_once_per_interval_and_never_when_off() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("usage");
        assert!(trig(&dir, None, T), "first render triggers");
        assert!(!trig(&dir, None, T + 1), "then not");
        assert!(!trig(&dir, None, T + EVERY_MS - 1));
        assert!(trig(&dir, None, T + EVERY_MS), "after the interval again");
        // a clock that went back does not lock it out
        assert!(trig(&dir, None, T), "stamp in the future → due");
        // the kill switch
        let off = base.path().join("off");
        assert!(!trig(&off, Some("0"), T));
        assert!(trig(&off, Some("1"), T));
        let mode = std::fs::metadata(dir.join(STAMP))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_held_lock_or_a_linked_stamp_never_triggers() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("usage");
        std::fs::create_dir_all(&dir).unwrap();
        // another renderer holds the lock
        let held = std::fs::File::create(dir.join(STAMP)).unwrap();
        // SAFETY: a valid fd.
        assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
        assert!(!trig(&dir, None, T));
        drop(held);
        assert!(trig(&dir, None, T), "free again");
        // a symlinked stamp: not followed, the target untouched
        let dir2 = base.path().join("usage2");
        std::fs::create_dir_all(&dir2).unwrap();
        let target = base.path().join("target");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, dir2.join(STAMP)).unwrap();
        assert!(!trig(&dir2, None, T));
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        // a failed spawn reports false
        let dir3 = base.path().join("usage3");
        assert!(!maybe_trigger(&dir3, None, T, || Err(
            std::io::Error::other("no")
        )));
    }
}
