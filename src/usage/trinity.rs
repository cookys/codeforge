//! Trinity's self-reported quota (`trinity-usage/1`), operator d-e31e39c0.
//!
//! The bot (`cookys/grokbot/trinity`) sends one line per wake, at most once per UTC hour:
//! `{"schema":"trinity-usage/1","observed_at":…,"pct":…,"resets_at":…}`. This module parses
//! it and keeps the newest observation in `trinity-quota.json`, which `codeforge usage` turns
//! into the weekly-window snapshot of the account `trinity-<sha8>`. Only the quota: token
//! counts and wakes are not imported (the wake count is hangar's hub proxy, PM 01M76GXN).
//! No network and no credential: the line is untrusted, and nothing in it except the three
//! fields below is stored.
//!
//! The sha8 is sha256 of the member address string, the same 8 hex digits `alias::path_hash8`
//! produces when that relative path does not exist. Hashing the string (not the canonicalized
//! path) keeps the account key stable across hosts and working directories.

use anyhow::{bail, Context, Result};
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

use super::quota;

/// Fleet member that reports. The account key is derived from this string.
pub const MEMBER: &str = "cookys/grokbot/trinity";
/// Line schema. Any other value skips the line.
pub const SCHEMA: &str = "trinity-usage/1";
/// `CODEFORGE_USAGE_TRINITY=0` skips [`snapshots`] during collect (the cache included).
pub const ENV_DISABLE: &str = "CODEFORGE_USAGE_TRINITY";
/// Quota cache next to `summary.json`.
pub const QUOTA_FILE: &str = "trinity-quota.json";

/// An `observed_at` later than now by more than this skips the line (clock skew allowance).
const SKEW_MS: i64 = 5 * 60_000;
/// A report bigger than this is refused, not parsed.
pub const MAX_BYTES: u64 = 64 << 10;
/// A report with more lines than this is refused, not parsed.
pub const MAX_LINES: usize = 200;
const QUOTA_MAX: u64 = 4096;

/// One accepted observation. Extra JSON fields never land here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Report {
    /// Unix milliseconds.
    pub observed_ms: i64,
    /// Used fraction, 0.0..=1.5.
    pub pct: f64,
    /// Unix seconds, after `observed_ms`.
    pub resets_at: i64,
}

/// What [`parse`] kept. `skipped` counts invalid lines only; blank lines are not counted.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub reports: Vec<Report>,
    pub skipped: usize,
}

#[derive(Deserialize)]
struct QuotaFile {
    observed_ms: i64,
    pct: f64,
    resets_at: i64,
}

/// `true` only when [`ENV_DISABLE`] is `0`. The caller then does not call [`snapshots`].
pub fn disabled() -> bool {
    std::env::var(ENV_DISABLE).is_ok_and(|v| v == "0")
}

fn hash8(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// `trinity-<sha8>` of [`MEMBER`]. Stable across hosts.
pub fn account_key() -> String {
    format!("trinity-{}", hash8(MEMBER.as_bytes()))
}

/// The one line the import command prints.
pub fn report_line(written: bool, skipped: usize) -> String {
    let what = if written {
        "已更新 trinity 配額"
    } else {
        "沒有比現有更新的配額"
    };
    format!("{what}，略過 {skipped} 行")
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let n = text.split('\n').count();
    if text.ends_with('\n') {
        n - 1
    } else {
        n
    }
}

/// Read `r` up to the file cap. Over the cap, or not UTF-8, is an error.
pub fn read_reader(mut r: impl Read) -> Result<String> {
    let mut buf = Vec::new();
    r.by_ref()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut buf)
        .context("讀不到配額檔")?;
    if buf.len() as u64 > MAX_BYTES {
        bail!("配額檔超過 64 KiB，拒絕匯入");
    }
    let text = String::from_utf8(buf).context("配額檔不是 UTF-8，拒絕匯入")?;
    if line_count(&text) > MAX_LINES {
        bail!("配額檔超過 200 行，拒絕匯入");
    }
    Ok(text)
}

/// [`read_reader`] on a file the operator named.
pub fn read_path(path: &Path) -> Result<String> {
    let shown: String = path.display().to_string().chars().take(256).collect();
    let file = std::fs::File::open(path).with_context(|| format!("讀不到配額檔 {shown}"))?;
    read_reader(file)
}

fn parse_line(line: &str, now_ms: i64) -> Option<Report> {
    let v: Value = serde_json::from_str(line).ok()?;
    let obj = v.as_object()?;
    if obj.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return None;
    }
    let observed_ms = DateTime::parse_from_rfc3339(obj.get("observed_at")?.as_str()?)
        .ok()?
        .timestamp_millis();
    if observed_ms > now_ms.saturating_add(SKEW_MS) {
        return None;
    }
    let pct = obj.get("pct")?.as_f64()?;
    if !pct.is_finite() || !(0.0..=1.5).contains(&pct) {
        return None;
    }
    let resets_at = DateTime::parse_from_rfc3339(obj.get("resets_at")?.as_str()?)
        .ok()?
        .timestamp();
    if resets_at <= observed_ms.div_euclid(1000) {
        return None;
    }
    Some(Report {
        observed_ms,
        pct,
        resets_at,
    })
}

