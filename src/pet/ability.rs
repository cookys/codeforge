//! Ability unlock lookup (spec §2.5 通用 Ability 解鎖表).
//!
//! Phase 3d uses this only to render the "next unlock" anchor next to the
//! XP bar — we surface the named target the player is working toward. The
//! actual ability *effects* (Quick Eye boosting Ghost hit rate, Focus Strike
//! granting Boss crits, etc.) are NOT implemented here; that belongs to a
//! combat-deepening phase.
//!
//! Stored as a `const` slice rather than a DB table because:
//!  - The list is small, fixed, and authored alongside spec text.
//!  - No migrations needed when we tweak names.
//!  - Lookup is O(6) — a binary search would be overkill.
//!
//! Sort invariant: entries are strictly ordered by `required_level`. Tests
//! assert this so a future edit cannot silently break `next_unlock()`.

// P1 lays this down; P3 (statusline + TUI render) is what first *reads*
// it from non-test code. Until then suppress dead_code without polluting
// the individual item attributes.
#![allow(dead_code)]

/// A single named unlock. `kind` is the descriptive class from the spec
/// (`passive` / `active` / `village`) and is kept around for future UI
/// hints; current consumers only need `name` and `required_level`.
#[derive(Debug, Clone, Copy)]
pub struct AbilityUnlock {
    pub required_level: u32,
    pub name: &'static str,
    pub kind: &'static str,
}

/// Spec §2.5 通用 Ability 解鎖表. Lv 50 is the village-legendary gate —
/// the name shown here is a placeholder; the actual legendary comes from
/// the pet's village (Iron Skin / Scripted / Concurrent / …) and will
/// slot in when the legendary system lands.
pub const ABILITY_UNLOCKS: &[AbilityUnlock] = &[
    AbilityUnlock { required_level: 5,  name: "Quick Eye",     kind: "passive" },
    AbilityUnlock { required_level: 10, name: "Focus Strike",  kind: "active"  },
    AbilityUnlock { required_level: 15, name: "Tome Sense",    kind: "passive" },
    AbilityUnlock { required_level: 20, name: "Village Aura",  kind: "passive" },
    AbilityUnlock { required_level: 30, name: "Memory Recall", kind: "passive" },
    AbilityUnlock { required_level: 50, name: "Legendary",     kind: "village" },
];

/// Return the next ability the pet will unlock, or `None` once every entry
/// in the table has been passed. Used by statusline + TUI pet panel.
pub fn next_unlock(level: u32) -> Option<&'static AbilityUnlock> {
    ABILITY_UNLOCKS.iter().find(|u| u.required_level > level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_strictly_increasing() {
        // Guard: next_unlock relies on the first `required_level > level`
        // being the correct answer. Out-of-order edits would silently
        // ship wrong hints.
        for pair in ABILITY_UNLOCKS.windows(2) {
            assert!(
                pair[0].required_level < pair[1].required_level,
                "ABILITY_UNLOCKS not strictly increasing around level {}",
                pair[0].required_level
            );
        }
    }

    #[test]
    fn next_unlock_lv1_is_quick_eye() {
        let u = next_unlock(1).expect("at least one target at Lv 1");
        assert_eq!(u.name, "Quick Eye");
        assert_eq!(u.required_level, 5);
    }

    #[test]
    fn next_unlock_at_exact_boundary_skips_current() {
        // Lv 10 means the player *has* Focus Strike; the next target is
        // Tome Sense, not Focus Strike itself.
        let u = next_unlock(10).expect("Tome Sense should be next");
        assert_eq!(u.name, "Tome Sense");
        assert_eq!(u.required_level, 15);
    }

    #[test]
    fn next_unlock_mid_stride_returns_nearest_future() {
        let u = next_unlock(7).expect("Focus Strike at Lv 10 is next");
        assert_eq!(u.required_level, 10);
        let u = next_unlock(12).unwrap();
        assert_eq!(u.required_level, 15);
        let u = next_unlock(25).unwrap();
        assert_eq!(u.required_level, 30);
    }

    #[test]
    fn next_unlock_between_30_and_50_points_at_legendary() {
        let u = next_unlock(35).expect("Legendary at Lv 50");
        assert_eq!(u.required_level, 50);
        assert_eq!(u.kind, "village");
    }

    #[test]
    fn next_unlock_none_past_legendary() {
        assert!(next_unlock(50).is_none(), "no unlocks past Lv 50");
        assert!(next_unlock(999).is_none());
    }
}
