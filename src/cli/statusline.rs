/// Statusline — 兩欄渲染：info（左）+ art（右）
/// info 欄 pad 到 panel_w，後接 2 空格，再寫 art（已 pad 到 ART_W）
/// UX Pro palette: 三層亮度 (Tier1=222-231 身份, Tier2=71-179 狀態, Tier3=236-246 背景)
use anyhow::Result;
use crate::db;
use crate::pet::live_state::LiveState;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
use owo_colors::OwoColorize;
use unicode_width::UnicodeWidthStr;
use std::io::{self, Write};
use rust_i18n::t;

pub fn run(ctx: &db::Context) -> Result<()> {
    let data = read_status_input();
    let conn = ctx.open_db()?;
    let has_pet = LiveState::exists(&conn).unwrap_or(false);

    let width: usize = std::env::var("CODEFORGE_WIDTH")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(100);

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if !has_pet {
        render_no_pet(&mut out, &data, width)?;
    } else {
        // LiveState composes daemon-authored pet_snapshot (fallback: Phase 1 pet)
        // with an overlay of unseen event_inbox XP — so the pet number reacts
        // to hook events immediately, without waiting for the daemon tick.
        let live = LiveState::load(&conn).unwrap_or_else(|_| LiveState {
            state: PetState::default(),
            pending_events: 0,
            pending_xp: 0,
        });
        let village = VILLAGES.iter().find(|v| v.id == live.state.village).unwrap_or(&VILLAGES[2]);
        render_full(&mut out, &data, &live.state, village, width)?;
    }

    out.flush()?;
    Ok(())
}

// ─── Input ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct StatusInput {
    raw: serde_json::Value,
    model: Option<String>,
    cwd: Option<String>,
    version: Option<String>,           // running session version (from /proc tree)
    latest_version: Option<String>,    // latest installed version (symlink target)
    update_available: bool,            // latest > session version
    message: Option<String>,           // pet speech bubble (from JSON "message" field)
    context_pct: Option<f64>,          // 0.0–1.0
    context_window_size: Option<u64>,
    five_hour_pct: Option<f64>,
    five_hour_resets_at: Option<i64>,
    seven_day_pct: Option<f64>,
    seven_day_resets_at: Option<i64>,
}

