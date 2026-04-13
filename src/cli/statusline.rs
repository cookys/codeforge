use anyhow::Result;
use crate::db;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;

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
    raw: serde_json::Value,
    model: Option<String>,        // e.g. "Sonnet 4.6"
    cwd: Option<String>,
    version: Option<String>,      // e.g. "2.1.101"
    // context_window.*
    context_pct: Option<f64>,     // 0.0–1.0（used_percentage / 100）
    context_window_size: Option<u64>,
    // rate_limits.five_hour.*
    five_hour_pct: Option<f64>,
    five_hour_resets_at: Option<i64>,  // Unix timestamp
    // rate_limits.seven_day.*
    seven_day_pct: Option<f64>,
    seven_day_resets_at: Option<i64>,
}

fn read_status_input() -> StatusInput {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() && !line.trim().is_empty() {
        // Debug：記錄原始 JSON 供分析（只在設定 CODEFORGE_DEBUG 時）
        if std::env::var("CODEFORGE_DEBUG").is_ok() {
            let _ = std::fs::write("/tmp/codeforge-sl.json", &line);
        }

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            // model: {"id": "claude-sonnet-4-6", "display_name": "Sonnet 4.6"}
            let model = v["model"]["display_name"].as_str()
                .or_else(|| v["model"]["id"].as_str())
                .or_else(|| v["model"].as_str())  // fallback: flat string
                .map(|s| s.replace("claude-", "").replace("-20", " 20"));

            // context_window: {"used_percentage": 69, "context_window_size": 200000, ...}
            let context_pct = v["context_window"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let context_window_size = v["context_window"]["context_window_size"].as_u64();

            // rate_limits: {"five_hour": {"used_percentage": 8, "resets_at": ...}}
            let five_hour_pct = v["rate_limits"]["five_hour"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let five_hour_resets_at = v["rate_limits"]["five_hour"]["resets_at"].as_i64();
            let seven_day_pct = v["rate_limits"]["seven_day"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let seven_day_resets_at = v["rate_limits"]["seven_day"]["resets_at"].as_i64();

            return StatusInput {
                raw: v.clone(),
                model,
                cwd: v["cwd"].as_str()
                    .or_else(|| v["workspace"]["current_dir"].as_str())
                    .map(|s| s.to_string()),
                version: v["version"].as_str().map(|s| s.to_string()),
                context_pct,
                context_window_size,
                five_hour_pct,
                five_hour_resets_at,
                seven_day_pct,
                seven_day_resets_at,
            };
        }
    }
    StatusInput::default()
}

// ─── Layout helpers ──────────────────────────────────────────────────────────

/// 可視欄數（box 字元 = 1 col，ASCII = 1 col）
fn vis(s: &str) -> usize { s.chars().count() }

/// 填充到恰好 width 可視欄
fn pad(s: &str, width: usize) -> String {
    let len = vis(s);
    if len >= width { s.to_string() } else { format!("{}{}", s, " ".repeat(width - len)) }
}

/// │ content（填充至 inner 欄）│
fn box_line(content: &str, inner: usize) -> String {
    format!("│{}│", pad(content, inner))
}

/// ╭── left ── right ─────────────────╮（總寬 = inner + 2）
fn header_line(left: &str, right: &str, inner: usize) -> String {
    // ╭(1)──(2) (1)left(1) ──(2) (1)right(1) ─(1)fill╮(1)  = 11 fixed
    let fill = inner.saturating_sub(11 + vis(left) + vis(right));
    format!("╭── {} ── {} ─{}╮", left, right, "─".repeat(fill))
}

/// ├── label ─────────────────────────┤（總寬 = inner + 2）
fn divider_line(label: &str, inner: usize) -> String {
    let fill = inner.saturating_sub(6 + vis(label));
    format!("├── {} {}┤", label, "─".repeat(fill))
}

/// ╰── left ─────────────[right]╯（總寬 = inner + 2）
fn footer_line(left: &str, right: &str, inner: usize) -> String {
    let fill = inner.saturating_sub(10 + vis(left) + vis(right));
    format!("╰── {} ──{}[{}]╯", left, "─".repeat(fill), right)
}

/// 產生 N 格進度條（▮ 填充，▯ 空）
fn pbar(pct: f64, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 1.0)) * width as f64) as usize;
    format!("{}{}", "▮".repeat(filled), "▯".repeat(width - filled))
}

/// 格式化秒數為人類可讀（38400 → "10h"，172800 → "2d"）
fn fmt_duration(secs: u64) -> String {
    if secs < 3600 { format!("{}m", secs / 60) }
    else if secs < 86400 { format!("{}h", secs / 3600) }
    else { format!("{}d", secs / 86400) }
}

