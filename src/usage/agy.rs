//! The local `agy` (Antigravity CLI) account's quota, as quota snapshots for the summary.
//!
//! `agy -p /usage` prints the two subscription pools as tab-separated lines, each a pool, a
//! window label, the REMAINING percent and the window's RFC3339 reset time (exit 0, no model
//! call):
//!
//! ```text
//! Gemini Models<TAB>Weekly Limit Remaining<TAB>60%<TAB>2026-11-13T23:43:49Z
//! Claude and GPT models<TAB>Five Hour Limit Remaining<TAB>100%<TAB>2026-11-11T14:41:51Z
//! ```
//!
//! That is another program's output (a trust boundary): a line is kept only when it has exactly
//! four tab-separated fields, a known pool and window label, a remaining percent that is an
//! integer 0..=100 followed by `%`, and a parsable time. The percent is REMAINING, so the
//! snapshot's `pct` (the USED fraction 0.0–1.0) is `1 − remaining/100`.
//!
//! The key is the `path_hash8` of the host's home dir — the same Google account on two hosts is
//! two rows until the operator names them. The capture is cached at
//! `<cache_dir>/agy-usage.json` (0600, atomic) and at most 10 minutes old; a failed run falls
//! back to a cache not older than [`quota::MAX_AGE_MS`]. `CODEFORGE_USAGE_AGY=0` never spawns
//! `agy`. The binary is spawned with cwd in the system temp dir, stdin/stderr closed, and
//! killed after 20 s; only an exit-0 stdout is a capture.

use chrono::DateTime;
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::path::Path;

use super::alias;
use super::quota;

/// The `agy` binary (resolved on PATH; no `agy` on PATH is no capture, not an error).
const BIN: &str = "agy";
/// The capture is rerun at most this often.
pub const CACHE_MS: i64 = 10 * 60 * 1000;
/// A `agy -p /usage` that runs longer is killed.
pub const TIMEOUT_S: u64 = 20;
/// Set to `0`: never spawn `agy`.
pub const ENV_DISABLE: &str = "CODEFORGE_USAGE_AGY";
/// Cache file name in the summary dir.
const CACHE_FILE: &str = "agy-usage.json";
/// The cache holds only the raw capture: anything bigger is no cache.
const CACHE_MAX: usize = 4096;

/// Pool label → account key prefix. One host, one Google account, two pools: the Gemini pool
/// and the Claude/GPT pool.
const POOLS: [(&str, &str); 2] = [
    ("Gemini Models", "agy"),
    ("Claude and GPT models", "agyclaude"),
];
/// The `Weekly Limit Remaining` window.
const WEEKLY: &str = "Weekly Limit Remaining";
/// The `Five Hour Limit Remaining` window.
const FIVE_HOUR: &str = "Five Hour Limit Remaining";

/// One pool's state while the lines are read: its account key and the windows seen so far.
struct Pool {
    alias: String,
    five_hour: Option<quota::Window>,
    seven_day: Option<quota::Window>,
}

