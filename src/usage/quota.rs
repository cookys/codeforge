//! Per-account quota snapshots (G36 plan §2–3).
//!
//! Every statusline run that sees `rate_limits` writes
//! `<live_base>/codeforge/usage/quota/<alias>.json` (0600, dir 0700, tmp + rename),
//! at most every [`WRITE_EVERY_MS`] unless a percentage changed. The usage row
//! reads the other accounts' snapshots back with a bounded read.
//!
//! Staleness: a window whose `resets_at` has passed is unknown; a snapshot older
//! than [`MAX_AGE_MS`] is dropped.
//!
//! Several sessions of one account write the same file, each with the `rate_limits` of
//! its own last API call, so an idle session holds an old value (measured 2026-11-06:
//! four values alternating every second, 5h 2–16 %, 7d 48–52 %, same reset times).
//! [`merge_snapshot`] therefore keeps a window's value only ever growing: within one
//! window (same `resets_at`) the larger percentage wins, a later window replaces an
//! earlier one, an earlier one is dropped. Each window records when and by which
//! session its value was seen (`seen_at_ms`, `seen_by`).

use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::alias::valid_alias;

pub const SCHEMA: &str = "codeforge.quota/1";
/// Rewrite an unchanged snapshot at most this often.
pub const WRITE_EVERY_MS: i64 = 30_000;
/// A snapshot not rewritten for this long is no longer shown.
pub const MAX_AGE_MS: i64 = 6 * 3600 * 1000;
/// Bounded read of the snapshot dir.
pub const MAX_FILES: usize = 16;
pub const MAX_BYTES: u64 = 4096;
/// Two `resets_at` closer than this are the same window (sessions may round differently).
pub const SAME_WINDOW_S: i64 = 600;

/// When and by which session a window's value was observed. The same value seen again
/// within WRITE_EVERY_MS is not rewritten, so `at_ms` can lag the latest sighting by that much.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub at_ms: i64,
    /// The writer's sanitized session id.
    pub session: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// 0.0–1.0
    pub pct: f64,
    /// unix seconds
    pub resets_at: i64,
    /// Provenance of `pct`; `None` from an older writer.
    pub seen: Option<Seen>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub alias: String,
    /// sha8 of the canonical config dir this account's sessions run in (`None` from a
    /// pre-phase-2 writer). Lets the collector map a transcript root to an alias.
    pub root: Option<String>,
    pub written_at_ms: i64,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

/// `<live_base>/codeforge/usage/quota`.
pub fn quota_dir(live_base: &Path) -> PathBuf {
    live_base
        .join(crate::pet::view::DIR)
        .join("usage")
        .join("quota")
}

