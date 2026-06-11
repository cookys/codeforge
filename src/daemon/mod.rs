//! Daemon tick loop — Phase 2a.
//!
//! Owns all derived-state writes (pet_snapshot, combat_log, game_world).
//! Runs under a tokio runtime; driven by the CLI subcommand in P5.
//!
//! Design per `.claude/rpg-engine-spec.md` + `doc/specs/codeforge-mud-engine.md`:
//! - 60s tick interval
//! - On startup: catch up missed ticks with 240 cap + sqrt tail
//! - Burst pacing: 100ms per catch-up tick (yield to runtime)
//! - Panic safety: `last_tick_at` only advances after a tick body succeeds
//! - ECS World held in memory across ticks; serialized to pet_snapshot each tick

pub mod catchup;
pub mod combat;
pub mod ecs;
pub mod events;
pub mod first_events;
pub mod inbox;
pub mod lifecycle;
pub mod loot;
pub mod mob;
pub mod mob_scanner;
pub mod mood;
pub mod strategy;
pub mod systems;
pub mod tick;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, MissedTickBehavior};

use crate::commentary::budget::OPT_IN_ENV;
use crate::commentary::dispatch;

pub const TICK_INTERVAL_SECS: u64 = 60;
pub const CATCHUP_BURST_MS: u64 = 100;

/// Run the daemon tick loop until `shutdown` signals stop.
///
/// Holds a GameWorld in memory across ticks. Initial load pulls from the
/// Phase 1 `pet` table (CLI-owned) or seeds defaults for fresh installs.
///
/// `db_path` is needed so async commentary dispatches (spawned outside
/// the tick tx, per Phase 3c P4) can open their own SQLite connection.
pub async fn run_tick_loop(
    conn: &mut Connection,
    tick_interval: Duration,
    shutdown: lifecycle::ShutdownGuard,
    db_path: PathBuf,
) -> Result<()> {
    let mut world = ecs::GameWorld::load_or_init(conn)?;

    // Opportunistic retention cleanup on startup — keeps inbox bounded.
    let _ = inbox::cleanup_expired(conn, inbox::RETENTION_SECS);

    startup_catchup(conn, &mut world).await?;

    // Used by the SessionLong commentary trigger. Recorded once here rather
    // than read every tick — daemon uptime is an in-memory fact.
    let session_started_at = now_unix_secs() as i64;

    let mut ticker = interval(tick_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Burst);
    // First call completes immediately — skip so the live loop doesn't
    // double-tick right after the catch-up phase.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let env = tick::TickEnv {
                    session_started_at,
                    emit_commentary: true,
                };
                let pending = tick::run_one_with_env(conn, &mut world, env)?;
                if let Some(pending) = pending {
                    spawn_commentary_dispatch(db_path.clone(), pending);
                }
            }
            _ = shutdown.notified() => {
                // Already checked should_stop() via notified() completing
                return Ok(());
            }
        }
        // After each tick, check if we should exit before waiting for next tick
        if shutdown.should_stop() {
            return Ok(());
        }
    }
}

/// Fire off the async dispatcher. Errors are logged to stderr — we never
/// let a commentary failure crash the daemon. Commentary is cosmetic; the
/// tick already committed, so the game state is intact either way.
fn spawn_commentary_dispatch(db_path: PathBuf, pending: dispatch::PendingDispatch) {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    // Keep the kind for the error log — pending is moved into the future.
    let kind_tag = pending.kind.as_str();
    tokio::spawn(async move {
        if let Err(e) = dispatch::execute_dispatch(db_path, pending, api_key).await {
            eprintln!(
                "codeforge commentary dispatch failed (kind={kind_tag}, opt_in_env={}): {e:#}",
                std::env::var(OPT_IN_ENV).unwrap_or_default()
            );
        }
    });
}

async fn startup_catchup(conn: &mut Connection, world: &mut ecs::GameWorld) -> Result<()> {
    let now = now_unix_secs();
    let last_tick = read_last_tick_at(conn)?;

    let last_tick = match last_tick {
        Some(t) => t,
        None => {
            seed_last_tick_at(conn, now)?;
            // Fresh install: still do one serialization so pet_snapshot exists
            // Fresh install: no LastMessage exists yet, so the TTL check is
            // moot — pass tick=0 as a placeholder.
            world.serialize_to_db(conn, 0)?;
            return Ok(());
        }
    };

    let elapsed = now.saturating_sub(last_tick);
    let missed = elapsed / TICK_INTERVAL_SECS;
    let effective = catchup::compute_effective_ticks(missed);

    for _ in 0..effective {
        tick::run_one(conn, world)?;
        tokio::time::sleep(Duration::from_millis(CATCHUP_BURST_MS)).await;
    }

    Ok(())
}

fn read_last_tick_at(conn: &Connection) -> Result<Option<u64>> {
    match conn.query_row("SELECT tick_at FROM last_tick_at WHERE id = 1", [], |r| {
        r.get::<_, i64>(0)
    }) {
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
    async fn fresh_db_seeds_anchor_and_snapshot_without_tick_replay() {
        let mut conn = fresh_conn();
        let mut world = ecs::GameWorld::load_or_init(&conn).unwrap();
        startup_catchup(&mut conn, &mut world).await.unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT tick_count FROM last_tick_at WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0); // seeded, not replayed

        // Snapshot should exist (fresh-install path serializes once)
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn startup_catchup_replays_missed_ticks_through_ecs() {
        let mut conn = fresh_conn();
        let ten_min_ago = now_unix_secs() - 600;
        conn.execute(
            "INSERT INTO last_tick_at (id, tick_at, tick_count) VALUES (1, ?1, 5)",
            rusqlite::params![ten_min_ago as i64],
        )
        .unwrap();
        // Seed an event that should get processed during catchup
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
            rusqlite::params![r#"{"event":"git_commit"}"#, ten_min_ago as i64],
        )
        .unwrap();

        let mut world = ecs::GameWorld::load_or_init(&conn).unwrap();
        startup_catchup(&mut conn, &mut world).await.unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT tick_count FROM last_tick_at WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 15); // 5 seeded + 10 replayed

        // Event processed in first replay tick — XP should be on pet_snapshot
        let xp: i64 = conn
            .query_row("SELECT xp FROM pet_snapshot WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(xp, 20);
    }

    #[tokio::test]
    async fn tick_loop_honors_shutdown() {
        let mut conn = fresh_conn();
        seed_last_tick_at(&conn, now_unix_secs()).unwrap();

        // ShutdownGuard 模式(C1):flag 為真相、Notify 只是喚醒提示 —— 觸發要兩者都做。
        let shutdown = lifecycle::ShutdownGuard::new();
        let flag = shutdown.should_stop.clone();
        let notify = shutdown.notify.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            notify.notify_one();
        });

        run_tick_loop(
            &mut conn,
            Duration::from_millis(10),
            shutdown,
            PathBuf::from(":memory:"),
        )
        .await
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT tick_count FROM last_tick_at WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 1);
    }
}
