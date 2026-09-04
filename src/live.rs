//! Live context files — writes small JSON snapshots to a RAM-backed base dir
//! so external readers (e.g. a companion status line) can poll session state
//! without touching the transcript.
//!
//! Base-dir resolution order (autopilot v2.36.1 plan §2.5, P1):
//!   `$AUTOPILOT_LIVE_DIR` → `$XDG_RUNTIME_DIR/autopilot` →
//!   `/dev/shm/autopilot-<uid>` → `/tmp/autopilot-<uid>`
//! Every candidate (override included) is accepted only if it resolves to a
//! `tmpfs`/`ramfs` mount. If every candidate is rejected, falls back to
//! `~/.autopilot` and prints exactly one warning line per process.
//!
//! Consumers append their own purpose segment (`context/`) to the returned
//! base; this module does not know about that segment.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Which candidate the resolver actually chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveBaseSource {
    /// `$AUTOPILOT_LIVE_DIR`, verified tmpfs/ramfs.
    Override,
    /// `$XDG_RUNTIME_DIR/autopilot`, verified tmpfs/ramfs.
    XdgRuntime,
    /// `/dev/shm/autopilot-<uid>`, verified tmpfs/ramfs.
    DevShm,
    /// `/tmp/autopilot-<uid>`, verified tmpfs/ramfs.
    Tmp,
    /// Every candidate was rejected — fell back to `~/.autopilot` (not
    /// verified as RAM-backed; may be on disk).
    Fallback,
}

/// Warn-once guard: at most one "no RAM-backed dir found" line per process.
static WARNED: AtomicBool = AtomicBool::new(false);

/// Cache the resolved base for the lifetime of the process (mount probing
/// shells out / reads /proc — no need to redo it per write).
static RESOLVED: OnceLock<(PathBuf, LiveBaseSource)> = OnceLock::new();

/// Resolve the live-context base directory per the fixed candidate order.
/// Idempotent per process (cached in a `OnceLock`).
pub fn resolve_live_base() -> (PathBuf, LiveBaseSource) {
    RESOLVED.get_or_init(resolve_live_base_uncached).clone()
}

fn resolve_live_base_uncached() -> (PathBuf, LiveBaseSource) {
    let uid = current_uid();

    let mut candidates: Vec<(PathBuf, LiveBaseSource)> = Vec::new();
    if let Ok(over) = std::env::var("AUTOPILOT_LIVE_DIR") {
        if !over.is_empty() {
            candidates.push((PathBuf::from(over), LiveBaseSource::Override));
        }
    }
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        if !xdg.is_empty() {
            candidates.push((
                PathBuf::from(xdg).join("autopilot"),
                LiveBaseSource::XdgRuntime,
            ));
        }
    }
    candidates.push((
        PathBuf::from(format!("/dev/shm/autopilot-{uid}")),
        LiveBaseSource::DevShm,
    ));
    candidates.push((
        PathBuf::from(format!("/tmp/autopilot-{uid}")),
        LiveBaseSource::Tmp,
    ));

    for (dir, source) in candidates {
        if is_ram_backed(&dir) {
            return (dir, source);
        }
    }

    warn_once("autopilot: no RAM-backed candidate for live context dir found; falling back to ~/.autopilot (disk-backed)");
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    (home.join(".autopilot"), LiveBaseSource::Fallback)
}

fn warn_once(msg: &str) {
    if WARNED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let _ = writeln!(io::stderr(), "{msg}");
    }
}

