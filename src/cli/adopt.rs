use anyhow::Result;
use crate::db;
use crate::pet::village::{VILLAGES, Village};
use crate::pet::state::PetState;
use crate::power::{PowerProvider, StubPowerProvider};
use crossterm::style::{Color, Stylize};
use std::io::Write;

pub fn run(ctx: &db::Context) -> Result<()> {
    ctx.ensure_initialized()?;

    let conn = ctx.open_db()?;

    // 已經選過村了嗎？
    if PetState::exists(&conn)? {
        let pet = PetState::load(&conn)?;
        println!("你已經選擇了 {} 村，本命寵是 {} Lv.{}", pet.village, pet.name, pet.level);
        println!("使用 `codeforge pet` 查看詳情");
        return Ok(());
    }

    // 顯示選村畫面
    render_village_menu()?;

    // 讀取用戶選擇
    print!("\n選擇村落（1-5，或 R 隨機選擇）：");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let choice = input.trim().to_lowercase();

    let (village, bonus_stats): (&Village, bool) = match choice.as_str() {
        "1" => (&VILLAGES[0], false),
        "2" => (&VILLAGES[1], false),
        "3" => (&VILLAGES[2], false),
        "4" => (&VILLAGES[3], false),
        "5" => (&VILLAGES[4], false),
        "r" | "random" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
            let idx = (t as usize) % VILLAGES.len();
            (&VILLAGES[idx], true)
        }
        _ => anyhow::bail!("無效的選擇，請輸入 1-5 或 R"),
    };

    // 使用 PowerProvider 計算初始屬性
    let provider = StubPowerProvider::default();
    let mut stats = provider.character_power(None);

    // Random Pick 加 10% 總屬性（Lucky Hatchling 效果）
    if bonus_stats {
        stats.atk = (stats.atk as f32 * 1.1) as u32;
        stats.hp  = (stats.hp  as f32 * 1.1) as u32;
        stats.def = (stats.def as f32 * 1.1) as u32;
        stats.sup = (stats.sup as f32 * 1.1) as u32;
        stats.ver = (stats.ver as f32 * 1.1) as u32;
    }

    // 儲存寵物
    let pet = PetState::new(village, &stats);
    pet.save(&conn)?;

    // 顯示開場動畫
    render_pet_intro(village, &pet, bonus_stats)?;

    Ok(())
}

fn render_village_menu() -> Result<()> {
    let mut stdout = std::io::stdout();

    // 標題
    writeln!(stdout, "\n  {}", "╔══════════════════════════════════════════════╗".with(Color::Yellow).bold())?;
    writeln!(stdout, "  {}", "║        歡迎來到 CodeForge — 選擇你的村落     ║".with(Color::Yellow).bold())?;
    writeln!(stdout, "  {}\n", "╚══════════════════════════════════════════════╝".with(Color::Yellow).bold())?;

    for (i, v) in VILLAGES.iter().enumerate() {
        // 村號與名稱
        write!(stdout, "  {}", format!("[{}] ", i + 1).with(Color::Yellow))?;
        write!(stdout, "{}", v.display_name.with(v.color).bold())?;
        writeln!(stdout, " — {} ({})", v.language, v.pet_name)?;

        // 寵物 ASCII（縮小版）
        for line in v.ascii_small.lines() {
            writeln!(stdout, "      {}", line.with(v.color))?;
        }

        // 簡短描述
        writeln!(stdout, "      {}\n", v.tagline.with(Color::AnsiValue(244)))?;
    }

    // Random Pick 選項
    writeln!(
        stdout,
        "  {}\n",
        "[R] 隨機選擇（+10% 全屬性，Lucky Hatchling 徽章，不可反悔）".with(Color::Cyan)
    )?;

    Ok(())
}

fn render_pet_intro(village: &Village, pet: &PetState, is_random: bool) -> Result<()> {
    let mut stdout = std::io::stdout();

    writeln!(stdout)?;
    if is_random {
        writeln!(
            stdout,
            "  {}",
            format!("✦ Lucky Hatchling！命運為你選擇了 {} ✦", village.display_name)
                .with(Color::Yellow)
                .bold()
        )?;
    } else {
        writeln!(
            stdout,
            "  {}",
            format!("你踏入了 {}", village.display_name).with(Color::Yellow).bold()
        )?;
    }
    writeln!(stdout)?;

    // 大一點的 ASCII art
    for line in village.ascii_full.lines() {
        writeln!(stdout, "  {}", line.with(village.color))?;
    }

    writeln!(stdout)?;
    writeln!(
        stdout,
        "  {}",
        format!("{} Lv.1  村落：{}", pet.name, village.display_name)
            .with(Color::White)
            .bold()
    )?;

    println!("  ATK:{:3}  HP:{:3}  DEF:{:3}  SUP:{:3}  VER:{:3}",
        pet.atk, pet.hp, pet.def, pet.sup, pet.ver);

    if is_random {
        println!("  ★ Lucky Hatchling 徽章解鎖！");
    }

    println!();
    println!("  接下來：`codeforge pet` 查看狀態，`codeforge learn` 開始養成");

    Ok(())
}
