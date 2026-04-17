//! Daemon tick loop — Phase 2a.
//!
//! Owns all derived-state writes (pet_snapshot, combat_log, game_world).
//! Runs under a tokio runtime; driven by the CLI subcommand in P5.
//!
//! Design per `.claude/rpg-engine-spec.md`:
//! - 60s tick interval
//! - On startup: catch up missed ticks with 240 cap + sqrt tail
//! - Burst pacing: 100ms per catch-up tick (yield to runtime)
//! - Panic safety: `last_tick_at` only advances after a tick body succeeds

pub mod catchup;
pub mod tick;

use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tokio::time::{interval, MissedTickBehavior};

/// Default tick interval (seconds). Live daemon uses this; tests inject shorter.
pub const TICK_INTERVAL_SECS: u64 = 60;

/// Pacing between catch-up ticks on startup (milliseconds). Yields to runtime
/// so the daemon stays responsive to shutdown while burning through a long gap.
pub const CATCHUP_BURST_MS: u64 = 100;

/// Run the daemon tick loop until `shutdown` is notified.
///
/// On entry: performs startup catch-up for ticks missed since `last_tick_at`.
/// Then ticks every `tick_interval` until shutdown.
pub async fn run_tick_loop(
    conn: &mut Connection,
    tick_interval: Duration,
    shutdown: Arc<Notify>,
) -> Result<()> {
    startup_catchup(conn).await?;

    let mut ticker = interval(tick_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Burst);
    // First call to tick() completes immediately — skip it so the live loop
    // doesn't double-tick right after the catchup phase.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                tick::run_one(conn)?;
            }
            _ = shutdown.notified() => {
                return Ok(());
            }
        }
    }
}

async fn startup_catchup(conn: &mut Connection) -> Result<()> {
    let now = now_unix_secs();
    let last_tick = read_last_tick_at(conn)?;

    // Fresh install: seed the anchor without replaying; avoids a flurry of
    // bogus "catch-up" ticks from `now` to `epoch`.
    let last_tick = match last_tick {
        Some(t) => t,
        None => {
            seed_last_tick_at(conn, now)?;
            return Ok(());
        }
    };

    let elapsed = now.saturating_sub(last_tick);
    let missed = elapsed / TICK_INTERVAL_SECS;
    let effective = catchup::compute_effective_ticks(missed);

    for _ in 0..effective {
        tick::run_one(conn)?;
        tokio::time::sleep(Duration::from_millis(CATCHUP_BURST_MS)).await;
    }

    Ok(())
}

fn read_last_tick_at(conn: &Connection) -> Result<Option<u64>> {
    match conn.query_row(
        "SELECT tick_at FROM last_tick_at WHERE id = 1",
        [],
        |r| r.get::<_, i64>(0),
    ) {
        Ok(v) => Ok(Some(v as u64)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn seed_last_tick_at(conn: &Connection, now: u64) -> Result<()> {
    conn.execute(
        "INSERT INTO last_tick_at (id, tick_at, tick_count) VALUES (1, ?1, 0)",
        rusqlite::params![now as i64],
    )?;
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

    #[tokio::test]
    async fn fresh_db_seeds_anchor_without_tick_replay() {
        let mut conn = fresh_conn();
        startup_catchup(&mut conn).await.unwrap();

        let count: i64 = conn.query_row(
            "SELECT tick_count FROM last_tick_at WHERE id = 1",
            [],
            |r| r.get(0),
        ).unwrap();
        // Seeded with count=0; no ticks replayed for a fresh install
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn startup_catchup_replays_missed_ticks() {
        let mut conn = fresh_conn();
        // Anchor set to 10 minutes ago → 10 missed ticks (below cap, full replay)
        let ten_min_ago = now_unix_secs() - 600;
        conn.execute(
            "INSERT INTO last_tick_at (id, tick_at, tick_count) VALUES (1, ?1, 5)",
            rusqlite::params![ten_min_ago as i64],
        ).unwrap();

        startup_catchup(&mut conn).await.unwrap();

        let count: i64 = conn.query_row(
            "SELECT tick_count FROM last_tick_at WHERE id = 1",
            [],
            |r| r.get(0),
        ).unwrap();
        // 10 catch-up ticks on top of the seeded 5 → 15
        assert_eq!(count, 15);
    }

    #[tokio::test]
    async fn tick_loop_honors_shutdown() {
        let mut conn = fresh_conn();
        // Seed anchor so catch-up is a no-op
        seed_last_tick_at(&conn, now_unix_secs()).unwrap();

        let shutdown = Arc::new(Notify::new());
        let shutdown_trigger = shutdown.clone();

        // Spawn shutdown notifier after a few ms
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            shutdown_trigger.notify_one();
        });

        // 10ms tick interval ensures at least one tick fires before shutdown
        let result = run_tick_loop(
            &mut conn,
            Duration::from_millis(10),
            shutdown,
        ).await;

        assert!(result.is_ok(), "tick loop should exit cleanly on shutdown");

        let count: i64 = conn.query_row(
            "SELECT tick_count FROM last_tick_at WHERE id = 1",
            [],
            |r| r.get(0),
        ).unwrap();
        // At least one tick should have fired in 50ms with 10ms interval
        assert!(count >= 1, "expected ≥1 tick, got {}", count);
    }
}