/// Eight lowercase hex digits.
pub fn is_hash8(s: &str) -> bool {
    s.len() == 8
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn window_json(w: &Option<Window>) -> Value {
    match w {
        Some(Window {
            pct,
            resets_at,
            seen: Some(seen),
        }) => {
            json!({ "pct": pct, "resets_at": resets_at, "seen_at_ms": seen.at_ms, "seen_by": seen.session })
        }
        Some(w) => json!({ "pct": w.pct, "resets_at": w.resets_at }),
        None => Value::Null,
    }
}

/// A session id as `sanitize_session_id` leaves it: 1–64 of `[A-Za-z0-9_-]`.
fn valid_session(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn window_from(v: &Value) -> Option<Window> {
    let pct = v["pct"].as_f64()?;
    let resets_at = v["resets_at"].as_i64()?;
    let seen = match (v["seen_at_ms"].as_i64(), v["seen_by"].as_str()) {
        (Some(at_ms), Some(session)) if valid_session(session) => Some(Seen {
            at_ms,
            session: session.to_string(),
        }),
        _ => None,
    };
    (pct.is_finite() && (0.0..=1.5).contains(&pct)).then_some(Window {
        pct,
        resets_at,
        seen,
    })
}

/// One window after `new` was observed on top of `old`: a later window replaces an
/// earlier one, an earlier one is dropped, and within one window the larger percentage
/// wins (a tie takes `new`, so its `seen` is the latest confirmation). A missing side
/// keeps the other.
pub fn merge_window(old: Option<&Window>, new: Option<&Window>) -> Option<Window> {
    match (old, new) {
        (None, n) => n.cloned(),
        (o, None) => o.cloned(),
        (Some(o), Some(n)) => {
            if n.resets_at >= o.resets_at + SAME_WINDOW_S {
                Some(n.clone())
            } else if n.resets_at + SAME_WINDOW_S <= o.resets_at {
                Some(o.clone())
            } else if n.pct >= o.pct {
                Some(n.clone())
            } else {
                Some(o.clone())
            }
        }
    }
}

/// The same percentage and reset time (provenance aside).
fn same_value(a: &Option<Window>, b: &Option<Window>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.pct == b.pct && a.resets_at == b.resets_at,
        _ => false,
    }
}

impl Snapshot {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": SCHEMA,
            "alias": self.alias,
            "root": self.root,
            "written_at_ms": self.written_at_ms,
            "five_hour": window_json(&self.five_hour),
            "seven_day": window_json(&self.seven_day),
        })
    }

    /// The right schema and a valid alias, or nothing.
    pub fn from_json(v: &Value) -> Option<Self> {
        if v["schema"] != SCHEMA {
            return None;
        }
        let alias = v["alias"].as_str()?;
        if !valid_alias(alias) {
            return None;
        }
        let root = match &v["root"] {
            Value::Null => None,
            Value::String(r) if is_hash8(r) => Some(r.clone()),
            _ => return None,
        };
        Some(Snapshot {
            alias: alias.to_string(),
            root,
            written_at_ms: v["written_at_ms"].as_i64()?,
            five_hour: window_from(&v["five_hour"]),
            seven_day: window_from(&v["seven_day"]),
        })
    }

    /// The more constrained window that is still running at `now_s`
    /// (`resets_at` in the future): `("5h" | "7d", window)`.
    pub fn constrained(&self, now_s: i64) -> Option<(&'static str, &Window)> {
        fn live(w: &Option<Window>, now_s: i64) -> Option<&Window> {
            w.as_ref().filter(|w| w.resets_at > now_s)
        }
        match (live(&self.five_hour, now_s), live(&self.seven_day, now_s)) {
            (Some(a), Some(b)) => Some(if b.pct >= a.pct { ("7d", b) } else { ("5h", a) }),
            (Some(a), None) => Some(("5h", a)),
            (None, Some(b)) => Some(("7d", b)),
            (None, None) => None,
        }
    }
}

/// Open for reading without following a symlink at the last component and without
/// blocking on a FIFO: the checks then run on the opened handle, so a path swapped
/// between a check and the open cannot redirect or stall the read.
#[cfg(unix)]
pub(crate) fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
pub(crate) fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

/// Read one snapshot file: opened with O_NOFOLLOW | O_NONBLOCK, then the handle must
/// be a regular file of at most MAX_BYTES, and at most MAX_BYTES are read; JSON with
/// the right schema. Anything else → None.
fn read_bounded(path: &Path) -> Option<Snapshot> {
    let file = open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_BYTES {
        return None;
    }
    Snapshot::from_json(&serde_json::from_slice(&buf).ok()?)
}

/// True when the file exists with any group/other permission bit set.
#[cfg(unix)]
fn wider_than_0600(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::symlink_metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 != 0)
}

#[cfg(not(unix))]
fn wider_than_0600(_path: &Path) -> bool {
    false
}

