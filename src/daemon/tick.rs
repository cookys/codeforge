//! Single-tick execution.
//!
//! Phase 2a scope: advance the `last_tick_at` anchor and bump `tick_count`.
//! Future phases will run ECS systems (P3), combat (P2b), etc. inside this
//! function; panic safety guarantee (tick body panic → anchor not advanced)
//! must hold across all additions.

use anyhow::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

/// Run one game tick against the given connection.
///
/// Panic safety: if execution fails partway (panic or Err return), the
/// `last_tick_at` row is NOT updated. On next daemon startup, the catch-up
/// logic replays the missed tick.
pub fn run_one(conn: &Connection) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

    // Upsert single-row anchor. ON CONFLICT needed because the row may not
    // yet exist on first tick of a fresh DB.
    conn.execute(
        "INSERT INTO last_tick_at (id, tick_at, tick_count) VALUES (1, ?1, 1)
         ON CONFLICT(id) DO UPDATE SET tick_at = ?1, tick_count = tick_count + 1",
        rusqlite::params![now],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn first_tick_inserts_anchor() {
        let conn = fresh_conn();
        run_one(&conn).unwrap();

        let (tick_at, count): (i64, i64) = conn.query_row(
            "SELECT tick_at, tick_count FROM last_tick_at WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();

        assert!(tick_at > 0);
        assert_eq!(count, 1);
    }

    #[test]
    fn subsequent_ticks_increment_count() {
        let conn = fresh_conn();
        run_one(&conn).unwrap();
        run_one(&conn).unwrap();
        run_one(&conn).unwrap();

        let count: i64 = conn.query_row(
            "SELECT tick_count FROM last_tick_at WHERE id = 1",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn anchor_is_single_row() {
        // CHECK (id = 1) enforces the singleton; verify we don't create
        // multiple rows even after many ticks.
        let conn = fresh_conn();
        for _ in 0..10 {
            run_one(&conn).unwrap();
        }
        let rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM last_tick_at",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(rows, 1);
    }
}
