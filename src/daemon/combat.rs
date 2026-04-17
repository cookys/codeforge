//! Auto-combat — Phase 2b P3.
//!
//! Per tick: load alive mobs in pet's zone, run one attack per mob, apply
//! counter-damage, mark defeats. Loot rolls + XP award live in `loot.rs`.
//!
//! RNG is deterministic: `Xorshift64*` seeded per-mob from `(tick_at, mob.id)`.
//! No external crate — tests can re-derive outcomes.

use anyhow::Result;
use rusqlite::Connection;

use super::ecs::{GameWorld, PetIdentity, PetStats, PetVitals};
use super::mob::MobKind;

/// Max mobs attacked per tick. Protects the 10ms tick budget against a
/// huge zone of mobs; the remainder get their turn on the next tick.
pub const MAX_ATTACKS_PER_TICK: usize = 50;

// ─── Deterministic RNG ──────────────────────────────────────────────

/// Xorshift64*. Always advances; never returns 0 (seed is `| 1`-forced).
#[derive(Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        // Non-zero invariant: the generator is degenerate if state ever hits 0
        Self(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1) using the high 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

/// SplitMix64-style mixer — combines tick_at + mob_id into a seed so each
/// (tick, mob) pair is statistically independent from others.
pub fn hash_seed(a: u64, b: u64) -> u64 {
    let mut x = a
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_mul(b.wrapping_add(1));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x
}

// ─── Data ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AliveMob {
    pub id: i64,
    /// Zone the mob belongs to. Loaded from DB but not read in current tick
    /// logic (combat filters by zone in the SQL query). Kept for upcoming
    /// Strategy Mode (Phase 3b) where cross-zone raids may reference it.
    #[allow(dead_code)]
    pub zone_id: String,
    pub kind: MobKind,
    pub name: String,
    pub hp: u32,
    pub hp_max: u32,
    pub atk: u32,
    pub def: u32,
    pub difficulty: u32,
}

#[derive(Debug, Clone)]
pub struct DefeatedMob {
    pub id: i64,
    pub kind: MobKind,
    pub name: String,
    pub hp_max: u32,
}

#[derive(Debug, Default)]
pub struct CombatSummary {
    pub attacks: usize,
    pub hits: usize,
    pub defeats: Vec<DefeatedMob>,
    pub pet_damage_taken: u32,
}

// ─── Formula ────────────────────────────────────────────────────────

/// Spec §2: `hit_chance = (pet.atk + pet.ver) / (mob.def + difficulty)`.
/// Clamped to [0.05, 0.95] so combat is never automatic in either direction.
pub fn hit_chance(pet_atk: u32, pet_ver: u32, mob_def: u32, difficulty: u32) -> f64 {
    let num = (pet_atk + pet_ver) as f64;
    let den = (mob_def + difficulty).max(1) as f64;
    (num / den).clamp(0.05, 0.95)
}

// ─── Tick driver ────────────────────────────────────────────────────

/// Run one combat tick. Reads alive mobs from DB, applies attacks, writes
/// back hp/defeated_at. Returns a summary — loot handling happens in
/// `loot.rs` against `summary.defeats`.
///
/// `rng_salt` must be monotonic per tick (use `tick_count`, not `tick_at`
/// seconds — multiple ticks in the same second would share a seed and
/// produce correlated outcomes, masking bugs in tests and causing
/// surprising streaks in production).
pub fn run_tick(
    conn: &Connection,
    world: &mut GameWorld,
    rng_salt: u64,
) -> Result<CombatSummary> {
    let zone_id = {
        let id = world.world().get::<&PetIdentity>(world.pet())?;
        id.village.clone()
    };
    let mobs = load_alive_mobs(conn, &zone_id, MAX_ATTACKS_PER_TICK)?;
    if mobs.is_empty() {
        return Ok(CombatSummary::default());
    }

    let (pet_atk, pet_ver, pet_def) = {
        let stats = world.world().get::<&PetStats>(world.pet())?;
        (stats.atk, stats.ver, stats.def)
    };

    let mut summary = CombatSummary::default();
    for mob in mobs {
        summary.attacks += 1;
        let mut rng = Rng::from_seed(hash_seed(rng_salt, mob.id as u64));
        let hc = hit_chance(pet_atk, pet_ver, mob.def, mob.difficulty);
        let roll = rng.next_f64();

        let mut defeated = false;
        if roll < hc {
            summary.hits += 1;
            let mult = rng.range_f64(0.8, 1.2);
            let damage = ((pet_atk as f64 * mult).ceil() as u32).max(1);
            let new_hp = mob.hp.saturating_sub(damage);
            if new_hp == 0 {
                // `defeated_at` gets its real unix-seconds timestamp in the
                // caller's combat_log write — keep `mobs.defeated_at` as
                // a non-null monotonic marker (rng_salt = tick_count is
                // monotonic, non-zero)
                mark_defeated(conn, mob.id, rng_salt as i64)?;
                defeated = true;
            } else {
                update_mob_hp(conn, mob.id, new_hp)?;
            }
        }

        // Counter-attack: mob hits back even on pet miss (pet always engages)
        let counter = mob.atk.saturating_sub(pet_def / 2);
        if counter > 0 {
            apply_pet_damage(world, counter);
            summary.pet_damage_taken = summary.pet_damage_taken.saturating_add(counter);
        }

        if defeated {
            summary.defeats.push(DefeatedMob {
                id: mob.id,
                kind: mob.kind,
                name: mob.name,
                hp_max: mob.hp_max,
            });
        }
    }

    Ok(summary)
}

