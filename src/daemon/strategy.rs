//! Strategy Mode — Phase 3b, spec §2 Strategy Mode (lines 147-159).
//!
//! Four playstyles the user toggles via `codeforge strategy <name>`:
//!
//! | Strategy   | ATK mult | DEF mult | Priority order                 |
//! |------------|----------|----------|--------------------------------|
//! | Aggressive | 1.3x     | 0.8x     | Boss > Elite > Zombie > rest   |
//! | Defensive  | 0.9x     | 1.4x     | Ghost > Zombie > Elite > rest  |
//! | Explorer   | 1.0x     | 1.0x     | no bias within zone (id order) |
//! | Scholar    | 0.8x     | 1.0x     | Boss > Elite > rest (tome)     |
//!
//! Explorer = the Phase 2b baseline — chosen as default so upgrading from v5
//! to v6 produces zero behavior change for existing players.

use crate::daemon::mob::MobKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Aggressive,
    Defensive,
    Explorer,
    Scholar,
}

impl Strategy {
    pub const ALL: [Strategy; 4] = [
        Self::Aggressive,
        Self::Defensive,
        Self::Explorer,
        Self::Scholar,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aggressive => "aggressive",
            Self::Defensive => "defensive",
            Self::Explorer => "explorer",
            Self::Scholar => "scholar",
        }
    }

    /// Case-insensitive parse. Accepts full names; short forms (agg/def/exp/sch)
    /// are intentionally NOT accepted here — a short-name typo like "aggro"
    /// should fail loudly rather than silently matching something else.
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "aggressive" => Self::Aggressive,
            "defensive" => Self::Defensive,
            "explorer" => Self::Explorer,
            "scholar" => Self::Scholar,
            _ => return None,
        })
    }

    /// Short 3-letter tag for the statusline segment.
    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Aggressive => "agg",
            Self::Defensive => "def",
            Self::Explorer => "exp",
            Self::Scholar => "sch",
        }
    }

    /// Damage multiplier applied to `pet.atk` before the 0.8..1.2 roll.
    pub fn atk_mult(self) -> f64 {
        match self {
            Self::Aggressive => 1.3,
            Self::Defensive => 0.9,
            Self::Explorer => 1.0,
            Self::Scholar => 0.8,
        }
    }

    /// DEF scaling applied before mob counter-attack subtraction. >1.0 means
    /// the pet absorbs more of the hit (Defensive); <1.0 means it takes more
    /// (Aggressive trades armor for sword).
    pub fn def_mult(self) -> f64 {
        match self {
            Self::Aggressive => 0.8,
            Self::Defensive => 1.4,
            Self::Explorer => 1.0,
            Self::Scholar => 1.0,
        }
    }

    /// Lower = attack this mob first. Ties broken by mob.id in the caller,
    /// giving deterministic ordering across ticks for a given alive set.
    ///
    /// "rest" bucket (everyone not explicitly preferred) gets the same key,
    /// so Explorer — which has no preference — produces `|_| 0` and falls
    /// through to the id-tiebreaker.
    pub fn priority_order(self, kind: MobKind) -> u8 {
        match self {
            Self::Aggressive => match kind {
                MobKind::Boss => 0,
                MobKind::Elite => 1,
                MobKind::Zombie => 2,
                _ => 3,
            },
            Self::Defensive => match kind {
                MobKind::Ghost => 0,
                MobKind::Zombie => 1,
                MobKind::Elite => 2,
                _ => 3,
            },
            // Spec §2 describes Explorer as "優先未探索 Zone 的 MOB"
            // (cross-zone priority). Phase 3b is single-zone (pet's home
            // village), so the cross-zone dimension has no meaning yet —
            // Explorer degenerates to within-zone id order. Phase 3a adds
            // Zone unlock + multi-zone raids and will revisit this branch.
            Self::Explorer => 0,
            // Scholar chases tome-dropping mobs (Boss + Elite per spec §2
            // Loot table). Zombies/Ghosts drop cleaner-class items that
            // aren't tomes, so they sink to the "rest" bucket.
            Self::Scholar => match kind {
                MobKind::Boss => 0,
                MobKind::Elite => 1,
                _ => 2,
            },
        }
    }
}

