//! Hourly Claude usage (`codeforge usage export --hourly`).
//!
//! One JSON document, `codeforge.usage-hourly/1`: per UTC hour, per account, per model,
//! per speed. Read-only over `requests`. No email, name, org, project or path.

use super::{alias::valid_alias, collect, summary};
use anyhow::{bail, Context as _, Result};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Error};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "codeforge.usage-hourly/1";
const MAX_ROWS: usize = 100_000;
/// Milliseconds in one UTC day. Same value as `collect`'s private `DAY_MS`.
const DAY_MS: i64 = 86_400_000;
const TOP_KEYS: [&str; 4] = ["schema", "generated_at_ms", "host", "rows"];
const ROW_KEYS: [&str; 10] = [
    "hour",
    "account",
    "model",
    "speed",
    "input",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "output",
    "requests",
];
const SPEEDS: [&str; 2] = ["standard", "fast"];
const COUNT_KEYS: [&str; 6] = [
    "input",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "output",
    "requests",
];

struct Group {
    hour: Option<String>,
    account: String,
    model: String,
    speed: String,
    /// input, cache_write_5m, cache_write_1h, cache_read, output, requests.
    counts: [u64; 6],
}

/// Build `codeforge.usage-hourly/1` from Claude rows still inside the 35-day horizon.
///
/// `host` is already `summary::host_alias` (never a hostname). `now_ms` is written as
/// `generated_at_ms` and is the end of the horizon (`collect::within`: not older than
/// [`collect::HORIZON_DAYS`]).
pub fn build(conn: &Connection, host: &str, now_ms: i64) -> Result<Value> {
    let cutoff = now_ms.saturating_sub(collect::HORIZON_DAYS.saturating_mul(DAY_MS));
    let mut st = conn.prepare(
        "SELECT strftime('%Y-%m-%dT%H', seen_ms / 1000, 'unixepoch'),
                account, model, speed,
                SUM(input), SUM(cw5m), SUM(cw1h), SUM(read), SUM(output), COUNT(*)
         FROM requests
         WHERE source = 'claude' AND seen_ms >= ?1
         GROUP BY 1, account, model, speed",
    )?;
    let grouped = st
        .query_map([cutoff], |row| {
            Ok(Group {
                hour: row.get(0)?,
                account: row.get(1)?,
                model: row.get(2)?,
                speed: row.get(3)?,
                counts: [
                    count_at(row, 4)?,
                    count_at(row, 5)?,
                    count_at(row, 6)?,
                    count_at(row, 7)?,
                    count_at(row, 8)?,
                    count_at(row, 9)?,
                ],
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Normalize the model id, then sum again: two off-grammar ids in one bucket become one
    // `unknown` row. The map's key order is the document order (hour, account, model, speed);
    // the unattributed sentinel is `""`, so it sorts before any alias.
    let mut acc: BTreeMap<(String, String, String, String), [u64; 6]> = BTreeMap::new();
    for g in grouped {
        let Some(hour) = g.hour else {
            bail!("有一筆用量的小時無法計算");
        };
        let model = collect::model_id(Some(g.model.as_str()));
        let slot = acc.entry((hour, g.account, model, g.speed)).or_default();
        for (dst, src) in slot.iter_mut().zip(g.counts) {
            *dst = dst.saturating_add(src).min(summary::UINT_MAX);
        }
    }
    let rows: Vec<Value> = acc
        .into_iter()
        .map(|((hour, account, model, speed), c)| {
            let account = if account == collect::UNATTRIBUTED {
                Value::Null
            } else {
                Value::String(account)
            };
            json!({
                "hour": hour,
                "account": account,
                "model": model,
                "speed": speed,
                "input": c[0],
                "cache_write_5m": c[1],
                "cache_write_1h": c[2],
                "cache_read": c[3],
                "output": c[4],
                "requests": c[5],
            })
        })
        .collect();
    Ok(json!({
        "schema": SCHEMA,
        "generated_at_ms": now_ms,
        "host": host,
        "rows": rows,
    }))
}

/// Keys, types and values. Anything else is refused. Rejected text is never echoed.
pub fn validate(v: &Value) -> Result<()> {
    let top = obj(v, &TOP_KEYS, "hourly")?;
    if top["schema"] != SCHEMA {
        bail!("hourly.schema: 不是 {SCHEMA}");
    }
    uint(&top["generated_at_ms"], "hourly.generated_at_ms")?;
    if !top["host"].as_str().is_some_and(is_host) {
        bail!("hourly.host: 不是 host-<8 hex>");
    }
    let rows = top["rows"].as_array().context("hourly.rows: 不是陣列")?;
    if rows.len() > MAX_ROWS {
        bail!("hourly.rows: 超過 {MAX_ROWS} 筆");
    }
    for (i, r) in rows.iter().enumerate() {
        let at = format!("hourly.rows[{i}]");
        let m = obj(r, &ROW_KEYS, &at)?;
        if !m["hour"].as_str().is_some_and(is_hour) {
            bail!("{at}.hour: 不是 UTC 小時（YYYY-MM-DDTHH）");
        }
        if !(m["account"].is_null() || m["account"].as_str().is_some_and(valid_alias)) {
            bail!("{at}.account: 不是合法別名，也不是 null（未歸屬）");
        }
        let model_ok = match m["model"].as_str() {
            Some(s) => collect::model_id(Some(s)) == s,
            None => false,
        };
        if !model_ok {
            bail!("{at}.model: 不符合 model id 格式");
        }
        if !m["speed"].as_str().is_some_and(|s| SPEEDS.contains(&s)) {
            bail!("{at}.speed: 不是 standard/fast");
        }
        for k in COUNT_KEYS {
            uint(&m[k], &format!("{at}.{k}"))?;
        }
    }
    Ok(())
}

fn count_at(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<u64> {
    Ok(match row.get_ref(idx)? {
        ValueRef::Integer(n) => u64::try_from(n).unwrap_or(0).min(summary::UINT_MAX),
        ValueRef::Real(n) => cap_real(n),
        ValueRef::Null => 0,
        other => {
            return Err(Error::InvalidColumnType(
                idx,
                "sum".to_string(),
                other.data_type(),
            ));
        }
    })
}

/// SQLite promotes an overflowing integer SUM to REAL. Cap it at the contract bound.
fn cap_real(n: f64) -> u64 {
    if !n.is_finite() || n <= 0.0 {
        0
    } else if n >= summary::UINT_MAX as f64 {
        summary::UINT_MAX
    } else {
        n as u64
    }
}

fn obj<'a>(v: &'a Value, keys: &[&str], at: &str) -> Result<&'a Map<String, Value>> {
    let Some(m) = v.as_object() else {
        bail!("{at}: 不是物件");
    };
    // the unexpected key itself is never echoed: it is exactly what must not leave the machine
    let extra = m.keys().filter(|k| !keys.contains(&k.as_str())).count();
    if extra > 0 {
        bail!("{at}: 有 {extra} 個不允許的欄位");
    }
    for k in keys {
        if !m.contains_key(*k) {
            bail!("{at}: 缺少欄位 `{k}`");
        }
    }
    Ok(m)
}

fn uint(v: &Value, at: &str) -> Result<()> {
    if v.as_u64().is_none_or(|n| n > summary::UINT_MAX) {
        bail!("{at}: 不是 0..=2^53-1 的整數");
    }
    Ok(())
}

/// `YYYY-MM-DDTHH` exactly. Chrono will not parse an hour with no minutes, so the
/// check appends `:00:00` and parses a full timestamp. The length is in chars (ASCII).
fn is_hour(s: &str) -> bool {
    if s.chars().count() != 13 {
        return false;
    }
    let mut full = String::with_capacity(19);
    full.push_str(s);
    full.push_str(":00:00");
    chrono::NaiveDateTime::parse_from_str(&full, "%Y-%m-%dT%H:%M:%S").is_ok()
}

fn is_host(s: &str) -> bool {
    s.strip_prefix("host-").is_some_and(super::quota::is_hash8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::store;

    const HOST: &str = "host-0123abcd";

    struct Ins<'a> {
        id: &'a str,
        source: &'a str,
        account: &'a str,
        model: &'a str,
        project: &'a str,
        speed: &'a str,
        seen_ms: i64,
        input: i64,
        output: i64,
        cw5m: i64,
        cw1h: i64,
        read: i64,
    }

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let (conn, _) = store::open(&store::usage_dir(dir.path())).unwrap();
        (dir, conn)
    }

    fn ins(conn: &Connection, r: &Ins<'_>) {
        conn.execute(
            "INSERT INTO requests VALUES (?1, '2026-11-04', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                r.id, r.source, r.account, r.model, r.project, r.speed, r.input, r.output, r.cw5m,
                r.cw1h, r.read, r.seen_ms
            ],
        )
        .unwrap();
    }

    fn ms(rfc3339: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .timestamp_millis()
    }

    fn counts(r: &Value) -> [u64; 6] {
        let n = |k: &str| r[k].as_u64().unwrap();
        [
            n("input"),
            n("cache_write_5m"),
            n("cache_write_1h"),
            n("cache_read"),
            n("output"),
            n("requests"),
        ]
    }

    #[test]
    fn sums_one_hour_splits_hours_nulls_unattributed_and_hides_project() {
        let (_dir, conn) = open();
        let now = ms("2026-11-04T15:00:00Z");
        let h13a = ms("2026-11-04T13:10:00Z");
        let h13b = ms("2026-11-04T13:50:00Z");
        let h14 = ms("2026-11-04T14:00:00Z");
        let cutoff = now - collect::HORIZON_DAYS * DAY_MS;
        let acct = "acct-1a2b3c4d";
        let model = "claude-opus-4-8";
        // two requests, same hour / account / model / speed, different projects: one summed row
        ins(
            &conn,
            &Ins {
                id: "a",
                source: "claude",
                account: acct,
                model,
                project: "repo-alpha",
                speed: "standard",
                seen_ms: h13a,
                input: 10,
                output: 1,
                cw5m: 2,
                cw1h: 3,
                read: 4,
            },
        );
        ins(
            &conn,
            &Ins {
                id: "b",
                source: "claude",
                account: acct,
                model,
                project: "user@example.com",
                speed: "standard",
                seen_ms: h13b,
                input: 7,
                output: 2,
                cw5m: 5,
                cw1h: 6,
                read: 8,
            },
        );
        // a later hour stays its own row
        ins(
            &conn,
            &Ins {
                id: "c",
                source: "claude",
                account: acct,
                model,
                project: "repo-alpha",
                speed: "standard",
                seen_ms: h14,
                input: 3,
                output: 4,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // the unattributed sentinel becomes null
        ins(
            &conn,
            &Ins {
                id: "d",
                source: "claude",
                account: collect::UNATTRIBUTED,
                model,
                project: "repo-alpha",
                speed: "standard",
                seen_ms: h13a,
                input: 5,
                output: 6,
                cw5m: 0,
                cw1h: 0,
                read: 1,
            },
        );
        // two off-grammar models collapse to one `unknown` row
        ins(
            &conn,
            &Ins {
                id: "e",
                source: "claude",
                account: acct,
                model: "not a model",
                project: "repo-alpha",
                speed: "fast",
                seen_ms: h13a,
                input: 8,
                output: 9,
                cw5m: 1,
                cw1h: 0,
                read: 0,
            },
        );
        ins(
            &conn,
            &Ins {
                id: "f",
                source: "claude",
                account: acct,
                model: "also/bad",
                project: "secret-proj",
                speed: "fast",
                seen_ms: h13b,
                input: 4,
                output: 5,
                cw5m: 0,
                cw1h: 1,
                read: 2,
            },
        );
        // exactly at the horizon is kept; one millisecond older is not
        ins(
            &conn,
            &Ins {
                id: "edge",
                source: "claude",
                account: "acct-edge0001",
                model,
                project: "edge-proj",
                speed: "standard",
                seen_ms: cutoff,
                input: 11,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        ins(
            &conn,
            &Ins {
                id: "old",
                source: "claude",
                account: "acct-oldrow01",
                model: "claude-old-model",
                project: "old-proj",
                speed: "standard",
                seen_ms: cutoff - 1,
                input: 777,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // Codex has no per-request rows in this report
        ins(
            &conn,
            &Ins {
                id: "codex",
                source: "codex",
                account: acct,
                model: "gpt-5.5-codex",
                project: "codex-proj",
                speed: "standard",
                seen_ms: h13a,
                input: 123_456,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );

        let v = build(&conn, HOST, now).unwrap();
        validate(&v).unwrap();
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(v["generated_at_ms"], now);
        assert_eq!(v["host"], HOST);
        let text = v.to_string();
        for secret in [
            "@",
            "repo-alpha",
            "secret-proj",
            "edge-proj",
            "old-proj",
            "codex-proj",
            "not a model",
            "also/bad",
            "claude-old-model",
            "gpt-5.5-codex",
            "user@example.com",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        let rows = v["rows"].as_array().unwrap();
        let keys: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r["hour"].as_str().unwrap().to_string(),
                    r["account"].as_str().map(str::to_string),
                    r["model"].as_str().unwrap().to_string(),
                    r["speed"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                (
                    "2026-09-30T15".into(),
                    Some("acct-edge0001".into()),
                    model.into(),
                    "standard".into()
                ),
                (
                    "2026-11-04T13".into(),
                    None,
                    model.into(),
                    "standard".into()
                ),
                (
                    "2026-11-04T13".into(),
                    Some(acct.into()),
                    model.into(),
                    "standard".into()
                ),
                (
                    "2026-11-04T13".into(),
                    Some(acct.into()),
                    "unknown".into(),
                    "fast".into()
                ),
                (
                    "2026-11-04T14".into(),
                    Some(acct.into()),
                    model.into(),
                    "standard".into()
                ),
            ]
        );
        assert_eq!(counts(&rows[0]), [11, 0, 0, 0, 0, 1]);
        assert_eq!(counts(&rows[1]), [5, 0, 0, 1, 6, 1]);
        assert_eq!(counts(&rows[2]), [17, 7, 9, 12, 3, 2]);
        assert_eq!(counts(&rows[3]), [12, 1, 1, 2, 14, 2]);
        assert_eq!(counts(&rows[4]), [3, 0, 0, 0, 4, 1]);
        assert!(rows
            .iter()
            .all(|r| r["input"] != 777 && r["input"] != 123_456));
    }

    #[test]
    fn validate_rejects_an_extra_key_a_bad_hour_and_an_email_account() {
        let (_dir, conn) = open();
        let now = ms("2026-11-04T15:00:00Z");
        let empty = build(&conn, HOST, now).unwrap();
        assert!(empty["rows"].as_array().unwrap().is_empty());
        validate(&empty).unwrap();
        ins(
            &conn,
            &Ins {
                id: "a",
                source: "claude",
                account: "acct-1a2b3c4d",
                model: "claude-opus-4-8",
                project: "repo-alpha",
                speed: "standard",
                seen_ms: ms("2026-11-04T13:00:00Z"),
                input: 1,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        let v = build(&conn, HOST, now).unwrap();
        validate(&v).unwrap();

        let mut extra = v.clone();
        extra["rows"][0]["project"] = json!("repo-alpha");
        let e = validate(&extra).unwrap_err().to_string();
        assert!(e.contains("不允許的欄位"), "{e}");
        assert!(!e.contains("repo-alpha") && !e.contains('@'), "{e}");

        let mut hour = v.clone();
        hour["rows"][0]["hour"] = json!("2026-11-04 13");
        assert!(validate(&hour).is_err());
        hour["rows"][0]["hour"] = json!("2026-11-04T25");
        assert!(validate(&hour).is_err());

        let mut email = v.clone();
        email["rows"][0]["account"] = json!("me@example.com");
        let e = validate(&email).unwrap_err().to_string();
        assert!(e.contains("不是合法別名"), "{e}");
        assert!(!e.contains('@') && !e.contains("example.com"), "{e}");

        let mut top = v.clone();
        top["email"] = json!("me@example.com");
        let e = validate(&top).unwrap_err().to_string();
        assert!(e.contains("不允許的欄位"), "{e}");
        assert!(!e.contains('@'), "{e}");
    }
}
