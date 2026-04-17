//! Phase 3c — tick→dispatch bridge.
//!
//! Splits commentary processing into two halves:
//!
//! 1. `decide_and_reserve` — runs **inside** the tick's SQLite transaction.
//!    Checks opt-in, runs trigger detection, picks the highest-priority
//!    trigger, verifies the 1/hour budget, and optimistically reserves the
//!    budget slot. Returns a `PendingDispatch` describing what to generate.
//!    Everything here is sync + fast (< 5ms tick-latency target).
//!
//! 2. `execute_dispatch` — `async`, spawned **outside** the tick tx. Opens
//!    its own SQLite connection, tries Haiku if `api_key` is present, falls
//!    back to rule-based otherwise, then writes a `commentary_feed` row +
//!    updates `commentary_history`. Runs completely off the tick path so
//!    an 8-second Haiku timeout never bleeds into daemon responsiveness
//!    (spec §設計約束 865).

use super::generator::{self, GenContext, Generated};
use super::repo::{self, InsertFeed};
use super::trigger::{self, Trigger};
use super::{Kind, Tone};
use crate::daemon::combat::DefeatedMob;
use crate::daemon::ecs::{GameWorld, Mood, PetIdentity, PetLevel, PetStrategy, PetVitals};
use crate::pet::session;
use crate::pet::village::VILLAGES;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

/// A decision to emit commentary, frozen at tick-commit time. Carries
/// everything the async dispatcher needs so it never has to read live ECS
/// state (which would race with the next tick's writes).
#[derive(Debug, Clone)]
pub struct PendingDispatch {
    pub kind: Kind,
    pub trigger: Trigger,
    pub tone: Tone,
    pub pet_name: String,
    pub pet_level: u32,
    pub pet_hp: u32,
    pub pet_mood: u8,
    pub tick_count: u64,
    pub now: i64,
    pub rng_salt: u64,
}

/// Run inside the tick tx. Returns `Some(PendingDispatch)` if a commentary
/// should be emitted this tick (caller is expected to `tokio::spawn`
/// `execute_dispatch` with the returned value, after committing the tx).
///
/// Side effects performed in the tx:
/// * `budget_reserve(now)` when a dispatch is scheduled (optimistic —
///   even if the later async dispatch fails at both Haiku + rule-based
///   paths, we'd rather skip this cycle than double-emit next tick);
/// * `budget_incr_skip()` when a trigger fired but the budget blocked it
///   (telemetry for future tuning).
pub fn decide_and_reserve(
    tx: &Connection,
    world: &GameWorld,
    defeats: &[DefeatedMob],
    level_before: u32,
    level_after: u32,
    session_started_at: i64,
    tick_count: u64,
    now: i64,
) -> Result<Option<PendingDispatch>> {
    // 1. Opt-in gate. Saves the remaining work when commentary is off —
    //    most users run with CODEFORGE_COMMENTARY unset (spec §3.9 default).
    if !super::budget::opt_in_enabled(tx)? {
        return Ok(None);
    }

    // 2. Detect triggers (pure logic).
    let last_seen_at = session::get_last_seen(tx)?;
    let trig_ctx = trigger::TickContext {
        now,
        defeats,
        level_before,
        level_after,
        session_started_at,
        last_seen_at,
    };
    let Some(best) = trigger::pick_best(trigger::detect(&trig_ctx)) else {
        return Ok(None);
    };

    // 3. Rate-limit gate.
    if !super::budget::can_emit(tx, now, best.kind())? {
        repo::budget_incr_skip(tx)?;
        return Ok(None);
    }

    // 4. Collect pet context (all from ECS, zero DB).
    let pet = world.pet();
    let identity = world.world().get::<&PetIdentity>(pet)?;
    let level = world.world().get::<&PetLevel>(pet)?;
    let vitals = world.world().get::<&PetVitals>(pet)?;
    let mood = world.world().get::<&Mood>(pet)?;
    let strategy_comp = world.world().get::<&PetStrategy>(pet)?;
    let strategy_str = strategy_comp.value.as_str().to_string();

    // Pet display name. pet table may not have a row on first tick of a
    // fresh install; fall back to village default (e.g. "Ferris" for rust).
    let pet_name = lookup_pet_name(tx, &identity.village)?;

    // 5. Reserve the budget slot. Prior value is discarded — if the async
    //    dispatch later fails at both generation paths, we don't restore
    //    it; unreserving creates its own races (the next tick's
    //    decide_and_reserve may have already reserved again) and the
    //    cost of a silent skip is one hour without commentary — tolerable.
    repo::budget_reserve(tx, now)?;

    let pending = PendingDispatch {
        kind: best.kind(),
        trigger: best,
        tone: Tone::new(identity.village.clone(), strategy_str),
        pet_name,
        pet_level: level.level,
        pet_hp: vitals.hp,
        // Clamp mood into the u8 range defensively. ECS Mood is u32 but spec
        // 0..=100 keeps it small; the cast can't lose information in practice.
        pet_mood: mood.value.min(u8::MAX as u32) as u8,
        tick_count,
        now,
        // Use tick_count as the rng salt — deterministic per tick, monotonic
        // (feedback: rng-salt-monotonic), so catch-up replays produce the
        // same phrase on re-execution.
        rng_salt: tick_count,
    };
    Ok(Some(pending))
}

