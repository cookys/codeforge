//! Phase 3e recipe table. Declarative data, referenced by `craft::craft`
//! and `craft::use_item` — keep this file opinion-free so new recipes can
//! be added by editing one list.
//!
//! Matching the spec §3.5:
//!
//! | Recipe              | Ingredients            | Effect on use                       |
//! |---------------------|------------------------|-------------------------------------|
//! | Refactor Blueprint  | 3× Pattern Fragment    | reduce_difficulty, 7 days, zone     |
//! | Ghost Repellent     | 5× Dead Code Crystal   | suppress_ghost_spawn, 3 days, zone  |
//! | Doppelganger Ward   | 2× Abstract Gem        | suppress_doppelganger_split, 2 days |
//!
//! All ingredient `kind`/`name` strings must match `daemon::loot` so
//! drops accumulate into the same `loot_inventory` rows the recipes
//! debit from.

use super::effects::{Effect, EffectKind};

/// One ingredient tuple inside a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ingredient {
    pub kind: &'static str,
    pub name: &'static str,
    pub quantity: u32,
}

/// Crafted output row. `kind` distinguishes product categories in the
/// inventory list ("blueprint" / "repellent" / "ward"); `name` is the
/// user-facing identifier users type in `codeforge use <name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Product {
    pub kind: &'static str,
    pub name: &'static str,
    pub quantity: u32,
}

/// A craftable recipe. `effect` is `Some` for consumables that can be
/// `codeforge use`-d; `None` would mean "craft for the collection / XP"
/// but we don't have non-consumable crafts yet.
#[derive(Debug, Clone)]
pub struct Recipe {
    pub product: Product,
    pub ingredients: &'static [Ingredient],
    pub effect: Option<Effect>,
}

/// The canonical recipe list. Returned as a slice rather than a HashMap
/// so iteration order is stable for display (matches spec table order).
pub fn recipes() -> &'static [Recipe] {
    static RECIPES: &[Recipe] = &[
        Recipe {
            product: Product {
                kind: "blueprint",
                name: "Refactor Blueprint",
                quantity: 1,
            },
            ingredients: &[Ingredient {
                kind: "fragment",
                name: "Pattern Fragment",
                quantity: 3,
            }],
            effect: Some(Effect {
                kind: EffectKind::ReduceDifficulty,
                duration_days: 7,
            }),
        },
        Recipe {
            product: Product {
                kind: "repellent",
                name: "Ghost Repellent",
                quantity: 1,
            },
            ingredients: &[Ingredient {
                kind: "crystal",
                name: "Dead Code Crystal",
                quantity: 5,
            }],
            effect: Some(Effect {
                kind: EffectKind::SuppressGhostSpawn,
                duration_days: 3,
            }),
        },
        Recipe {
            product: Product {
                kind: "ward",
                name: "Doppelganger Ward",
                quantity: 1,
            },
            ingredients: &[Ingredient {
                kind: "gem",
                name: "Abstract Gem",
                quantity: 2,
            }],
            effect: Some(Effect {
                kind: EffectKind::SuppressDoppelgangerSplit,
                duration_days: 2,
            }),
        },
    ];
    RECIPES
}

/// Case-insensitive lookup by product name. Returns `None` for unknown
/// names (CLI surfaces the error).
pub fn find_recipe(name: &str) -> Option<&'static Recipe> {
    let lower = name.trim().to_ascii_lowercase();
    recipes().iter().find(|r| r.product.name.to_ascii_lowercase() == lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recipe_has_unique_product_name() {
        let names: Vec<_> = recipes().iter().map(|r| r.product.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            names.len(),
            "duplicate product name detected — user-facing collision"
        );
    }

    #[test]
    fn every_recipe_has_at_least_one_ingredient() {
        for r in recipes() {
            assert!(
                !r.ingredients.is_empty(),
                "recipe {:?} has no ingredients — would free-mint loot",
                r.product.name
            );
        }
    }

    #[test]
    fn every_recipe_has_effect_today() {
        // Invariant: every crafted item is also usable. Without an effect
        // the craft → use loop has no point. If a non-consumable recipe
        // is ever added, relax this but keep a unit test for the crafts
        // that SHOULD have an effect (the three §3.5 recipes).
        for r in recipes() {
            assert!(r.effect.is_some(), "recipe {:?} missing effect", r.product.name);
        }
    }

    #[test]
    fn find_recipe_case_insensitive() {
        assert!(find_recipe("refactor blueprint").is_some());
        assert!(find_recipe("REFACTOR BLUEPRINT").is_some());
        assert!(find_recipe("  Ghost Repellent  ").is_some());
    }

    #[test]
    fn find_recipe_unknown_returns_none() {
        assert!(find_recipe("Nonexistent Gadget").is_none());
    }

    #[test]
    fn recipe_durations_match_spec() {
        assert_eq!(
            find_recipe("Refactor Blueprint")
                .unwrap()
                .effect
                .as_ref()
                .unwrap()
                .duration_days,
            7
        );
        assert_eq!(
            find_recipe("Ghost Repellent")
                .unwrap()
                .effect
                .as_ref()
                .unwrap()
                .duration_days,
            3
        );
        assert_eq!(
            find_recipe("Doppelganger Ward")
                .unwrap()
                .effect
                .as_ref()
                .unwrap()
                .duration_days,
            2
        );
    }
}
