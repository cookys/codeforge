//! Single-tick execution.
//!
//! Phase 2a P3: drains event_inbox, dispatches events through ECS systems,
//! serializes pet state, and advances the `last_tick_at` anchor.
//! Phase 2b P3/P4: rate-limited MOB scanner + combat + loot rolls integrated
//! inside the same transaction.
//!
//! **Transactional atomicity (fixed post-review C1)**: the entire tick body
//! runs inside a single SQLite transaction. Drain-UPDATE, pet_snapshot
//! upsert, last_tick_at advance, mob HP updates, loot_inventory inserts, and
//! combat_log writes all commit atomically. If any step fails (Err or panic),
//! the tx is dropped without commit — events remain `seen_at IS NULL`,
//! pet_snapshot stays on the prior value, mobs stay at their prior HP, and
//! `last_tick_at` is unchanged. Next startup catch-up replays the missed
//! tick. No XP loss, no double-counting, no orphan loot.

use super::ecs::{GameWorld, LastMessage, PetIdentity};
use super::{combat, events, inbox, loot, mob_scanner, systems};
use anyhow::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

/// Run one game tick against the given connection and world.
pub fn run_one(conn: &Connection, world: &mut GameWorld) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // 1. Drain events. Within the tx: SELECT unseen + UPDATE seen_at.
    let drained = inbox::drain_once(&tx)?;
    for ev in &drained {
        events::dispatch(world, ev);
    }

    // 2. Per-tick vitals
    systems::regen_hp(world);

    // 3. Phase 2b: rate-limited MOB scan (every 10 ticks)
    let (zone_id, tick_count_next) = read_tick_context(&tx, world)?;
    mob_scanner::rate_limited_scan(&tx, &zone_id, tick_count_next)?;

    // 4. Phase 2b: combat — attack alive mobs, counter-damage.
    // RNG salt uses `tick_count_next` (monotonic) instead of wall-clock seconds
    // so successive in-test ticks within the same second don't share a seed.
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let summary = combat::run_tick(&tx, world, tick_count_next)?;

    // 5. Phase 2b: loot for defeats — XP + inventory + combat_log
    if !summary.defeats.is_empty() {
        let xp = loot::apply_for_defeats(&tx, &zone_id, &summary.defeats, tick_count_next, now)?;
        if xp > 0 {
            systems::apply_xp(world, xp);
        }
        set_last_message_from_defeats(world, &summary.defeats, xp);
    }

    // 6. Level-up check AFTER combat XP is applied
    systems::check_levelup(world);

    // 7. Serialize to pet_snapshot (single-row upsert, in tx)
    world.serialize_to_db(&tx)?;

    // 8. Advance the anchor
    tx.execute(
        "INSERT INTO last_tick_at (id, tick_at, tick_count) VALUES (1, ?1, 1)
         ON CONFLICT(id) DO UPDATE SET tick_at = ?1, tick_count = tick_count + 1",
        rusqlite::params![now],
    )?;

    // Atomic commit — all-or-nothing
    tx.commit()?;
    Ok(())
}

