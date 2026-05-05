//! Mood decay system (spec §3.2).
//!
//! Four independent signals drive mood per tick:
//!   +10  最近 24h 有 coding activity   (rate-limited: once per 24h)
//!    -8  每 6h 無 activity             (repeats every 6h of continuous idle)
//!   +20  打倒 Boss                     (per boss kill this tick)
//!   -15  HP < 30%                      (on threshold *entry* only, not every tick below)
//!
//! Bookkeeping state (when each rate-limited signal last fired, and whether
//! HP was low last tick) lives on a separate `MoodBookkeeping` ECS component
//! that is **not persisted** to `pet_snapshot` — on daemon restart it resets
//! to defaults. The artifact: the first tick after restart may re-apply the
//! activity bump or idle penalty, which is a minor UX quirk, not a
//! correctness issue. Mood value + tick_stamp themselves persist via
//! `Mood` / `pet_snapshot`.
//!
//! All time math is in unix seconds so `event_inbox.created_at` (which is
//! stored in seconds) needs no conversion. The decay logic is a set of
//! pure functions so we can unit-test each rule in isolation.

use super::combat::DefeatedMob;
use super::ecs::{GameWorld, Mood, MoodBookkeeping, PetVitals, MOOD_MAX, MOOD_MIN};
use super::mob::MobKind;
use anyhow::Result;
use rusqlite::Connection;

// ─── Tunables（spec §3.2 — 偏離需 review 記錄）─────────────────────────

const SECONDS_PER_HOUR: i64 = 3600;
pub const ACTIVITY_WINDOW_SECS: i64 = 24 * SECONDS_PER_HOUR;
pub const IDLE_BUCKET_SECS: i64 = 6 * SECONDS_PER_HOUR;
pub const ACTIVITY_BUMP: i32 = 10;
pub const IDLE_PENALTY: i32 = -8;
pub const BOSS_BUMP: i32 = 20;
pub const HP_LOW_PENALTY: i32 = -15;
pub const HP_LOW_RATIO: f32 = 0.30;

// ─── Rule functions (pure) ──────────────────────────────────────────

/// +10 if activity in last 24h AND we haven't already applied it within the
/// last 24h window. `seconds_since_activity` is `i64::MAX` when the inbox
/// has never fired — treat as "no activity", no bump.
pub fn activity_bump(
    seconds_since_activity: i64,
    activity_last_bump_at_unix: i64,
    now_unix: i64,
) -> i32 {
    if seconds_since_activity < ACTIVITY_WINDOW_SECS
        && now_unix.saturating_sub(activity_last_bump_at_unix) >= ACTIVITY_WINDOW_SECS
    {
        ACTIVITY_BUMP
    } else {
        0
    }
}

/// -8 if idle ≥6h AND we haven't already applied a penalty in the current 6h
/// bucket. One penalty per tick — a 14-day absence is handled by the daemon's
/// catchup mechanism which caps replays, so we don't need to catch up all
/// buckets at once.
pub fn idle_penalty(
    seconds_since_activity: i64,
    idle_last_penalty_at_unix: i64,
    now_unix: i64,
) -> i32 {
    if seconds_since_activity >= IDLE_BUCKET_SECS
        && now_unix.saturating_sub(idle_last_penalty_at_unix) >= IDLE_BUCKET_SECS
    {
        IDLE_PENALTY
    } else {
        0
    }
}

/// +20 per Boss kill in this tick's defeat list. Unbounded — multi-boss
/// kills in a single tick (e.g., after long catchup) add cumulatively.
pub fn boss_bump(defeats: &[DefeatedMob]) -> i32 {
    let boss_count = defeats.iter().filter(|d| d.kind == MobKind::Boss).count();
    (boss_count as i32).saturating_mul(BOSS_BUMP)
}

/// -15 only on the tick where HP *enters* the <30% band from above. Prevents
/// the penalty from firing every single tick while HP is low (which would
/// crush mood regardless of activity).
pub fn hp_entry_penalty(prior_low: bool, current_low: bool) -> i32 {
    if !prior_low && current_low {
        HP_LOW_PENALTY
    } else {
        0
    }
}

// ─── Orchestrator ───────────────────────────────────────────────────

