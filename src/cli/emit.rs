//! `codeforge emit` — hooks push events into event_inbox.
//!
//! Invoked by Claude Code hook scripts on session_start / session_end /
//! file_saved / git_commit / etc. Writes one row to event_inbox with the
//! event name plus any extra key=value fields baked into a JSON payload.
//!
//! Daemon-down durability: this command does NOT require the daemon to be
//! running. It opens the DB, runs migrations (idempotent), INSERTs, exits.
//! When the daemon is next up, it drains queued events.

use crate::db::Context;
use anyhow::{anyhow, Context as AnyhowContext, Result};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hook events are metadata blobs — not content. 64KiB upper bound
/// protects the inbox from runaway hook scripts or malformed JSON.
pub const MAX_PAYLOAD_BYTES: usize = 65_536;

/// Emit an event.
///
/// Modes:
/// - `--json '{"event":"foo","extra":"bar"}'`: raw payload, stored as-is
/// - `event_name` + optional `fields`: payload built as
///   `{"event":"<event_name>","ts":<unix_ts>,<fields>}`
pub fn run(
    ctx: &Context,
    event: Option<String>,
    fields: Vec<String>,
    json: Option<String>,
) -> Result<()> {
    let payload = match (json, event) {
        (Some(j), _) => {
            if j.len() > MAX_PAYLOAD_BYTES {
                return Err(anyhow!(
                    "--json payload 超過 {}B 上限（實際 {}B）",
                    MAX_PAYLOAD_BYTES,
                    j.len()
                ));
            }
            // Validate it parses as an object (catches typos early)
            let parsed: serde_json::Value = serde_json::from_str(&j)
                .with_context(|| format!("--json 參數不是合法 JSON：{}", j))?;
            if !parsed.is_object() {
                return Err(anyhow!("--json 必須是 JSON object，得到：{}", parsed));
            }
            j
        }
        (None, Some(name)) => {
            let built = build_payload(&name, &fields)?;
            if built.len() > MAX_PAYLOAD_BYTES {
                return Err(anyhow!(
                    "組裝後 payload 超過 {}B 上限（實際 {}B）",
                    MAX_PAYLOAD_BYTES,
                    built.len()
                ));
            }
            built
        }
        (None, None) => {
            return Err(anyhow!("必須指定 event 名稱或 --json payload"));
        }
    };

    // Ensure DB exists + migrations applied (hook may fire before `codeforge init`)
    let conn = ctx.open_db()?;
    crate::db::migrations::run(&conn)?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    conn.execute(
        "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
        rusqlite::params![payload, now],
    )?;

    Ok(())
}

fn build_payload(event: &str, fields: &[String]) -> Result<String> {
    let mut obj = serde_json::Map::new();
    obj.insert("event".into(), serde_json::Value::String(event.to_string()));
    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    obj.insert("ts".into(), serde_json::Value::Number(ts.into()));

    for field in fields {
        let (k, v) = field
            .split_once('=')
            .ok_or_else(|| anyhow!("--field 必須是 KEY=VALUE，得到：{}", field))?;
        obj.insert(k.to_string(), parse_value(v));
    }

    Ok(serde_json::to_string(&obj)?)
}

/// Best-effort parse: integers → Number, other → String.
/// Hooks rarely need nested JSON; for that, use --json.
fn parse_value(raw: &str) -> serde_json::Value {
    if let Ok(n) = raw.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    serde_json::Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_payload_includes_event_and_ts() {
        let s = build_payload("session_start", &[]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["event"], "session_start");
        assert!(v["ts"].is_number());
    }

    #[test]
    fn build_payload_parses_fields() {
        let fields = vec!["files=7".to_string(), "sha=abc123".to_string()];
        let s = build_payload("git_commit", &fields).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["event"], "git_commit");
        assert_eq!(v["files"], 7); // parsed as integer
        assert_eq!(v["sha"], "abc123"); // kept as string
    }

    #[test]
    fn build_payload_rejects_bad_field() {
        let fields = vec!["no_equals_sign".to_string()];
        let err = build_payload("x", &fields).unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn build_payload_roundtrip_under_limit() {
        // Normal-sized payload with ~10 fields fits well under cap
        let fields: Vec<String> = (0..10).map(|i| format!("k{i}=v{i}")).collect();
        let s = build_payload("x", &fields).unwrap();
        assert!(s.len() < MAX_PAYLOAD_BYTES);
    }

    #[test]
    fn parse_value_numbers_and_strings() {
        assert_eq!(parse_value("42"), serde_json::Value::Number(42.into()));
        assert_eq!(parse_value("-5"), serde_json::Value::Number((-5i64).into()));
        assert_eq!(parse_value("abc"), serde_json::Value::String("abc".into()));
        // Decimals stay as string — keep the integer/string split simple
        assert_eq!(
            parse_value("3.14"),
            serde_json::Value::String("3.14".into())
        );
    }
}
