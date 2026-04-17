use anyhow::Result;
use crate::db;
use crate::pet::live_state::LiveState;
use crate::pet::village::VILLAGES;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
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
    let village = VILLAGES.iter().find(|v| v.id == pet.village).unwrap_or(&VILLAGES[2]);

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    // 村落標題
    stdout.set_color(ColorSpec::new().set_fg(Some(village.color)).set_bold(true))?;
    writeln!(stdout, "\n  {} — {}", village.display_name, village.language)?;
    stdout.reset()?;

    // 寵物 ASCII
    stdout.set_color(ColorSpec::new().set_fg(Some(village.color)))?;
    for line in village.ascii_full.lines() {
        writeln!(stdout, "  {}", line)?;
    }
    stdout.reset()?;

    writeln!(stdout)?;

    // 名稱與等級
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(stdout, "  {} ", pet.name)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
    writeln!(stdout, "Lv.{}", pet.level)?;
    stdout.reset()?;

    // XP bar
    let xp_pct = (pet.xp as f32 / pet.xp_to_next as f32 * 20.0) as usize;
    let xp_bar: String = "▮".repeat(xp_pct) + &"▯".repeat(20 - xp_pct);
    println!("  XP  {} {}/{}", xp_bar, pet.xp, pet.xp_to_next);

    // 屬性
    println!();
    println!("  ATK:{:3}  HP:{:3}  DEF:{:3}  SUP:{:3}  VER:{:3}",
        pet.atk, pet.hp, pet.def, pet.sup, pet.ver);

    // 徽章
    let badges = crate::pet::badges::list(&conn)?;
    if !badges.is_empty() {
        println!();
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
        write!(stdout, "  ★ 徽章 ({})：", badges.len())?;
        stdout.reset()?;
        println!("{}", badges.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join("  "));
    }

    println!();

    Ok(())
}
