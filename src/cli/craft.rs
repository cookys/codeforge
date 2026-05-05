//! `codeforge craft <recipe>` — Phase 3e crafting CLI.
//!
//! Looks up the recipe by product name (case-insensitive), runs the
//! single-transaction debit/credit via `craft::craft`. Surfaces the
//! product name + material cost so the user sees what just happened.

use anyhow::{bail, Result};

use crate::craft;
use crate::db;

pub fn run(ctx: &db::Context, name: Option<String>) -> Result<()> {
    ctx.ensure_initialized()?;
    let mut conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    let Some(raw) = name else {
        list_recipes();
        return Ok(());
    };

    let recipe = match craft::find_recipe(&raw) {
        Some(r) => r,
        None => {
            bail!("未知配方「{}」。執行 `codeforge craft` 無參數查看列表", raw);
        }
    };

    craft::craft(&mut conn, recipe, unix_now())?;

    let ings: Vec<String> = recipe
        .ingredients
        .iter()
        .map(|i| format!("{}× {}", i.quantity, i.name))
        .collect();
    println!(
        "✓ 合成成功：{}  （消耗 {}）",
        recipe.product.name,
        ings.join(" + ")
    );
    if let Some(effect) = recipe.effect.as_ref() {
        println!(
            "  使用時觸發：{}（持續 {} 天）— 執行 `codeforge use \"{}\"`",
            effect.kind.as_str(),
            effect.duration_days,
            recipe.product.name,
        );
    }
    Ok(())
}

fn list_recipes() {
    println!("\n可用配方：");
    for r in craft::recipes() {
        let ings: Vec<String> = r
            .ingredients
            .iter()
            .map(|i| format!("{}× {}", i.quantity, i.name))
            .collect();
        let effect_note = r
            .effect
            .as_ref()
            .map(|e| format!("  （{} / {}d）", e.kind.as_str(), e.duration_days))
            .unwrap_or_default();
        println!(
            "  · {}  ⇐  {}{}",
            r.product.name,
            ings.join(" + "),
            effect_note,
        );
    }
    println!("\n用法：codeforge craft \"Ghost Repellent\"");
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
    use rusqlite::Connection;
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
    fn run_without_name_lists_recipes_without_error() {
        let (_t, ctx) = scratch_ctx();
        // No panic + no error path when listing (sanity check).
        run(&ctx, None).unwrap();
    }

    #[test]
    fn run_rejects_unknown_recipe() {
        let (_t, ctx) = scratch_ctx();
        let err = run(&ctx, Some("Unicorn Elixir".to_string())).unwrap_err();
        assert!(err.to_string().contains("未知配方"));
    }

    #[test]
    fn run_end_to_end_craft_debits_and_credits() {
        let (_t, ctx) = scratch_ctx();
        // Seed materials
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('crystal', 'Dead Code Crystal', 5, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        run(&ctx, Some("Ghost Repellent".to_string())).unwrap();

        let conn: Connection = ctx.open_db().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Ghost Repellent'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        let remaining: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Dead Code Crystal'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(remaining, 0, "5 - 5 = 0 crystals");
    }

    #[test]
    fn run_surfaces_material_shortage() {
        let (_t, ctx) = scratch_ctx();
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('crystal', 'Dead Code Crystal', 2, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        let err = run(&ctx, Some("Ghost Repellent".to_string())).unwrap_err();
        assert!(err.to_string().contains("材料不足"));
    }
}
