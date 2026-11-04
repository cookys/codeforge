//! G40 quota mirror reader: resolves `<live_dir>`, lists and reads `<live_dir>/quota/<provider>/<alias>.json`
//! with the same skip/count rules as `pickViews`/`pickProviders` in `plugins/forge-stats/hooks/quota-view.ts`.
//! Read-only. Unlike the mod, files are opened `O_NOFOLLOW` and checked with fstat on the OPENED file, and at most
//! cap+1 bytes are read, so a swap between check and read cannot slip a link or a huge file through.
//!
//! Spec: `doc/plans/2026-11-03-g40-usage-accounts.md` section 7 and 10.

use super::quota_mirror::{
    merge_quota, parse_quota_view, MergedQuota, QuotaView, ALIAS_PATTERN, MAX_ROWS, NS_SCHEMA,
    PROVIDER_PATTERN, VIEW_MAX_BYTES,
};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_PROVIDERS: usize = 64;
pub const NS_MAX_BYTES: usize = 4096;

// ---- `<live_dir>` ----

#[derive(Debug, Clone, Default)]
pub struct QuotaEnv {
    pub autopilot_live_dir: Option<String>,
    pub xdg_runtime_dir: Option<String>,
    pub home: Option<String>,
}

impl QuotaEnv {
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        QuotaEnv {
            autopilot_live_dir: get("AUTOPILOT_LIVE_DIR"),
            xdg_runtime_dir: get("XDG_RUNTIME_DIR"),
            home: get("HOME"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandSource {
    Override,
    Xdg,
    XdgInferred,
    Shm,
    Fallback,
}

fn trim_slash(p: &str) -> String {
    if p.len() > 1 {
        let t = p.trim_end_matches('/');
        if t.is_empty() {
            "/".to_string()
        } else {
            t.to_string()
        }
    } else {
        p.to_string()
    }
}

/// The directories `<live_dir>` can be, in autopilot's order (identical to `quotaLiveCandidates` in quota-view.ts):
/// `$AUTOPILOT_LIVE_DIR` alone; else `$XDG_RUNTIME_DIR/autopilot` or (XDG unset) `/run/user/<uid>/autopilot`;
/// then `/dev/shm/autopilot-<uid>`; last `~/.autopilot`. A `/tmp` candidate is never listed (a mirror found only
/// there is not a mirror; RAM-backing cannot be judged). An unknown uid drops the uid-named candidates.
pub fn live_candidates(env: &QuotaEnv, uid: Option<u32>) -> Vec<(PathBuf, CandSource)> {
    if let Some(o) = &env.autopilot_live_dir {
        return vec![(PathBuf::from(trim_slash(o)), CandSource::Override)];
    }
    let mut out = Vec::new();
    if let Some(x) = &env.xdg_runtime_dir {
        out.push((
            PathBuf::from(format!("{}/autopilot", trim_slash(x))),
            CandSource::Xdg,
        ));
    } else if let Some(u) = uid {
        out.push((
            PathBuf::from(format!("/run/user/{u}/autopilot")),
            CandSource::XdgInferred,
        ));
    }
    if let Some(u) = uid {
        out.push((
            PathBuf::from(format!("/dev/shm/autopilot-{u}")),
            CandSource::Shm,
        ));
    }
    if let Some(h) = &env.home {
        out.push((
            PathBuf::from(format!("{}/.autopilot", trim_slash(h))),
            CandSource::Fallback,
        ));
    }
    out
}

/// The first candidate that exists (the override and the fallback need no check).
pub fn live_dir(
    env: &QuotaEnv,
    uid: Option<u32>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    live_candidates(env, uid)
        .into_iter()
        .find(|(d, s)| matches!(s, CandSource::Override | CandSource::Fallback) || exists(d))
        .map(|(d, _)| d)
}

#[cfg(unix)]
fn current_uid() -> Option<u32> {
    // SAFETY: getuid() has no preconditions and never fails.
    Some(unsafe { libc::getuid() })
}
#[cfg(not(unix))]
fn current_uid() -> Option<u32> {
    None
}

fn is_real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_dir())
}

/// `<live_dir>` for this process (env + uid), or None.
pub fn resolve_live_dir() -> Option<PathBuf> {
    live_dir(&QuotaEnv::from_process(), current_uid(), is_real_dir)
}

// ---- reading ----

enum Read1 {
    Missing,
    Rejected,
    Text(String),
}

/// An opened directory: a descriptor taken with `O_DIRECTORY | O_NOFOLLOW`, plus the path it was opened at.
/// Every child is opened RELATIVE to the descriptor (`openat`), so swapping the directory (or a parent) for a
/// symlink after the check cannot redirect a read; the path is used only to list names, and the listing is
/// accepted only when the path still names the same inode as the descriptor, before and after.
struct Dir {
    file: File,
    path: PathBuf,
}

#[cfg(unix)]
fn open_flags(dir: bool) -> i32 {
    let base = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    if dir {
        base | libc::O_DIRECTORY
    } else {
        base | libc::O_NONBLOCK
    }
}

#[cfg(unix)]
fn open_dir(path: &Path) -> std::io::Result<Dir> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(open_flags(true) & !libc::O_RDONLY)
        .open(path)?;
    Ok(Dir {
        file,
        path: path.to_path_buf(),
    })
}

