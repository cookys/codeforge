use crate::db;
use crate::pet::live_state::LiveState;
use crate::pet::village::VILLAGES;
use anyhow::Result;
use crossterm::style::{Color, Stylize};
use std::io::Write;

pub fn run(ctx: &db::Context) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;

    let live = match LiveState::load(&conn)? {
        Some(s) => s,
        None => {
            println!("還沒有本命寵物！執行 `codeforge adopt` 選擇你的村落");
            return Ok(());
        }
    };
    let pet = &live.state;
    let village = VILLAGES
        .iter()
        .find(|v| v.id == pet.village)
        .unwrap_or(&VILLAGES[2]);

    let mut stdout = std::io::stdout();

    // 村落標題
    writeln!(
        stdout,
        "\n  {}",
        format!("{} — {}", village.display_name, village.language)
            .with(village.color)
            .bold()
    )?;

    // 寵物 ASCII
    for line in village.ascii_full.lines() {
        writeln!(stdout, "  {}", line.with(village.color))?;
    }

    writeln!(stdout)?;

    // 名稱與等級
    write!(stdout, "  {} ", pet.name.as_str().with(Color::White).bold())?;
    writeln!(
        stdout,
        "{}",
        format!("Lv.{}", pet.level).with(Color::Yellow)
    )?;

    // XP bar
    let xp_pct = (pet.xp as f32 / pet.xp_to_next as f32 * 20.0) as usize;
    let xp_bar: String = "▮".repeat(xp_pct) + &"▯".repeat(20 - xp_pct);
    println!("  XP  {} {}/{}", xp_bar, pet.xp, pet.xp_to_next);

    // 屬性
    println!();
    println!(
        "  ATK:{:3}  HP:{:3}  DEF:{:3}  SUP:{:3}  VER:{:3}",
        pet.atk, pet.hp, pet.def, pet.sup, pet.ver
    );

    // 徽章
    let badges = crate::pet::badges::list(&conn)?;
    if !badges.is_empty() {
        println!();
        write!(
            stdout,
            "  {}",
            format!("★ 徽章 ({})：", badges.len()).with(Color::Yellow)
        )?;
        println!(
            "{}",
            badges
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join("  ")
        );
    }

    render_recent_combat(&mut stdout, &conn)?;
    render_inventory(&mut stdout, &conn)?;

    println!();

    Ok(())
}

/// Last 3 combat_log rows — show daemon activity at a glance.
fn render_recent_combat(stdout: &mut impl Write, conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT mob_kind, mob_name, xp_gained, loot, occurred_at
         FROM combat_log ORDER BY id DESC LIMIT 3",
    )?;
    let rows: Vec<(String, String, i64, Option<String>, String)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    if rows.is_empty() {
        return Ok(());
    }
    println!();
    writeln!(stdout, "  {}", "⚔ 最近擊殺：".with(Color::Cyan).bold())?;
    for (kind, name, xp, loot, at) in rows {
        let short_name = truncate_display(&name, 42);
        let loot_s = loot.unwrap_or_default();
        let loot_tail = if loot_s.is_empty() {
            String::new()
        } else {
            format!(" · {loot_s}")
        };
        println!("    [{at}] {kind:<12} {short_name}  (+{xp} XP){loot_tail}");
    }
    Ok(())
}

/// Aggregated loot inventory — one line per (kind, name).
fn render_inventory(stdout: &mut impl Write, conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT kind, name, quantity FROM loot_inventory
         ORDER BY kind, name",
    )?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if rows.is_empty() {
        return Ok(());
    }
    println!();
    writeln!(
        stdout,
        "  {}",
        format!("🎒 Inventory ({} 類)：", rows.len())
            .with(Color::Green)
            .bold()
    )?;
    for (kind, name, qty) in rows {
        if qty == 1 {
            println!("    · {name} ({kind})");
        } else {
            println!("    · {name} × {qty} ({kind})");
        }
    }
    Ok(())
}

/// CJK-safe character-count truncate per HARD RULE. Adds `…` if cut.
fn truncate_display(s: &str, max_chars: usize) -> String {
    let taken: String = s.chars().take(max_chars).collect();
    if taken.chars().count() < s.chars().count() {
        format!("{taken}…")
    } else {
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_display_ascii() {
        assert_eq!(truncate_display("abcdef", 10), "abcdef");
        assert_eq!(truncate_display("abcdef", 3), "abc…");
    }

    #[test]
    fn truncate_display_cjk_safe() {
        // 6 CJK chars: the byte-index `&s[..3]` would panic on UTF-8 boundaries.
        // Our .chars().take(3) path handles it correctly.
        let s = "代號七七七號";
        let out = truncate_display(s, 3);
        assert_eq!(out.chars().count(), 4); // 3 chars + …
        assert!(out.ends_with('…'));
    }
}
