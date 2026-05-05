//! `codeforge use <item>` — Phase 3e active-item consumption.
//!
//! Resolves the item name against the recipe table, debits one from the
//! pet's inventory, and inserts a row into `active_effects`. Zone scope
//! comes from the pet's current village (Phase 3a `pet.village`).
//!
//! Daemon reads: `daemon::mob_scanner` checks `is_effect_active(
//! SuppressGhostSpawn, zone, now)` before generating Ghost MOBs.
//! The two other effects (ReduceDifficulty, SuppressDoppelgangerSplit)
//! are stored today; their runtime consumers land in later phases.

use anyhow::{bail, Result};

use crate::craft;
use crate::db;
use crate::pet::live_state::LiveState;

pub fn run(ctx: &db::Context, name: String) -> Result<()> {
    ctx.ensure_initialized()?;
    let mut conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    // Resolve the canonical recipe name up-front so the final message
    // matches the recipe table regardless of user casing.
    let recipe = match craft::find_recipe(&name) {
        Some(r) => r,
        None => bail!("未知 item「{}」。執行 `codeforge craft` 查看可用清單", name),
    };

    let zone = LiveState::load(&conn)?.map(|l| l.state.village);

    let now = unix_now();
    let expires_at = craft::use_item(&mut conn, recipe.product.name, zone.as_deref(), now)?;
    let remaining_days = ((expires_at - now) / 86_400).max(0);
    let scope = match zone {
        Some(z) => format!("Zone {z}"),
        None => "全域".to_string(),
    };
    let effect_kind = recipe
        .effect
        .as_ref()
        .map(|e| e.kind.as_str())
        .unwrap_or("-");

    println!(
        "✓ 使用「{}」— 效果 {}，生效 {}，剩餘 {} 天",
        recipe.product.name, effect_kind, scope, remaining_days
    );
    Ok(())
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use tempfile::TempDir;

    fn scratch_ctx() -> (TempDir, db::Context) {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join(".codeforge");
        std::fs::create_dir_all(&project_dir).unwrap();
        let db_path = temp.path().join("state.db");
        let ctx = db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path,
        };
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        drop(conn);
        (temp, ctx)
    }

    #[test]
    fn run_rejects_unknown_item() {
        let (_t, ctx) = scratch_ctx();
        let err = run(&ctx, "Unicorn Potion".to_string()).unwrap_err();
        assert!(err.to_string().contains("未知 item"));
    }

    #[test]
    fn run_case_insensitive_lookup() {
        let (_t, ctx) = scratch_ctx();
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('repellent', 'Ghost Repellent', 1, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);
        run(&ctx, "ghost repellent".to_string()).unwrap();
    }

    #[test]
    fn run_inserts_active_effect_scoped_to_home_zone() {
        let (_t, ctx) = scratch_ctx();
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next,
                              atk, hp, def, sup, ver)
             VALUES (1, 'python', 'Spam', 3, 0, 100, 10, 50, 10, 10, 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('repellent', 'Ghost Repellent', 1, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        run(&ctx, "Ghost Repellent".to_string()).unwrap();

        let conn = ctx.open_db().unwrap();
        let zone: String = conn
            .query_row("SELECT zone_id FROM active_effects LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(zone, "python", "zone comes from pet.village");
    }

    #[test]
    fn run_without_pet_falls_back_to_global_effect() {
        // No pet adopted → zone=None on the active_effects row.
        let (_t, ctx) = scratch_ctx();
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('blueprint', 'Refactor Blueprint', 1, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        run(&ctx, "Refactor Blueprint".to_string()).unwrap();

        let conn = ctx.open_db().unwrap();
        let zone: Option<String> = conn
            .query_row("SELECT zone_id FROM active_effects LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(zone, None, "no pet → global effect (zone_id NULL)");
    }

    #[test]
    fn run_rejects_when_item_not_owned() {
        let (_t, ctx) = scratch_ctx();
        let err = run(&ctx, "Ghost Repellent".to_string()).unwrap_err();
        assert!(err.to_string().contains("沒有"));
    }
}