/// Async, spawned outside the tick tx. Opens its own SQLite connection and
/// writes the feed row + history. Never panics — any error is captured and
/// returned so the spawn site can decide whether to log.
///
/// Behaviour:
/// * If `api_key` is `Some`, try `generate_haiku` first (8s timeout).
/// * On Haiku failure or when `api_key` is `None`, fall through to rule-based.
/// * Both writes are inside a single nested tx so the feed + history
///   entries land atomically.
pub async fn execute_dispatch(
    db_path: PathBuf,
    pending: PendingDispatch,
    api_key: Option<String>,
) -> Result<()> {
    let gen_ctx = GenContext {
        pet_name: &pending.pet_name,
        pet_level: pending.pet_level,
        pet_hp: pending.pet_hp,
        pet_mood: pending.pet_mood,
        tone: pending.tone.clone(),
        trigger: &pending.trigger,
    };

    let generated: Generated = match api_key {
        Some(key) if !key.is_empty() => {
            match generator::generate_haiku(&key, &gen_ctx).await {
                Ok(g) => g,
                Err(_e) => generate_rule_blocking(&db_path, &gen_ctx, &pending)?,
            }
        }
        _ => generate_rule_blocking(&db_path, &gen_ctx, &pending)?,
    };

    write_feed_and_history(&db_path, &pending, &generated)?;
    Ok(())
}

/// Rule-based generation requires a DB connection (to check dedup history).
/// Wrap it behind a small helper so both paths share the same DB-open config.
fn generate_rule_blocking(
    db_path: &std::path::Path,
    gen_ctx: &GenContext<'_>,
    pending: &PendingDispatch,
) -> Result<Generated> {
    let conn = open_dispatch_conn(db_path)?;
    generator::generate_rule_based(&conn, gen_ctx, pending.rng_salt, pending.now)
}

fn write_feed_and_history(
    db_path: &std::path::Path,
    pending: &PendingDispatch,
    generated: &Generated,
) -> Result<()> {
    let conn = open_dispatch_conn(db_path)?;
    let tx = conn.unchecked_transaction()?;
    repo::insert_feed(
        &tx,
        InsertFeed {
            created_at: pending.now,
            tick_count: pending.tick_count,
            kind: pending.kind,
            tone: &pending.tone,
            phrase: &generated.phrase,
            phrase_hash: &generated.phrase_hash,
            source: generated.source,
        },
    )?;
    repo::record_history(&tx, &generated.phrase_hash, pending.kind, pending.now)?;
    tx.commit()?;
    Ok(())
}