/// Open `name` (one path component, already checked by `slug_ok`/the caller) relative to `dir`.
#[cfg(unix)]
fn open_child(dir: &Dir, name: &str, want_dir: bool) -> std::io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Err(std::io::Error::other("not a single path component"));
    }
    let c = std::ffi::CString::new(name).map_err(std::io::Error::other)?;
    // SAFETY: a valid directory fd we own for the call and a NUL-terminated name; the returned fd is
    // checked for -1 and then owned exactly once by the File.
    let fd = unsafe { libc::openat(dir.file.as_raw_fd(), c.as_ptr(), open_flags(want_dir)) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: fd is a fresh, valid descriptor that nothing else owns.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn same_inode(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn open_dir(path: &Path) -> std::io::Result<Dir> {
    if !std::fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    Ok(Dir {
        file: File::open(path)?,
        path: path.to_path_buf(),
    })
}

#[cfg(not(unix))]
fn open_child(dir: &Dir, name: &str, want_dir: bool) -> std::io::Result<File> {
    let p = dir.path.join(name);
    let m = std::fs::symlink_metadata(&p)?;
    if m.file_type().is_symlink() || m.file_type().is_dir() != want_dir {
        return Err(std::io::Error::other("not the expected kind"));
    }
    File::open(p)
}

#[cfg(not(unix))]
fn same_inode(_a: &std::fs::Metadata, _b: &std::fs::Metadata) -> bool {
    true
}

fn open_child_dir(dir: &Dir, name: &str) -> std::io::Result<Dir> {
    let file = open_child(dir, name, true)?;
    if !file.metadata()?.is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    Ok(Dir {
        file,
        path: dir.path.join(name),
    })
}

/// Read a child file of `dir`: opened relative to the descriptor without following a link or blocking on a
/// FIFO, fstat-checked on the opened file (regular, <= cap), and at most cap+1 bytes read.
fn read_capped(dir: &Dir, name: &str, cap: usize) -> Read1 {
    let f = match open_child(dir, name, false) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read1::Missing,
        Err(_) => return Read1::Rejected, // a link (ELOOP), no permission, ...
    };
    let Ok(meta) = f.metadata() else {
        return Read1::Rejected;
    };
    if !meta.is_file() || meta.len() > cap as u64 {
        return Read1::Rejected;
    }
    let mut buf = Vec::new();
    if f.take(cap as u64 + 1).read_to_end(&mut buf).is_err() || buf.len() > cap {
        return Read1::Rejected;
    }
    match String::from_utf8(buf) {
        Ok(s) => Read1::Text(s),
        Err(_) => Read1::Rejected,
    }
}

fn slug_ok(s: &str) -> bool {
    // PROVIDER_PATTERN == ALIAS_PATTERN == ^[a-z0-9-]{1,24}$
    debug_assert_eq!(PROVIDER_PATTERN, ALIAS_PATTERN);
    !s.is_empty()
        && s.chars().count() <= 24
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaRaw {
    pub provider: String,
    pub alias: String,
    pub view: QuotaView,
}

/// What one read of the mirror kept. `present`: `<live_dir>/quota` exists as a real directory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaState {
    pub present: bool,
    pub ns: Option<String>,
    pub raw: Vec<QuotaRaw>,
    pub bad: usize,
    pub skipped: usize,
}

fn parse_ns(text: &str) -> Option<String> {
    if text.len() > NS_MAX_BYTES {
        return None;
    }
    let v: Value = serde_json::from_str(text).ok()?;
    let o = v.as_object()?;
    if o.get("schema")?.as_str()? != NS_SCHEMA {
        return None;
    }
    let ns = o.get("namespace")?.as_str()?;
    (!ns.is_empty()).then(|| ns.to_string())
}

/// At most this many directory entries are looked at per directory: the statusline runs this in every
/// session, so a directory flooded with entries costs a bounded amount of work and memory.
pub const DIR_SCAN_BUDGET: usize = 1024;

/// One listing of `dir`: names with the entry's own type (from the directory entry, no stat per entry),
/// at most `DIR_SCAN_BUDGET` entries, sorted. `truncated` when entries were left unread. `None` when the
/// directory cannot be listed or its path no longer names the opened descriptor (a swap: rejected).
struct Listing {
    names: Vec<(String, std::fs::FileType)>,
    truncated: bool,
}

fn list(dir: &Dir) -> Option<Listing> {
    let held = dir.file.metadata().ok()?;
    let before = std::fs::symlink_metadata(&dir.path).ok()?;
    if !same_inode(&held, &before) {
        return None;
    }
    let mut rd = std::fs::read_dir(&dir.path).ok()?;
    let mut names = Vec::new();
    let mut truncated = false;
    for (seen, e) in rd.by_ref().enumerate() {
        if seen == DIR_SCAN_BUDGET {
            truncated = true;
            break;
        }
        let Ok(e) = e else { continue };
        let (Ok(name), Ok(ft)) = (e.file_name().into_string(), e.file_type()) else {
            continue;
        };
        names.push((name, ft));
    }
    let after = std::fs::symlink_metadata(&dir.path).ok()?;
    if !same_inode(&held, &after) {
        return None;
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));
    Some(Listing { names, truncated })
}