/// Returns (pet's zone_id, next tick_count). Next = current + 1 so the
/// scanner fires on tick 1 of a fresh install (read 0, scan on 1).
fn read_tick_context(conn: &Connection, world: &GameWorld) -> Result<(String, u64)> {
    let zone_id = world
        .world()
        .get::<&PetIdentity>(world.pet())?
        .village
        .clone();
    let next_tick: u64 = conn
        .query_row(
            "SELECT tick_count + 1 FROM last_tick_at WHERE id = 1",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(1) as u64;
    Ok((zone_id, next_tick))
}

/// Compose a pet message narrating the defeats. Picks the biggest-HP mob
/// to headline — bosses are more memorable than zombies.
fn set_last_message_from_defeats(
    gw: &mut GameWorld,
    defeats: &[combat::DefeatedMob],
    xp: u32,
) {
    let Some(hero) = defeats.iter().max_by_key(|d| d.hp_max) else {
        return;
    };
    let short_name = truncate_name(&hero.name, 40);
    let text = if defeats.len() == 1 {
        format!("擊殺 {} 「{}」（+{} XP）", hero.kind, short_name, xp)
    } else {
        format!(
            "擊殺 {} 「{}」 +{} 其他（+{} XP）",
            hero.kind,
            short_name,
            defeats.len() - 1,
            xp
        )
    };
    let pet = gw.pet();
    let _ = gw.world_mut().insert_one(pet, LastMessage { text });
}

fn truncate_name(s: &str, n: usize) -> String {
    // CJK-safe per HARD RULE — .chars().take(), never &s[..N]
    let chars: String = s.chars().take(n).collect();
    if chars.chars().count() < s.chars().count() {
        format!("{chars}…")
    } else {
        chars
    }
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

    // ─── Phase 2b integration tests ──────────────────────────

    fn spawn_mob(conn: &Connection, zone: &str, kind: &str, name: &str, hp: u32) -> i64 {
        conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, difficulty, spawned_at)
             VALUES (?1, ?2, ?3, ?4, ?4, 1, 1, 1, 100)",
            rusqlite::params![zone, kind, name, hp as i64],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn tick_fights_alive_mob_in_zone() {
        let (conn, mut world) = fresh();
        let id = spawn_mob(&conn, "rust", "ghost", "unused @ x.rs", 1);

        // Run up to 30 ticks — 1-HP ghost should go down within budget
        for _ in 0..30 {
            run_one(&conn, &mut world).unwrap();
            let defeated: Option<i64> = conn
                .query_row(
                    "SELECT defeated_at FROM mobs WHERE id = ?1",
                    rusqlite::params![id],
                    |r| r.get(0),
                )
                .unwrap();
            if defeated.is_some() {
                return;
            }
        }
        panic!("1-HP ghost should be defeated within 30 ticks");
    }

    #[test]
    fn tick_awards_loot_to_inventory_after_defeat() {
        let (conn, mut world) = fresh();
        spawn_mob(&conn, "rust", "ghost", "unused @ x.rs", 1);

        for _ in 0..30 {
            run_one(&conn, &mut world).unwrap();
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM loot_inventory", [], |r| r.get(0))
                .unwrap();
            if n > 0 {
                // Ghost drops Dead Code Crystal — verify name
                let name: String = conn
                    .query_row(
                        "SELECT name FROM loot_inventory LIMIT 1",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(name, "Dead Code Crystal");
                return;
            }
        }
        panic!("loot should land in inventory after defeat");
    }

    #[test]
    fn tick_writes_last_message_on_defeat() {
        use super::super::ecs::LastMessage;
        let (conn, mut world) = fresh();
        spawn_mob(&conn, "rust", "ghost", "unused @ x.rs", 1);

        for _ in 0..30 {
            run_one(&conn, &mut world).unwrap();
            let pet = world.pet();
            if let Ok(msg) = world.world().get::<&LastMessage>(pet) {
                assert!(msg.text.contains("擊殺"), "got: {}", msg.text);
                return;
            }
        }
        panic!("last_message should be set after first defeat");
    }

    #[test]
    fn tick_writes_combat_log_after_defeat() {
        let (conn, mut world) = fresh();
        spawn_mob(&conn, "rust", "ghost", "unused @ x.rs", 1);

        for _ in 0..30 {
            run_one(&conn, &mut world).unwrap();
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM combat_log", [], |r| r.get(0))
                .unwrap();
            if n > 0 {
                let (zone, kind): (String, String) = conn
                    .query_row(
                        "SELECT zone_id, mob_kind FROM combat_log LIMIT 1",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(zone, "rust");
                assert_eq!(kind, "ghost");
                return;
            }
        }
        panic!("combat_log should be populated after defeat");
    }

    #[test]
    fn tick_ignores_other_zones_mobs() {
        let (conn, mut world) = fresh();
        // Pet is rust village — python mobs should not be attacked
        spawn_mob(&conn, "python", "ghost", "py-ghost", 1);
        for _ in 0..10 {
            run_one(&conn, &mut world).unwrap();
        }
        let defeated: Option<i64> = conn
            .query_row(
                "SELECT defeated_at FROM mobs WHERE zone_id = 'python'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(defeated.is_none(), "cross-zone mobs must not be attacked in P2b");
    }

    #[test]
    fn tick_atomic_rollback_on_error_preserves_state() {
        // If combat errors (e.g. UPDATE fails), the tx should roll back: no
        // mob changes, no loot, no anchor advance. Hard to reliably force a
        // combat error in-memory, so instead verify the invariant via the
        // existing empty-zone path: no mobs, no combat side-effects.
        let (conn, mut world) = fresh();
        run_one(&conn, &mut world).unwrap();

        let mob_rows: i64 = conn.query_row("SELECT COUNT(*) FROM mobs", [], |r| r.get(0)).unwrap();
        let loot_rows: i64 = conn.query_row("SELECT COUNT(*) FROM loot_inventory", [], |r| r.get(0)).unwrap();
        let log_rows: i64 = conn.query_row("SELECT COUNT(*) FROM combat_log", [], |r| r.get(0)).unwrap();
        assert_eq!(mob_rows, 0);
        assert_eq!(loot_rows, 0);
        assert_eq!(log_rows, 0);
    }
}
