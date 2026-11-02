//! MOB model — the monsters daemon scans out of the codebase.
//!
//! Six kinds per `doc/specs/codeforge-mud-engine.md` §2:
//! - **boss**: function > 100 lines; high HP, lots of XP
//! - **elite**: high cyclomatic complexity; medium HP, medium XP
//! - **zombie**: TODO/FIXME cluster; low HP, many, TODO Cleaner loot
//! - **ghost**: unused imports / dead code; one-shot, drops Dead Code Crystal
//! - **doppelganger**: duplicate code block across two files
//!   (`dup_detect`); splits on defeat — see [`child_stats`]
//! - **void**: missing test coverage (Phase 3 — deferred)

use std::fmt;

/// Doppelganger split cap: scanner-spawned mobs are generation 0, each
/// split adds 1, and a mob at this generation no longer splits — so one
/// scanner-spawned Doppelganger yields at most two descendants (B10,
/// proposal Q2 = A).
pub const MAX_SPLIT_GENERATION: u32 = 2;

/// Stats of a split child from its parent's `hp_max` / `atk` / `def`:
/// each × 0.7, rounded up, at least 1 (B10, proposal Q3 = A). Integer
/// arithmetic so the result is exact. `difficulty` is inherited as-is.
pub fn child_stats(hp_max: u32, atk: u32, def: u32) -> (u32, u32, u32) {
    let scale = |x: u32| (((x as u64) * 7).div_ceil(10).max(1)) as u32;
    (scale(hp_max), scale(atk), scale(def))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MobKind {
    Boss,
    Elite,
    Zombie,
    Ghost,
    Doppelganger,
    Void,
}

impl MobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boss => "boss",
            Self::Elite => "elite",
            Self::Zombie => "zombie",
            Self::Ghost => "ghost",
            Self::Doppelganger => "doppelganger",
            Self::Void => "void",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "boss" => Self::Boss,
            "elite" => Self::Elite,
            "zombie" => Self::Zombie,
            "ghost" => Self::Ghost,
            "doppelganger" => Self::Doppelganger,
            "void" => Self::Void,
            _ => return None,
        })
    }

    /// Base stats (hp, atk, def, difficulty) per kind.
    /// Spec §2 defines the flavor; numbers tuned so a Lv.1 pet (atk 10-18)
    /// can eventually grind zombies/ghosts while bosses require many ticks.
    pub fn base_stats(self) -> (u32, u32, u32, u32) {
        match self {
            Self::Boss => (200, 25, 20, 5),
            Self::Elite => (100, 18, 12, 3),
            Self::Zombie => (30, 12, 5, 1),
            Self::Ghost => (10, 8, 3, 1),
            Self::Doppelganger => (80, 15, 10, 4),
            Self::Void => (60, 20, 8, 3),
        }
    }
}

impl fmt::Display for MobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Specification for a MOB to be spawned. Scanner produces these; persistence
/// layer upserts into `mobs` table (INSERT OR IGNORE against the partial
/// unique index keyed on (zone_id, kind, name) WHERE defeated_at IS NULL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobSpec {
    pub zone_id: String,
    pub kind: MobKind,
    pub name: String,
    pub hp: u32,
    pub hp_max: u32,
    pub atk: u32,
    pub def: u32,
    pub difficulty: u32,
    /// Phase 2c: relative path from the scan root. Enables Local Map to
    /// group MOBs by top-level directory. None for legacy / non-file mobs.
    pub origin_path: Option<String>,
}

impl MobSpec {
    pub fn new(zone_id: &str, kind: MobKind, name: String) -> Self {
        let (hp, atk, def, difficulty) = kind.base_stats();
        Self {
            zone_id: zone_id.to_string(),
            kind,
            name,
            hp,
            hp_max: hp,
            atk,
            def,
            difficulty,
            origin_path: None,
        }
    }

    /// Phase 2c: attach a repo-relative origin path (builder-style).
    pub fn with_origin_path(mut self, path: impl Into<String>) -> Self {
        self.origin_path = Some(path.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in [
            MobKind::Boss,
            MobKind::Elite,
            MobKind::Zombie,
            MobKind::Ghost,
            MobKind::Doppelganger,
            MobKind::Void,
        ] {
            assert_eq!(MobKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(MobKind::from_str("unknown"), None);
    }

    #[test]
    fn base_stats_ordering_matches_spec() {
        // Spec intuition: boss > elite > doppelganger > void > zombie > ghost by HP
        let (boss_hp, _, _, _) = MobKind::Boss.base_stats();
        let (elite_hp, _, _, _) = MobKind::Elite.base_stats();
        let (dop_hp, _, _, _) = MobKind::Doppelganger.base_stats();
        let (void_hp, _, _, _) = MobKind::Void.base_stats();
        let (zombie_hp, _, _, _) = MobKind::Zombie.base_stats();
        let (ghost_hp, _, _, _) = MobKind::Ghost.base_stats();
        assert!(boss_hp > elite_hp);
        assert!(elite_hp > dop_hp);
        assert!(dop_hp > void_hp);
        assert!(void_hp > zombie_hp);
        assert!(zombie_hp > ghost_hp);
    }

    #[test]
    fn spec_new_fills_hp_max_from_kind() {
        let s = MobSpec::new("rust", MobKind::Boss, "src/foo.rs".to_string());
        assert_eq!(s.hp, s.hp_max);
        let (expected_hp, _, _, _) = MobKind::Boss.base_stats();
        assert_eq!(s.hp_max, expected_hp);
    }

    #[test]
    fn child_stats_scale_by_seventy_percent_ceil_min_one() {
        assert_eq!(child_stats(80, 15, 3), (56, 11, 3));
        assert_eq!(child_stats(56, 11, 3), (40, 8, 3));
        assert_eq!(child_stats(1, 1, 0), (1, 1, 1));
    }
}
