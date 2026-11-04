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

/// Open without following a final symlink or blocking on a FIFO.
fn open_nofollow(path: &Path) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(std::io::Error::other("symlink"));
        }
        File::open(path)
    }
}

fn read_capped(path: &Path, cap: usize) -> Read1 {
    let f = match open_nofollow(path) {
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

fn sorted_names(dir: &Path) -> Option<Vec<(String, std::fs::FileType, u64)>> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut v: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let ft = e.file_type().ok()?;
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            Some((name, ft, size))
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    Some(v)
}

/// Read the whole mirror under `live_dir`. Never panics, never errors: a missing or unreadable directory is `present: false`.
pub fn read_mirror(live_dir: &Path) -> QuotaState {
    let qdir = live_dir.join("quota");
    if !is_real_dir(&qdir) {
        return QuotaState::default();
    }
    let mut st = QuotaState {
        present: true,
        ..Default::default()
    };
    let Some(top) = sorted_names(&qdir) else {
        return st;
    };
    if let Some((_, ft, size)) = top.iter().find(|(n, _, _)| n == "namespace.json") {
        if ft.is_file() && *size <= NS_MAX_BYTES as u64 {
            match read_capped(&qdir.join("namespace.json"), NS_MAX_BYTES) {
                Read1::Text(t) => st.ns = parse_ns(&t),
                Read1::Rejected => st.bad += 1,
                Read1::Missing => {}
            }
        }
    }
    let providers: Vec<&str> = top
        .iter()
        .filter(|(n, ft, _)| ft.is_dir() && slug_ok(n))
        .map(|(n, _, _)| n.as_str())
        .collect();
    st.skipped = providers.len().saturating_sub(MAX_PROVIDERS);
    let mut reads = 0usize;
    for provider in providers.into_iter().take(MAX_PROVIDERS) {
        let pdir = qdir.join(provider);
        // looked at again right before listing: a swap to a link is a reject
        match std::fs::symlink_metadata(&pdir) {
            Err(_) => continue,
            Ok(m) if !m.file_type().is_dir() => {
                st.bad += 1;
                continue;
            }
            Ok(_) => {}
        }
        let Some(listed) = sorted_names(&pdir) else {
            continue;
        };
        for (name, ft, size) in &listed {
            if !name.ends_with(".json") || name.starts_with('.') {
                continue; // lock files, temp files, dot files: silently skipped
            }
            let alias = &name[..name.len() - ".json".len()];
            if !slug_ok(alias) || !ft.is_file() || *size > VIEW_MAX_BYTES as u64 {
                st.bad += 1;
                continue;
            }
            if reads >= MAX_ROWS {
                st.skipped += 1;
                continue;
            }
            reads += 1;
            match read_capped(&pdir.join(name), VIEW_MAX_BYTES) {
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
    match read_capped(path, cap) {
        Read1::Text(t) => Ok(t),
        Read1::Rejected => Err(true),
        Read1::Missing => Err(false),
    }
}
