use anyhow::Result;
use crate::db;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;

/// Claude Code 每次 render cycle 呼叫此指令
/// stdin 接收 JSON（model/tokens/cost/session），stdout 輸出多行 ANSI
pub fn run(ctx: &db::Context) -> Result<()> {
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
                // Claude Code 可能用不同欄位名稱
                model: v["model"].as_str()
                    .or_else(|| v["model_id"].as_str())
                    .or_else(|| v["modelId"].as_str())
                    .map(|s| {
                        // 縮短 model 名稱（claude-opus-4-6 → opus-4.6）
                        s.replace("claude-", "")
                          .replace("-20", " 20")
                    }),
                git_branch: v["git_branch"].as_str()
                    .or_else(|| v["branch"].as_str())
                    .map(|s| s.to_string()),
                cwd: v["cwd"].as_str().map(|s| s.to_string()),
            };
        }
    }
    StatusInput::default()
}

/// 計算字串的終端機可視欄數（box 字元 = 1 col，ASCII = 1 col）
fn vis(s: &str) -> usize {
    s.chars().count()
}

/// 將字串填充到恰好 width 可視欄（不會截斷 ASCII，只補空格）
fn pad(s: &str, width: usize) -> String {
    let len = vis(s);
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}

/// │ content （填充至 inner 欄）│
fn box_line(content: &str, inner: usize) -> String {
    format!("│{}│", pad(content, inner))
}

/// ╭── cwd ── branch ─────╮
fn header_line(cwd: &str, branch: &str, inner: usize) -> String {
    // fixed: ╭(1) ──(2) (1) cwd (1) ──(2) (1) branch (1) ─(1) padding ╮(1) = 11
    let overhead = 11;
    let fill = inner.saturating_sub(overhead + vis(cwd) + vis(branch));
    format!("╭── {} ── {} ─{}╮", cwd, branch, "─".repeat(fill))
}

/// ├── Label ──────┤
fn divider_line(label: &str, inner: usize) -> String {
    // fixed: ├(1) ──(2) (1) label (1) fill ┤(1) = 6
    let fill = inner.saturating_sub(6 + vis(label));
    format!("├── {} {}┤", label, "─".repeat(fill))
}

/// ╰── left ──────[right]╯
fn footer_line(left: &str, right: &str, inner: usize) -> String {
    // fixed: ╰(1) ──(2) (1) left (1) ──(2) fill [(1) right ](1) ╯(1) = 10
    let fill = inner.saturating_sub(10 + vis(left) + vis(right));
    format!("╰── {} ──{}[{}]╯", left, "─".repeat(fill), right)
}

fn render_no_pet(
    stdout: &mut StandardStream,
    data: &StatusInput,
    width: usize,
) -> Result<()> {
    let inner = width.saturating_sub(2);
    let mut spec = ColorSpec::new();
    spec.set_fg(Some(Color::Ansi256(244)));
    stdout.set_color(&spec)?;
    writeln!(stdout, "╭{}╮", "─".repeat(inner))?;
    writeln!(stdout, "{}", box_line("  CodeForge — 執行 `codeforge adopt` 開始你的旅程", inner))?;
    let model_str = data.model.as_deref().unwrap_or("—");
    writeln!(stdout, "{}", footer_line("Memory: inactive", model_str, inner))?;
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
    const ART_W: usize = 15;
    let panel_w = width.saturating_sub(ART_W);
    let inner = panel_w.saturating_sub(2);

    let branch = data.git_branch.clone()
        .or_else(|| git_branch())
        .unwrap_or_else(|| "—".to_string());
    let cwd = data.cwd.clone()
        .or_else(|| current_dir_short())
        .unwrap_or_else(|| "~".to_string());
    let model_str = data.model.as_deref().unwrap_or("—");

    // XP bar（6 格）
    let xp_filled = (pet.xp as f32 / pet.xp_to_next as f32 * 6.0) as usize;
    let xp_bar = format!("{}{}", "▮".repeat(xp_filled.min(6)), "▯".repeat(6 - xp_filled.min(6)));

    // HP bar（6 格）
    let hp_filled = ((pet.hp as f32 / 100.0) * 6.0).min(6.0) as usize;
    let hp_bar = format!("{}{}", "▮".repeat(hp_filled), "▯".repeat(6 - hp_filled));

    // 6 行面板內容（不含 art 欄）
    let panels: [String; 6] = [
        header_line(&cwd, &branch, inner),
        box_line(&format!("  Lv.{}  XP {} {}/{}", pet.level, xp_bar, pet.xp, pet.xp_to_next), inner),
        divider_line(&village.display_name, inner),
        box_line(&format!("  {} Lv.{}  HP {} {}", pet.name, pet.level, hp_bar, pet.hp), inner),
        box_line(&format!("  ATK:{:2}  DEF:{:2}  SUP:{:2}  VER:{:2}", pet.atk, pet.def, pet.sup, pet.ver), inner),
        footer_line("Memory: active", model_str, inner),
    ];

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // 村落顏色（art 欄）
    let mut art_spec = ColorSpec::new();
    art_spec.set_fg(Some(village.color));
    // 面板顏色（中性灰）
    let mut panel_spec = ColorSpec::new();
    panel_spec.set_fg(Some(Color::Ansi256(252)));

    for (i, panel_line) in panels.iter().enumerate() {
        let art = art_lines.get(i).copied().unwrap_or("");

        // art 欄：村落顏色，固定 15 cols
        stdout.set_color(&art_spec)?;
        write!(stdout, "{:<ART_W$}", art)?;

        // 面板欄：中性灰
        stdout.set_color(&panel_spec)?;
        writeln!(stdout, "{}", panel_line)?;
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
    // 截短：用 chars 避免在多 byte 字元中間截斷
    const MAX: usize = 30;
    if path.chars().count() > MAX {
        let tail: String = path.chars().rev().take(MAX - 1).collect::<String>()
            .chars().rev().collect();
        Some(format!("…{}", tail))
    } else {
        Some(path)
    }
}
