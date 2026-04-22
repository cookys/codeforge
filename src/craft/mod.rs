//! Phase 3e — Loot Crafting + Active Item use.
//!
//! Two halves:
//! 1. **Recipes** (pure data): declare `Recipe`s with ingredient loot
//!    tuples + product tuple + an optional `Effect`. Crafting is a
//!    debit-materials → credit-product transaction against
//!    `loot_inventory`.
//! 2. **Effects** (DB writes): `apply_effect` inserts an `active_effects`
//!    row with an absolute `expires_at` unix-seconds. The daemon + CLI
//!    read the live set via `active_effects_for_zone`.
//!
//! Keeping the module pure-data lets tests cover the recipe table and the
//! materials math without a DB; the few `&Connection`-taking helpers are
//! thin wrappers over SQL.

pub mod effects;
pub mod recipes;

pub use effects::{active_effects_for_zone, is_effect_active, ActiveEffect, EffectKind};
pub use recipes::{find_recipe, recipes, Recipe};

use anyhow::{bail, Result};
use rusqlite::Connection;

/// Execute `recipe` against the current `loot_inventory`. Debits every
/// ingredient, credits the product. All-or-nothing: wrapped in a single
/// transaction so an insufficient-material check can't leave the inventory
/// in a half-consumed state.
///
/// `now` is unix seconds for `last_acquired_at` on the credited product.
pub fn craft(conn: &mut Connection, recipe: &Recipe, now: i64) -> Result<()> {
    let tx = conn.transaction()?;
    // Ingredients: debit each.
    for ing in recipe.ingredients {
        let have: i64 = tx
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE kind = ?1 AND name = ?2",
                rusqlite::params![ing.kind, ing.name],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if (have as u32) < ing.quantity {
            bail!(
                "材料不足：需要 {}× {}，持有 {}×",
                ing.quantity, ing.name, have
            );
        }
        // Decrement — if it would hit zero, delete the row so the
        // CHECK (quantity > 0) constraint isn't violated and the
        // inventory listing stays tidy.
        let remaining = have - ing.quantity as i64;
        if remaining == 0 {
            tx.execute(
                "DELETE FROM loot_inventory WHERE kind = ?1 AND name = ?2",
                rusqlite::params![ing.kind, ing.name],
            )?;
        } else {
            tx.execute(
                "UPDATE loot_inventory SET quantity = ?1
                 WHERE kind = ?2 AND name = ?3",
                rusqlite::params![remaining, ing.kind, ing.name],
            )?;
        }
    }
    // Product: upsert.
    tx.execute(
        "INSERT INTO loot_inventory
             (kind, name, quantity, first_acquired_at, last_acquired_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(kind, name) DO UPDATE SET
             quantity = loot_inventory.quantity + excluded.quantity,
             last_acquired_at = excluded.last_acquired_at",
        rusqlite::params![
            recipe.product.kind,
            recipe.product.name,
            recipe.product.quantity as i64,
            now,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// Consume one unit of `item_name` from inventory and insert a matching
/// `active_effects` row. `zone_id = None` for global effects. Returns the
/// effect's `expires_at` so the CLI can print a human-readable line.
pub fn use_item(
    conn: &mut Connection,
    item_name: &str,
    zone_id: Option<&str>,
    now: i64,
) -> Result<i64> {
    let recipe = recipes()
        .iter()
        .find(|r| r.product.name == item_name)
        .ok_or_else(|| anyhow::anyhow!("未知或不可使用的 item「{}」", item_name))?;
    let effect = recipe
        .effect
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("item「{}」沒有主動效果", item_name))?;

    let tx = conn.transaction()?;

    // Guard against stacking: if the same (effect_kind, zone_id) is
    // already live, refuse the use so the player doesn't double-charge
    // an inventory unit against a zone that's already protected. The
    // spec declares a flat duration — no stacking mechanic — so this
    // matches intent. Use `SELECT 1` over COUNT(*) so the plan can
    // short-circuit on the first matching row.
    let already_live: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM active_effects
             WHERE effect_kind = ?1
               AND ((zone_id IS NULL AND ?2 IS NULL) OR zone_id = ?2)
               AND expires_at > ?3
             LIMIT 1",
            rusqlite::params![effect.kind.as_str(), zone_id, now],
            |r| r.get(0),
        )
        .ok();
    if already_live.is_some() {
        bail!(
            "「{}」的效果仍在此區域生效中，到期後再使用",
            item_name
        );
    }

    let have: i64 = tx
        .query_row(
            "SELECT quantity FROM loot_inventory WHERE kind = ?1 AND name = ?2",
            rusqlite::params![recipe.product.kind, item_name],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if have < 1 {
        bail!("你沒有任何「{}」可使用", item_name);
    }
    let remaining = have - 1;
    if remaining == 0 {
        tx.execute(
            "DELETE FROM loot_inventory WHERE kind = ?1 AND name = ?2",
            rusqlite::params![recipe.product.kind, item_name],
        )?;
    } else {
        tx.execute(
            "UPDATE loot_inventory SET quantity = ?1
             WHERE kind = ?2 AND name = ?3",
            rusqlite::params![remaining, recipe.product.kind, item_name],
        )?;
    }
    let expires_at = effects::apply_effect(
        &tx,
        effect.kind,
        zone_id,
        now,
        effect.duration_days,
        item_name,
    )?;
    tx.commit()?;
    Ok(expires_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn insert_material(conn: &Connection, kind: &str, name: &str, qty: u32) {
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES (?1, ?2, ?3, 0, 0)",
            rusqlite::params![kind, name, qty as i64],
        )
        .unwrap();
    }

    #[test]
    fn craft_happy_path_debits_and_credits() {
        let mut conn = fresh();
        insert_material(&conn, "fragment", "Pattern Fragment", 3);
        let recipe = find_recipe("Refactor Blueprint").unwrap();
        craft(&mut conn, recipe, 1_000).unwrap();

        // Fragments consumed → row removed
        let remaining: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(quantity),0) FROM loot_inventory
                 WHERE name = 'Pattern Fragment'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);

        // Blueprint credited
        let blueprints: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Refactor Blueprint'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(blueprints, 1);
    }

    #[test]
    fn craft_respects_partial_inventory_leftover() {
        let mut conn = fresh();
        insert_material(&conn, "fragment", "Pattern Fragment", 5);
        let recipe = find_recipe("Refactor Blueprint").unwrap();
        craft(&mut conn, recipe, 1_000).unwrap();

        // 5 - 3 = 2 fragments left
        let remaining: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Pattern Fragment'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 2);
    }

    #[test]
    fn craft_rejects_insufficient_materials() {
        let mut conn = fresh();
        insert_material(&conn, "fragment", "Pattern Fragment", 2); // need 3
        let recipe = find_recipe("Refactor Blueprint").unwrap();
        let err = craft(&mut conn, recipe, 1_000).unwrap_err();
        assert!(err.to_string().contains("材料不足"));
        // Inventory untouched on error (transaction rollback)
        let remaining: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Pattern Fragment'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 2, "rollback preserved the 2 fragments");
    }

    #[test]
    fn use_item_debits_inventory_and_inserts_effect() {
        let mut conn = fresh();
        insert_material(&conn, "repellent", "Ghost Repellent", 1);
        let expires_at = use_item(&mut conn, "Ghost Repellent", Some("rust"), 10_000).unwrap();

        // Ghost Repellent consumed → row removed
        let have: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM loot_inventory WHERE name = 'Ghost Repellent'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(have, 0);

        // Active effect inserted with a 3-day window
        let (kind, zone, expires): (String, Option<String>, i64) = conn
            .query_row(
                "SELECT effect_kind, zone_id, expires_at FROM active_effects LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(kind, "suppress_ghost_spawn");
        assert_eq!(zone.as_deref(), Some("rust"));
        assert_eq!(expires, expires_at);
        assert_eq!(expires, 10_000 + 3 * 24 * 60 * 60);
    }

    #[test]
    fn use_item_rejects_unknown_item() {
        let mut conn = fresh();
        let err = use_item(&mut conn, "Mystery Gadget", None, 1_000).unwrap_err();
        assert!(err.to_string().contains("未知"));
    }

    #[test]
    fn use_item_rejects_when_none_in_inventory() {
        let mut conn = fresh();
        // No Ghost Repellent in inventory
        let err = use_item(&mut conn, "Ghost Repellent", Some("rust"), 1_000).unwrap_err();
        assert!(err.to_string().contains("沒有"));
    }

    #[test]
    fn use_item_rollback_preserves_inventory_on_error() {
        // Using an unknown item should not touch inventory or effects.
        let mut conn = fresh();
        insert_material(&conn, "fragment", "Pattern Fragment", 3);
        let _ = use_item(&mut conn, "Mystery Gadget", None, 1_000).unwrap_err();
        let n: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Pattern Fragment'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
        let effects: i64 = conn
            .query_row("SELECT COUNT(*) FROM active_effects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(effects, 0);
    }

    #[test]
    fn use_item_rolls_back_inventory_when_effect_insert_fails() {
        // Real rollback test: simulate a mid-transaction failure by
        // dropping `active_effects` before calling `use_item`. The
        // inventory DELETE/UPDATE must revert when the subsequent
        // INSERT into active_effects fails.
        let mut conn = fresh();
        insert_material(&conn, "repellent", "Ghost Repellent", 1);
        conn.execute("DROP TABLE active_effects", []).unwrap();

        let err = use_item(&mut conn, "Ghost Repellent", Some("rust"), 1_000).unwrap_err();
        // The inner rusqlite error mentions the missing table — enough
        // to prove we reached (and failed on) the INSERT path.
        let msg = err.to_string();
        assert!(
            msg.contains("no such table") || msg.contains("active_effects"),
            "unexpected error: {msg}"
        );

        // Crucial assertion: inventory untouched after rollback.
        let have: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Ghost Repellent'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(have, 1, "inventory must survive INSERT-failed rollback");
    }

    #[test]
    fn use_item_rejects_second_use_while_effect_still_live() {
        // Regression for review r1 finding #1: two successive uses
        // against the same zone must debit only one inventory unit.
        let mut conn = fresh();
        insert_material(&conn, "repellent", "Ghost Repellent", 2);
        use_item(&mut conn, "Ghost Repellent", Some("rust"), 10_000).unwrap();

        // Second use while the first is live must bail.
        let err = use_item(&mut conn, "Ghost Repellent", Some("rust"), 10_001).unwrap_err();
        assert!(err.to_string().contains("生效中"));

        // Inventory has 1 remaining (only the first use consumed).
        let have: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Ghost Repellent'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(have, 1);

        // Exactly one active_effects row — no duplicate insertion.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM active_effects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn use_item_permits_reuse_after_expiry() {
        // After the first effect expires, a second use should succeed.
        let mut conn = fresh();
        insert_material(&conn, "repellent", "Ghost Repellent", 2);
        use_item(&mut conn, "Ghost Repellent", Some("rust"), 0).unwrap();
        // Jump past the 3-day window (3*86400 = 259200)
        let later = (3 * 86_400) + 1;
        use_item(&mut conn, "Ghost Repellent", Some("rust"), later).unwrap();

        // Two active_effects rows total — the expired one + the fresh one.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM active_effects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn use_item_permits_same_item_in_different_zones() {
        let mut conn = fresh();
        insert_material(&conn, "repellent", "Ghost Repellent", 2);
        use_item(&mut conn, "Ghost Repellent", Some("rust"), 10_000).unwrap();
        // Different zone — must succeed.
        use_item(&mut conn, "Ghost Repellent", Some("python"), 10_001).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM active_effects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }
}