/// Merge `snap` into the file on disk ([`merge_window`] per window) and write the
/// result, unless it holds the same values as the file, which was written less than
/// WRITE_EVERY_MS ago and is not wider than 0600 (a wider one is merged and rewritten at once: the
/// shared writer creates it 0600). Returns the merged snapshot (what this account's
/// row should show) and whether it wrote. An invalid alias writes nothing and returns
/// `snap` as it came.
///
/// Read and write are not one atomic step: two sessions writing within the same moment
/// can still let the lower value land, and the next write of the higher one restores it.
pub fn merge_snapshot(dir: &Path, snap: &Snapshot) -> std::io::Result<(Snapshot, bool)> {
    if !valid_alias(&snap.alias) {
        return Ok((snap.clone(), false));
    }
    let file = format!("{}.json", snap.alias);
    let path = dir.join(&file);
    let old = read_bounded(&path);
    let merged = match &old {
        None => snap.clone(),
        Some(old) => Snapshot {
            alias: snap.alias.clone(),
            root: snap.root.clone().or_else(|| old.root.clone()),
            written_at_ms: snap.written_at_ms,
            five_hour: merge_window(old.five_hour.as_ref(), snap.five_hour.as_ref()),
            seven_day: merge_window(old.seven_day.as_ref(), snap.seven_day.as_ref()),
        },
    };
    if let Some(old) = old.as_ref().filter(|_| !wider_than_0600(&path)) {
        let age = snap.written_at_ms - old.written_at_ms;
        if same_value(&old.five_hour, &merged.five_hour)
            && same_value(&old.seven_day, &merged.seven_day)
            && (0..WRITE_EVERY_MS).contains(&age)
        {
            return Ok((merged, false));
        }
    }
    crate::live::write_live_json(dir, &file, &merged.to_json())?;
    Ok((merged, true))
}

/// The other accounts' snapshots worth showing at `now_ms`: not `own_alias`,
/// written within MAX_AGE_MS, with a running window; most constrained first.
/// Reads at most MAX_FILES `*.json` entries (by name order); bad ones are skipped.
pub fn read_others(dir: &Path, own_alias: &str, now_ms: i64) -> Vec<Snapshot> {
    let now_s = now_ms.div_euclid(1000);
    let mut out: Vec<Snapshot> = read_all(dir)
        .into_iter()
        .filter(|s| s.alias != own_alias)
        .filter(|s| {
            now_ms
                .checked_sub(s.written_at_ms)
                .is_some_and(|age| (0..MAX_AGE_MS).contains(&age))
        })
        .filter(|s| s.constrained(now_s).is_some())
        .collect();
    out.sort_by(|a, b| {
        let pa = a.constrained(now_s).map_or(0.0, |(_, w)| w.pct);
        let pb = b.constrained(now_s).map_or(0.0, |(_, w)| w.pct);
        pb.total_cmp(&pa).then_with(|| a.alias.cmp(&b.alias))
    });
    out
}

