//! Single-tick execution.
//!
//! Phase 2a P3: drains event_inbox, dispatches events through ECS systems,
//! serializes pet state, and advances the `last_tick_at` anchor.
//!
//! Panic safety: if execution fails partway (panic or Err return), the
//! `last_tick_at` row is NOT updated. On next daemon startup, the catch-up
//! logic replays the missed tick. For events already drained but not
//! committed, the `seen_at` write is atomic per drain, so they won't
//! double-count on replay.

use super::{ecs::GameWorld, events, inbox, systems};
use anyhow::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

/// Run one game tick against the given connection and world.
pub fn run_one(conn: &Connection, world: &mut GameWorld) -> Result<()> {
    // 1. Drain events. drain_once marks rows as seen atomically.
    let drained = inbox::drain_once(conn)?;

    // 2. Dispatch events → ECS state changes
    for ev in &drained {
        events::dispatch(world, ev);
    }

    // 3. Per-tick systems
    systems::regen_hp(world);
    systems::check_levelup(world);

    // 4. Serialize to pet_snapshot (single-row upsert)
    world.serialize_to_db(conn)?;

    // 5. Advance the anchor LAST. If anything above fails, the anchor
    // stays on the previous tick's timestamp, so catch-up replays.
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
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

    fn fresh() -> (Connection, GameWorld) {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        let world = GameWorld::load_or_init(&conn).unwrap();
        (conn, world)
    }

    #[test]
    fn first_tick_inserts_anchor_and_snapshot() {
        let (conn, mut world) = fresh();
        run_one(&conn, &mut world).unwrap();

        let tick_count: i64 = conn
            .query_row("SELECT tick_count FROM last_tick_at WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tick_count, 1);

        let snapshot_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot", [], |r| r.get(0))
            .unwrap();
        assert_eq!(snapshot_rows, 1);
    }

    #[test]
    fn tick_processes_inbox_event_and_awards_xp() {
        let (conn, mut world) = fresh();
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
            rusqlite::params![r#"{"event":"git_commit","sha":"abc"}"#, 100i64],
        )
        .unwrap();

        run_one(&conn, &mut world).unwrap();

        let xp: i64 = conn
            .query_row("SELECT xp FROM pet_snapshot WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(xp, 20);

        // Event marked seen
        let seen: Option<i64> = conn
            .query_row("SELECT seen_at FROM event_inbox WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(seen.is_some());
    }

    #[test]
    fn tick_runs_regen_when_pet_damaged() {
        let (conn, mut world) = fresh();
        let pet = world.pet();
        {
            use super::super::ecs::PetVitals;
            let mut v = world.world_mut().get::<&mut PetVitals>(pet).unwrap();
            v.hp = 1;
        }
        run_one(&conn, &mut world).unwrap();

        let hp: i64 = conn
            .query_row("SELECT hp FROM pet_snapshot WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hp, 1 + super::systems::REGEN_PER_TICK as i64);
    }

    #[test]
    fn subsequent_ticks_increment_count() {
        let (conn, mut world) = fresh();
        run_one(&conn, &mut world).unwrap();
        run_one(&conn, &mut world).unwrap();
        run_one(&conn, &mut world).unwrap();

        let count: i64 = conn
            .query_row("SELECT tick_count FROM last_tick_at WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn anchor_is_single_row() {
        let (conn, mut world) = fresh();
        for _ in 0..10 {
            run_one(&conn, &mut world).unwrap();
        }
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM last_tick_at", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    /// KR: `Tick 計算 < 10ms` per mud-engine spec §設計約束 5.
    /// Not Criterion-quality, but catches regressions orders of magnitude off.
    #[test]
    fn tick_budget_under_10ms_average() {
        let (conn, mut world) = fresh();
        // Prime: first tick pays schema cache costs
        run_one(&conn, &mut world).unwrap();

        const N: u32 = 50;
        let start = std::time::Instant::now();
        for _ in 0..N {
            run_one(&conn, &mut world).unwrap();
        }
        let elapsed = start.elapsed();
        let avg_us = elapsed.as_micros() / N as u128;
        assert!(
            avg_us < 10_000,
            "tick avg {avg_us}µs exceeds 10ms budget (spec §設計約束 5)"
        );
    }

    /// Verifies tick budget holds even with a realistic event burst
    /// (e.g., git_commit after a long idle that queued 50 events).
    #[test]
    fn tick_budget_survives_event_burst() {
        let (conn, mut world) = fresh();
        for i in 0..50 {
            conn.execute(
                "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
                rusqlite::params![r#"{"event":"git_commit"}"#, 100i64 + i],
            )
            .unwrap();
        }

        let start = std::time::Instant::now();
        run_one(&conn, &mut world).unwrap();
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 50,
            "tick with 50-event burst took {elapsed_ms}ms (should be well under tick interval)"
        );
    }
}
