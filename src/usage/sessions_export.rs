//! Per-session Claude usage (`codeforge usage export --sessions`).
//!
//! One JSON document, `codeforge.usage-sessions/1`: per session, per account, per model.
//! Read-only over `requests` with a non-NULL `session`. Codex is not included in v1 (its
//! usage lives in `codex_sessions` without per-request rows keyed for this report). No
//! email, name, org, project or path.

use super::{alias::valid_alias, collect, summary};
use anyhow::{bail, Context as _, Result};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Error};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "codeforge.usage-sessions/1";
pub const FILE: &str = "usage-sessions.json";
/// How far back rows are included (days). Shorter than collect's 35-day horizon.
pub const HORIZON_DAYS: i64 = 8;
const MAX_ROWS: usize = 100_000;
/// Milliseconds in one UTC day. Same value as `collect`'s private `DAY_MS`.
const DAY_MS: i64 = 86_400_000;
const TOP_KEYS: [&str; 4] = ["schema", "generated_at_ms", "host", "rows"];
const ROW_KEYS: [&str; 11] = [
    "session",
    "account",
    "model",
    "first_ms",
    "last_ms",
    "input",
    "output",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "requests",
];
const COUNT_KEYS: [&str; 6] = [
    "input",
    "output",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "requests",
];

struct Group {
    session: String,
    account: String,
    model: String,
    first_ms: i64,
    last_ms: i64,
    /// input, output, cache_write_5m, cache_write_1h, cache_read, requests.
    counts: [u64; 6],
}

/// (session, account, model) → (first_ms, last_ms, counts).
type Acc = BTreeMap<(String, String, String), (i64, i64, [u64; 6])>;