/// Read the whole mirror under `live_dir`. Never panics, never errors: a missing or unreadable directory is
/// `present: false`. Bounded: at most `DIR_SCAN_BUDGET` entries per directory, `MAX_PROVIDERS` provider
/// directories and `MAX_ROWS` views are touched; the rest is only counted (`skipped`).
pub fn read_mirror(live_dir: &Path) -> QuotaState {
    let Ok(top_dir) = open_dir(&live_dir.join("quota")) else {
        return QuotaState::default();
    };
    let mut st = QuotaState {
        present: true,
        ..Default::default()
    };
    let Some(top) = list(&top_dir) else {
        st.bad += 1;
        return st;
    };
    if top.truncated {
        st.skipped += 1;
    }
    if top.names.iter().any(|(n, _)| n == "namespace.json") {
        match read_capped(&top_dir, "namespace.json", NS_MAX_BYTES) {
            Read1::Text(t) => st.ns = parse_ns(&t),
            Read1::Rejected => st.bad += 1,
            Read1::Missing => {}
        }
    }
    let providers: Vec<&str> = top
        .names
        .iter()
        .filter(|(n, ft)| (ft.is_dir() || ft.is_symlink()) && slug_ok(n))
        .map(|(n, _)| n.as_str())
        .collect();
    st.skipped += providers.len().saturating_sub(MAX_PROVIDERS);
    let mut reads = 0usize;
    for provider in providers.into_iter().take(MAX_PROVIDERS) {
        // opened relative to the quota descriptor with O_NOFOLLOW: a link here is a reject
        let pdir = match open_child_dir(&top_dir, provider) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                st.bad += 1;
                continue;
            }
        };
        let Some(listed) = list(&pdir) else {
            st.bad += 1;
            continue;
        };
        if listed.truncated {
            st.skipped += 1;
        }
        for (name, ft) in &listed.names {
            if !name.ends_with(".json") || name.starts_with('.') {
                continue; // lock files, temp files, dot files: silently skipped
            }
            let alias = &name[..name.len() - ".json".len()];
            if !slug_ok(alias) || !ft.is_file() {
                st.bad += 1;
                continue;
            }
            if reads >= MAX_ROWS {
                st.skipped += 1;
                continue;
            }
            reads += 1;
            match read_capped(&pdir, name, VIEW_MAX_BYTES) {
                Read1::Missing => {}
                Read1::Rejected => st.bad += 1,
                Read1::Text(t) => match parse_quota_view(&t, provider, alias) {
                    Ok(view) => {
                        st.bad += view.rejected;
                        st.raw.push(QuotaRaw {
                            provider: provider.to_string(),
                            alias: alias.to_string(),
                            view,
                        });
                    }
                    Err(_) => st.bad += 1,
                },
            }
        }
    }
    st
}

#[derive(Debug, Clone, PartialEq)]
pub struct QEntry {
    pub provider: String,
    pub alias: String,
    pub key: String,
    pub m: MergedQuota,
}