// ─── DB helpers ─────────────────────────────────────────────────────

fn load_alive_mobs(conn: &Connection, zone_id: &str, cap: usize) -> Result<Vec<AliveMob>> {
    let mut stmt = conn.prepare(
        "SELECT id, zone_id, kind, name, hp, hp_max, atk, def, difficulty
         FROM mobs
         WHERE zone_id = ?1 AND defeated_at IS NULL
         ORDER BY id
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![zone_id, cap as i64], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)? as u32,
            r.get::<_, i64>(5)? as u32,
            r.get::<_, i64>(6)? as u32,
            r.get::<_, i64>(7)? as u32,
            r.get::<_, i64>(8)? as u32,
        ))
    })?;
    let mut out = Vec::with_capacity(cap.min(64));
    for row in rows {
        let (id, zone_id, k, name, hp, hp_max, atk, def, difficulty) = row?;
        let kind = MobKind::from_str(&k).unwrap_or(MobKind::Ghost);
        out.push(AliveMob {
            id,
            zone_id,
            kind,
            name,
            hp,
            hp_max,
            atk,
            def,
            difficulty,
        });
    }
    Ok(out)
}

fn update_mob_hp(conn: &Connection, id: i64, hp: u32) -> Result<()> {
    conn.execute(
        "UPDATE mobs SET hp = ?1 WHERE id = ?2 AND defeated_at IS NULL",
        rusqlite::params![hp as i64, id],
    )?;
    Ok(())
}

fn mark_defeated(conn: &Connection, id: i64, at: i64) -> Result<()> {
    conn.execute(
        "UPDATE mobs SET hp = 0, defeated_at = ?1
         WHERE id = ?2 AND defeated_at IS NULL",
        rusqlite::params![at, id],
    )?;
    Ok(())
}

