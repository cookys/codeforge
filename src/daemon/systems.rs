//! Per-tick systems that mutate GameWorld.
//!
//! Each system is a pure function over the ECS. Order matters only as noted.
//! Phase 2a P3 scope: xp / regen / levelup / message. Combat/loot systems
//! live in P2b.

use super::ecs::{GameWorld, LastMessage, PetLevel, PetStats, PetVitals};

/// HP regeneration per tick. Caps at hp_max.
/// Default: +5 HP / tick. Future: modulate by village ability / mood.
pub const REGEN_PER_TICK: u32 = 5;

pub fn regen_hp(gw: &mut GameWorld) {
    let pet = gw.pet();
    if let Ok(mut vitals) = gw.world_mut().get::<&mut PetVitals>(pet) {
        let new_hp = vitals.hp.saturating_add(REGEN_PER_TICK);
        vitals.hp = new_hp.min(vitals.hp_max);
    }
}

/// Apply an XP delta to the pet. Positive only (no XP loss design in P2a).
/// Does NOT handle level-up — call `check_levelup` after.
pub fn apply_xp(gw: &mut GameWorld, delta: u32) {
    let pet = gw.pet();
    if let Ok(mut level) = gw.world_mut().get::<&mut PetLevel>(pet) {
        level.xp = level.xp.saturating_add(delta);
    }
}

/// Check for level-up conditions; may promote level multiple times if xp is
/// far past threshold (e.g. after a big event batch).
/// Level progression: xp_to_next *= 1.5 per level; each stat +1 on level-up.
/// hp_max also +10 on level-up (and hp restored).
pub fn check_levelup(gw: &mut GameWorld) {
    let pet = gw.pet();
    let mut ups = 0u32;

    loop {
        let needs_up = {
            let level = match gw.world().get::<&PetLevel>(pet) {
                Ok(l) => l,
                Err(_) => return,
            };
            level.xp >= level.xp_to_next
        };
        if !needs_up {
            break;
        }

        // XP + xp_to_next advance
        {
            let mut level = match gw.world_mut().get::<&mut PetLevel>(pet) {
                Ok(l) => l,
                Err(_) => return,
            };
            level.xp = level.xp.saturating_sub(level.xp_to_next);
            level.level = level.level.saturating_add(1);
            // xp_to_next * 1.5, guarded against overflow
            level.xp_to_next = level
                .xp_to_next
                .saturating_add(level.xp_to_next / 2)
                .max(100);
        }

        // Stats +1 each
        if let Ok(mut stats) = gw.world_mut().get::<&mut PetStats>(pet) {
            stats.atk = stats.atk.saturating_add(1);
            stats.def = stats.def.saturating_add(1);
            stats.sup = stats.sup.saturating_add(1);
            stats.ver = stats.ver.saturating_add(1);
        }

        // HP_max +10; restore HP
        if let Ok(mut vitals) = gw.world_mut().get::<&mut PetVitals>(pet) {
            vitals.hp_max = vitals.hp_max.saturating_add(10);
            vitals.hp = vitals.hp_max;
        }

        ups += 1;
        // Safety valve: cap one tick's level-ups to avoid pathological loops
        if ups >= 100 {
            break;
        }
    }
}

/// Write a speech bubble text. Phase 3b will replace with Haiku commentary.
/// P2a: available but not yet wired into tick pipeline.
#[allow(dead_code)]
pub fn set_message(gw: &mut GameWorld, text: impl Into<String>, tick_stamp: u64) {
    let pet = gw.pet();
    let text = text.into();
    // Insert-or-replace via hecs — insert_one on existing replaces.
    let _ = gw
        .world_mut()
        .insert_one(pet, LastMessage { text, tick_stamp });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh_world() -> GameWorld {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        GameWorld::load_or_init(&conn).unwrap()
    }

    #[test]
    fn regen_caps_at_hp_max() {
        let mut gw = fresh_world();
        // Default: hp = 10, hp_max = 10 — regen should be a no-op
        regen_hp(&mut gw);
        let v = gw.world().get::<&PetVitals>(gw.pet()).unwrap();
        assert_eq!(v.hp, 10);
    }

    #[test]
    fn regen_heals_damaged_pet() {
        let mut gw = fresh_world();
        let pet = gw.pet();
        {
            let mut v = gw.world_mut().get::<&mut PetVitals>(pet).unwrap();
            v.hp = 1;
        }
        regen_hp(&mut gw);
        let v = gw.world().get::<&PetVitals>(pet).unwrap();
        assert_eq!(v.hp, 1 + REGEN_PER_TICK);
    }

    #[test]
    fn apply_xp_increments_xp() {
        let mut gw = fresh_world();
        apply_xp(&mut gw, 30);
        apply_xp(&mut gw, 15);
        let l = gw.world().get::<&PetLevel>(gw.pet()).unwrap();
        assert_eq!(l.xp, 45);
    }

    #[test]
    fn apply_xp_saturates_not_panics_on_overflow() {
        let mut gw = fresh_world();
        apply_xp(&mut gw, u32::MAX);
        apply_xp(&mut gw, u32::MAX);
        let l = gw.world().get::<&PetLevel>(gw.pet()).unwrap();
        assert_eq!(l.xp, u32::MAX);
    }

    #[test]
    fn levelup_promotes_when_threshold_reached() {
        let mut gw = fresh_world();
        apply_xp(&mut gw, 100); // default xp_to_next = 100
        check_levelup(&mut gw);
        let l = gw.world().get::<&PetLevel>(gw.pet()).unwrap();
        assert_eq!(l.level, 2);
        assert_eq!(l.xp, 0);
        assert_eq!(l.xp_to_next, 150); // 100 * 1.5
        let s = gw.world().get::<&PetStats>(gw.pet()).unwrap();
        assert_eq!(s.atk, 11);
        assert_eq!(s.ver, 11);
        let v = gw.world().get::<&PetVitals>(gw.pet()).unwrap();
        assert_eq!(v.hp_max, 20); // 10 + 10
        assert_eq!(v.hp, 20); // restored to hp_max
    }

    #[test]
    fn levelup_cascades_on_big_xp_batch() {
        let mut gw = fresh_world();
        apply_xp(&mut gw, 10_000);
        check_levelup(&mut gw);
        let l = gw.world().get::<&PetLevel>(gw.pet()).unwrap();
        // Large XP should produce multiple level-ups
        assert!(
            l.level > 2,
            "expected multiple level-ups, got level {}",
            l.level
        );
    }

    #[test]
    fn levelup_no_panic_on_pathological_input() {
        let mut gw = fresh_world();
        apply_xp(&mut gw, u32::MAX);
        check_levelup(&mut gw); // Must not infinite-loop or panic
        let l = gw.world().get::<&PetLevel>(gw.pet()).unwrap();
        assert!(l.level > 1); // Promoted at least once
    }

    #[test]
    fn set_message_records_text() {
        let mut gw = fresh_world();
        set_message(&mut gw, "hi", 42);
        let m = gw.world().get::<&LastMessage>(gw.pet()).unwrap();
        assert_eq!(m.text, "hi");
        assert_eq!(m.tick_stamp, 42);
    }

    #[test]
    fn set_message_replaces_previous() {
        let mut gw = fresh_world();
        set_message(&mut gw, "first", 10);
        set_message(&mut gw, "second", 11);
        let m = gw.world().get::<&LastMessage>(gw.pet()).unwrap();
        assert_eq!(m.text, "second");
        assert_eq!(m.tick_stamp, 11);
    }
}