fn open_dispatch_conn(db_path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    // Match Context::open_db verbatim — WAL + FK + busy_timeout. FK is
    // currently a no-op (commentary tables define no FKs today) but
    // consistent PRAGMA state across connections avoids surprise when
    // Phase 5 Nation plugin tables reference commentary kinds.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}

fn lookup_pet_name(conn: &Connection, village_id: &str) -> Result<String> {
    // 1. Phase 1 pet table — authoritative when present (user can rename
    //    via `codeforge adopt` flow).
    let from_row: Option<String> = conn
        .query_row("SELECT name FROM pet WHERE id = 1", [], |r| r.get(0))
        .optional()?;
    if let Some(name) = from_row {
        return Ok(name);
    }
    // 2. Village default (first-tick window on fresh install).
    Ok(VILLAGES
        .iter()
        .find(|v| v.id == village_id)
        .map(|v| v.pet_name.to_string())
        .unwrap_or_else(|| "Pet".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::Kind;
    use crate::daemon::combat::DefeatedMob;
    use crate::daemon::ecs::GameWorld;
    use crate::daemon::mob::MobKind;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn enable_opt_in(conn: &Connection) {
        repo::set_opt_in_flag(conn, true).unwrap();
    }

    #[test]
    fn decide_returns_none_when_opt_out() {
        // Env var must be unset for this test; settings flag stays at default 0.
        std::env::remove_var(super::super::budget::OPT_IN_ENV);
        let conn = fresh_conn();
        let mut world = GameWorld::load_or_init(&conn).unwrap();
        let boss = DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "boss".into(),
            hp_max: 100,
        };
        let res = decide_and_reserve(&conn, &mut world, &[boss], 1, 2, 0, 1, 1000).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn decide_returns_none_when_no_trigger() {
        let conn = fresh_conn();
        enable_opt_in(&conn);
        let world = GameWorld::load_or_init(&conn).unwrap();
        let res = decide_and_reserve(&conn, &world, &[], 1, 1, 1000, 1, 1001).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn decide_reserves_budget_and_returns_pending_when_trigger_fires() {
        let conn = fresh_conn();
        enable_opt_in(&conn);
        let world = GameWorld::load_or_init(&conn).unwrap();
        let boss = DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "src/auth.rs".into(),
            hp_max: 100,
        };
        let res = decide_and_reserve(&conn, &world, &[boss], 1, 1, 0, 7, 12_345).unwrap();
        let pending = res.expect("boss kill should produce a pending dispatch");
        assert_eq!(pending.kind, Kind::BossKill);
        assert_eq!(pending.tick_count, 7);
        assert_eq!(pending.now, 12_345);

        // Budget has been reserved at `now`.
        let last = repo::budget_last_emit(&conn).unwrap();
        assert_eq!(last, Some(12_345));
    }

    #[test]
    fn decide_rate_limits_second_call_within_window() {
        let conn = fresh_conn();
        enable_opt_in(&conn);
        let world = GameWorld::load_or_init(&conn).unwrap();
        let boss = DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "x".into(),
            hp_max: 100,
        };
        // First call reserves at t=0
        let first = decide_and_reserve(&conn, &world, &[boss.clone()], 1, 1, 0, 1, 0).unwrap();
        assert!(first.is_some());
        // Second call 30 min later — within 1h window — should be None
        let second = decide_and_reserve(&conn, &world, &[boss], 1, 1, 0, 2, 1800).unwrap();
        assert!(second.is_none());

        // Skip counter bumped
        let skips: i64 = conn
            .query_row(
                "SELECT consecutive_skip FROM commentary_budget WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(skips, 1);
    }

    #[test]
    fn decide_priority_picks_level_up_over_boss() {
        let conn = fresh_conn();
        enable_opt_in(&conn);
        let world = GameWorld::load_or_init(&conn).unwrap();
        let boss = DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "x".into(),
            hp_max: 100,
        };
        // Both fire: boss kill + level up (2 → 3)
        let res = decide_and_reserve(&conn, &world, &[boss], 2, 3, 0, 1, 100).unwrap();
        let pending = res.expect("level up should win priority over boss kill");
        assert_eq!(pending.kind, Kind::LevelUp);
    }

    #[test]
    fn execute_dispatch_rule_based_writes_feed_and_history() {
        // End-to-end rule path: no api_key → rule-based → persisted in the
        // feed + history.
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("state.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            crate::db::migrations::run(&conn).unwrap();
        }
        let pending = PendingDispatch {
            kind: Kind::BossKill,
            trigger: Trigger::BossKill {
                mob_name: "AuthBoss".into(),
                mob_kind: MobKind::Boss,
            },
            tone: Tone::new("rust", "aggressive"),
            pet_name: "Ferris".into(),
            pet_level: 5,
            pet_hp: 45,
            pet_mood: 60,
            tick_count: 42,
            now: 1000,
            rng_salt: 0,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            execute_dispatch(db_path.clone(), pending, None).await.unwrap();
        });

        let conn = Connection::open(&db_path).unwrap();
        let rows = repo::recent_feed(&conn, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, Kind::BossKill);
        assert_eq!(rows[0].source, super::super::Source::Rule);
        assert!(!rows[0].phrase.is_empty());

        // History recorded
        let count: i64 = conn
            .query_row("SELECT use_count FROM commentary_history WHERE kind='boss_kill'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn execute_dispatch_empty_api_key_still_rule_based() {
        // Treat empty-string key same as absent key — dream/compile.rs does the
        // same check.
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("state.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            crate::db::migrations::run(&conn).unwrap();
        }
        let pending = PendingDispatch {
            kind: Kind::LevelUp,
            trigger: Trigger::LevelUp { prev_level: 4, new_level: 5 },
            tone: Tone::new("python", "explorer"),
            pet_name: "Spam".into(),
            pet_level: 5,
            pet_hp: 30,
            pet_mood: 60,
            tick_count: 10,
            now: 2000,
            rng_salt: 3,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            execute_dispatch(db_path.clone(), pending, Some(String::new())).await.unwrap();
        });

        let conn = Connection::open(&db_path).unwrap();
        let rows = repo::recent_feed(&conn, 5).unwrap();
        assert_eq!(rows[0].source, super::super::Source::Rule);
    }
}