/// 格式化 context window（1000000 → "1M"，200000 → "200k"）
fn fmt_ctx_window(n: u64) -> String {
    if n >= 1_000_000 { format!("{}M", n / 1_000_000) }
    else if n >= 1_000 { format!("{}k", n / 1_000) }
    else { format!("{}", n) }
}

// ─── Renderers ───────────────────────────────────────────────────────────────

fn render_no_pet(stdout: &mut StandardStream, data: &StatusInput, width: usize) -> Result<()> {
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

    let branch = git_branch().unwrap_or_else(|| "—".to_string());
    let cwd = data.cwd.clone()
        .or_else(|| current_dir_short())
        .unwrap_or_else(|| "~".to_string());

    // 轉 home-relative 再截短（/home/user/x → ~/x）
    let cwd_rel = to_home_rel(&cwd);
    let cwd_short = shorten_path(&cwd_rel, 22);
    let branch_short = shorten_str(&branch, 22);

    // Model 顯示（含 context window 大小）
    let model_display = match (&data.model, data.context_window_size) {
        (Some(m), Some(cw)) => format!("{} ({})", m, fmt_ctx_window(cw)),
        (Some(m), None) => m.clone(),
        (None, _) => "—".to_string(),
    };

    // XP bar
    let xp_filled = (pet.xp as f32 / pet.xp_to_next as f32 * 6.0) as usize;
    let xp_bar = format!("{}{}", "▮".repeat(xp_filled.min(6)), "▯".repeat(6 - xp_filled.min(6)));

    // HP bar
    let hp_filled = ((pet.hp as f32 / 100.0) * 6.0).min(6.0) as usize;
    let hp_bar = format!("{}{}", "▮".repeat(hp_filled), "▯".repeat(6 - hp_filled));

    // 6 行面板文字（不含 art 欄）
    let line0 = header_line(&model_display, &format!("{} ── {}", cwd_short, branch_short), inner);
    let line2 = divider_line(&village.display_name, inner);
    let line3 = box_line(&format!("  {} Lv.{}  HP {} {}  XP {} {}/{}",
        pet.name, pet.level, hp_bar, pet.hp, xp_bar, pet.xp, pet.xp_to_next), inner);
    let line4 = box_line(&format!("  ATK:{:2}  DEF:{:2}  SUP:{:2}  VER:{:2}",
        pet.atk, pet.def, pet.sup, pet.ver), inner);
    let line5 = footer_line("Memory: active", data.model.as_deref().unwrap_or("—"), inner);

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // ── 顏色規格 ───────────────────────────────────────────────
    let art_spec  = cs(village.color);             // 村落色（art 欄）
    let border    = cs(Color::Ansi256(238));        // 暗灰（box 框線）
    let bright    = cs(Color::White);               // 亮白（header model+cwd）
    let village_c = cs(village.color);              // 村落色（divider）
    let pet_c     = cs(Color::Cyan);                // 青色（pet name/HP/XP）
    let stat_c    = cs(Color::Ansi256(245));        // 灰（stats 數字）
    let footer_c  = cs(Color::Ansi256(238));        // 暗灰（footer）

    // Row 0: header（亮白）
    write_art_row(stdout, art_lines.get(0), &art_spec)?;
    stdout.set_color(&bright)?; writeln!(stdout, "{}", line0)?;

    // Row 1: usage bars（語義顏色，逐段輸出）
    write_art_row(stdout, art_lines.get(1), &art_spec)?;
    write_usage_row(stdout, data, inner, &border)?;

    // Row 2: village divider（村落色）
    write_art_row(stdout, art_lines.get(2), &art_spec)?;
    stdout.set_color(&village_c)?; writeln!(stdout, "{}", line2)?;

    // Row 3: pet HP + XP（青色）
    write_art_row(stdout, art_lines.get(3), &art_spec)?;
    stdout.set_color(&pet_c)?; writeln!(stdout, "{}", line3)?;

    // Row 4: stats（灰）
    write_art_row(stdout, art_lines.get(4), &art_spec)?;
    stdout.set_color(&stat_c)?; writeln!(stdout, "{}", line4)?;

    // Row 5: footer（暗灰）
    write_art_row(stdout, art_lines.get(5), &art_spec)?;
    stdout.set_color(&footer_c)?; writeln!(stdout, "{}", line5)?;

    stdout.reset()?;
    Ok(())
}

/// 快速建立 ColorSpec
fn cs(c: Color) -> ColorSpec {
    let mut s = ColorSpec::new();
    s.set_fg(Some(c));
    s
}

/// 輸出 art 欄（固定 15 cols，村落色）
fn write_art_row(stdout: &mut StandardStream, art: Option<&&str>, spec: &ColorSpec) -> Result<()> {
    stdout.set_color(spec)?;
    write!(stdout, "{:<15}", art.copied().unwrap_or(""))?;
    Ok(())
}

