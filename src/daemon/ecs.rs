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
                        atk, def, sup, ver
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
                    ))
                },
            )
            .optional()?;

        let (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver) =
            if let Some((village, level, hp, hp_max, xp, xp_to_next, atk, def, sup, ver)) =
                from_snapshot
            {
                (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver)
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
                (village, level, xp, xp_to_next, atk, hp, hp_max, def, sup, ver)
            };

        let pet = world.spawn((
            PetIdentity { village },
            PetLevel { level, xp, xp_to_next },
            PetVitals { hp, hp_max },
            PetStats { atk, def, sup, ver },
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

        let now_iso = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, last_message, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
}
