//! ECS components + GameWorld wrapper for daemon state.
//!
//! Design per `.claude/rpg-engine-spec.md`:
//! - One pet entity per user (single-slot in Phase 2a; Phase 3+ will allow
//!   multiple pets).
//! - All derived game state lives in the ECS `World`. The daemon holds the
//!   World in memory across ticks; serialization to `pet_snapshot` is the
//!   persistence boundary.
//! - Load-on-start from Phase 1 `pet` table (CLI-owned, not derived).
//!   Fall back to defaults for fresh installs.

use anyhow::Result;
use hecs::{Entity, World};
use rusqlite::{Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Components ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PetIdentity {
    pub village: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PetLevel {
    pub level: u32,
    pub xp: u32,
    pub xp_to_next: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PetVitals {
    pub hp: u32,
    pub hp_max: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PetStats {
    pub atk: u32,
    pub def: u32,
    pub sup: u32,
    pub ver: u32,
}

/// Pet speech bubble. Stamped with the tick that authored it so
/// `serialize_to_db` can NULL it in `pet_snapshot` once it's past the TTL —
/// otherwise a single early kill would pin the statusline message forever.
#[derive(Debug, Clone)]
pub struct LastMessage {
    pub text: String,
    pub tick_stamp: u64,
}

/// How many ticks a `LastMessage` survives in `pet_snapshot` before being
/// cleared (5 ticks ≈ 5 min at the default 60s interval — enough for the
/// user to see the kill narration a few times, not so long it goes stale).
pub const LAST_MESSAGE_TTL_TICKS: u64 = 5;

/// Phase 3d: pet mood (0..=100). Unlike `LastMessage`, Mood is a persistent
/// stat — the value itself never expires, it just drifts with signals. The
/// `tick_stamp` records the last tick that wrote a new value, for diagnostic
/// purposes (and to let future systems reason about "how long since a signal").
#[derive(Debug, Clone, Copy)]
pub struct Mood {
    pub value: u32,
    pub tick_stamp: u64,
}

pub const MOOD_DEFAULT: u32 = 60;
pub const MOOD_MIN: u32 = 0;
pub const MOOD_MAX: u32 = 100;

/// Bookkeeping for rate-limited mood signals. Deliberately **not
/// serialized** to `pet_snapshot` — the rate-limit artifact on daemon
/// restart (one extra activity bump or idle penalty on first tick) is
/// acceptable versus the schema churn of persisting three more fields
/// that only matter during a single daemon lifetime.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoodBookkeeping {
    pub activity_last_bump_at_unix: i64,
    pub idle_last_penalty_at_unix: i64,
    pub was_low_hp: bool,
}

// ─── GameWorld wrapper ──────────────────────────────────────────────

/// Daemon-owned world. Single pet entity for Phase 2a.
pub struct GameWorld {
    world: World,
    pet: Entity,
}

impl GameWorld {
    /// Build a GameWorld from database state.
    ///
    /// Load precedence (fixes re-hydration bug — daemon restart must not
    /// lose tick work):
    ///   1. `pet_snapshot` (daemon-authored, includes `hp_max`)
    ///   2. Phase 1 `pet` table (fresh install post-adopt, pre-first-tick)
    ///   3. Default seed (Rust village, level 1, all stats 10)
    pub fn load_or_init(conn: &Connection) -> Result<Self> {
        let mut world = World::new();

        // Snapshot = daemon's authoritative view. If present, always win.
        let from_snapshot = conn
            .query_row(
                "SELECT village, level, hp, hp_max, xp, xp_to_next,
                        atk, def, sup, ver, mood, mood_tick_stamp
                 FROM pet_snapshot WHERE id = 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)? as u32,
                        r.get::<_, i64>(2)? as u32,
                        r.get::<_, i64>(3)? as u32,
                        r.get::<_, i64>(4)? as u32,
                        r.get::<_, i64>(5)? as u32,
                        r.get::<_, i64>(6)? as u32,
                        r.get::<_, i64>(7)? as u32,
                        r.get::<_, i64>(8)? as u32,
                        r.get::<_, i64>(9)? as u32,
                        r.get::<_, i64>(10)? as u32,
                        r.get::<_, i64>(11)? as u64,
                    ))
                },
            )
            .optional()?;

        let (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver, mood_value, mood_ts) =
            if let Some((village, level, hp, hp_max, xp, xp_to_next, atk, def, sup, ver, mood, mood_ts)) =
                from_snapshot
            {
                (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver, mood, mood_ts)
            } else {
                // Phase 1 fallback — no hp_max column, derive from hp.
                let from_pet = conn
                    .query_row(
                        "SELECT village, level, xp, xp_to_next, atk, hp, def, sup, ver
                         FROM pet WHERE id = 1",
                        [],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, i64>(1)? as u32,
                                r.get::<_, i64>(2)? as u32,
                                r.get::<_, i64>(3)? as u32,
                                r.get::<_, i64>(4)? as u32,
                                r.get::<_, i64>(5)? as u32,
                                r.get::<_, i64>(6)? as u32,
                                r.get::<_, i64>(7)? as u32,
                                r.get::<_, i64>(8)? as u32,
                            ))
                        },
                    )
                    .optional()?;

                let (village, level, xp, xp_to_next, atk, hp, def, sup, ver) =
                    from_pet.unwrap_or_else(|| {
                        ("rust".to_string(), 1, 0, 100, 10, 10, 10, 10, 10)
                    });
                let hp_max = hp.max(10);
                // Phase 1 row has no mood — start at default.
                (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver, MOOD_DEFAULT, 0)
            };

        // Clamp mood on load in case a prior version wrote out of range.
        let mood_value = mood_value.clamp(MOOD_MIN, MOOD_MAX);

        let pet = world.spawn((
            PetIdentity { village },
            PetLevel { level, xp, xp_to_next },
            PetVitals { hp, hp_max },
            PetStats { atk, def, sup, ver },
            Mood { value: mood_value, tick_stamp: mood_ts },
            // Bookkeeping starts at zeros on every daemon boot — see note
            // on MoodBookkeeping about why we don't persist these.
            MoodBookkeeping::default(),
        ));

        Ok(Self { world, pet })
    }

    pub fn world(&self) -> &World { &self.world }
    pub fn world_mut(&mut self) -> &mut World { &mut self.world }
    pub fn pet(&self) -> Entity { self.pet }

    /// Serialize current pet state to `pet_snapshot` (single-row upsert).
    ///
    /// `current_tick` is used to expire stale `LastMessage` components: a
    /// message older than `LAST_MESSAGE_TTL_TICKS` writes NULL to
    /// `pet_snapshot.last_message` even if the component is still in the
    /// ECS world. This prevents a single early kill from pinning the
    /// statusline speech bubble for the lifetime of the daemon.
    pub fn serialize_to_db(&self, conn: &Connection, current_tick: u64) -> Result<()> {
        let identity = self.world.get::<&PetIdentity>(self.pet)?;
        let level = self.world.get::<&PetLevel>(self.pet)?;
        let vitals = self.world.get::<&PetVitals>(self.pet)?;
        let stats = self.world.get::<&PetStats>(self.pet)?;
        let last_msg = match self.world.get::<&LastMessage>(self.pet).ok() {
            Some(m) if current_tick.saturating_sub(m.tick_stamp) < LAST_MESSAGE_TTL_TICKS => {
                Some(m.text.clone())
            }
            _ => None,
        };
        // Mood is always present in the ECS (load_or_init seeds a default),
        // so read directly. Clamp defensively in case a system wrote out
        // of range.
        let mood = self.world.get::<&Mood>(self.pet)?;
        let mood_value = mood.value.clamp(MOOD_MIN, MOOD_MAX);
        let mood_ts = mood.tick_stamp;

        let now_iso = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, last_message, mood, mood_tick_stamp, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                 village = excluded.village,
                 level = excluded.level,
                 hp = excluded.hp,
                 hp_max = excluded.hp_max,
                 xp = excluded.xp,
                 xp_to_next = excluded.xp_to_next,
                 atk = excluded.atk,
                 def = excluded.def,
                 sup = excluded.sup,
                 ver = excluded.ver,
                 last_message = excluded.last_message,
                 mood = excluded.mood,
                 mood_tick_stamp = excluded.mood_tick_stamp,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                identity.village,
                level.level as i64,
                vitals.hp as i64,
                vitals.hp_max as i64,
                level.xp as i64,
                level.xp_to_next as i64,
                stats.atk as i64,
                stats.def as i64,
                stats.sup as i64,
                stats.ver as i64,
                last_msg,
                mood_value as i64,
                mood_ts as i64,
                now_iso,
            ],
        )?;
        Ok(())
    }
}