#[cfg(unix)]
extern "C" {
    fn getuid() -> u32;
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // Avoid pulling in the `libc` crate for one syscall — declare it inline.
    // SAFETY: getuid() has no preconditions and never fails.
    unsafe { getuid() }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

/// True if `dir` (or its nearest existing ancestor, if `dir` doesn't exist
/// yet) sits on a `tmpfs`/`ramfs` mount.
fn is_ram_backed(dir: &Path) -> bool {
    let probe_path = nearest_existing_ancestor(dir);
    match probe_fstype_findmnt(&probe_path) {
        Some(fstype) => is_ram_fstype(&fstype),
        None => match probe_fstype_proc_mounts(&probe_path) {
            Some(fstype) => is_ram_fstype(&fstype),
            None => false,
        },
    }
}

fn is_ram_fstype(fstype: &str) -> bool {
    let f = fstype.trim();
    f == "tmpfs" || f == "ramfs"
}

/// Walk up from `dir` until we find a path that exists. `/` always exists,
/// so this terminates.
fn nearest_existing_ancestor(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    loop {
        if cur.exists() {
            return cur;
        }
        match cur.parent() {
            Some(p) if !p.as_os_str().is_empty() => cur = p.to_path_buf(),
            _ => return PathBuf::from("/"),
        }
    }
}

/// Shell out to `findmnt -T <dir> -o FSTYPE -n`. Returns `None` if the
/// binary is absent or the invocation fails (caller falls back to
/// `/proc/mounts`).
fn probe_fstype_findmnt(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("findmnt")
        .arg("-T")
        .arg(dir)
        .arg("-o")
        .arg("FSTYPE")
        .arg("-n")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Fallback fstype probe: longest-prefix match over `/proc/mounts` (or the
/// path in `CODEFORGE_PROC_MOUNTS`, for tests).
fn probe_fstype_proc_mounts(dir: &Path) -> Option<String> {
    let mounts_path = std::env::var("CODEFORGE_PROC_MOUNTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/proc/mounts"));
    let content = fs::read_to_string(&mounts_path).ok()?;

    let target = dir.to_string_lossy();
    let mut best: Option<(usize, String)> = None;
    for line in content.lines() {
        // Format: <device> <mountpoint> <fstype> <options> <dump> <pass>
        let mut fields = line.split_whitespace();
        let _device = fields.next();
        let mountpoint = fields.next()?;
        let fstype = fields.next()?;

        if target == mountpoint
            || target.starts_with(&format!("{mountpoint}/"))
            || mountpoint == "/"
        {
            let len = mountpoint.len();
            if best.as_ref().is_none_or(|(best_len, _)| len > *best_len) {
                best = Some((len, fstype.to_string()));
            }
        }
    }
    best.map(|(_, fstype)| fstype)
}

/// Sanitize a raw `session_id` into a filesystem-safe token.
///
/// Rule (normative, autopilot v2.36.1 plan §2.5): replace every Unicode
/// scalar value not in `[A-Za-z0-9_-]` with a single `_` (per scalar), then
/// keep the first 64 scalars. Empty input (after replacement — the length
/// check applies to the original string) yields `"unknown"`.
pub fn sanitize_session_id(raw: &str) -> String {
    if raw.is_empty() {
        return "unknown".to_string();
    }
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

/// Write `value` as `<base>/<file_name>`, mode 0600, atomically (same-dir
/// temp file + rename). Creates `base` (and its ancestors) with mode 0700
/// if it doesn't exist yet.
pub fn write_live_json(base: &Path, file_name: &str, value: &serde_json::Value) -> io::Result<()> {
    create_dir_all_0700(base)?;

    let body = serde_json::to_vec(value)?;
    let final_path = base.join(file_name);

    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{file_name}."))
        .suffix(".tmp")
        .tempfile_in(base)?;
    tmp.write_all(&body)?;
    tmp.flush()?;
    set_mode_0600(tmp.path())?;

    tmp.persist(&final_path).map_err(|e| e.error)?;

    Ok(())
}

#[cfg(unix)]
fn create_dir_all_0700(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir)?;
    let perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(dir, perms)
}

#[cfg(not(unix))]
fn create_dir_all_0700(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn set_mode_0600(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_mode_0600(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serializes tests that mutate process-global env vars — `cargo test`
    // runs tests in threads within one process, and env vars are global.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn fake_findmnt_dir(script: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("findmnt");
        fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    /// Replaces PATH entirely with `dir` for the duration of `f` — a real
    /// system `findmnt` anywhere else on PATH would otherwise answer ahead
    /// of (or instead of) the fixture we're trying to simulate, since
    /// `Command::new("findmnt")` does a normal PATH search.
    fn with_only_path<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
        let orig = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", dir);
        let result = f();
        std::env::set_var("PATH", orig);
        result
    }

    /// (a) fake findmnt returning tmpfs for the XDG candidate ⇒ chosen.
    #[test]
    fn findmnt_tmpfs_selects_xdg_candidate() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin_dir = fake_findmnt_dir("#!/bin/sh\necho tmpfs\n");
        let xdg_dir = tempfile::tempdir().unwrap();

        std::env::remove_var("AUTOPILOT_LIVE_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", xdg_dir.path());

        let (base, source) = with_only_path(bin_dir.path(), resolve_live_base_uncached_for_test);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_dir.path().join("autopilot"));

        std::env::remove_var("XDG_RUNTIME_DIR");
    }

    /// (b) fake findmnt returning ext4 for every candidate ⇒ base is
    /// ~/.autopilot and exactly one warning line on stderr.
    #[test]
    fn findmnt_ext4_everywhere_falls_back_and_warns_once() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin_dir = fake_findmnt_dir("#!/bin/sh\necho ext4\n");
        let xdg_dir = tempfile::tempdir().unwrap();

        std::env::remove_var("AUTOPILOT_LIVE_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", xdg_dir.path());
        WARNED.store(false, Ordering::SeqCst);

        let (base, source) = with_only_path(bin_dir.path(), resolve_live_base_uncached_for_test);

        assert_eq!(source, LiveBaseSource::Fallback);
        assert_eq!(base, dirs::home_dir().unwrap().join(".autopilot"));
        // Warned exactly once for this resolution.
        assert!(WARNED.load(Ordering::SeqCst));

        std::env::remove_var("XDG_RUNTIME_DIR");
    }

    /// (c) findmnt absent + CODEFORGE_PROC_MOUNTS fixture ⇒ /proc/mounts
    /// path works.
    #[test]
    fn missing_findmnt_falls_back_to_proc_mounts() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let empty_bin_dir = tempfile::tempdir().unwrap();
        let xdg_dir = tempfile::tempdir().unwrap();
        let xdg_path = xdg_dir.path().join("autopilot");
        fs::create_dir_all(&xdg_path).unwrap();

        let mounts = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            mounts.path(),
            format!(
                "tmpfs {} tmpfs rw,relatime 0 0\n/dev/sda1 / ext4 rw 0 0\n",
                xdg_path.display()
            ),
        )
        .unwrap();

        std::env::remove_var("AUTOPILOT_LIVE_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", xdg_dir.path());
        std::env::set_var("CODEFORGE_PROC_MOUNTS", mounts.path());

        let (base, source) =
            with_only_path(empty_bin_dir.path(), resolve_live_base_uncached_for_test);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_path);

        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::remove_var("CODEFORGE_PROC_MOUNTS");
    }

    /// (d) ext4 override + tmpfs XDG ⇒ XDG chosen (override rejected, not
    /// fatal — falls through to the next candidate).
    #[test]
    fn ext4_override_falls_through_to_tmpfs_xdg() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let override_dir = tempfile::tempdir().unwrap();
        let xdg_dir = tempfile::tempdir().unwrap();
        let xdg_path = xdg_dir.path().join("autopilot");
        fs::create_dir_all(&xdg_path).unwrap();

        let mounts = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            mounts.path(),
            format!(
                "tmpfs {} tmpfs rw,relatime 0 0\n/dev/sda1 {} ext4 rw 0 0\n/dev/sda1 / ext4 rw 0 0\n",
                xdg_path.display(),
                override_dir.path().display()
            ),
        )
        .unwrap();

        let empty_bin_dir = tempfile::tempdir().unwrap();
        std::env::set_var("AUTOPILOT_LIVE_DIR", override_dir.path());
        std::env::set_var("XDG_RUNTIME_DIR", xdg_dir.path());
        std::env::set_var("CODEFORGE_PROC_MOUNTS", mounts.path());

        let (base, source) =
            with_only_path(empty_bin_dir.path(), resolve_live_base_uncached_for_test);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_path);

        std::env::remove_var("AUTOPILOT_LIVE_DIR");
        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::remove_var("CODEFORGE_PROC_MOUNTS");
    }

    /// (f) writer produces mode 0600 and atomic rename (no `.tmp` left).
    #[test]
    fn write_live_json_sets_mode_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("context");
        let value = serde_json::json!({"a": 1});
        write_live_json(&base, "s.json", &value).unwrap();

        let final_path = base.join("s.json");
        assert!(final_path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&final_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let dir_mode = fs::metadata(&base).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
        }

        let entries: Vec<_> = fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "expected exactly one file, got {entries:?}"
        );

        let read_back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&final_path).unwrap()).unwrap();
        assert_eq!(read_back, value);
    }

    /// (e) sanitiser vectors — loaded from the shared fixture file.
    #[test]
    fn sanitize_session_id_matches_shared_vectors() {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/session-id-vectors.json"
        );
        let content = fs::read_to_string(fixture_path)
            .unwrap_or_else(|e| panic!("failed to read {fixture_path}: {e}"));
        let vectors: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
        assert!(!vectors.is_empty(), "fixture file must not be empty");
        for v in vectors {
            let input = v["input"].as_str().unwrap();
            let expected = v["expected"].as_str().unwrap();
            assert_eq!(
                sanitize_session_id(input),
                expected,
                "mismatch for input {input:?}"
            );
        }
    }

    #[test]
    fn sanitize_session_id_basic_cases() {
        assert_eq!(sanitize_session_id(""), "unknown");
        assert_eq!(
            sanitize_session_id("93196c52-25cb-47ca-821c-cec391832eed"),
            "93196c52-25cb-47ca-821c-cec391832eed"
        );
        assert_eq!(sanitize_session_id("a/b:c d"), "a_b_c_d");
        let long_input: String = "a".repeat(70);
        let expected: String = "a".repeat(64);
        assert_eq!(sanitize_session_id(&long_input), expected);
    }

    // Test-only re-export so tests can call the uncached resolver directly
    // (the real `resolve_live_base()` caches in a `OnceLock` for the life
    // of the process, which would make every test after the first see a
    // stale answer).
    fn resolve_live_base_uncached_for_test() -> (PathBuf, LiveBaseSource) {
        resolve_live_base_uncached()
    }
}