/// Every valid snapshot in `dir`, any age: name-sorted, the first MAX_FILES `*.json`, read
/// bounded; a file whose stem is not the alias in its body is skipped (a stray or
/// hand-edited file cannot inject another identifier).
pub fn read_all(dir: &Path) -> Vec<Snapshot> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    names.sort();
    names
        .iter()
        .take(MAX_FILES)
        .filter_map(|p| {
            let s = read_bounded(p)?;
            let stem = p.file_stem().and_then(|n| n.to_str())?;
            (stem == s.alias).then_some(s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_793_600_000_000;
    const NOW_S: i64 = NOW_MS / 1000;

    fn snap(
        alias: &str,
        at_ms: i64,
        five: Option<(f64, i64)>,
        seven: Option<(f64, i64)>,
    ) -> Snapshot {
        let w = |x: Option<(f64, i64)>| {
            x.map(|(pct, resets_at)| Window {
                pct,
                resets_at,
                seen: None,
            })
        };
        Snapshot {
            alias: alias.into(),
            root: None,
            written_at_ms: at_ms,
            five_hour: w(five),
            seven_day: w(seven),
        }
    }

    #[test]
    fn a_file_named_otherwise_than_its_alias_is_skipped_and_root_round_trips() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        let mut s = snap("acct-b", NOW_MS, Some((0.4, NOW_S + 60)), None);
        s.root = Some("1a2b3c4d".into());
        crate::live::write_live_json(&dir, "acct-b.json", &s.to_json()).unwrap();
        // a stray file named after an email-shaped string, carrying a valid body
        crate::live::write_live_json(
            &dir,
            "me@example.com.json",
            &snap("acct-c", NOW_MS, Some((0.4, NOW_S + 60)), None).to_json(),
        )
        .unwrap();
        let all = read_all(&dir);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].root.as_deref(), Some("1a2b3c4d"));
        // a malformed root refuses the snapshot
        let mut v = s.to_json();
        v["root"] = json!("/home/x/.claude");
        assert_eq!(Snapshot::from_json(&v), None);
    }

    #[test]
    fn json_round_trip_and_refusals() {
        let s = snap("acct-a", NOW_MS, Some((0.32, NOW_S + 60)), None);
        assert_eq!(Snapshot::from_json(&s.to_json()), Some(s.clone()));
        let mut v = s.to_json();
        v["schema"] = json!("codeforge.quota/2");
        assert_eq!(Snapshot::from_json(&v), None);
        let mut v = s.to_json();
        v["alias"] = json!("me@example.com");
        assert_eq!(Snapshot::from_json(&v), None);
        let mut v = s.to_json();
        v["five_hour"]["pct"] = json!(f64::NAN.to_string());
        assert_eq!(Snapshot::from_json(&v).unwrap().five_hour, None);
    }

    #[test]
    fn constrained_skips_expired_windows_and_picks_the_higher() {
        let s = snap(
            "acct-a",
            NOW_MS,
            Some((0.9, NOW_S + 60)),
            Some((0.5, NOW_S + 9000)),
        );
        assert_eq!(
            s.constrained(NOW_S).map(|(l, w)| (l, w.pct)),
            Some(("5h", 0.9))
        );
        // the 5h window reset: unknown, the 7d one stands
        let s = snap(
            "acct-a",
            NOW_MS,
            Some((0.9, NOW_S - 1)),
            Some((0.5, NOW_S + 9000)),
        );
        assert_eq!(
            s.constrained(NOW_S).map(|(l, w)| (l, w.pct)),
            Some(("7d", 0.5))
        );
        // both reset: nothing to show
        let s = snap("acct-a", NOW_MS, Some((0.9, NOW_S)), Some((0.5, NOW_S - 5)));
        assert_eq!(s.constrained(NOW_S), None);
    }

    #[cfg(unix)]
    #[test]
    fn write_is_0600_in_a_0700_dir_and_throttled() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let s = snap("acct-a", NOW_MS, Some((0.32, NOW_S + 60)), None);
        assert!(merge_snapshot(&dir, &s).unwrap().1);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join("acct-a.json")), 0o600);
        assert_eq!(mode(&dir), 0o700);
        // same numbers 10 s later: not rewritten
        let later = Snapshot {
            written_at_ms: NOW_MS + 10_000,
            ..s.clone()
        };
        assert!(!merge_snapshot(&dir, &later).unwrap().1);
        // a changed percentage: rewritten at once
        let changed = snap("acct-a", NOW_MS + 11_000, Some((0.33, NOW_S + 60)), None);
        assert!(merge_snapshot(&dir, &changed).unwrap().1);
        // the same numbers 30 s on: rewritten (keeps the age fresh)
        let aged = Snapshot {
            written_at_ms: NOW_MS + 41_000,
            ..changed
        };
        assert!(merge_snapshot(&dir, &aged).unwrap().1);
        // a snapshot found wider than 0600 is rewritten at once, back to 0600
        std::fs::set_permissions(
            dir.join("acct-a.json"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let soon = Snapshot {
            written_at_ms: NOW_MS + 42_000,
            ..aged.clone()
        };
        assert!(merge_snapshot(&dir, &soon).unwrap().1);
        assert_eq!(mode(&dir.join("acct-a.json")), 0o600);
        // the rewrite still merges: a lower value of the same window does not land
        std::fs::set_permissions(
            dir.join("acct-a.json"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let lower = snap("acct-a", NOW_MS + 43_000, Some((0.02, NOW_S + 60)), None);
        let (merged, wrote) = merge_snapshot(&dir, &lower).unwrap();
        assert!(wrote);
        assert_eq!(mode(&dir.join("acct-a.json")), 0o600);
        assert_eq!(merged.five_hour.map(|w| w.pct), Some(0.33));
        assert_eq!(
            read_bounded(&dir.join("acct-a.json"))
                .unwrap()
                .five_hour
                .map(|w| w.pct),
            Some(0.33)
        );
        // an invalid alias is never written
        let bad = snap("Me@x", NOW_MS, Some((0.1, NOW_S + 60)), None);
        assert!(!merge_snapshot(&dir, &bad).unwrap().1);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    fn seen(w: Option<Window>, at_ms: i64, session: &str) -> Option<Window> {
        w.map(|w| Window {
            seen: Some(Seen {
                at_ms,
                session: session.to_string(),
            }),
            ..w
        })
    }

    fn on_disk(dir: &Path) -> (Option<f64>, Option<f64>) {
        let s = read_bounded(&dir.join("acct-a.json")).unwrap();
        (s.five_hour.map(|w| w.pct), s.seven_day.map(|w| w.pct))
    }

    /// Measured 2026-11-06: an active and an idle session of one account alternate
    /// every second, the idle one carrying its last (lower) value of the same window.
    #[test]
    fn interleaved_sessions_never_lower_a_window() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        let fresh = |at| {
            let mut s = snap(
                "acct-a",
                at,
                Some((0.15, NOW_S + 60)),
                Some((0.52, NOW_S + 9000)),
            );
            s.five_hour = seen(s.five_hour, at, "sess-active");
            s.seven_day = seen(s.seven_day, at, "sess-active");
            s
        };
        let stale = |at| {
            snap(
                "acct-a",
                at,
                Some((0.02, NOW_S + 60)),
                Some((0.48, NOW_S + 9000)),
            )
        };
        for i in 0..6 {
            let at = NOW_MS + i * 1000;
            let (merged, _) = if i % 2 == 0 {
                merge_snapshot(&dir, &fresh(at)).unwrap()
            } else {
                merge_snapshot(&dir, &stale(at)).unwrap()
            };
            assert_eq!(on_disk(&dir), (Some(0.15), Some(0.52)), "write {i}");
            assert_eq!(merged.five_hour.as_ref().map(|w| w.pct), Some(0.15));
            assert_eq!(merged.seven_day.as_ref().map(|w| w.pct), Some(0.52));
        }
        // provenance names the session that saw the kept value, and when; the same value
        // again inside WRITE_EVERY_MS is not rewritten, so it is the first sighting
        let kept = read_bounded(&dir.join("acct-a.json")).unwrap();
        assert_eq!(
            kept.five_hour.unwrap().seen,
            Some(Seen {
                at_ms: NOW_MS,
                session: "sess-active".into()
            })
        );
    }

    #[test]
    fn a_later_window_replaces_an_earlier_one_is_dropped() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        merge_snapshot(&dir, &snap("acct-a", NOW_MS, Some((0.9, NOW_S + 60)), None)).unwrap();
        // the 5h window reset: a later resets_at with a lower value replaces it
        let next = snap("acct-a", NOW_MS + 1000, Some((0.03, NOW_S + 18_060)), None);
        merge_snapshot(&dir, &next).unwrap();
        assert_eq!(on_disk(&dir), (Some(0.03), None));
        // an idle session still holding the old window does not bring it back
        let old = snap("acct-a", NOW_MS + 2000, Some((0.9, NOW_S + 60)), None);
        let (merged, _) = merge_snapshot(&dir, &old).unwrap();
        assert_eq!(on_disk(&dir), (Some(0.03), None));
        assert_eq!(merged.five_hour.unwrap().resets_at, NOW_S + 18_060);
        // resets_at a few seconds apart is the same window: the larger value stands
        let jitter = snap("acct-a", NOW_MS + 3000, Some((0.01, NOW_S + 18_063)), None);
        merge_snapshot(&dir, &jitter).unwrap();
        assert_eq!(on_disk(&dir), (Some(0.03), None));
        // a writer that saw no 7d window keeps the stored one
        merge_snapshot(
            &dir,
            &snap("acct-a", NOW_MS + 4000, None, Some((0.4, NOW_S + 9000))),
        )
        .unwrap();
        merge_snapshot(
            &dir,
            &snap("acct-a", NOW_MS + 5000, Some((0.05, NOW_S + 18_060)), None),
        )
        .unwrap();
        assert_eq!(on_disk(&dir), (Some(0.05), Some(0.4)));
    }

    #[test]
    fn seen_round_trips_and_a_bad_writer_id_is_dropped() {
        let mut s = snap("acct-a", NOW_MS, Some((0.32, NOW_S + 60)), None);
        s.five_hour = seen(s.five_hour, NOW_MS - 5, "0f3c-session_1");
        assert_eq!(Snapshot::from_json(&s.to_json()), Some(s.clone()));
        let mut v = s.to_json();
        v["five_hour"]["seen_by"] = json!("../etc");
        let w = Snapshot::from_json(&v).unwrap().five_hour.unwrap();
        assert_eq!((w.pct, w.seen), (0.32, None));
    }

    #[test]
    fn read_others_filters_and_orders() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        for s in [
            snap("acct-own", NOW_MS, Some((0.99, NOW_S + 60)), None),
            snap(
                "acct-b",
                NOW_MS - 1000,
                Some((0.2, NOW_S + 60)),
                Some((0.81, NOW_S + 9000)),
            ),
            snap("acct-c", NOW_MS - 2000, Some((0.12, NOW_S + 60)), None),
            snap(
                "acct-old",
                NOW_MS - MAX_AGE_MS,
                Some((0.5, NOW_S + 60)),
                None,
            ),
            snap("acct-reset", NOW_MS, Some((0.7, NOW_S - 1)), None),
        ] {
            crate::live::write_live_json(&dir, &format!("{}.json", s.alias), &s.to_json()).unwrap();
        }
        let got: Vec<String> = read_others(&dir, "acct-own", NOW_MS)
            .into_iter()
            .map(|s| s.alias)
            .collect();
        assert_eq!(got, ["acct-b", "acct-c"]);
        assert!(read_others(&base.path().join("missing"), "x", NOW_MS).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn read_is_bounded_and_skips_links_big_and_bad_files() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        let good = snap("acct-b", NOW_MS, Some((0.4, NOW_S + 60)), None);
        crate::live::write_live_json(&dir, "acct-b.json", &good.to_json()).unwrap();
        // a symlink to a good snapshot elsewhere is not read
        let elsewhere = base.path().join("acct-l.json");
        std::fs::write(
            &elsewhere,
            snap("acct-l", NOW_MS, Some((0.4, NOW_S + 60)), None)
                .to_json()
                .to_string(),
        )
        .unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join("acct-l.json")).unwrap();
        // over 4 KiB (a valid snapshot padded with spaces)
        let mut big = snap("acct-big", NOW_MS, Some((0.4, NOW_S + 60)), None)
            .to_json()
            .to_string();
        big.push_str(&" ".repeat(5000));
        std::fs::write(dir.join("acct-big.json"), big).unwrap();
        std::fs::write(dir.join("acct-bad.json"), "{not json").unwrap();
        std::fs::create_dir(dir.join("acct-dir.json")).unwrap();
        let got: Vec<String> = read_others(&dir, "acct-own", NOW_MS)
            .into_iter()
            .map(|s| s.alias)
            .collect();
        assert_eq!(got, ["acct-b"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_is_skipped_without_blocking() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        let good = snap("acct-b", NOW_MS, Some((0.4, NOW_S + 60)), None);
        crate::live::write_live_json(&dir, "acct-b.json", &good.to_json()).unwrap();
        let fifo =
            std::ffi::CString::new(dir.join("acct-f.json").as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path; mkfifo has no other preconditions.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        // no writer ever opens it: a blocking open would hang this test
        let (tx, rx) = std::sync::mpsc::channel();
        let d = dir.clone();
        std::thread::spawn(move || {
            let got: Vec<String> = read_others(&d, "acct-own", NOW_MS)
                .into_iter()
                .map(|s| s.alias)
                .collect();
            let _ = tx.send(got);
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("read_others blocked on a FIFO");
        assert_eq!(got, ["acct-b"]);
    }

    #[test]
    fn read_takes_at_most_max_files() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("q");
        for i in 0..(MAX_FILES + 4) {
            let s = snap(
                &format!("acct-{i:02}"),
                NOW_MS,
                Some((0.1, NOW_S + 60)),
                None,
            );
            crate::live::write_live_json(&dir, &format!("{}.json", s.alias), &s.to_json()).unwrap();
        }
        assert_eq!(read_others(&dir, "x", NOW_MS).len(), MAX_FILES);
    }
}
