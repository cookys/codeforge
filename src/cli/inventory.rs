//! `codeforge inventory` — Phase 3e read surface.
//!
//! Lists every `loot_inventory` row plus the active_effects roll-up for
//! the pet's current home zone. Purely a read — no craft / use side
//! effects.

use anyhow::Result;
use rusqlite::Connection;
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::craft::{active_effects_for_zone, recipes, ActiveEffect, Recipe};
use crate::db;
use crate::pet::live_state::LiveState;

pub fn run(ctx: &db::Context) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    let zone = LiveState::load(&conn)?
        .map(|l| l.state.village)
        .unwrap_or_else(|| "rust".to_string());

    let now = unix_now();
    let effects = active_effects_for_zone(&conn, &zone, now)?;
    let items = load_inventory(&conn)?;

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    render_items(&mut stdout, &items)?;
    render_effects(&mut stdout, &zone, &effects, now)?;
    render_recipes(&mut stdout, recipes())?;
    Ok(())
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_inventory(conn: &Connection) -> Result<Vec<(String, String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT kind, name, quantity FROM loot_inventory
         ORDER BY kind, name",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn render_items(
    out: &mut StandardStream,
    items: &[(String, String, i64)],
) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(out, "  🎒 Inventory ({} 類)", items.len())?;
    out.reset()?;
    if items.is_empty() {
        writeln!(out, "    （空的 — 去打 MOB 收集材料！）")?;
        return Ok(());
    }
    for (kind, name, qty) in items {
        if *qty == 1 {
            writeln!(out, "    · {name} ({kind})")?;
        } else {
            writeln!(out, "    · {name} × {qty} ({kind})")?;
        }
    }
    Ok(())
}

fn render_effects(
    out: &mut StandardStream,
    zone: &str,
    effects: &[ActiveEffect],
    now: i64,
) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(out, "  ⚡ 生效中 effects @ {zone}")?;
    out.reset()?;
    if effects.is_empty() {
        writeln!(out, "    （無）")?;
        return Ok(());
    }
    for e in effects {
        let remaining = (e.expires_at - now).max(0);
        let days = remaining / 86_400;
        let hours = (remaining % 86_400) / 3_600;
        let scope = e.zone_id.as_deref().unwrap_or("全域");
        writeln!(
            out,
            "    · {} ({}) — 剩餘 {}d{}h（來源：{}）",
            e.kind.as_str(),
            scope,
            days,
            hours,
            e.source_item,
        )?;
    }
    Ok(())
}

fn render_recipes(out: &mut StandardStream, recipes: &[Recipe]) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(out, "  🛠  可用配方（`codeforge craft <name>`）")?;
    out.reset()?;
    for r in recipes {
        let ings: Vec<String> = r
            .ingredients
            .iter()
            .map(|i| format!("{}× {}", i.quantity, i.name))
            .collect();
        let effect = r
            .effect
            .as_ref()
            .map(|e| format!(" · 效果 {}（{}d）", e.kind.as_str(), e.duration_days))
            .unwrap_or_default();
        writeln!(
            out,
            "    · {}  ⇐  {}{}",
            r.product.name,
            ings.join(" + "),
            effect,
        )?;
    }
    writeln!(out)?;
    Ok(())
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

    #[test]
    fn load_inventory_orders_stably() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('crystal', 'Dead Code Crystal', 3, 0, 0),
                    ('fragment', 'Pattern Fragment', 5, 0, 0),
                    ('gem', 'Abstract Gem', 1, 0, 0)",
            [],
        )
        .unwrap();
        let rows = load_inventory(&conn).unwrap();
        // Ordered by (kind, name) — crystal < fragment < gem alphabetically.
        assert_eq!(rows[0].0, "crystal");
        assert_eq!(rows[1].0, "fragment");
        assert_eq!(rows[2].0, "gem");
    }

    #[test]
    fn load_inventory_empty_returns_empty_vec() {
        let conn = fresh();
        let rows = load_inventory(&conn).unwrap();
        assert!(rows.is_empty());
    }
}