fn apply_pet_damage(gw: &mut GameWorld, dmg: u32) {
    let pet = gw.pet();
    if let Ok(mut vitals) = gw.world_mut().get::<&mut PetVitals>(pet) {
        vitals.hp = vitals.hp.saturating_sub(dmg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    // ─── Rng determinism ───────────────────────────────────────

    #[test]
    fn rng_same_seed_same_stream() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn rng_f64_in_range() {
        let mut r = Rng::from_seed(7);
        for _ in 0..1000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn rng_range_f64_bounded() {
        let mut r = Rng::from_seed(9);
        for _ in 0..1000 {
            let v = r.range_f64(0.8, 1.2);
            assert!((0.8..1.2).contains(&v));
        }
    }

    #[test]
    fn hash_seed_has_avalanche() {
        // Different inputs → different outputs (not a formal avalanche test,
        // just a smoke check)
        let samples: std::collections::HashSet<u64> =
            (0..50).map(|i| hash_seed(1_000_000, i)).collect();
        assert_eq!(samples.len(), 50);
    }

    // ─── Hit chance formula ────────────────────────────────────

    #[test]
    fn hit_chance_clamps_low() {
        // Very weak pet vs very tanky mob — floor at 5%
        let h = hit_chance(1, 1, 1000, 1000);
        assert!((0.04..=0.06).contains(&h), "got {h}");
    }

    #[test]
    fn hit_chance_clamps_high() {
        // Very strong pet vs weak mob — ceiling at 95%
        let h = hit_chance(500, 500, 1, 1);
        assert!((0.94..=0.96).contains(&h));
    }

    #[test]
    fn hit_chance_monotonic_in_atk() {
        // Higher ATK → higher (or equal, when clamped) hit chance
        let a = hit_chance(10, 5, 20, 5);
        let b = hit_chance(20, 5, 20, 5);
        assert!(b >= a);
    }

    #[test]
    fn hit_chance_handles_zero_def() {
        // Zero def shouldn't divide-by-zero; max(1) guard
        let _ = hit_chance(10, 5, 0, 0);
    }

    // ─── End-to-end combat tick ────────────────────────────────

    fn fresh() -> (Connection, GameWorld) {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        let world = GameWorld::load_or_init(&conn).unwrap();
        (conn, world)
    }

    fn spawn_mob(conn: &Connection, zone: &str, kind: MobKind, name: &str, hp: u32, atk: u32, def: u32) -> i64 {
        conn.execute(
            "INSERT INTO mobs (zone_id, kind, name, hp, hp_max, atk, def, difficulty, spawned_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 1, 100)",
            rusqlite::params![zone, kind.as_str(), name, hp as i64, atk as i64, def as i64],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn empty_zone_produces_empty_summary() {
        let (conn, mut world) = fresh();
        let s = run_tick(&conn, &mut world, 100).unwrap();
        assert_eq!(s.attacks, 0);
        assert_eq!(s.defeats.len(), 0);
    }

    #[test]
    fn one_hp_mob_gets_defeated_when_hit() {
        // Make sure at least one scenario reliably defeats: pet_atk large,
        // mob hp=1, guaranteed hit via clamp at 95%
        let (conn, mut world) = fresh();
        spawn_mob(&conn, "rust", MobKind::Ghost, "dummy", 1, 1, 1);

        // Run several ticks — one of them will land a hit with 95% hit rate
        let mut defeated = false;
        for t in 100..120 {
            let s = run_tick(&conn, &mut world, t).unwrap();
            if !s.defeats.is_empty() {
                defeated = true;
                break;
            }
        }
        assert!(defeated, "1-HP mob should be defeated within 20 ticks");
    }

    #[test]
    fn defeated_mobs_not_re_attacked() {
        // Defeat a mob, next tick should find zero alive → empty summary
        let (conn, mut world) = fresh();
        let id = spawn_mob(&conn, "rust", MobKind::Ghost, "x", 1, 1, 1);
        conn.execute(
            "UPDATE mobs SET defeated_at = 999 WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();

        let s = run_tick(&conn, &mut world, 200).unwrap();
        assert_eq!(s.attacks, 0);
    }

    #[test]
    fn mob_counter_damages_pet() {
        // Strong mob, weak pet — pet HP should drop even if pet misses
        let (conn, mut world) = fresh();
        spawn_mob(&conn, "rust", MobKind::Boss, "big", 1000, 100, 10);

        let hp_before = world.world().get::<&PetVitals>(world.pet()).unwrap().hp;
        let s = run_tick(&conn, &mut world, 500).unwrap();
        let hp_after = world.world().get::<&PetVitals>(world.pet()).unwrap().hp;

        assert!(s.pet_damage_taken > 0);
        assert!(hp_after < hp_before);
    }

    #[test]
    fn pet_hp_cannot_go_negative() {
        // Saturating_sub guards against u32 underflow
        let (conn, mut world) = fresh();
        for i in 0..5 {
            spawn_mob(&conn, "rust", MobKind::Boss, &format!("b{i}"), 1000, 200, 10);
        }
        for t in 100..200 {
            run_tick(&conn, &mut world, t).unwrap();
        }
        let hp = world.world().get::<&PetVitals>(world.pet()).unwrap().hp;
        // Must be a valid u32, not underflowed to huge number
        assert!(hp < 1_000_000, "hp underflowed: {hp}");
    }

    #[test]
    fn max_attacks_per_tick_cap_respected() {
        let (conn, mut world) = fresh();
        for i in 0..100 {
            spawn_mob(&conn, "rust", MobKind::Zombie, &format!("z{i}"), 1, 1, 1);
        }
        let s = run_tick(&conn, &mut world, 100).unwrap();
        assert!(s.attacks <= MAX_ATTACKS_PER_TICK);
    }

    #[test]
    fn attacks_only_pet_home_zone() {
        let (conn, mut world) = fresh();
        // Pet's home village is "rust" (default seed). Spawn only python mobs.
        for i in 0..3 {
            spawn_mob(&conn, "python", MobKind::Zombie, &format!("py-z{i}"), 1, 1, 1);
        }
        let s = run_tick(&conn, &mut world, 100).unwrap();
        assert_eq!(s.attacks, 0, "pet should not attack other zones' mobs");
    }

    #[test]
    fn defeated_mob_hp_is_zero() {
        let (conn, mut world) = fresh();
        let id = spawn_mob(&conn, "rust", MobKind::Ghost, "weak", 1, 1, 1);

        for t in 100..130 {
            let s = run_tick(&conn, &mut world, t).unwrap();
            if !s.defeats.is_empty() {
                let hp: i64 = conn
                    .query_row("SELECT hp FROM mobs WHERE id = ?1", rusqlite::params![id], |r| {
                        r.get(0)
                    })
                    .unwrap();
                assert_eq!(hp, 0);
                return;
            }
        }
        panic!("defeat didn't happen within 30 ticks");
    }
}
