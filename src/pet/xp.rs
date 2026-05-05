//! XP award path for CLI commands (learn/search/ingest) — bridge to Phase 2a.
//!
//! Phase 1: directly updated the `pet` table.
//! Phase 2a P8: emits an `xp_award` event to `event_inbox`. The daemon
//! drains it on the next tick, updates the ECS world, and writes
//! `pet_snapshot`. Meanwhile the CLI read path (`pet::live_state::LiveState`)
//! overlays unseen events so statusline reflects the XP immediately — no
//! daemon-up requirement for UX.
//!
//! Durability: `xp_events` continues to record every award for audit / retro.

use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db;

/// Log an XP award event and queue it for the daemon.
///
/// `amount` may be zero or negative; only positive values are queued
/// (daemon has no XP-loss mechanic in P2a). The `xp_events` log is
/// written regardless — the row is a historical record.
pub fn award(ctx: &db::Context, source: &str, amount: i64, detail: Option<&str>) -> Result<()> {
    let conn = ctx.open_db()?;

    conn.execute(
        "INSERT INTO xp_events (source, xp_delta, detail) VALUES (?1, ?2, ?3)",
        rusqlite::params![source, amount, detail],
    )?;

    if amount > 0 {
        let xp = (amount as u64).min(u32::MAX as u64) as u32;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let payload = serde_json::json!({
            "event": "xp_award",
            "ts": now,
            "source": source,
            "xp": xp,
            "detail": detail,
        })
        .to_string();
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
            rusqlite::params![payload, now],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Context;
    use tempfile::tempdir;

    fn fresh_ctx() -> (tempfile::TempDir, Context) {
        let dir = tempdir().unwrap();
        let ctx = Context {
            project_dir: dir.path().join(".codeforge"),
            brain_dir: dir.path().join("brain"),
            db_path: dir.path().join("state.db"),
        };
        let conn = ctx.open_db().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        (dir, ctx)
    }

    #[test]
    fn positive_award_logs_and_emits() {
        let (_dir, ctx) = fresh_ctx();
        award(&ctx, "learn", 5, Some("topic")).unwrap();

        let conn = ctx.open_db().unwrap();
        let logged: i64 = conn
            .query_row(
                "SELECT xp_delta FROM xp_events WHERE source = 'learn'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logged, 5);

        let payload: String = conn
            .query_row(
                "SELECT payload FROM event_inbox WHERE seen_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["event"], "xp_award");
        assert_eq!(v["xp"], 5);
        assert_eq!(v["source"], "learn");
    }

    #[test]
    fn zero_or_negative_logs_but_does_not_emit() {
        let (_dir, ctx) = fresh_ctx();
        award(&ctx, "search", 0, None).unwrap();
        award(&ctx, "search", -3, None).unwrap();

        let conn = ctx.open_db().unwrap();
        let logged: i64 = conn
            .query_row("SELECT COUNT(*) FROM xp_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logged, 2);
        let emitted: i64 = conn
            .query_row("SELECT COUNT(*) FROM event_inbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(emitted, 0);
    }

    #[test]
    fn large_amount_is_saturated_to_u32_max() {
        let (_dir, ctx) = fresh_ctx();
        award(&ctx, "ingest", i64::MAX, None).unwrap();
        let conn = ctx.open_db().unwrap();
        let payload: String = conn
            .query_row(
                "SELECT payload FROM event_inbox WHERE seen_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["xp"], u32::MAX);
    }
}