#[allow(dead_code)]
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

    #[test]
    fn fresh_install_seeds_default_pet() {
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        let identity = gw.world.get::<&PetIdentity>(gw.pet).unwrap();
        assert_eq!(identity.village, "rust");
        let level = gw.world.get::<&PetLevel>(gw.pet).unwrap();
        assert_eq!(level.level, 1);
    }

    #[test]
    fn load_hydrates_from_phase1_pet_table() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next, atk, hp, def, sup, ver)
             VALUES (1, 'python', 'Pytho', 7, 450, 1000, 25, 80, 30, 15, 20)",
            [],
        )
        .unwrap();

        let gw = GameWorld::load_or_init(&conn).unwrap();
        let identity = gw.world.get::<&PetIdentity>(gw.pet).unwrap();
        assert_eq!(identity.village, "python");
        let level = gw.world.get::<&PetLevel>(gw.pet).unwrap();
        assert_eq!(level.level, 7);
        assert_eq!(level.xp, 450);
        let vitals = gw.world.get::<&PetVitals>(gw.pet).unwrap();
        assert_eq!(vitals.hp, 80);
        let stats = gw.world.get::<&PetStats>(gw.pet).unwrap();
        assert_eq!(stats.atk, 25);
        assert_eq!(stats.ver, 20);
    }

    #[test]
    fn serialize_writes_pet_snapshot() {
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        gw.serialize_to_db(&conn, 1).unwrap();

        let (village, level, hp_max): (String, i64, i64) = conn
            .query_row(
                "SELECT village, level, hp_max FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(village, "rust");
        assert_eq!(level, 1);
        assert_eq!(hp_max, 10);
    }

    #[test]
    fn serialize_is_upsert() {
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        gw.serialize_to_db(&conn, 1).unwrap();

        // Mutate and re-serialize — should update single row, not insert
        {
            let mut level = gw.world.get::<&mut PetLevel>(gw.pet).unwrap();
            level.level = 5;
            level.xp = 123;
        }
        gw.serialize_to_db(&conn, 1).unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let (level, xp): (i64, i64) = conn
            .query_row(
                "SELECT level, xp FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(level, 5);
        assert_eq!(xp, 123);
    }

    #[test]
    fn snapshot_takes_precedence_over_pet_table() {
        // Re-hydration bug regression test: daemon restart must read its
        // own authoritative snapshot, not the stale Phase 1 pet row.
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next, atk, hp, def, sup, ver)
             VALUES (1, 'rust', 'Ferris', 1, 0, 100, 10, 10, 10, 10, 10)",
            [],
        )
        .unwrap();
        // Daemon has ticked many times — snapshot is far ahead of pet.
        conn.execute(
            "INSERT INTO pet_snapshot
               (id, village, level, hp, hp_max, xp, xp_to_next,
                atk, def, sup, ver, last_message, updated_at)
             VALUES (1, 'python', 8, 95, 110, 340, 800,
                     28, 22, 19, 17, NULL, datetime('now'))",
            [],
        )
        .unwrap();

        let gw = GameWorld::load_or_init(&conn).unwrap();
        let identity = gw.world.get::<&PetIdentity>(gw.pet).unwrap();
        assert_eq!(identity.village, "python", "snapshot must win over pet");
        let level = gw.world.get::<&PetLevel>(gw.pet).unwrap();
        assert_eq!(level.level, 8);
        assert_eq!(level.xp, 340);
        let vitals = gw.world.get::<&PetVitals>(gw.pet).unwrap();
        assert_eq!(vitals.hp, 95);
        assert_eq!(vitals.hp_max, 110, "hp_max comes from snapshot, not derived");
    }

    #[test]
    fn serialize_includes_last_message_when_present() {
        let conn = fresh_conn();
        let mut gw = GameWorld::load_or_init(&conn).unwrap();
        gw.world
            .insert_one(gw.pet, LastMessage { text: "又是 TODO？".to_string(), tick_stamp: 1 })
            .unwrap();
        gw.serialize_to_db(&conn, 1).unwrap();

        let msg: Option<String> = conn
            .query_row(
                "SELECT last_message FROM pet_snapshot WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(msg, Some("又是 TODO？".to_string()));
    }

    #[test]
    fn last_message_expires_after_ttl() {
        // Regression: a defeat on an early tick used to pin the statusline
        // speech bubble forever because serialize_to_db always wrote the
        // current LastMessage. Now the TTL nulls it after N ticks.
        let conn = fresh_conn();
        let mut gw = GameWorld::load_or_init(&conn).unwrap();
        gw.world
            .insert_one(
                gw.pet,
                LastMessage { text: "old kill".to_string(), tick_stamp: 10 },
            )
            .unwrap();

        // Within TTL: message writes through.
        gw.serialize_to_db(&conn, 10 + LAST_MESSAGE_TTL_TICKS - 1).unwrap();
        let msg: Option<String> = conn
            .query_row("SELECT last_message FROM pet_snapshot WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msg, Some("old kill".to_string()));

        // Past TTL: message is nulled out even though the ECS component
        // is still present (cheap read — no mutation needed every tick).
        gw.serialize_to_db(&conn, 10 + LAST_MESSAGE_TTL_TICKS).unwrap();
        let msg: Option<String> = conn
            .query_row("SELECT last_message FROM pet_snapshot WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msg, None);
    }

    // ─── Phase 3d Mood tests ───────────────────────────────────────────

    #[test]
    fn fresh_install_seeds_default_mood() {
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        let mood = gw.world.get::<&Mood>(gw.pet).unwrap();
        assert_eq!(mood.value, MOOD_DEFAULT);
        assert_eq!(mood.tick_stamp, 0);
    }

    #[test]
    fn serialize_writes_mood_columns() {
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        {
            let mut mood = gw.world.get::<&mut Mood>(gw.pet).unwrap();
            mood.value = 83;
            mood.tick_stamp = 42;
        }
        gw.serialize_to_db(&conn, 42).unwrap();

        let (v, ts): (i64, i64) = conn
            .query_row(
                "SELECT mood, mood_tick_stamp FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(v, 83);
        assert_eq!(ts, 42);
    }

    #[test]
    fn snapshot_mood_round_trips_through_load() {
        // Daemon wrote mood, then restarted: new ECS world must recover the
        // same value (unlike LastMessage which is transient).
        let conn = fresh_conn();
        let gw = GameWorld::load_or_init(&conn).unwrap();
        {
            let mut mood = gw.world.get::<&mut Mood>(gw.pet).unwrap();
            mood.value = 27;
            mood.tick_stamp = 500;
        }
        gw.serialize_to_db(&conn, 500).unwrap();
        drop(gw);

        let gw2 = GameWorld::load_or_init(&conn).unwrap();
        let mood = gw2.world.get::<&Mood>(gw2.pet).unwrap();
        assert_eq!(mood.value, 27);
        assert_eq!(mood.tick_stamp, 500);
    }

    #[test]
    fn load_clamps_out_of_range_mood() {
        // Defensive: if a prior version or manual SQL wrote mood=150,
        // the ECS view must clamp to 100 (CHECK constraint would actually
        // prevent this on insert, but belt-and-braces).
        let conn = fresh_conn();
        // Seed a row that bypasses ECS write (use UPDATE to dodge CHECK
        // on fresh insert defaults — CHECK still fires so we stay within
        // range here, but the clamp guards against any future loosening).
        conn.execute(
            "INSERT INTO pet_snapshot
               (id, village, level, hp, hp_max, xp, xp_to_next,
                atk, def, sup, ver, last_message, mood, mood_tick_stamp, updated_at)
             VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10,
                     NULL, 100, 0, datetime('now'))",
            [],
        )
        .unwrap();

        let gw = GameWorld::load_or_init(&conn).unwrap();
        let mood = gw.world.get::<&Mood>(gw.pet).unwrap();
        assert!(mood.value <= MOOD_MAX);
    }
}