/// The capture's lines as quota snapshots: one per pool that has at least one valid window.
/// `observed_ms` is when the text was captured (the cache's own time on a cache read);
/// `key_hash` is the `path_hash8` of the host's home dir. The percent is REMAINING, so
/// `pct` (the used fraction) is `1 − remaining/100`; `resets_at` is the RFC3339 time in unix
/// seconds. Any line off the shape is skipped, never an error.
pub fn parse(text: &str, observed_ms: i64, key_hash: &str) -> Vec<quota::Snapshot> {
    let mut pools: [Option<Pool>; 2] = [None, None];
    for line in text.lines() {
        let mut f = line.split('\t');
        let (Some(pool), Some(label), Some(remaining_s), Some(time_s)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        if f.next().is_some() {
            // not exactly four fields
            continue;
        }
        let Some(i) = POOLS.iter().position(|(label, _)| *label == pool) else {
            continue;
        };
        let slot = pools[i].get_or_insert_with(|| Pool {
            alias: format!("{}-{key_hash}", POOLS[i].1),
            five_hour: None,
            seven_day: None,
        });
        // an integer 0..=100 followed by `%`; nothing else is a percent
        let digits = remaining_s
            .strip_suffix('%')
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
        let Some(remaining) = digits
            .and_then(|d| d.parse::<u32>().ok())
            .filter(|n| *n <= 100)
        else {
            continue;
        };
        let Some(resets_at) = DateTime::parse_from_rfc3339(time_s)
            .ok()
            .map(|t| t.timestamp())
        else {
            continue;
        };
        let used = (100 - remaining) as f64 / 100.0;
        let w = quota::Window {
            pct: used,
            resets_at,
            seen: Some(quota::Seen {
                at_ms: observed_ms,
                session: String::new(),
            }),
        };
        match label {
            WEEKLY => slot.seven_day = Some(w),
            FIVE_HOUR => slot.five_hour = Some(w),
            _ => {}
        }
    }
    pools
        .into_iter()
        .flatten()
        .filter(|p| p.five_hour.is_some() || p.seven_day.is_some())
        .map(|p| quota::Snapshot {
            alias: p.alias,
            org: None,
            email: None,
            root: None,
            written_at_ms: observed_ms,
            five_hour: p.five_hour,
            seven_day: p.seven_day,
        })
        .collect()
}

/// The cache file: the raw capture and when it was taken.
#[derive(Deserialize)]
struct Cache {
    observed_ms: i64,
    text: String,
}

/// The cache at `path`, or nothing: missing, unopened, over the cap, or not the shape.
fn read_cache(path: &Path) -> Option<Cache> {
    let file = quota::open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > CACHE_MAX as u64 {
        return None;
    }
    let mut buf = Vec::new();
    file.take(CACHE_MAX as u64 + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > CACHE_MAX as u64 {
        return None;
    }
    serde_json::from_slice(&buf).ok()
}

/// The host's agy snapshots at `now_ms`, from the cache at `cache_dir` (`agy-usage.json`)
/// while it is younger than [`CACHE_MS`] (then `run` is not called), else from `run` — whose
/// `Some` text, when it parses to at least one snapshot, becomes the new cache. A `None` run
/// (no `agy` on PATH, the off switch, a non-zero exit, or a timeout) and a parse that finds
/// nothing fall back to the cache while it is younger than [`quota::MAX_AGE_MS`], else return
/// no snapshots. The key is the `path_hash8` of `home` (canonicalized).
pub fn snapshots(
    cache_dir: &Path,
    home: &Path,
    now_ms: i64,
    run: impl FnOnce() -> Option<String>,
) -> Vec<quota::Snapshot> {
    let key_hash = alias::path_hash8(home);
    let cache = read_cache(&cache_dir.join(CACHE_FILE));
    let within =
        |at_ms: i64, limit: i64| (0..limit).contains(&now_ms.checked_sub(at_ms).unwrap_or(limit));
    if let Some(c) = cache.as_ref().filter(|c| within(c.observed_ms, CACHE_MS)) {
        return parse(&c.text, c.observed_ms, &key_hash);
    }
    let fallback = || {
        cache
            .as_ref()
            .filter(|c| within(c.observed_ms, quota::MAX_AGE_MS))
            .map(|c| parse(&c.text, c.observed_ms, &key_hash))
            .unwrap_or_default()
    };
    match run() {
        Some(text) => {
            let snaps = parse(&text, now_ms, &key_hash);
            if snaps.is_empty() {
                return fallback();
            }
            let _ = crate::live::write_live_json(
                cache_dir,
                CACHE_FILE,
                &json!({ "observed_ms": now_ms, "text": text }),
            );
            snaps
        }
        None => fallback(),
    }
}

/// `agy -p /usage` (cwd in the system temp dir, stdin and stderr closed, killed after
/// [`TIMEOUT_S`]), `Some(stdout)` only on exit 0. The timeout and the kill are the house
/// wrapper (`platform::low_priority_command` — `timeout` on Unix, the pid guard on Windows,
/// the same bound `llm` runs its headless CLIs under). Never spawned when `agy` does not
/// resolve on PATH or when [`ENV_DISABLE`] is `0`.
pub fn run_agy() -> Option<String> {
    if std::env::var(ENV_DISABLE).is_ok_and(|v| v == "0") {
        return None;
    }
    let child = crate::platform::low_priority_command(&[BIN, "-p", "/usage"], TIMEOUT_S)
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // dropped only after the child is reaped, so a recycled pid is never signalled
    let _guard = crate::platform::guard_timeout(child.id(), TIMEOUT_S);
    let out = child.wait_with_output().ok()?;
    drop(_guard);
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_793_600_000_000;
    const KEY: &str = "1a2b3c4d";
    /// The real capture of `agy -p /usage` (2026-11-11).
    const CAPTURE: &str = "Gemini Models\tWeekly Limit Remaining\t60%\t2026-11-13T23:43:49Z\n\
                           Gemini Models\tFive Hour Limit Remaining\t98%\t2026-11-11T14:25:15Z\n\
                           Claude and GPT models\tWeekly Limit Remaining\t84%\t2026-11-12T23:02:04Z\n\
                           Claude and GPT models\tFive Hour Limit Remaining\t100%\t2026-11-11T14:41:51Z\n";

    fn ts(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp()
    }

    fn home() -> std::path::PathBuf {
        std::path::PathBuf::from("/home/t")
    }

    fn write_cache(dir: &Path, observed_ms: i64, text: &str) {
        crate::live::write_live_json(
            dir,
            CACHE_FILE,
            &json!({ "observed_ms": observed_ms, "text": text }),
        )
        .unwrap();
    }

    #[test]
    fn the_real_capture_is_two_snapshots_with_used_fractions() {
        let v = parse(CAPTURE, NOW_MS, KEY);
        assert_eq!(v.len(), 2);
        let g = &v[0];
        assert_eq!(g.alias, format!("agy-{KEY}"));
        assert_eq!(
            (g.org.as_ref(), g.email.as_ref(), g.root.as_ref()),
            (None, None, None)
        );
        assert_eq!(g.written_at_ms, NOW_MS);
        let seen = Some(quota::Seen {
            at_ms: NOW_MS,
            session: String::new(),
        });
        assert_eq!(
            g.seven_day,
            Some(quota::Window {
                pct: 0.40,
                resets_at: ts("2026-11-13T23:43:49Z"),
                seen: seen.clone(),
            })
        );
        assert_eq!(
            g.five_hour,
            Some(quota::Window {
                pct: 0.02,
                resets_at: ts("2026-11-11T14:25:15Z"),
                seen: seen.clone(),
            })
        );
        let c = &v[1];
        assert_eq!(c.alias, format!("agyclaude-{KEY}"));
        assert_eq!(
            c.seven_day,
            Some(quota::Window {
                pct: 0.16,
                resets_at: ts("2026-11-12T23:02:04Z"),
                seen: seen.clone(),
            })
        );
        assert_eq!(
            c.five_hour,
            Some(quota::Window {
                pct: 0.0,
                resets_at: ts("2026-11-11T14:41:51Z"),
                seen,
            })
        );
    }

    #[test]
    fn malformed_unknown_and_out_of_range_lines_are_skipped() {
        let bad = [
            "Gemini Models\tWeekly Limit Remaining\t101%\t2026-11-13T23:43:49Z",
            "Gemini Models\tFive Hour Limit Remaining\tx%\t2026-11-11T14:25:15Z",
            "Gemini Models\tWeekly Limit Remaining\t60\t2026-11-13T23:43:49Z",
            "Gemini Models\tWeekly Limit Remaining\t-5%\t2026-11-13T23:43:49Z",
            "Gemini Models\tWeekly Limit Remaining\t60% \t2026-11-13T23:43:49Z",
            "Gemini Models\tWeekly Limit Remaining\t60%\tnot-a-time",
            "Gemini Models\tMonthly Limit Remaining\t60%\t2026-11-13T23:43:49Z",
            "Other Pool\tWeekly Limit Remaining\t60%\t2026-11-13T23:43:49Z",
            "Claude and GPT models\tWeekly Limit Remaining\t84%",
            "Claude and GPT models\tWeekly Limit Remaining\t84%\t2026-11-12T23:02:04Z\textra",
            "",
        ]
        .join("\n");
        // no pool has a valid window: no snapshot at all
        assert!(parse(&bad, NOW_MS, KEY).is_empty());
        // one valid line for one pool: that pool stands with its one window
        let mixed = format!(
            "{bad}\nClaude and GPT models\tFive Hour Limit Remaining\t100%\t2026-11-11T14:41:51Z"
        );
        let v = parse(&mixed, NOW_MS, KEY);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].alias, format!("agyclaude-{KEY}"));
        assert_eq!(v[0].seven_day, None);
        assert!(v[0].five_hour.is_some());
    }

    #[test]
    fn a_fresh_cache_is_parsed_without_calling_run() {
        let dir = tempfile::tempdir().unwrap();
        write_cache(dir.path(), NOW_MS - 60_000, CAPTURE);
        let got = snapshots(dir.path(), &home(), NOW_MS, || -> Option<String> {
            panic!("run must not be called on a fresh cache")
        });
        assert_eq!(got.len(), 2);
        // the cache's own observation time, not now
        assert_eq!(got[0].written_at_ms, NOW_MS - 60_000);
        assert_eq!(got[0].alias, format!("agy-{}", alias::path_hash8(&home())));
    }

    #[test]
    fn a_stale_cache_is_returned_when_the_run_fails() {
        let dir = tempfile::tempdir().unwrap();
        // older than CACHE_MS, younger than quota::MAX_AGE_MS
        write_cache(dir.path(), NOW_MS - 11 * 60_000, CAPTURE);
        let got = snapshots(dir.path(), &home(), NOW_MS, || None);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].written_at_ms, NOW_MS - 11 * 60_000);
        // an over-aged cache is not returned
        std::fs::remove_file(dir.path().join(CACHE_FILE)).unwrap();
        write_cache(dir.path(), NOW_MS - 7 * 3_600_000, CAPTURE);
        assert!(snapshots(dir.path(), &home(), NOW_MS, || None).is_empty());
    }

    #[test]
    fn no_cache_and_no_run_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(snapshots(dir.path(), &home(), NOW_MS, || None).is_empty());
    }

    #[test]
    fn a_good_run_writes_the_cache_and_the_next_call_does_not_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || Some(CAPTURE.to_string()));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].written_at_ms, NOW_MS);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join(CACHE_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        // the cache is now fresh: the next call parses it and never calls run
        let again = snapshots(
            dir.path(),
            &home(),
            NOW_MS + 60_000,
            || -> Option<String> { panic!("run must not be called on a fresh cache") },
        );
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].written_at_ms, NOW_MS);
    }

    #[test]
    fn an_unparsable_run_falls_back_to_the_stale_cache_without_rewriting_it() {
        let dir = tempfile::tempdir().unwrap();
        write_cache(dir.path(), NOW_MS - 11 * 60_000, CAPTURE);
        let got = snapshots(dir.path(), &home(), NOW_MS, || Some("garbage\n".into()));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].written_at_ms, NOW_MS - 11 * 60_000);
        // the cache was not overwritten with the garbage
        let c: Cache =
            serde_json::from_slice(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap()).unwrap();
        assert_eq!(c.observed_ms, NOW_MS - 11 * 60_000);
        assert_eq!(c.text, CAPTURE);
    }

    #[test]
    fn the_off_switch_never_spawns() {
        std::env::set_var(ENV_DISABLE, "0");
        assert_eq!(run_agy(), None);
        std::env::remove_var(ENV_DISABLE);
    }
}