/// Parse `text`. Pure: `now_ms` is the clock. Blank lines are skipped and not counted; every
/// other line that is not one valid report is counted.
pub fn parse(text: &str, now_ms: i64) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reports = Vec::new();
    let mut skipped = 0usize;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(line, now_ms) {
            Some(r) => reports.push(r),
            None => skipped += 1,
        }
    }
    Parsed { reports, skipped }
}

/// Write the newest of `reports` to `summary_dir`/[`QUOTA_FILE`] when it is newer than the
/// file's `observed_ms`. Returns whether the file was written.
pub fn import(reports: &[Report], summary_dir: &Path) -> Result<bool> {
    let Some(r) = reports.iter().max_by_key(|r| r.observed_ms) else {
        return Ok(false);
    };
    let path = summary_dir.join(QUOTA_FILE);
    if let Some(old) = super::fetch::read_capped_json::<QuotaFile>(&path, QUOTA_MAX) {
        if old.observed_ms >= r.observed_ms {
            return Ok(false);
        }
    }
    let body = json!({
        "observed_ms": r.observed_ms,
        "pct": r.pct,
        "resets_at": r.resets_at,
    });
    crate::live::write_live_json(summary_dir, QUOTA_FILE, &body)
        .map_err(|e| anyhow::anyhow!("無法寫入 trinity 配額（{:?}）", e.kind()))?;
    Ok(true)
}

fn fresh(observed_ms: i64, now_ms: i64) -> bool {
    (0..quota::MAX_AGE_MS).contains(&now_ms.checked_sub(observed_ms).unwrap_or(quota::MAX_AGE_MS))
}