/// Merge every view (section 5.2) at `now_s`. A view with no row left, or only peer rows we may not use, is no entry.
pub fn quota_entries(state: &QuotaState, now_s: i64) -> Vec<QEntry> {
    let mut out: Vec<QEntry> = Vec::new();
    for r in &state.raw {
        if r.view.rows.is_empty() {
            continue;
        }
        let m = merge_quota(&r.view, state.ns.as_deref(), now_s);
        if !m.had_observation && !m.placeholder {
            continue;
        }
        out.push(QEntry {
            provider: r.provider.clone(),
            alias: r.alias.clone(),
            key: format!("{}:{}", r.provider, r.alias),
            m,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// `Some(entries)` when `<live_dir>/quota` exists as a real directory, else `None` (today's statusline output, unchanged).
pub fn load_from(live_dir: &Path, now_s: i64) -> Option<Vec<QEntry>> {
    let st = read_mirror(live_dir);
    st.present.then(|| quota_entries(&st, now_s))
}

/// The statusline's one call. Never panics: a panic anywhere in the read is `None`.
pub fn load_for_statusline(now_s: i64) -> Option<Vec<QEntry>> {
    std::panic::catch_unwind(|| load_from(&resolve_live_dir()?, now_s))
        .ok()
        .flatten()
}

/// Test seam: `Err(true)` rejected, `Err(false)` missing.
#[cfg(test)]
pub(crate) fn read_capped_for_test(path: &Path, cap: usize) -> Result<String, bool> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return Err(true);
    };
    let Ok(dir) = open_dir(parent) else {
        return Err(true);
    };
    match read_capped(&dir, name, cap) {
        Read1::Text(t) => Ok(t),
        Read1::Rejected => Err(true),
        Read1::Missing => Err(false),
    }
}

#[cfg(all(test, unix))]
mod fd_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// review g1 f4cb14df: a directory swapped for a symlink AFTER it was opened cannot redirect a read.
    /// The path-based read (what the code did before) follows the swap; the descriptor-relative read does not,
    /// and the listing through the swapped path is refused.
    #[test]
    fn a_directory_swapped_for_a_link_after_open_cannot_redirect_reads() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("claude");
        let evil = t.path().join("evil");
        std::fs::create_dir(&real).unwrap();
        std::fs::create_dir(&evil).unwrap();
        std::fs::write(real.join("work.json"), "real").unwrap();
        std::fs::write(evil.join("work.json"), "evil").unwrap();

        let dir = open_dir(&real).unwrap();
        // the swap: the checked directory moves away and a link to the attacker's directory takes its name
        std::fs::rename(&real, t.path().join("moved")).unwrap();
        symlink(&evil, &real).unwrap();

        // the old, path-based read follows the swap: this is the bug the test guards against
        assert_eq!(
            std::fs::read_to_string(real.join("work.json")).unwrap(),
            "evil"
        );
        // descriptor-relative: still the directory that was checked
        match read_capped(&dir, "work.json", 64) {
            Read1::Text(s) => assert_eq!(s, "real"),
            _ => panic!("expected the original file"),
        }
        // and listing through the swapped path is refused
        assert!(list(&dir).is_none());
    }

    #[test]
    fn opening_a_child_directory_that_is_a_link_is_refused() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("elsewhere")).unwrap();
        symlink(t.path().join("elsewhere"), t.path().join("claude")).unwrap();
        let top = open_dir(t.path()).unwrap();
        assert!(open_child_dir(&top, "claude").is_err());
        assert!(open_dir(&t.path().join("claude")).is_err());
    }

    /// review g1 518ba2c8: a flooded directory costs at most DIR_SCAN_BUDGET entries, and the rest is counted.
    #[test]
    fn a_flooded_directory_is_listed_within_the_budget_and_counted() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("quota").join("claude");
        std::fs::create_dir_all(&p).unwrap();
        for i in 0..(DIR_SCAN_BUDGET + 200) {
            std::fs::write(p.join(format!("junk-{i:05}.lock")), "").unwrap();
        }
        let dir = open_dir(&p).unwrap();
        let l = list(&dir).unwrap();
        assert_eq!(l.names.len(), DIR_SCAN_BUDGET);
        assert!(l.truncated);
        let st = read_mirror(t.path());
        assert!(st.present);
        assert!(st.skipped >= 1, "a truncated listing is counted as skipped");
        assert_eq!(st.bad, 0);
    }
}
