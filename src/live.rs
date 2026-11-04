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
    RESOLVED
        .get_or_init(|| resolve_live_base_with(&LiveEnv::from_process_env()))
        .clone()
}

/// Env-derived inputs and the `findmnt` program of the base resolution.
/// Parameters, not reads inside the resolver: env-var test seams race under
/// cargo test (`set_var` is process-global, and a replaced `PATH` breaks
/// every concurrently spawned command).
struct LiveEnv {
    /// `AUTOPILOT_LIVE_DIR`
    override_dir: Option<String>,
    /// `XDG_RUNTIME_DIR`
    xdg_runtime_dir: Option<String>,
    /// `CODEFORGE_PROC_MOUNTS` (None = the real `/proc/mounts`)
    proc_mounts: Option<PathBuf>,
    /// The program run as `findmnt` (production: the bare name, PATH-searched).
    findmnt: PathBuf,
}

impl LiveEnv {
    fn from_process_env() -> Self {
        LiveEnv {
            override_dir: std::env::var("AUTOPILOT_LIVE_DIR").ok(),
            xdg_runtime_dir: std::env::var("XDG_RUNTIME_DIR").ok(),
            proc_mounts: std::env::var("CODEFORGE_PROC_MOUNTS")
                .ok()
                .map(PathBuf::from),
            findmnt: PathBuf::from("findmnt"),
        }
    }
}