/// Read the most recent `event_inbox.created_at` and return seconds between
/// that and `now_unix`. Returns `i64::MAX` (effectively "infinite idle") if
/// the inbox has never held an event.
///
/// Even events still marked `seen_at IS NULL` count — activity is activity,
/// whether or not the daemon has processed it yet.
pub fn seconds_since_last_activity(conn: &Connection, now_unix: i64) -> Result<i64> {
    let last: Option<i64> = conn
        .query_row("SELECT max(created_at) FROM event_inbox", [], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap_or(None);
    match last {
        Some(ts) => Ok(now_unix.saturating_sub(ts).max(0)),
        None => Ok(i64::MAX),
    }
}

/// Apply all four mood rules in one pass. Called AFTER combat so boss
/// defeats and post-counter HP are in their final state, and BEFORE
/// `serialize_to_db` so the new mood lands in the same snapshot row.
pub fn update_mood(
    gw: &mut GameWorld,
    defeats: &[DefeatedMob],
    conn: &Connection,
    now_unix: i64,
    current_tick: u64,
) -> Result<()> {
    let pet = gw.pet();
    let seconds_idle = seconds_since_last_activity(conn, now_unix)?;

    // Snapshot current HP ratio before we touch anything else.
    let current_low = {
        let vitals = gw.world().get::<&PetVitals>(pet)?;
        let hp_max = vitals.hp_max.max(1) as f32;
        (vitals.hp as f32 / hp_max) < HP_LOW_RATIO
    };

    // Load bookkeeping snapshot (fallback to defaults — see note above
    // about transient lifecycle).
    let (activity_last, idle_last, prior_low) = gw
        .world()
        .get::<&MoodBookkeeping>(pet)
        .map(|b| {
            (
                b.activity_last_bump_at_unix,
                b.idle_last_penalty_at_unix,
                b.was_low_hp,
            )
        })
        .unwrap_or((0, 0, current_low));

    let d_activity = activity_bump(seconds_idle, activity_last, now_unix);
    let d_idle = idle_penalty(seconds_idle, idle_last, now_unix);
    let d_boss = boss_bump(defeats);
    let d_hp = hp_entry_penalty(prior_low, current_low);
    let delta = d_activity
        .saturating_add(d_idle)
        .saturating_add(d_boss)
        .saturating_add(d_hp);

    if delta != 0 {
        let mut mood = gw.world_mut().get::<&mut Mood>(pet)?;
        let new_value = apply_delta(mood.value, delta);
        mood.value = new_value;
        mood.tick_stamp = current_tick;
    }

    // Update bookkeeping (always, so subsequent ticks rate-limit correctly).
    // insert_one replaces an existing component in hecs; compute the new
    // values first, then hand a bare component to a freshly-scoped
    // mutable borrow so we don't overlap `pet` with the insert call.
    let next_bookkeeping = MoodBookkeeping {
        activity_last_bump_at_unix: if d_activity > 0 {
            now_unix
        } else {
            activity_last
        },
        idle_last_penalty_at_unix: if d_idle < 0 { now_unix } else { idle_last },
        was_low_hp: current_low,
    };
    gw.world_mut().insert_one(pet, next_bookkeeping)?;

    Ok(())
}

/// Saturate mood delta into the valid `MOOD_MIN..=MOOD_MAX` band. Works in
/// i32 space so a large negative delta doesn't wrap on the u32 subtract.
fn apply_delta(value: u32, delta: i32) -> u32 {
    let next = (value as i32).saturating_add(delta);
    next.clamp(MOOD_MIN as i32, MOOD_MAX as i32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── pure rule fns ─────────────────────────────────────────────

    #[test]
    fn activity_bump_fires_when_within_24h_and_cooldown_elapsed() {
        // 5 min of activity recency, last bump a year ago → +10
        assert_eq!(activity_bump(300, 0, 86_400 + 1), ACTIVITY_BUMP);
    }

    #[test]
    fn activity_bump_blocked_by_cooldown() {
        // Activity present but we already bumped 10 min ago → 0
        let now = 10_000_000;
        let last_bump = now - 600;
        assert_eq!(activity_bump(60, last_bump, now), 0);
    }

    #[test]
    fn activity_bump_zero_when_idle_beyond_24h() {
        // No activity in 25h → no bump regardless of cooldown.
        assert_eq!(activity_bump(25 * 3600, 0, 100_000_000), 0);
    }

    #[test]
    fn activity_bump_handles_infinite_idle_sentinel() {
        // seconds_since_activity = i64::MAX means "never any event".
        assert_eq!(activity_bump(i64::MAX, 0, 100_000_000), 0);
    }

    #[test]
    fn idle_penalty_fires_at_6h_boundary() {
        assert_eq!(idle_penalty(IDLE_BUCKET_SECS, 0, 100_000), IDLE_PENALTY);
    }

    #[test]
    fn idle_penalty_blocked_until_next_bucket() {
        let now = 100_000;
        // Already penalized 1h ago — next penalty not until 6h later.
        let last_pen = now - 3600;
        assert_eq!(idle_penalty(IDLE_BUCKET_SECS + 3600, last_pen, now), 0);
    }

    #[test]
    fn idle_penalty_repeats_every_bucket() {
        let now = 100_000;
        let last_pen = now - IDLE_BUCKET_SECS;
        assert_eq!(
            idle_penalty(IDLE_BUCKET_SECS + 10, last_pen, now),
            IDLE_PENALTY
        );
    }

    #[test]
    fn idle_penalty_zero_when_recent_activity() {
        // 1h of idle only — no penalty yet.
        assert_eq!(idle_penalty(3600, 0, 100_000), 0);
    }

    #[test]
    fn boss_bump_per_boss() {
        let defeats = vec![
            DefeatedMob {
                id: 1,
                kind: MobKind::Boss,
                name: "b1".into(),
                hp_max: 100,
            },
            DefeatedMob {
                id: 2,
                kind: MobKind::Zombie,
                name: "z".into(),
                hp_max: 10,
            },
            DefeatedMob {
                id: 3,
                kind: MobKind::Boss,
                name: "b2".into(),
                hp_max: 80,
            },
        ];
        assert_eq!(boss_bump(&defeats), 2 * BOSS_BUMP);
    }

    #[test]
    fn boss_bump_zero_on_non_boss_defeats() {
        let defeats = vec![DefeatedMob {
            id: 1,
            kind: MobKind::Ghost,
            name: "g".into(),
            hp_max: 5,
        }];
        assert_eq!(boss_bump(&defeats), 0);
    }

    #[test]
    fn hp_entry_penalty_only_on_threshold_cross() {
        assert_eq!(hp_entry_penalty(false, true), HP_LOW_PENALTY);
        assert_eq!(
            hp_entry_penalty(true, true),
            0,
            "must not fire every tick below"
        );
        assert_eq!(
            hp_entry_penalty(true, false),
            0,
            "recovery is not a penalty"
        );
        assert_eq!(hp_entry_penalty(false, false), 0);
    }

    #[test]
    fn apply_delta_clamps_to_band() {
        assert_eq!(apply_delta(60, 10), 70);
        assert_eq!(apply_delta(95, 20), MOOD_MAX, "cap at 100");
        assert_eq!(apply_delta(5, -50), MOOD_MIN, "floor at 0");
        assert_eq!(apply_delta(0, -10), 0);
    }

    // ─── integration: update_mood end-to-end via tick-like fixture ─

    use crate::daemon::ecs::MOOD_DEFAULT;
    use crate::db::migrations;

    fn fresh() -> (Connection, GameWorld) {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        // load_or_init already seeds Mood + MoodBookkeeping::default().
        let world = GameWorld::load_or_init(&conn).unwrap();
        (conn, world)
    }

    #[test]
    fn update_mood_applies_activity_bump_on_first_tick_with_recent_event() {
        let (conn, mut gw) = fresh();
        let now = 1_000_000;
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES ('{}', ?1)",
            [now - 60],
        )
        .unwrap();

        update_mood(&mut gw, &[], &conn, now, 1).unwrap();
        let mood = gw.world().get::<&Mood>(gw.pet()).unwrap();
        assert_eq!(mood.value, MOOD_DEFAULT + ACTIVITY_BUMP as u32);
    }

    #[test]
    fn update_mood_applies_idle_penalty_on_long_absence() {
        // Empty inbox → seconds_since = i64::MAX → activity_bump = 0
        // (since we're past the 24h window too), idle_penalty fires.
        let (conn, mut gw) = fresh();
        update_mood(&mut gw, &[], &conn, 1_000_000, 1).unwrap();
        let mood = gw.world().get::<&Mood>(gw.pet()).unwrap();
        assert_eq!(
            mood.value,
            (MOOD_DEFAULT as i32 + IDLE_PENALTY) as u32,
            "no activity ever = pure -8 bucket, no activity bump"
        );
    }

    #[test]
    fn update_mood_can_combine_activity_and_idle_signals() {
        // Spec allows both to fire when activity was *somewhat* recent
        // (within 24h) but also >6h ago: +10 activity - 8 idle = +2.
        let (conn, mut gw) = fresh();
        let now = 1_000_000;
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES ('{}', ?1)",
            [now - 10 * 3600],
        )
        .unwrap();
        update_mood(&mut gw, &[], &conn, now, 1).unwrap();
        let mood = gw.world().get::<&Mood>(gw.pet()).unwrap();
        assert_eq!(
            mood.value as i32,
            MOOD_DEFAULT as i32 + ACTIVITY_BUMP + IDLE_PENALTY,
            "overlap allowed: 10h-old activity still within 24h window"
        );
    }

    #[test]
    fn update_mood_applies_boss_bump_per_kill() {
        let (conn, mut gw) = fresh();
        // Recent activity so no idle penalty interferes.
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES ('{}', ?1)",
            [1_000_000 - 60],
        )
        .unwrap();

        let defeats = vec![DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "big".into(),
            hp_max: 500,
        }];
        update_mood(&mut gw, &defeats, &conn, 1_000_000, 1).unwrap();

        let mood = gw.world().get::<&Mood>(gw.pet()).unwrap();
        // +10 (activity) + +20 (boss) = +30
        assert_eq!(
            mood.value,
            MOOD_DEFAULT + ACTIVITY_BUMP as u32 + BOSS_BUMP as u32
        );
    }

    #[test]
    fn update_mood_applies_hp_low_penalty_on_entry_only() {
        let (conn, mut gw) = fresh();
        // Recent activity, no idle penalty.
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES ('{}', ?1)",
            [1_000_000 - 60],
        )
        .unwrap();
        // Drop HP under 30% of hp_max (hp_max = 10 on fresh install; <3 is low).
        {
            let pet = gw.pet();
            let mut v = gw.world_mut().get::<&mut PetVitals>(pet).unwrap();
            v.hp = 1;
        }

        update_mood(&mut gw, &[], &conn, 1_000_000, 1).unwrap();
        let v1 = gw.world().get::<&Mood>(gw.pet()).unwrap().value;

        // Second tick, still low HP — must NOT re-apply the -15.
        update_mood(&mut gw, &[], &conn, 1_000_060, 2).unwrap();
        let v2 = gw.world().get::<&Mood>(gw.pet()).unwrap().value;

        // Between the two ticks: first: +10 activity -15 hp → -5 from 60 → 55
        // Second tick: +0 (already in low state, activity cooldown active) → 55
        assert_eq!(
            v1,
            (MOOD_DEFAULT as i32 + ACTIVITY_BUMP + HP_LOW_PENALTY) as u32
        );
        assert_eq!(
            v2, v1,
            "hp_low penalty must not fire every tick while below"
        );
    }

    #[test]
    fn update_mood_tick_stamp_only_moves_on_change() {
        let (conn, mut gw) = fresh();
        // No events → infinite idle → but only one bucket triggers
        // (same tick, different calls).
        let now = 1_000_000;
        update_mood(&mut gw, &[], &conn, now, 5).unwrap();
        let ts1 = gw.world().get::<&Mood>(gw.pet()).unwrap().tick_stamp;
        assert_eq!(ts1, 5, "idle penalty fired → tick_stamp updated");

        // Immediate re-call: idle cooldown still hot, no signals → no delta.
        update_mood(&mut gw, &[], &conn, now, 6).unwrap();
        let ts2 = gw.world().get::<&Mood>(gw.pet()).unwrap().tick_stamp;
        assert_eq!(ts2, 5, "no delta → tick_stamp stays put");
    }

    #[test]
    fn seconds_since_last_activity_returns_sentinel_on_empty_inbox() {
        let (conn, _) = fresh();
        let s = seconds_since_last_activity(&conn, 1_000_000).unwrap();
        assert_eq!(s, i64::MAX);
    }
}