/// Build `codeforge.usage-sessions/1` from Claude rows with a non-NULL session still
/// inside the 8-day horizon.
///
/// `host` is already `summary::host_alias` (never a hostname). `now_ms` is written as
/// `generated_at_ms` and is the end of the horizon.
pub fn build(conn: &Connection, host: &str, now_ms: i64) -> Result<Value> {
    let cutoff = now_ms.saturating_sub(HORIZON_DAYS.saturating_mul(DAY_MS));
    let mut st = conn.prepare(
        "SELECT session, account, model,
                MIN(seen_ms), MAX(seen_ms),
                SUM(input), SUM(output), SUM(cw5m), SUM(cw1h), SUM(read), COUNT(*)
         FROM requests
         WHERE source = 'claude' AND session IS NOT NULL AND seen_ms >= ?1
         GROUP BY session, account, model",
    )?;
    let grouped = st
        .query_map([cutoff], |row| {
            Ok(Group {
                session: row.get(0)?,
                account: row.get(1)?,
                model: row.get(2)?,
                first_ms: row.get(3)?,
                last_ms: row.get(4)?,
                counts: [
                    count_at(row, 5)?,
                    count_at(row, 6)?,
                    count_at(row, 7)?,
                    count_at(row, 8)?,
                    count_at(row, 9)?,
                    count_at(row, 10)?,
                ],
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Normalize the model id, then sum again: two off-grammar ids in one bucket become one
    // `unknown` row. Key order is document order (session, account, model); the unattributed
    // sentinel is `""`, so it sorts before any alias.
    let mut acc: Acc = Acc::new();
    for g in grouped {
        if !usable_session(&g.session) {
            bail!("有一筆用量的 session 不合規格");
        }
        let model = collect::model_id(Some(g.model.as_str()));
        let slot = acc
            .entry((g.session, g.account, model))
            .or_insert((g.first_ms, g.last_ms, [0; 6]));
        slot.0 = slot.0.min(g.first_ms);
        slot.1 = slot.1.max(g.last_ms);
        for (dst, src) in slot.2.iter_mut().zip(g.counts) {
            *dst = dst.saturating_add(src).min(summary::UINT_MAX);
        }
    }
    let rows: Vec<Value> = acc
        .into_iter()
        .map(|((session, account, model), (first_ms, last_ms, c))| {
            let account = if account == collect::UNATTRIBUTED {
                Value::Null
            } else {
                Value::String(account)
            };
            json!({
                "session": session,
                "account": account,
                "model": model,
                "first_ms": first_ms,
                "last_ms": last_ms,
                "input": c[0],
                "output": c[1],
                "cache_write_5m": c[2],
                "cache_write_1h": c[3],
                "cache_read": c[4],
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
    let top = obj(v, &TOP_KEYS, "sessions")?;
    if top["schema"] != SCHEMA {
        bail!("sessions.schema: 不是 {SCHEMA}");
    }
    uint(&top["generated_at_ms"], "sessions.generated_at_ms")?;
    if !top["host"].as_str().is_some_and(is_host) {
        bail!("sessions.host: 不是 host-<8 hex>");
    }
    let rows = top["rows"].as_array().context("sessions.rows: 不是陣列")?;
    if rows.len() > MAX_ROWS {
        bail!("sessions.rows: 超過 {MAX_ROWS} 筆");
    }
    for (i, r) in rows.iter().enumerate() {
        let at = format!("sessions.rows[{i}]");
        let m = obj(r, &ROW_KEYS, &at)?;
        if !m["session"].as_str().is_some_and(usable_session) {
            bail!("{at}.session: 不是合法 session id");
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
        uint(&m["first_ms"], &format!("{at}.first_ms"))?;
        uint(&m["last_ms"], &format!("{at}.last_ms"))?;
        let first = m["first_ms"].as_u64().unwrap_or(0);
        let last = m["last_ms"].as_u64().unwrap_or(0);
        if first > last {
            bail!("{at}: first_ms 大於 last_ms");
        }
        for k in COUNT_KEYS {
            uint(&m[k], &format!("{at}.{k}"))?;
        }
    }
    Ok(())
}

/// Write `usage-sessions.json` next to `summary.json` (0600, tmp + rename).
pub fn write(dir: &std::path::Path, v: &Value) -> Result<std::path::PathBuf> {
    crate::live::write_live_json(dir, FILE, v)
        .map_err(|_| anyhow::anyhow!("無法寫入 usage-sessions.json"))?;
    Ok(dir.join(FILE))
}

/// A session id as pins/statusline accept: 1–64 of `[A-Za-z0-9_-]` (no `@`).
fn usable_session(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
        session: Option<&'a str>,
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
            "INSERT INTO requests
             (request_id, day, source, account, model, project, speed,
              input, output, cw5m, cw1h, read, seen_ms, session)
             VALUES (?1, '2026-11-04', ?2, ?3, ?4, ?5, 'standard',
                     ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                r.id, r.source, r.account, r.model, r.project, r.input, r.output, r.cw5m, r.cw1h,
                r.read, r.seen_ms, r.session
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
            n("output"),
            n("cache_write_5m"),
            n("cache_write_1h"),
            n("cache_read"),
            n("requests"),
        ]
    }

    #[test]
    fn groups_sums_horizon_excludes_null_session_nulls_unattributed_and_orders() {
        let (_dir, conn) = open();
        let now = ms("2026-11-12T15:00:00Z");
        let t1 = ms("2026-11-11T10:00:00Z");
        let t2 = ms("2026-11-11T12:00:00Z");
        let t3 = ms("2026-11-12T01:00:00Z");
        let cutoff = now - HORIZON_DAYS * DAY_MS;
        let acct = "acct-1a2b3c4d";
        let model = "claude-opus-4-8";

        // two requests same session/account/model: one summed row, first/last span
        ins(
            &conn,
            &Ins {
                id: "a",
                source: "claude",
                account: acct,
                model,
                project: "repo-alpha",
                session: Some("sess-a"),
                seen_ms: t1,
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
                session: Some("sess-a"),
                seen_ms: t2,
                input: 7,
                output: 2,
                cw5m: 5,
                cw1h: 6,
                read: 8,
            },
        );
        // another session
        ins(
            &conn,
            &Ins {
                id: "c",
                source: "claude",
                account: acct,
                model,
                project: "repo-alpha",
                session: Some("sess-b"),
                seen_ms: t3,
                input: 3,
                output: 4,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // unattributed → null
        ins(
            &conn,
            &Ins {
                id: "d",
                source: "claude",
                account: collect::UNATTRIBUTED,
                model,
                project: "repo-alpha",
                session: Some("sess-a"),
                seen_ms: t1,
                input: 5,
                output: 6,
                cw5m: 0,
                cw1h: 0,
                read: 1,
            },
        );
        // NULL session excluded
        ins(
            &conn,
            &Ins {
                id: "nullsess",
                source: "claude",
                account: acct,
                model,
                project: "secret-proj",
                session: None,
                seen_ms: t1,
                input: 999,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // exactly at the horizon kept; one ms older dropped
        ins(
            &conn,
            &Ins {
                id: "edge",
                source: "claude",
                account: "acct-edge0001",
                model,
                project: "edge-proj",
                session: Some("sess-edge"),
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
                session: Some("sess-old"),
                seen_ms: cutoff - 1,
                input: 777,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // Codex source excluded even with a session
        ins(
            &conn,
            &Ins {
                id: "codex",
                source: "codex",
                account: acct,
                model: "gpt-5.5-codex",
                project: "codex-proj",
                session: Some("sess-codex"),
                seen_ms: t1,
                input: 123_456,
                output: 0,
                cw5m: 0,
                cw1h: 0,
                read: 0,
            },
        );
        // off-grammar models collapse to unknown
        ins(
            &conn,
            &Ins {
                id: "e",
                source: "claude",
                account: acct,
                model: "not a model",
                project: "repo-alpha",
                session: Some("sess-b"),
                seen_ms: t3,
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
                session: Some("sess-b"),
                seen_ms: t3,
                input: 4,
                output: 5,
                cw5m: 0,
                cw1h: 1,
                read: 2,
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
            "sess-old",
            "sess-codex",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        let rows = v["rows"].as_array().unwrap();
        let keys: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r["session"].as_str().unwrap().to_string(),
                    r["account"].as_str().map(str::to_string),
                    r["model"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                ("sess-a".into(), None, model.into(),),
                ("sess-a".into(), Some(acct.into()), model.into(),),
                ("sess-b".into(), Some(acct.into()), model.into(),),
                ("sess-b".into(), Some(acct.into()), "unknown".into(),),
                (
                    "sess-edge".into(),
                    Some("acct-edge0001".into()),
                    model.into(),
                ),
            ]
        );
        assert_eq!(counts(&rows[0]), [5, 6, 0, 0, 1, 1]);
        assert_eq!(rows[0]["first_ms"], t1);
        assert_eq!(rows[0]["last_ms"], t1);
        assert_eq!(counts(&rows[1]), [17, 3, 7, 9, 12, 2]);
        assert_eq!(rows[1]["first_ms"], t1);
        assert_eq!(rows[1]["last_ms"], t2);
        assert_eq!(counts(&rows[2]), [3, 4, 0, 0, 0, 1]);
        assert_eq!(counts(&rows[3]), [12, 14, 1, 1, 2, 2]);
        assert_eq!(counts(&rows[4]), [11, 0, 0, 0, 0, 1]);
        assert!(rows
            .iter()
            .all(|r| r["input"] != 777 && r["input"] != 999 && r["input"] != 123_456));
    }

    #[test]
    fn validate_rejects_extra_key_bad_session_and_email_account() {
        let (_dir, conn) = open();
        let now = ms("2026-11-12T15:00:00Z");
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
                session: Some("sess-a"),
                seen_ms: ms("2026-11-11T13:00:00Z"),
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

        let mut bad_sess = v.clone();
        bad_sess["rows"][0]["session"] = json!("user@host");
        let e = validate(&bad_sess).unwrap_err().to_string();
        assert!(e.contains("不是合法 session"), "{e}");
        assert!(!e.contains('@'), "{e}");

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