fn resolve_live_base_with(env: &LiveEnv) -> (PathBuf, LiveBaseSource) {
    let uid = current_uid();

    let mut candidates: Vec<(PathBuf, LiveBaseSource)> = Vec::new();
    if let Some(over) = &env.override_dir {
        if !over.is_empty() {
            candidates.push((PathBuf::from(over), LiveBaseSource::Override));
        }
    }
    if let Some(xdg) = &env.xdg_runtime_dir {
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
        if is_ram_backed(&dir, env) {
            return (dir, source);
        }
    }

    // On Windows the disk-backed fallback is the expected case, not worth a warning per render.
    if !cfg!(windows) {
        warn_once("autopilot: no RAM-backed candidate for live context dir found; falling back to ~/.autopilot (disk-backed)");
    }
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
fn is_ram_backed(dir: &Path, env: &LiveEnv) -> bool {
    // Windows has no tmpfs/ramfs (and no findmnt or /proc/mounts): never RAM-backed.
    if cfg!(windows) {
        return false;
    }
    let probe_path = nearest_existing_ancestor(dir);
    match probe_fstype_findmnt(&env.findmnt, &probe_path) {
        Some(fstype) => is_ram_fstype(&fstype),
        None => match probe_fstype_proc_mounts(env.proc_mounts.as_deref(), &probe_path) {
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
fn probe_fstype_findmnt(program: &Path, dir: &Path) -> Option<String> {
    let output = std::process::Command::new(program)
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
fn probe_fstype_proc_mounts(mounts_override: Option<&Path>, dir: &Path) -> Option<String> {
    let mounts_path = mounts_override.unwrap_or_else(|| Path::new("/proc/mounts"));
    let content = fs::read_to_string(mounts_path).ok()?;

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
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    // mode(0o700) applies to every directory this call creates, not only the leaf —
    // plain create_dir_all left new ancestors (e.g. `$XDG_RUNTIME_DIR/autopilot`) at 0775.
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
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
    #[cfg(unix)]
    use std::sync::Mutex;

    // Serializes the tests that touch the process-global `WARNED` flag.
    // They pass env values as `LiveEnv` parameters and never mutate env vars.
    #[cfg(unix)]
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
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

    /// (a) fake findmnt returning tmpfs for the XDG candidate ⇒ chosen.
    #[cfg(unix)]
    #[test]
    fn findmnt_tmpfs_selects_xdg_candidate() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin_dir = fake_findmnt_dir("#!/bin/sh\necho tmpfs\n");
        let xdg_dir = tempfile::tempdir().unwrap();

        let env = LiveEnv {
            override_dir: None,
            xdg_runtime_dir: Some(xdg_dir.path().to_string_lossy().into_owned()),
            proc_mounts: None,
            findmnt: bin_dir.path().join("findmnt"),
        };

        let (base, source) = resolve_live_base_with(&env);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_dir.path().join("autopilot"));
    }

    /// (b) fake findmnt returning ext4 for every candidate ⇒ base is
    /// ~/.autopilot and exactly one warning line on stderr.
    #[cfg(unix)]
    #[test]
    fn findmnt_ext4_everywhere_falls_back_and_warns_once() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin_dir = fake_findmnt_dir("#!/bin/sh\necho ext4\n");
        let xdg_dir = tempfile::tempdir().unwrap();

        let env = LiveEnv {
            override_dir: None,
            xdg_runtime_dir: Some(xdg_dir.path().to_string_lossy().into_owned()),
            proc_mounts: None,
            findmnt: bin_dir.path().join("findmnt"),
        };
        WARNED.store(false, Ordering::SeqCst);

        let (base, source) = resolve_live_base_with(&env);

        assert_eq!(source, LiveBaseSource::Fallback);
        assert_eq!(base, dirs::home_dir().unwrap().join(".autopilot"));
        // Warned exactly once for this resolution.
        assert!(WARNED.load(Ordering::SeqCst));
    }

    /// (c) findmnt absent + CODEFORGE_PROC_MOUNTS fixture ⇒ /proc/mounts
    /// path works.
    #[cfg(unix)]
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

        let env = LiveEnv {
            override_dir: None,
            xdg_runtime_dir: Some(xdg_dir.path().to_string_lossy().into_owned()),
            proc_mounts: Some(mounts.path().to_path_buf()),
            // An absolute path that does not exist: findmnt is missing.
            findmnt: empty_bin_dir.path().join("findmnt"),
        };

        let (base, source) = resolve_live_base_with(&env);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_path);
    }

    /// (d) ext4 override + tmpfs XDG ⇒ XDG chosen (override rejected, not
    /// fatal — falls through to the next candidate).
    #[cfg(unix)]
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
        let env = LiveEnv {
            override_dir: Some(override_dir.path().to_string_lossy().into_owned()),
            xdg_runtime_dir: Some(xdg_dir.path().to_string_lossy().into_owned()),
            proc_mounts: Some(mounts.path().to_path_buf()),
            findmnt: empty_bin_dir.path().join("findmnt"),
        };

        let (base, source) = resolve_live_base_with(&env);

        assert_eq!(source, LiveBaseSource::XdgRuntime);
        assert_eq!(base, xdg_path);
    }

    /// (g) newly created ancestors are 0700 too, not just the leaf — the
    /// `$XDG_RUNTIME_DIR/autopilot` parent used to come out 0775.
    #[cfg(unix)]
    #[test]
    fn write_live_json_creates_ancestors_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("autopilot");
        let base = parent.join("context");
        write_live_json(&base, "s.json", &serde_json::json!({"a": 1})).unwrap();
        let mode = fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
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

    /// The forge-stats mod tests the same sanitiser in TypeScript from a `.ts` mirror of the
    /// fixture (a mod test can import neither JSON nor files outside the mod's folder).
    #[test]
    fn forge_stats_vectors_mirror_the_fixture() {
        let root = env!("CARGO_MANIFEST_DIR");
        let fixture: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(format!("{root}/tests/fixtures/session-id-vectors.json")).unwrap(),
        )
        .unwrap();
        let mirror = fs::read_to_string(format!(
            "{root}/plugins/forge-stats/tests/session-id-vectors.ts"
        ))
        .unwrap();
        let start = mirror.find(" = [").expect("mirror has `= [`") + 3;
        let mirrored: serde_json::Value = serde_json::from_str(mirror[start..].trim()).unwrap();
        assert_eq!(mirrored, fixture, "regenerate the mirror from the fixture");
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
}
