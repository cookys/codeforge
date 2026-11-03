//! Per-account quota snapshots (G36 plan §2–3).
//!
//! Every statusline run that sees `rate_limits` writes
//! `<live_base>/codeforge/usage/quota/<alias>.json` (0600, dir 0700, tmp + rename),
//! at most every [`WRITE_EVERY_MS`] unless a percentage changed. The usage row
//! reads the other accounts' snapshots back with a bounded read.
//!
//! Staleness: a window whose `resets_at` has passed is unknown; a snapshot older
//! than [`MAX_AGE_MS`] is dropped.

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

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// 0.0–1.0
    pub pct: f64,
    /// unix seconds
    pub resets_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub alias: String,
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

fn window_json(w: &Option<Window>) -> Value {
    match w {
        Some(w) => json!({ "pct": w.pct, "resets_at": w.resets_at }),
        None => Value::Null,
    }
}

fn window_from(v: &Value) -> Option<Window> {
    let pct = v["pct"].as_f64()?;
    let resets_at = v["resets_at"].as_i64()?;
    (pct.is_finite() && (0.0..=1.5).contains(&pct)).then_some(Window { pct, resets_at })
}

impl Snapshot {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": SCHEMA,
            "alias": self.alias,
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
        Some(Snapshot {
            alias: alias.to_string(),
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

/// Read one snapshot file: a regular file (not a symlink), at most MAX_BYTES by
/// metadata and by bytes read, JSON with the right schema. Anything else → None.
fn read_bounded(path: &Path) -> Option<Snapshot> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() as u64 > MAX_BYTES {
        return None;
    }
    Snapshot::from_json(&serde_json::from_slice(&buf).ok()?)
}

/// Write `snap` unless the file on disk already holds the same percentages and
/// was written less than WRITE_EVERY_MS ago. Returns whether it wrote.
pub fn write_snapshot(dir: &Path, snap: &Snapshot) -> std::io::Result<bool> {
    if !valid_alias(&snap.alias) {
        return Ok(false);
    }
    let file = format!("{}.json", snap.alias);
    if let Some(old) = read_bounded(&dir.join(&file)) {
        let age = snap.written_at_ms - old.written_at_ms;
        if old.five_hour == snap.five_hour
            && old.seven_day == snap.seven_day
            && (0..WRITE_EVERY_MS).contains(&age)
        {
            return Ok(false);
        }
    }
    crate::live::write_live_json(dir, &file, &snap.to_json())?;
    Ok(true)
}

/// The other accounts' snapshots worth showing at `now_ms`: not `own_alias`,
/// written within MAX_AGE_MS, with a running window; most constrained first.
/// Reads at most MAX_FILES `*.json` entries (by name order); bad ones are skipped.
pub fn read_others(dir: &Path, own_alias: &str, now_ms: i64) -> Vec<Snapshot> {
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
    let now_s = now_ms.div_euclid(1000);
    let mut out: Vec<Snapshot> = names
        .iter()
        .take(MAX_FILES)
        .filter_map(|p| read_bounded(p))
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
        let w = |x: Option<(f64, i64)>| x.map(|(pct, resets_at)| Window { pct, resets_at });
        Snapshot {
            alias: alias.into(),
            written_at_ms: at_ms,
            five_hour: w(five),
            seven_day: w(seven),
        }
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
        assert!(write_snapshot(&dir, &s).unwrap());
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join("acct-a.json")), 0o600);
        assert_eq!(mode(&dir), 0o700);
        // same numbers 10 s later: not rewritten
        let later = Snapshot {
            written_at_ms: NOW_MS + 10_000,
            ..s.clone()
        };
        assert!(!write_snapshot(&dir, &later).unwrap());
        // a changed percentage: rewritten at once
        let changed = snap("acct-a", NOW_MS + 11_000, Some((0.33, NOW_S + 60)), None);
        assert!(write_snapshot(&dir, &changed).unwrap());
        // the same numbers 30 s on: rewritten (keeps the age fresh)
        let aged = Snapshot {
            written_at_ms: NOW_MS + 41_000,
            ..changed
        };
        assert!(write_snapshot(&dir, &aged).unwrap());
        // an invalid alias is never written
        let bad = snap("Me@x", NOW_MS, Some((0.1, NOW_S + 60)), None);
        assert!(!write_snapshot(&dir, &bad).unwrap());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
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