/// usage 行逐段輸出，每段按 pct 選色（green / yellow / red）
fn write_usage_row(stdout: &mut StandardStream, data: &StatusInput, inner: usize, border: &ColorSpec) -> Result<()> {
    // 收集各段（text, pct）
    let mut segments: Vec<(String, f64)> = Vec::new();

    if let Some(pct) = data.five_hour_pct {
        let remain = data.five_hour_resets_at
            .map(|t| format!(" {}", fmt_remaining(t))).unwrap_or_default();
        segments.push((format!("5h {} {:>2.0}%{}", pbar(pct, 6), pct * 100.0, remain), pct));
    }
    if let Some(pct) = data.seven_day_pct {
        let remain = data.seven_day_resets_at
            .map(|t| format!(" {}", fmt_remaining(t))).unwrap_or_default();
        segments.push((format!("7d {} {:>2.0}%{}", pbar(pct, 6), pct * 100.0, remain), pct));
    }
    if let Some(pct) = data.context_pct {
        segments.push((format!("ctx {} {:>2.0}%", pbar(pct, 6), pct * 100.0), pct));
    }

    // 輸出左框 │
    stdout.set_color(border)?;
    write!(stdout, "│  ")?;

    let sep_color = cs(Color::Ansi256(238));
    let mut visible_used: usize = 2; // "  " after │

    for (i, (text, pct)) in segments.iter().enumerate() {
        if i > 0 {
            stdout.set_color(&sep_color)?;
            write!(stdout, "  ·  ")?;
            visible_used += 5;
        }
        let color = usage_color(*pct);
        stdout.set_color(&cs(color))?;
        write!(stdout, "{}", text)?;
        visible_used += vis(text);
    }

    if segments.is_empty() {
        stdout.set_color(&cs(Color::Ansi256(238)))?;
        write!(stdout, "—")?;
        visible_used += 1;
    }

    // 補空白到 inner 欄，再輸出右框 │
    let pad_needed = inner.saturating_sub(visible_used);
    stdout.set_color(border)?;
    writeln!(stdout, "{}│", " ".repeat(pad_needed))?;
    Ok(())
}

/// 計算距某 Unix timestamp 的剩餘時間字串（"4h2m"、"2d3h"）
fn fmt_remaining(resets_at: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let secs = (resets_at - now).max(0) as u64;
    if secs >= 86400 {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    } else if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}m", secs / 60)
    }
}

/// pct（0.0–1.0）→ 語義顏色
fn usage_color(pct: f64) -> Color {
    if pct < 0.50 { Color::Green }
    else if pct < 0.80 { Color::Yellow }
    else { Color::Red }
}

/// 建構 usage stats 行（純文字版，供 no-color fallback）
fn build_usage_line(data: &StatusInput) -> String {
    let mut parts: Vec<String> = Vec::new();

    // 5h rate limit
    if let Some(pct) = data.five_hour_pct {
        let remain = data.five_hour_resets_at
            .map(|t| format!(" {}", fmt_remaining(t)))
            .unwrap_or_default();
        parts.push(format!("5h {} {:>2.0}%{}", pbar(pct, 6), pct * 100.0, remain));
    }

    // 7d rate limit
    if let Some(pct) = data.seven_day_pct {
        let remain = data.seven_day_resets_at
            .map(|t| format!(" {}", fmt_remaining(t)))
            .unwrap_or_default();
        parts.push(format!("7d {} {:>2.0}%{}", pbar(pct, 6), pct * 100.0, remain));
    }

    // Context window 使用率
    if let Some(pct) = data.context_pct {
        parts.push(format!("ctx {} {:>2.0}%", pbar(pct, 6), pct * 100.0));
    }

    if parts.is_empty() {
        if std::env::var("CODEFORGE_DEBUG").is_ok() {
            let keys: Vec<String> = data.raw.as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            return format!("keys: {}", keys.join(", "));
        }
        return "—".to_string();
    }

    parts.join("  ·  ")
}

/// 將完整路徑轉為 home-relative（/home/user/x → ~/x）
fn to_home_rel(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if path.starts_with(home_str.as_ref()) {
            return format!("~{}", &path[home_str.len()..]);
        }
    }
    path.to_string()
}

/// 截短路徑（保留尾端，加 … 前綴）
fn shorten_path(path: &str, max: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max { return path.to_string(); }
    let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
    format!("…{}", tail)
}

/// 截短字串（保留前端，加 … 後綴）
fn shorten_str(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { return s.to_string(); }
    let head: String = chars[..max - 1].iter().collect();
    format!("{}…", head)
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
    const MAX: usize = 25;
    if path.chars().count() > MAX {
        let tail: String = path.chars().rev().take(MAX - 1).collect::<String>()
            .chars().rev().collect();
        Some(format!("…{}", tail))
    } else {
        Some(path)
    }
}