pub const DEFAULT_STRATEGY: Strategy = Strategy::Explorer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_variants() {
        for s in Strategy::ALL {
            assert_eq!(Strategy::from_str(s.as_str()), Some(s));
        }
    }

    #[test]
    fn from_str_is_case_insensitive_and_trims() {
        assert_eq!(Strategy::from_str("Aggressive"), Some(Strategy::Aggressive));
        assert_eq!(Strategy::from_str("SCHOLAR"), Some(Strategy::Scholar));
        assert_eq!(
            Strategy::from_str("  defensive  "),
            Some(Strategy::Defensive)
        );
    }

    #[test]
    fn from_str_rejects_short_names() {
        assert_eq!(Strategy::from_str("agg"), None);
        assert_eq!(Strategy::from_str("aggro"), None);
        assert_eq!(Strategy::from_str(""), None);
    }

    #[test]
    fn atk_mult_ordering_matches_spec() {
        // Aggressive hits hardest, Scholar softest
        assert!(Strategy::Aggressive.atk_mult() > Strategy::Explorer.atk_mult());
        assert!(Strategy::Explorer.atk_mult() > Strategy::Defensive.atk_mult());
        assert!(Strategy::Defensive.atk_mult() > Strategy::Scholar.atk_mult());
    }

    #[test]
    fn def_mult_defensive_is_highest() {
        let vals = Strategy::ALL.map(|s| s.def_mult());
        let max = vals.iter().cloned().fold(0.0_f64, f64::max);
        assert_eq!(max, Strategy::Defensive.def_mult());
    }

    #[test]
    fn aggressive_prioritises_boss_over_zombie() {
        let s = Strategy::Aggressive;
        assert!(s.priority_order(MobKind::Boss) < s.priority_order(MobKind::Zombie));
        assert!(s.priority_order(MobKind::Boss) < s.priority_order(MobKind::Ghost));
        assert!(s.priority_order(MobKind::Elite) < s.priority_order(MobKind::Ghost));
    }

    #[test]
    fn defensive_prioritises_ghost_over_elite() {
        let s = Strategy::Defensive;
        assert!(s.priority_order(MobKind::Ghost) < s.priority_order(MobKind::Elite));
        assert!(s.priority_order(MobKind::Zombie) < s.priority_order(MobKind::Elite));
    }

    #[test]
    fn scholar_prioritises_tome_droppers() {
        let s = Strategy::Scholar;
        assert!(s.priority_order(MobKind::Boss) < s.priority_order(MobKind::Zombie));
        assert!(s.priority_order(MobKind::Elite) < s.priority_order(MobKind::Ghost));
    }

    #[test]
    fn explorer_has_no_bias() {
        let s = Strategy::Explorer;
        for kind in [
            MobKind::Boss,
            MobKind::Elite,
            MobKind::Zombie,
            MobKind::Ghost,
            MobKind::Doppelganger,
            MobKind::Void,
        ] {
            assert_eq!(s.priority_order(kind), 0, "{kind:?} should be neutral");
        }
    }

    #[test]
    fn short_tags_are_unique_and_short() {
        use std::collections::HashSet;
        let tags: HashSet<_> = Strategy::ALL.iter().map(|s| s.short_tag()).collect();
        assert_eq!(tags.len(), 4);
        for s in Strategy::ALL {
            assert!(s.short_tag().len() <= 8);
        }
    }

    #[test]
    fn default_is_explorer_for_backcompat() {
        // Explorer produces 1.0/1.0 multipliers + no priority bias,
        // matching Phase 2b behaviour pre-strategy.
        assert_eq!(DEFAULT_STRATEGY, Strategy::Explorer);
        assert_eq!(DEFAULT_STRATEGY.atk_mult(), 1.0);
        assert_eq!(DEFAULT_STRATEGY.def_mult(), 1.0);
    }
}