/// The quota snapshot in `summary_dir`/`trinity-quota.json`, or nothing when the file is
/// missing, older than [`quota::MAX_AGE_MS`], or already past `resets_at`.
pub fn snapshots(summary_dir: &Path, now_ms: i64) -> Vec<quota::Snapshot> {
    let Some(q) =
        super::fetch::read_capped_json::<QuotaFile>(&summary_dir.join(QUOTA_FILE), QUOTA_MAX)
    else {
        return Vec::new();
    };
    if !q.pct.is_finite() || !(0.0..=1.5).contains(&q.pct) {
        return Vec::new();
    }
    if !fresh(q.observed_ms, now_ms) {
        return Vec::new();
    }
    if q.resets_at <= now_ms.div_euclid(1000) {
        return Vec::new();
    }
    vec![quota::Snapshot {
        alias: account_key(),
        org: None,
        email: None,
        root: None,
        written_at_ms: q.observed_ms,
        five_hour: None,
        seven_day: Some(quota::Window {
            pct: q.pct,
            resets_at: q.resets_at,
            seen: Some(quota::Seen {
                at_ms: q.observed_ms,
                session: String::new(),
            }),
        }),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::alias;

    const SENTINEL: &str = "secret-SENTINEL";

    fn ms(rfc3339: &str) -> i64 {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .timestamp_millis()
    }

    fn line(observed: &str, pct: &str, resets: &str) -> String {
        format!(
            r#"{{"schema":"trinity-usage/1","observed_at":{observed},"pct":{pct},"resets_at":{resets},"input":5,"email":"{SENTINEL}"}}"#
        )
    }

    fn ok(observed: &str, pct: &str) -> String {
        line(&format!("\"{observed}\""), pct, "\"2026-11-18T00:00:00Z\"")
    }

    #[test]
    fn account_key_is_stable_and_has_no_at() {
        let key = account_key();
        assert_eq!(
            key,
            format!("trinity-{}", alias::path_hash8(Path::new(MEMBER)))
        );
        assert!(alias::valid_alias(&key), "{key}");
        assert!(!key.contains('@'));
        assert_eq!(key.len(), "trinity-".len() + 8);
    }

    #[test]
    fn parse_keeps_valid_lines_and_counts_the_rest() {
        let now = ms("2026-11-12T04:00:00Z");
        let text = [
            ok("2026-11-12T03:12:00+08:00", "0.34"),
            String::new(),
            "\r".into(),
            ok("2026-11-12T04:04:59Z", "0"),
            ok("2026-11-12T04:05:01Z", "0.1"),
            ok("2026-11-12T03:00:00Z", "34"),
            ok("2026-11-12T03:00:00Z", "-0.01"),
            ok("2026-11-12T03:00:00Z", "\"0.3\""),
            ok("2026-11-12T03:00:00Z", "null"),
            ok("2026-11-12 03:00:00", "0.3"),
            line("\"2026-11-12T03:00:00Z\"", "0.3", "\"2026-11-12T02:00:00Z\""),
            line("\"2026-11-12T03:00:00Z\"", "0.3", "null"),
            r#"{"schema":"trinity-usage/2","observed_at":"2026-11-12T03:00:00Z","pct":0.3,"resets_at":"2026-11-18T00:00:00Z"}"#.into(),
            r#"{"observed_at":"2026-11-12T03:00:00Z","pct":0.3,"resets_at":"2026-11-18T00:00:00Z"}"#.into(),
            "not json".into(),
            "[1]".into(),
        ]
        .join("\n");
        let p = parse(&format!("\u{feff}{text}\n"), now);
        assert_eq!(
            p.reports,
            vec![
                Report {
                    observed_ms: ms("2026-11-11T19:12:00Z"),
                    pct: 0.34,
                    resets_at: ms("2026-11-18T00:00:00Z") / 1000,
                },
                Report {
                    observed_ms: ms("2026-11-12T04:04:59Z"),
                    pct: 0.0,
                    resets_at: ms("2026-11-18T00:00:00Z") / 1000,
                },
            ]
        );
        assert_eq!(p.skipped, 12);
        assert_eq!(parse("", now).skipped, 0);
    }

    fn read_file(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(QUOTA_FILE)).unwrap()
    }

    #[test]
    fn only_a_newer_observation_overwrites_and_a_past_reset_drops_the_snapshot() {
        let now = ms("2026-11-12T04:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        assert!(!import(&[], dir.path()).unwrap());
        assert!(!dir.path().join(QUOTA_FILE).exists());

        let two = parse(
            &[
                ok("2026-11-12T02:00:00Z", "0.30"),
                ok("2026-11-12T03:00:00Z", "0.41"),
            ]
            .join("\n"),
            now,
        );
        assert!(import(&two.reports, dir.path()).unwrap());
        let before = read_file(dir.path());
        let v: Value = serde_json::from_str(&before).unwrap();
        assert_eq!(
            v,
            json!({"observed_ms": ms("2026-11-12T03:00:00Z"), "pct": 0.41,
                   "resets_at": ms("2026-11-18T00:00:00Z") / 1000})
        );
        assert!(!before.contains(SENTINEL) && !before.contains('@'));

        for older in ["2026-11-12T03:00:00Z", "2026-11-12T02:30:00Z"] {
            let p = parse(&ok(older, "0.99"), now);
            assert!(!import(&p.reports, dir.path()).unwrap());
            assert_eq!(read_file(dir.path()), before);
        }

        let snap = snapshots(dir.path(), now);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].alias, account_key());
        assert!(snap[0].five_hour.is_none());
        assert_eq!(snap[0].seven_day.as_ref().unwrap().pct, 0.41);
        // older than MAX_AGE_MS
        assert!(snapshots(dir.path(), ms("2026-11-12T03:00:00Z") + quota::MAX_AGE_MS).is_empty());

        let newer = parse(
            &line(
                "\"2026-11-12T03:30:00Z\"",
                "0.5",
                "\"2026-11-12T05:00:00Z\"",
            ),
            now,
        );
        assert!(import(&newer.reports, dir.path()).unwrap());
        assert_eq!(snapshots(dir.path(), now).len(), 1);
        assert!(snapshots(dir.path(), ms("2026-11-12T05:00:00Z")).is_empty());
    }

    #[test]
    fn read_refuses_over_the_cap_and_a_missing_file() {
        let err = read_path(Path::new("/no/such/trinity-quota.jsonl")).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("讀不到配額檔"), "{msg}");

        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.jsonl");
        std::fs::write(&big, vec![b'a'; MAX_BYTES as usize + 1]).unwrap();
        assert!(format!("{:#}", read_path(&big).unwrap_err()).contains("超過 64 KiB"));

        let many = dir.path().join("many.jsonl");
        std::fs::write(&many, "\n".repeat(MAX_LINES + 1)).unwrap();
        assert!(format!("{:#}", read_path(&many).unwrap_err()).contains("超過 200 行"));

        let fine = dir.path().join("ok.jsonl");
        std::fs::write(&fine, "\n").unwrap();
        assert_eq!(read_path(&fine).unwrap(), "\n");

        let bad = dir.path().join("bad.jsonl");
        std::fs::write(&bad, [0xff, 0xfe]).unwrap();
        assert!(format!("{:#}", read_path(&bad).unwrap_err()).contains("不是 UTF-8"));
    }

    #[test]
    fn the_off_switch_is_only_zero() {
        assert!(!disabled());
    }
}
