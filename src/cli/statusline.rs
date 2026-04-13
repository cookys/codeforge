use anyhow::Result;
use crate::db;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;

/// Claude Code 每次 render cycle 呼叫此指令
/// stdin 接收 JSON（model/tokens/cost/session），stdout 輸出多行 ANSI
pub fn run(ctx: &db::Context) -> Result<()> {
    // 讀取 Claude Code 傳入的 JSON（可能沒有，忽略錯誤）
    let status_data = read_status_input();

    let conn = ctx.open_db()?;
    let has_pet = PetState::exists(&conn).unwrap_or(false);

    let width: usize = std::env::var("CODEFORGE_WIDTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    if !has_pet {
        render_no_pet(&mut stdout, &status_data, width)?;
    } else {
        let pet = PetState::load(&conn).unwrap_or_default();
        let village = VILLAGES.iter().find(|v| v.id == pet.village).unwrap_or(&VILLAGES[2]);
        render_full(&mut stdout, &status_data, &pet, village, width)?;
    }

    stdout.flush()?;
    Ok(())
}

#[derive(Default)]
struct StatusInput {
    model: Option<String>,
    tokens_used: Option<u64>,
    session_cost: Option<f64>,
    git_branch: Option<String>,
    cwd: Option<String>,
}

fn read_status_input() -> StatusInput {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            return StatusInput {
                model: v["model"].as_str().map(|s| s.to_string()),
                tokens_used: v["tokens_used"].as_u64(),
                session_cost: v["session_cost"].as_f64(),
                git_branch: v["git_branch"].as_str().map(|s| s.to_string()),
                cwd: v["cwd"].as_str().map(|s| s.to_string()),
            };
        }
    }
    StatusInput::default()
}

fn render_no_pet(
    stdout: &mut StandardStream,
    data: &StatusInput,
    width: usize,
) -> Result<()> {
    let inner = width.saturating_sub(2);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Ansi256(244))))?;
    writeln!(stdout, "╭{}╮", "─".repeat(inner))?;
    let msg = "  CodeForge — 執行 /codeforge:adopt 開始你的旅程";
    writeln!(stdout, "│{:<width$}│", msg, width = inner)?;
    if let Some(model) = &data.model {
        writeln!(stdout, "│{:>width$}│", format!("[{}] ", model), width = inner)?;
    }
    writeln!(stdout, "╰{}╯", "─".repeat(inner))?;
    stdout.reset()?;
    Ok(())
}

fn render_full(
    stdout: &mut StandardStream,
    data: &StatusInput,
    pet: &PetState,
    village: &crate::pet::village::Village,
    width: usize,
) -> Result<()> {
    // 寵物 art 佔左 15 col，右側面板
    let panel_width = width.saturating_sub(17);
    let inner = panel_width.saturating_sub(2);

    // 取得 git info（fallback to data.git_branch）
    let branch = data.git_branch.clone()
        .or_else(|| git_branch())
        .unwrap_or_else(|| "main".to_string());
    let cwd = data.cwd.clone()
        .or_else(|| current_dir_short())
        .unwrap_or_else(|| "~".to_string());

    // XP bar（10 chars）
    let xp_pct = (pet.xp as f32 / pet.xp_to_next as f32 * 6.0) as usize;
    let xp_bar: String = "▮".repeat(xp_pct) + &"▯".repeat(6 - xp_pct);

    // HP bar
    let hp_pct = ((pet.hp as f32 / 100.0) * 6.0).min(6.0) as usize;
    let hp_bar: String = "▮".repeat(hp_pct) + &"▯".repeat(6 - hp_pct);

    let model_str = data.model.as_deref().unwrap_or("—");
    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // 6 行輸出
    let lines: [(Option<&str>, String); 6] = [
        (art_lines.get(0).copied(), format!(
            "╭── {} ── {} ─{}╮",
            cwd, branch,
            "─".repeat(inner.saturating_sub(cwd.len() + branch.len() + 8))
        )),
        (art_lines.get(1).copied(), format!(
            "│  Lv.{}  XP {} {}/{}{}│",
            pet.level, xp_bar, pet.xp, pet.xp_to_next,
            " ".repeat(inner.saturating_sub(30))
        )),
        (art_lines.get(2).copied(), format!(
            "├── {} {}{}┤",
            village.display_name,
            "─".repeat(inner.saturating_sub(village.display_name.len() + 4)),
            ""
        )),
        (art_lines.get(3).copied(), format!(
            "│  {} Lv.{}  HP {} {}{}│",
            pet.name, pet.level, hp_bar, pet.hp,
            " ".repeat(inner.saturating_sub(pet.name.len() + 20))
        )),
        (art_lines.get(4).copied(), format!(
            "│  ATK:{:2}  DEF:{:2}  SUP:{:2}  VER:{:2}{}│",
            pet.atk, pet.def, pet.sup, pet.ver,
            " ".repeat(inner.saturating_sub(30))
        )),
        (art_lines.get(5).copied(), format!(
            "╰── Memory: active ──{}[{}]╯",
            "─".repeat(inner.saturating_sub(model_str.len() + 22)),
            model_str
        )),
    ];

    stdout.set_color(ColorSpec::new().set_fg(Some(village.color)))?;
    for (art_line, panel_line) in &lines {
        let art = art_line.unwrap_or("          ");
        writeln!(stdout, "{:<15}{}", art, panel_line)?;
    }
    stdout.reset()?;

    Ok(())
}

fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn current_dir_short() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let home = dirs::home_dir()?;
    let path = if let Ok(rel) = cwd.strip_prefix(&home) {
        format!("~/{}", rel.display())
    } else {
        cwd.display().to_string()
    };
    // 截短到 30 chars
    if path.len() > 30 {
        Some(format!("…{}", &path[path.len() - 29..]))
    } else {
        Some(path)
    }
}