fn read_status_input() -> StatusInput {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() && !line.trim().is_empty() {
        if std::env::var("CODEFORGE_DEBUG").is_ok() {
            let _ = std::fs::write("/tmp/codeforge-sl.json", &line);
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let model = v["model"]["display_name"].as_str()
                .or_else(|| v["model"]["id"].as_str())
                .or_else(|| v["model"].as_str())
                .map(|s| s.replace("claude-", "").replace("-20", " 20"));

            let context_pct = v["context_window"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let context_window_size = v["context_window"]["context_window_size"].as_u64();
            let five_hour_pct = v["rate_limits"]["five_hour"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let five_hour_resets_at = v["rate_limits"]["five_hour"]["resets_at"].as_i64();
            let seven_day_pct = v["rate_limits"]["seven_day"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let seven_day_resets_at = v["rate_limits"]["seven_day"]["resets_at"].as_i64();

            // Compute version info once — avoids triple subprocess spawn.
            // session_v: running CC version from /proc tree; latest_v: from `claude --version`
            let session_v = claude_session_version();
            let latest_v  = claude_latest_version();
            let update_available = matches!((&session_v, &latest_v),
                (Some(s), Some(l)) if s != l);

            return StatusInput {
                raw: v.clone(),
                model,
                cwd: v["cwd"].as_str()
                    .or_else(|| v["workspace"]["current_dir"].as_str())
                    .map(|s| s.to_string()),
                version: session_v.or_else(|| latest_v.clone()),
                latest_version: latest_v,
                update_available,
                message: v["message"].as_str().map(|s| s.to_string()),
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

// ─── UX Pro color palette (ANSI256 → truecolor) ───────────────────────────────

type Rgb = (u8, u8, u8);

const DELIM:    Rgb = (0x44, 0x44, 0x44); // 238 — ( )
const MODEL_C:  Rgb = (0xFF, 0xD7, 0x87); // 222 — model name
const CTX_SZ:   Rgb = (0xD7, 0xAF, 0x5F); // 179 — ctx window size
const CWD_C:    Rgb = (0x5F, 0xAF, 0xAF); // 73  — cwd path
const BRANCH_C: Rgb = (0x87, 0xD7, 0x87); // 114 — git branch
const BAR_LOW:  Rgb = (0x87, 0xD7, 0x87); // 114 — bar <50%
const BAR_MID:  Rgb = (0xD7, 0xAF, 0x5F); // 179 — bar 50-80%
const BAR_HIGH: Rgb = (0xD7, 0x5F, 0x5F); // 167 — bar >80%
const BAR_EMPTY:Rgb = (0x30, 0x30, 0x30); // 236 — bar ▯
const BAR_LBL:  Rgb = (0x8A, 0x8A, 0x8A); // 245 — "5h" "7d" "ctx"
const REMAIN:   Rgb = (0x58, 0x58, 0x58); // 240 — "4h2m"
const PET_NAME: Rgb = (0xEE, 0xEE, 0xEE); // 255 — pet name
const PET_LV:   Rgb = (0xFF, 0xD7, 0x87); // 222 — "Lv.N"
const STAT_LBL: Rgb = (0x58, 0x58, 0x58); // 240 — "ATK:"
const STAT_VAL: Rgb = (0x94, 0x94, 0x94); // 246 — stat numbers
const MEM_ACT:  Rgb = (0x5F, 0xAF, 0x5F); // 71  — memory active
const UPDATE_C: Rgb = (0xFF, 0xAF, 0x00); // 214 — amber update banner

// ─── Color helpers ────────────────────────────────────────────────────────────

fn tc(s: &str, (r, g, b): Rgb) -> String {
    format!("{}", s.truecolor(r, g, b))
}

fn tc_bold(s: &str, (r, g, b): Rgb) -> String {
    format!("{}", s.truecolor(r, g, b).bold())
}

fn tcs(s: String, (r, g, b): Rgb) -> String {
    format!("{}", s.truecolor(r, g, b))
}

/// Wrap inner content in thin ▏▕ vertical bar delimiters (U+258F / U+2595)
/// inner_vis = visible char count of `inner` (not counting ANSI codes)
fn seg(inner: &str, inner_vis: usize) -> (String, usize) {
    let s = format!("{}{}{}", tc("▏", DELIM), inner, tc("▕", DELIM));
    (s, inner_vis + 2) // ▏(1col) + content + ▕(1col)
}

fn bar_rgb(pct: f64) -> Rgb {
    if pct < 0.5 { BAR_LOW } else if pct < 0.8 { BAR_MID } else { BAR_HIGH }
}

/// Colored progress bar (filled=semantic, empty=dim 236)
fn colored_bar(pct: f64, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 1.0)) * width as f64) as usize;
    let empty = width - filled;
    format!("{}{}",
        tcs("▮".repeat(filled), bar_rgb(pct)),
        tcs("▯".repeat(empty), BAR_EMPTY)
    )
}

/// HP bar color: green >60%, amber 30-60%, red <30%
fn hp_rgb(hp: u32) -> Rgb {
    if hp > 60 { BAR_LOW } else if hp > 30 { BAR_MID } else { BAR_HIGH }
}

/// Visible column width (handles wide chars: CJK=2col, ASCII/▮▯=1col)
/// Use this for calculating layout, NOT str.len() or chars().count()
fn vis(s: &str) -> usize { UnicodeWidthStr::width(s) }

/// Shorten path keeping tail, using column width (~/projects/very/long → …/very/long)
fn shorten_path(path: &str, max_cols: usize) -> String {
    if max_cols == 0 { return "…".to_string(); }
    if vis(path) <= max_cols { return path.to_string(); }
    // Walk from end, collect chars until we've used max_cols-1 columns
    let mut cols = 0usize;
    let tail: String = path.chars().rev().take_while(|c| {
        let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
        if cols + w > max_cols - 1 { return false; }
        cols += w; true
    }).collect::<String>().chars().rev().collect();
    format!("…{}", tail)
}

/// Shorten string keeping head, using column width
fn shorten_str(s: &str, max_cols: usize) -> String {
    if max_cols == 0 { return "…".to_string(); }
    if vis(s) <= max_cols { return s.to_string(); }
    let mut cols = 0usize;
    let head: String = s.chars().take_while(|c| {
        let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
        if cols + w > max_cols - 1 { return false; }
        cols += w; true
    }).collect();
    format!("{}…", head)
}

fn to_home_rel(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(h.as_ref()) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

fn fmt_ctx_window(n: u64) -> String {
    if n >= 1_000_000 { format!("{}M", n / 1_000_000) }
    else if n >= 1_000 { format!("{}k", n / 1_000) }
    else { format!("{}", n) }
}

/// Strip ANSI/VT escape sequences from a string for visible-width measurement.
/// Handles CSI sequences (ESC [ ... <final>) where final byte is 0x40–0x7E,
/// covering SGR (colors/bold), cursor movement, erase, etc.
/// Note: OSC sequences (ESC ] ... BEL/ST) are not produced by owo_colors and
/// are not handled here — callers must only pass owo_colors output.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            // Consume until ANSI CSI final byte (0x40–0x7E = '@'–'~')
            for c2 in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&c2) { break; }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Visible column width of an ANSI string (excludes escape codes)
fn ansi_vis(s: &str) -> usize { vis(&strip_ansi(s)) }

/// Pad an ANSI string to at least `width` visible columns with trailing spaces
fn pad_to_vis(s: &str, width: usize) -> String {
    let w = ansi_vis(s);
    if w < width { format!("{}{}", s, " ".repeat(width - w)) } else { s.to_string() }
}

fn fmt_remaining(resets_at: i64) -> String {
    let secs = (resets_at - chrono::Utc::now().timestamp()).max(0) as u64;
    if secs >= 86400 { format!("{}d{}h", secs / 86400, (secs % 86400) / 3600) }
    else if secs >= 3600 { format!("{}h{}m", secs / 3600, (secs % 3600) / 60) }
    else { format!("{}m", secs / 60) }
}

/// Extract Claude version from a binary path like:
///   /home/user/.local/share/claude/versions/2.1.107
fn extract_claude_ver(path: &str) -> Option<String> {
    let after = path.split("/versions/").nth(1)?;
    let ver = after.split('/').next()?;
    if ver.starts_with(|c: char| c.is_ascii_digit()) {
        Some(ver.to_string())
    } else {
        None
    }
}

/// Walk up the /proc process tree to find the Claude Code binary version.
/// Claude Code uses a symlink model: ~/.local/share/claude/versions/X.Y.Z
/// The running session's ancestor process has that path as its exe.
#[cfg(target_os = "linux")]
fn claude_session_version() -> Option<String> {
    fn ppid(pid: u32) -> Option<u32> {
        let s = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        s.lines()
            .find(|l| l.starts_with("PPid:\t"))
            .and_then(|l| l.split('\t').nth(1)?.trim().parse().ok())
    }
    let mut pid = std::process::id();
    for _ in 0..10 {
        pid = ppid(pid)?;
        if pid <= 1 { break; }
        if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
            if let Some(ver) = extract_claude_ver(&exe.to_string_lossy()) {
                return Some(ver);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn claude_session_version() -> Option<String> { None }

/// Version of the Claude binary on disk — follows symlink to latest installed.
/// Output format: "2.1.107 (Claude Code)" → strip → "2.1.107"
fn claude_latest_version() -> Option<String> {
    std::process::Command::new("claude")
        .arg("--version")
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            let t = s.trim();
            // strip_prefix/strip_suffix: one-shot match, unlike trim_*_matches which repeats
            let t = t.strip_prefix("claude ").unwrap_or(t);
            let t = t.strip_suffix(" (Claude Code)").unwrap_or(t);
            t.to_string()
        })
        .filter(|s| !s.is_empty() && s.starts_with(|c: char| c.is_ascii_digit()))
}

fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output().ok()
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
    Some(shorten_path(&path, 25))
}

// ─── Usage segment builder ────────────────────────────────────────────────────

/// Usage segment WITHOUT ▏▕ wrappers — for box layout row 1
fn usage_bare(label: &str, pct: f64, remain: Option<String>) -> String {
    let bar = colored_bar(pct, 6);
    let pct_num = format!("{:.0}%", pct * 100.0);
    let (r, g, b) = bar_rgb(pct);
    let remain_part = match &remain {
        Some(s) => format!(" {}", tc(s, REMAIN)),
        None => String::new(),
    };
    format!("{} {} {}{}",
        tc(label, BAR_LBL), bar, tc(&pct_num, (r, g, b)), remain_part,
    )
}

fn usage_seg(label: &str, pct: f64, remain: Option<String>) -> String {
    let bar = colored_bar(pct, 6);
    let pct_num = format!("{:.0}%", pct * 100.0);
    let (r, g, b) = bar_rgb(pct);

    let remain_part = match &remain {
        Some(s) => format!(" {}", tc(s, REMAIN)),
        None => String::new(),
    };
    let remain_vis = remain.as_deref().map(|s| 1 + vis(s)).unwrap_or(0);

    // inner: "5h ▮▮▯▯▯▯ 8% 4h2m"
    let inner = format!("{} {} {}{}",
        tc(label, BAR_LBL),
        bar,
        tc(&pct_num, (r, g, b)),
        remain_part
    );
    // vis: label(N) + 1 + 6(bar) + 1 + pct_num.len() + remain_vis
    let inner_vis = vis(label) + 1 + 6 + 1 + vis(&pct_num) + remain_vis;

    seg(&inner, inner_vis).0
}

// ─── Position-based renderer ──────────────────────────────────────────────────

/// 一行的兩欄資料
struct Row {
    /// art 欄（None = 不寫 art，讓 col 0..ART_W 保持空白）
    art: Option<String>,
    /// info 欄（從 col INFO_COL 開始寫）
    info: String,
}

/// 核心渲染引擎：info 欄（左）pad 到 info_w 寬，後接 2 空格，再寫 art（右）
///
/// art 已在 art() closure 中 pad 到 ART_W，讓右側欄對齊。
/// 不需要 MoveToColumn — 純 write! + writeln!，相容所有環境。
/// info_w=0（render_no_pet）時 None art 行直接輸出 info，不加尾端空格。
fn render_rows<W: Write>(out: &mut W, rows: &[Row], info_w: usize) -> Result<()> {
    for row in rows {
        match &row.art {
            Some(art) => writeln!(out, "{}  {}", pad_to_vis(&row.info, info_w), art)?,
            None      => writeln!(out, "{}", row.info)?,
        }
    }
    Ok(())
}

fn render_no_pet<W: Write>(out: &mut W, data: &StatusInput, width: usize) -> Result<()> {
    // Row 0: identity
    let mut id_parts: Vec<String> = Vec::new();
    id_parts.push(seg(&tc_bold("CodeForge", PET_NAME), vis("CodeForge")).0);
    if let Some(m) = &data.model {
        let m_s = shorten_str(m, 20);
        id_parts.push(seg(&tc_bold(&m_s, MODEL_C), vis(&m_s)).0);
    }
    let cwd_raw = data.cwd.clone().or_else(current_dir_short).unwrap_or_else(|| "~".to_string());
    let cwd_s = shorten_path(&to_home_rel(&cwd_raw), 28);
    id_parts.push(seg(&tc(&cwd_s, CWD_C), vis(&cwd_s)).0);
    if let Some(b) = git_branch() {
        let b_s = shorten_str(&b, 22);
        id_parts.push(seg(&tc(&b_s, BRANCH_C), vis(&b_s)).0);
    }

    let rows = vec![
        Row { art: None, info: id_parts.join("  ") },
        Row { art: None, info: build_usage_line(data, width) },
    ];
    render_rows(out, &rows, 0)
}

fn render_full<W: Write>(
    out: &mut W,
    data: &StatusInput,
    pet: &PetState,
    village: &crate::pet::village::Village,
    width: usize,
) -> Result<()> {
    const ART_W: usize = 10;
    let panel_w = width.saturating_sub(ART_W + 2);
    let vrgb = village.rgb();

    let branch = git_branch().unwrap_or_else(|| "—".to_string());
    let cwd_raw = data.cwd.clone()
        .or_else(|| current_dir_short())
        .unwrap_or_else(|| "~".to_string());
    let cwd_home = to_home_rel(&cwd_raw);

    // Helper: wrap content in box row │ content {pad} │ = exactly panel_w visible
    let box_mid = |content: &str, left: &str, right: &str| -> String {
        let pad = panel_w.saturating_sub(vis(left) + ansi_vis(content) + vis(right));
        format!("{}{}{}{}", tc(left, DELIM), content, " ".repeat(pad), tc(right, DELIM))
    };

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // ── Row 0: top border ╭─ model ─── cwd ─── branch ─────────────────────────╮
    //
    // Box-drawing corners connect continuously — no ▏▕ vs ── conflict.
    // Overhead: "╭─ "(3) + " ─── "(5) + " ─── "(5) + "──╮"(3) = 16

    let (model_text, model_vis) = match (&data.model, data.context_window_size) {
        (Some(m), Some(cw)) => {
            let m_s = shorten_str(m, 18);
            let cw_s = fmt_ctx_window(cw);
            let v = vis(&m_s) + 3 + vis(&cw_s); // "m (cw)"
            (format!("{} {}{}{}",
                tc_bold(&m_s, MODEL_C),
                tc("(", DELIM), tc(&cw_s, CTX_SZ), tc(")", DELIM),
            ), v)
        }
        (Some(m), None) => {
            let m_s = shorten_str(m, 18);
            let v = vis(&m_s);
            (tc_bold(&m_s, MODEL_C), v)
        }
        _ => (tc("—", STAT_VAL), 1),
    };

    // cwd: try full, truncate if branch would get < 6 chars
    let cwd_full_vis = vis(&cwd_home);
    let branch_budget_full = panel_w.saturating_sub(16 + model_vis + cwd_full_vis);
    let (cwd_display, cwd_vis) = if branch_budget_full >= 6 {
        (cwd_home.clone(), cwd_full_vis)
    } else {
        let max = panel_w.saturating_sub(16 + model_vis + 6).max(5);
        let s = shorten_path(&cwd_home, max);
        let v = vis(&s);
        (s, v)
    };
    // branch_avail.max(4) ensures branch always gets ≥4 cols, but if panel_w is
    // very small the row may slightly exceed panel_w (r0_fill saturates to 0).
    // Not addressed for degenerate terminals — default width is 100.
    let branch_avail = panel_w.saturating_sub(16 + model_vis + cwd_vis).max(4);
    let branch_short = shorten_str(&branch, branch_avail);
    let branch_vis   = vis(&branch_short);
    let r0_fill = panel_w.saturating_sub(16 + model_vis + cwd_vis + branch_vis);

    let row0_info = format!("{}{}{}{}{}{}{}{}",
        tc("╭─ ", DELIM),
        model_text,
        tc(" ─── ", DELIM),
        tc(&cwd_display, CWD_C),
        tc(" ─── ", DELIM),
        tc(&branch_short, BRANCH_C),
        tcs("─".repeat(r0_fill), DELIM),
        tc("──╮", DELIM),
    );

    // ── Row 1: usage bars inside box │ 5h ▮▯ 5%  7d ▮▮ 39%  ctx ▮▮▮ 54%   │

    let row1_info = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(pct) = data.five_hour_pct {
            parts.push(usage_bare("5h", pct, data.five_hour_resets_at.map(fmt_remaining)));
        }
        if let Some(pct) = data.seven_day_pct {
            parts.push(usage_bare("7d", pct, data.seven_day_resets_at.map(fmt_remaining)));
        }
        if let Some(pct) = data.context_pct {
            parts.push(usage_bare("ctx", pct, None));
        }
        let content = parts.join("  ");
        box_mid(&content, "│ ", "│")
    };

    // ── Row 2: village divider ├─ The Forge-Ruins ──────────────────────────┤

    let vkey = format!("village.{}.name", village.id);
    let vname = t!(&vkey);
    let vname_vis = vis(&*vname);
    let r2_fill = panel_w.saturating_sub(3 + vname_vis + 1 + 1); // ├─ + name + space + ┤
    let row2_info = format!("{}{}{}{}{}",
        tc("├─ ", DELIM),
        tc_bold(&*vname, vrgb),
        tc(" ", DELIM),
        tcs("─".repeat(r2_fill), DELIM),
        tc("┤", DELIM),
    );

    // ── Version info ─────────────────────────────────────────────────────────

    // When update available: show "⬆ vX.Y.Z" (latest = upgrade target)
    // When up to date:       show "vX.Y.Z"   (current running, dimmed)
    let (ver_str, ver_vis) = if data.update_available {
        let latest = data.latest_version.as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let s = format!("{} {}", tc_bold("⬆", UPDATE_C), tc(&latest, UPDATE_C));
        let w = 2 + vis(&latest); // ⬆(1) + space(1) + version
        (s, w)
    } else {
        let cur = data.version.as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let w = vis(&cur);
        (tc(&cur, DELIM), w)
    };

    // ── Art helper: pad art line to ART_W, colored with village rgb ──────────

    let art = |i: usize| -> Option<String> {
        art_lines.get(i).map(|s| {
            let w = vis(s);
            let pad = if w < ART_W { " ".repeat(ART_W - w) } else { String::new() };
            format!("{}{}", tcs(s.to_string(), vrgb), pad)
        })
    };
    let art_s = |i: usize| -> String {
        art(i).unwrap_or_else(|| " ".repeat(ART_W))
    };

    // ── Rows 3-5: pet stats or speech bubble ─────────────────────────────────
    //
    // Normal:
    //   │ Ferris Lv.1  HP █░░ ...                                    │  art[3]
    //   │ ATK: 18  DEF: 20  ...                                       │  art[4]
    //   ╰─ Memory: active ──────────────────────────────── v2.1.105 ──╯  art[5]
    //
    // With bubble (right-aligned, tail in 2-char gap → right-pointing):
    //   │              ╭──────────────────────────╮\  art[3]
    //   │              │ Hello! Let's code today! │ > art[4]
    //   ╰─ Memory: active ──────────────────────────── v2.1.105 ──╯/  art[5]
    //
    // Tail shape in gap (cols panel_w, panel_w+1):
    //   row3: \·   row4: ·>   row5: /·   → reading down: \ / > \ /

    if let Some(msg) = data.message.as_deref() {
        // Write rows 0-2 with normal render_rows
        render_rows(out, &[
            Row { art: art(0), info: row0_info },
            Row { art: art(1), info: row1_info },
            Row { art: art(2), info: row2_info },
        ], panel_w)?;

        // Bubble geometry (right-aligned: right edge at panel_w-1)
        // Clamp message to max safe width to prevent row overflow.
        // Max safe = panel_w - 7: bw_max(panel_w-3) - 4 content overhead
        let max_msg = panel_w.saturating_sub(7).max(1);
        let msg_display = shorten_str(msg, max_msg);
        let msg_vis_w = vis(&msg_display);
        // bw = bubble width: │ + space + msg + space + │
        let bw = (msg_vis_w + 4).min(panel_w.saturating_sub(3)).max(6);
        let inner_pad = bw.saturating_sub(4 + msg_vis_w);
        // Frame left overhead: │(1) + space(1) = 2
        let left_fill = panel_w.saturating_sub(2 + bw);

        // Row 3: │ {left}╭{─×bw-2}╮  — bubble top, right edge at panel_w-1
        // Width: 2 + left_fill + bw = panel_w ✓
        let r3 = format!("{}{}{}{}{}",
            tc("│ ", DELIM),
            " ".repeat(left_fill),
            tc("╭", vrgb),
            tcs("─".repeat(bw.saturating_sub(2)), vrgb),
            tc("╮", vrgb),
        );
        // Gap: `\ ` → \ at panel_w, space at panel_w+1
        writeln!(out, "{}{} {}", pad_to_vis(&r3, panel_w), tc("\\", vrgb), art_s(3))?;

        // Row 4: │ {left}│ {msg}{pad}·│  — bubble content
        // Width: 2 + left_fill + 2 + msg_vis_w + inner_pad + 1 + 1 = panel_w ✓
        let r4 = format!("{}{}{}{}{}{}{}",
            tc("│ ", DELIM),
            " ".repeat(left_fill),
            tc("│ ", vrgb),
            tc(&msg_display, PET_NAME),
            " ".repeat(inner_pad),
            tc(" ", vrgb),
            tc("│", vrgb),
        );
        // Gap: ` >` → space at panel_w, > at panel_w+1
        writeln!(out, "{} {}{}", pad_to_vis(&r4, panel_w), tc(">", vrgb), art_s(4))?;

        // Row 5: normal frame bottom border, tail `/` in gap
        // "╰─ "(3) + mem_label + " " + mem_status + " "(1) + fill + " "(1) + ver + " ──╯"(4)
        // vis() is unicode-width-aware so translations auto-adapt, but keep
        // ui.memory_label + ui.status_active short (≤8 vis each) to avoid panel overflow.
        let mem_label = t!("ui.memory_label").to_string();
        let mem_status = t!("ui.status_active").to_string();
        let r5_fixed = 3 + vis(&mem_label) + 1 + vis(&mem_status) + 2 + ver_vis + 4;
        let r5_fill = panel_w.saturating_sub(r5_fixed);
        let r5 = format!("{}{}{}{}{}{}{}",
            tc("╰─ ", DELIM),
            tc(&mem_label, STAT_LBL),
            tc_bold(&format!(" {}", mem_status), MEM_ACT),
            tc(" ", DELIM),
            tcs("─".repeat(r5_fill), DELIM),
            tc(" ", DELIM),
            format!("{}{}", ver_str, tc(" ──╯", DELIM)),
        );
        // Gap: `/ ` → / at panel_w, space at panel_w+1
        writeln!(out, "{}{} {}", pad_to_vis(&r5, panel_w), tc("/", vrgb), art_s(5))?;

    } else {
        // ── Row 3: pet HP / XP ───────────────────────────────────────────────

        let hp_pct = (pet.hp as f64 / 100.0).clamp(0.0, 1.0);
        let hp_filled = (hp_pct * 6.0) as usize;
        let hp_bar = format!("{}{}",
            tcs("█".repeat(hp_filled), hp_rgb(pet.hp)),
            tcs("░".repeat(6 - hp_filled), BAR_EMPTY),
        );
        let xp_filled = ((pet.xp as f64 / pet.xp_to_next as f64).clamp(0.0, 1.0) * 6.0) as usize;
        let xp_bar = format!("{}{}",
            tcs("█".repeat(xp_filled), vrgb),
            tcs("░".repeat(6 - xp_filled), BAR_EMPTY),
        );
        let r3_content = format!("{} {}  {} {} {}  {} {} {}/{}",
            tc_bold(&pet.name, PET_NAME),
            tc(&format!("{}{}", t!("stat.lv"), pet.level), PET_LV),
            tc(&*t!("stat.hp"), STAT_LBL), hp_bar, tc(&pet.hp.to_string(), hp_rgb(pet.hp)),
            tc(&*t!("stat.xp"), STAT_LBL), xp_bar,
            tc(&pet.xp.to_string(), vrgb),
            tc(&pet.xp_to_next.to_string(), STAT_VAL),
        );
        let row3_info = box_mid(&r3_content, "│ ", "│");

        // ── Row 4: stats ─────────────────────────────────────────────────────

        let r4_content = format!("{} {}  {} {}  {} {}  {} {}",
            tc(&*t!("stat.atk"), STAT_LBL), tc(&format!("{:2}", pet.atk), STAT_VAL),
            tc(&*t!("stat.def"), STAT_LBL), tc(&format!("{:2}", pet.def), STAT_VAL),
            tc(&*t!("stat.sup"), STAT_LBL), tc(&format!("{:2}", pet.sup), STAT_VAL),
            tc(&*t!("stat.ver"), STAT_LBL), tc(&format!("{:2}", pet.ver), STAT_VAL),
        );
        let row4_info = box_mid(&r4_content, "│ ", "│");

        // ── Row 5: bottom border ─────────────────────────────────────────────
        // "╰─ "(3) + mem_label + " " + mem_status + " "(1) + fill + " "(1) + ver + " ──╯"(4)
        // vis() is unicode-width-aware so translations auto-adapt, but keep
        // ui.memory_label + ui.status_active short (≤8 vis each) to avoid panel overflow.
        let mem_label = t!("ui.memory_label").to_string();
        let mem_status = t!("ui.status_active").to_string();
        let r5_fixed = 3 + vis(&mem_label) + 1 + vis(&mem_status) + 2 + ver_vis + 4;
        let r5_fill = panel_w.saturating_sub(r5_fixed);
        let row5_info = format!("{}{}{}{}{}{}{}",
            tc("╰─ ", DELIM),
            tc(&mem_label, STAT_LBL),
            tc_bold(&format!(" {}", mem_status), MEM_ACT),
            tc(" ", DELIM),
            tcs("─".repeat(r5_fill), DELIM),
            tc(" ", DELIM),
            format!("{}{}", ver_str, tc(" ──╯", DELIM)),
        );

        let rows = vec![
            Row { art: art(0), info: row0_info },
            Row { art: art(1), info: row1_info },
            Row { art: art(2), info: row2_info },
            Row { art: art(3), info: row3_info },
            Row { art: art(4), info: row4_info },
            Row { art: art(5), info: row5_info },
        ];
        render_rows(out, &rows, panel_w)?;
    }

    Ok(())
}

// ─── Usage line builder ────────────────────────────────────────────────────────

fn build_usage_line(data: &StatusInput, _panel_w: usize) -> String {
    let mut segs: Vec<String> = Vec::new();

    if let Some(pct) = data.five_hour_pct {
        let remain = data.five_hour_resets_at.map(fmt_remaining);
        segs.push(usage_seg("5h", pct, remain));
    }
    if let Some(pct) = data.seven_day_pct {
        let remain = data.seven_day_resets_at.map(fmt_remaining);
        segs.push(usage_seg("7d", pct, remain));
    }
    if let Some(pct) = data.context_pct {
        segs.push(usage_seg("ctx", pct, None));
    }

    if segs.is_empty() {
        if std::env::var("CODEFORGE_DEBUG").is_ok() {
            let keys: Vec<String> = data.raw.as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            return tc(&format!("keys: {}", keys.join(", ")), STAT_VAL);
        }
        return tc("—", DELIM);
    }

    segs.join("  ")
}

