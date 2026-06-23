use crate::commentary::display::statusline_override;
use crate::daemon::strategy::{Strategy, DEFAULT_STRATEGY};
use crate::db;
use crate::pet::ability::next_unlock;
use crate::pet::live_state::LiveState;
use crate::pet::session::{get_last_seen, update_last_seen, WelcomeBackSummary};
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
/// Statusline — 兩欄渲染：info（左）+ art（右）
/// info 欄 pad 到 panel_w，後接 2 空格，再寫 art（已 pad 到 ART_W）
/// UX Pro palette: 三層亮度 (Tier1=222-231 身份, Tier2=71-179 狀態, Tier3=236-246 背景)
use anyhow::Result;
use owo_colors::OwoColorize;
use rust_i18n::t;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

pub fn run(ctx: &db::Context) -> Result<()> {
    let mut data = read_status_input();
    let conn = ctx.open_db()?;
    let has_pet = LiveState::exists(&conn).unwrap_or(false);

    let width: usize = std::env::var("CODEFORGE_WIDTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    let stdout = io::stdout();
    let mut out = stdout.lock();

    // LiveState composes daemon-authored pet_snapshot (fallback: Phase 1 pet)
    // with an overlay of unseen event_inbox XP — so the pet number reacts
    // to hook events immediately, without waiting for the daemon tick.
    let loaded = if has_pet {
        LiveState::load(&conn).ok().flatten()
    } else {
        None
    };

    // Phase 3d §3.1: Welcome Back Report. Compute BEFORE updating
    // last_seen — otherwise the first post-gap call overwrites its own
    // baseline. Only rendered when the session gap ≥ ABSENCE_THRESHOLD,
    // so intra-session statusline refreshes (~every 5s) stay silent.
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let hp_snapshot = loaded
        .as_ref()
        .and_then(|l| l.mood.map(|_| (l.state.hp, l.state.hp.max(1))));
    // hp_max isn't exposed on LiveState yet — PetState lacks it too since
    // Phase 1 only had hp. The snapshot does, but adding a field is beyond
    // P5 scope. Pass None so render_lines skips the HP line rather than
    // fabricating a misleading percentage. P3d follow-up in backlog.
    let _ = hp_snapshot;
    if let Ok(Some(last)) = get_last_seen(&conn) {
        if let Ok(Some(summary)) = WelcomeBackSummary::build(&conn, last, now_unix, None) {
            if summary.has_content() || summary.absence_seconds >= 3600 {
                // Print each line plainly — avoids fighting the panel's
                // ANSI framing. Prefix with a soft indicator so the user
                // spots it amid normal output.
                for line in summary.render_lines() {
                    writeln!(out, "{}", line)?;
                }
            }
        }
    }
    // Window is closed — next statusline call sees a fresh baseline.
    let _ = update_last_seen(&conn, now_unix);

    match loaded {
        Some(live) => {
            // Phase 3c P5: commentary bubble. Priority order when picking
            // the speech-bubble text:
            //   1. stdin "message" (explicit caller intent — never stomped)
            //   2. Commentary feed (5% roll, ≤1h old, opt-in gate)
            //   3. daemon-authored last_message (combat narration)
            if data.message.is_none() {
                if let Ok(Some(phrase)) = statusline_override(&conn, now_unix) {
                    data.message = Some(phrase);
                } else {
                    data.message = live.last_message.clone();
                }
            }
            let village = VILLAGES
                .iter()
                .find(|v| v.id == live.state.village)
                .unwrap_or(&VILLAGES[2]);
            // Phase 3b: current strategy from snapshot (falls back to
            // DEFAULT_STRATEGY when the snapshot predates v6 — wait-for-tick
            // window is one daemon cycle).
            let strategy = live.strategy.unwrap_or(DEFAULT_STRATEGY);
            render_full(&mut out, &data, &live.state, village, strategy, width)?;
        }
        None => render_no_pet(&mut out, &data, width)?,
    }

    out.flush()?;

    // Task 8: fire-and-forget detached probe spawn (after render, non-blocking).
    // Conditions: opted_in && should_refresh && lock acquired → spawn child.
    crate::mnemos::health::maybe_spawn_probe();

    Ok(())
}

// ─── Input ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct StatusInput {
    model: Option<String>,
    cwd: Option<String>,
    version: Option<String>, // running session version (from /proc tree)
    latest_version: Option<String>, // latest installed version (symlink target)
    update_available: bool,  // latest > session version
    message: Option<String>, // pet speech bubble (from JSON "message" field)
    context_pct: Option<f64>, // 0.0–1.0
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
            let model = v["model"]["display_name"]
                .as_str()
                .or_else(|| v["model"]["id"].as_str())
                .or_else(|| v["model"].as_str())
                .map(|s| s.replace("claude-", "").replace("-20", " 20"));

            let context_pct = v["context_window"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let context_window_size = v["context_window"]["context_window_size"].as_u64();
            let five_hour_pct = v["rate_limits"]["five_hour"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let five_hour_resets_at = v["rate_limits"]["five_hour"]["resets_at"].as_i64();
            let seven_day_pct = v["rate_limits"]["seven_day"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let seven_day_resets_at = v["rate_limits"]["seven_day"]["resets_at"].as_i64();

            // Compute version info once — avoids triple subprocess spawn.
            // session_v: running CC version from /proc tree; latest_v: from `claude --version`
            let session_v = claude_session_version();
            let latest_v = claude_latest_version();
            let update_available = matches!((&session_v, &latest_v),
                (Some(s), Some(l)) if s != l);

            return StatusInput {
                model,
                cwd: v["cwd"]
                    .as_str()
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

const DELIM: Rgb = (0x44, 0x44, 0x44); // 238 — ( )
const DELIM_DIM: Rgb = (0x30, 0x30, 0x30); // 236 — dimmed separator (ctx>80%)
const AHEAD_BEHIND: Rgb = (0x87, 0xAF, 0xD7); // 110 — git ⇡⇣ chip (soft blue)
const HINT_C: Rgb = (0x5F, 0x5F, 0x5F); // 240 — onboarding hint (slightly above DELIM)
const MODEL_C: Rgb = (0xFF, 0xD7, 0x87); // 222 — model name
const CTX_SZ: Rgb = (0xD7, 0xAF, 0x5F); // 179 — ctx window size
const CWD_C: Rgb = (0x5F, 0xAF, 0xAF); // 73  — cwd path
const BRANCH_C: Rgb = (0x87, 0xD7, 0x87); // 114 — git branch
const BAR_LOW: Rgb = (0x87, 0xD7, 0x87); // 114 — bar <50%
const BAR_MID: Rgb = (0xD7, 0xAF, 0x5F); // 179 — bar 50-80%
const BAR_HIGH: Rgb = (0xD7, 0x5F, 0x5F); // 167 — bar >80%
const BAR_EMPTY: Rgb = (0x30, 0x30, 0x30); // 236 — bar ▯
const BAR_LBL: Rgb = (0x8A, 0x8A, 0x8A); // 245 — "5h" "7d" "ctx"
const REMAIN: Rgb = (0x58, 0x58, 0x58); // 240 — "4h2m"
const PET_NAME: Rgb = (0xEE, 0xEE, 0xEE); // 255 — pet name
const PET_LV: Rgb = (0xFF, 0xD7, 0x87); // 222 — "Lv.N"
const STAT_LBL: Rgb = (0x58, 0x58, 0x58); // 240 — "ATK:"
const STAT_VAL: Rgb = (0x94, 0x94, 0x94); // 246 — stat numbers
const MEM_ACT: Rgb = (0x5F, 0xAF, 0x5F); // 71  — memory active
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

fn bar_rgb(pct: f64) -> Rgb {
    if pct < 0.5 {
        BAR_LOW
    } else if pct < 0.8 {
        BAR_MID
    } else {
        BAR_HIGH
    }
}

/// Colored progress bar (filled=semantic, empty=dim 236)
fn colored_bar(pct: f64, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 1.0)) * width as f64) as usize;
    let empty = width - filled;
    format!(
        "{}{}",
        tcs("▮".repeat(filled), bar_rgb(pct)),
        tcs("▯".repeat(empty), BAR_EMPTY)
    )
}

/// HP bar color: green >60%, amber 30-60%, red <30%
fn hp_rgb(hp: u32) -> Rgb {
    if hp > 60 {
        BAR_LOW
    } else if hp > 30 {
        BAR_MID
    } else {
        BAR_HIGH
    }
}

/// Visible column width (handles wide chars: CJK=2col, ASCII/▮▯=1col)
/// Use this for calculating layout, NOT str.len() or chars().count()
fn vis(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Shorten path keeping tail, using column width (~/projects/very/long → …/very/long)
fn shorten_path(path: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return "…".to_string();
    }
    if vis(path) <= max_cols {
        return path.to_string();
    }
    // Walk from end, collect chars until we've used max_cols-1 columns
    let mut cols = 0usize;
    let tail: String = path
        .chars()
        .rev()
        .take_while(|c| {
            let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
            if cols + w > max_cols - 1 {
                return false;
            }
            cols += w;
            true
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{}", tail)
}

/// Shorten string keeping head, using column width
fn shorten_str(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return "…".to_string();
    }
    if vis(s) <= max_cols {
        return s.to_string();
    }
    let mut cols = 0usize;
    let head: String = s
        .chars()
        .take_while(|c| {
            let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
            if cols + w > max_cols - 1 {
                return false;
            }
            cols += w;
            true
        })
        .collect();
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
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{}", n)
    }
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
                if ('\x40'..='\x7e').contains(&c2) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Visible column width of an ANSI string (excludes escape codes)
fn ansi_vis(s: &str) -> usize {
    vis(&strip_ansi(s))
}

/// Pad an ANSI string to at least `width` visible columns with trailing spaces
fn pad_to_vis(s: &str, width: usize) -> String {
    let w = ansi_vis(s);
    if w < width {
        format!("{}{}", s, " ".repeat(width - w))
    } else {
        s.to_string()
    }
}

fn fmt_remaining(resets_at: i64) -> String {
    let secs = (resets_at - chrono::Utc::now().timestamp()).max(0) as u64;
    if secs >= 86400 {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    } else if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}m", secs / 60)
    }
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
        if pid <= 1 {
            break;
        }
        if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
            if let Some(ver) = extract_claude_ver(&exe.to_string_lossy()) {
                return Some(ver);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn claude_session_version() -> Option<String> {
    None
}

/// Version of the Claude binary on disk — follows symlink to latest installed.
/// Output format: "2.1.107 (Claude Code)" → strip → "2.1.107"
fn claude_latest_version() -> Option<String> {
    std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()
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
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Local ahead / behind counts vs upstream. Returns (ahead, behind).
/// `None` means no upstream is set, no git repo, or the command failed —
/// caller renders nothing in that case. Powerlevel10k-style indicator.
fn git_ahead_behind() -> Option<(u32, u32)> {
    let out = std::process::Command::new("git")
        .args(["rev-list", "--count", "--left-right", "@{u}...HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.split_whitespace();
    let behind: u32 = parts.next()?.parse().ok()?;
    let ahead: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// Powerlevel10k-style `⇡N⇣M` chip. Empty string when in sync (or unknown).
fn fmt_ahead_behind(ab: Option<(u32, u32)>) -> String {
    match ab {
        Some((0, 0)) | None => String::new(),
        Some((a, 0)) => format!("⇡{}", a),
        Some((0, b)) => format!("⇣{}", b),
        Some((a, b)) => format!("⇡{}⇣{}", a, b),
    }
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
    format!(
        "{} {} {}{}",
        tc(label, BAR_LBL),
        bar,
        tc(&pct_num, (r, g, b)),
        remain_part,
    )
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
            None => writeln!(out, "{}", row.info)?,
        }
    }
    Ok(())
}

fn render_no_pet<W: Write>(out: &mut W, data: &StatusInput, width: usize) -> Result<()> {
    // Row 0 — identity strip (no box, no chip wrappers).
    // Composition: model ── cwd ⇡⇣ branch ─────── version
    // Cross-mode anchor: same model/cwd/branch/version order as render_full's
    // top border, just without the ╭─...─╮ frame. Adopting a pet is a "bloom"
    // (extra rows appear), not a re-layout.
    let dimmed = data.context_pct.is_some_and(|p| p > 0.8);
    let delim_c = if dimmed { DELIM_DIM } else { DELIM };

    let (model_text, model_vis) = render_model(&data.model, MODEL_C);

    let cwd_raw = data
        .cwd
        .clone()
        .or_else(current_dir_short)
        .unwrap_or_else(|| "~".to_string());
    let cwd_home = to_home_rel(&cwd_raw);

    let branch_full = git_branch().unwrap_or_else(|| "—".to_string());
    let ab_str = fmt_ahead_behind(git_ahead_behind());
    // ab_part carries ONLY its leading space (matches render_full's pattern).
    // The space BEFORE branch is always written explicitly so cwd and branch
    // can't glue together when ab_part is empty (no-upstream case).
    let (ab_part, ab_vis) = if ab_str.is_empty() {
        (String::new(), 0)
    } else {
        (format!(" {}", tc(&ab_str, AHEAD_BEHIND)), 1 + vis(&ab_str))
    };

    let (ver_str, ver_vis) = render_version(data, delim_c);

    // Layout: " " model " ── " cwd <ab_part> " " branch " " fill(N) " " version
    //   fixed widths: 1 + model + 4 + ab_vis + 1 + 1 + 1 + ver_vis
    let fixed = 1 + model_vis + 4 + ab_vis + 1 + 1 + 1 + ver_vis;
    // Allocate cwd ≥ 10 cols, branch ≥ 4 cols.
    let cwd_budget = width.saturating_sub(fixed + 4).max(10);
    let cwd_display = shorten_path(&cwd_home, cwd_budget);
    let cwd_vis = vis(&cwd_display);
    let branch_budget = width.saturating_sub(fixed + cwd_vis).max(4);
    let branch_display = shorten_str(&branch_full, branch_budget);
    let branch_vis = vis(&branch_display);
    let fill = width.saturating_sub(fixed + cwd_vis + branch_vis).max(2);

    let row0 = format!(
        " {}{}{}{} {} {} {}",
        model_text,
        tc(" ── ", delim_c),
        tc(&cwd_display, CWD_C),
        ab_part,
        tc(&branch_display, BRANCH_C),
        tcs("─".repeat(fill), delim_c),
        ver_str,
    );

    // Row 1 — usage bars on LEFT, onboarding hint right-aligned on the
    // SAME row. Hint stays visible regardless of usage data because
    // "no pet adopted" is orthogonal to "Claude is reporting usage" —
    // the user can be deep in a CC session and still need the nudge.
    // Degradation: if the row is too narrow to fit both, hint shrinks
    // to "→ adopt"; if still no room, hint is dropped entirely.
    let mut usage_parts: Vec<String> = Vec::new();
    if let Some(pct) = data.five_hour_pct {
        usage_parts.push(usage_bare(
            "5h",
            pct,
            data.five_hour_resets_at.map(fmt_remaining),
        ));
    }
    if let Some(pct) = data.seven_day_pct {
        usage_parts.push(usage_bare(
            "7d",
            pct,
            data.seven_day_resets_at.map(fmt_remaining),
        ));
    }
    if let Some(pct) = data.context_pct {
        usage_parts.push(usage_bare("ctx", pct, None));
    }
    let usage_left = if usage_parts.is_empty() {
        tc("─", delim_c)
    } else {
        usage_parts.join("   ")
    };
    let usage_left_vis = ansi_vis(&usage_left);
    // Two hint forms, picked by available width
    let hint_long = "→ codeforge adopt";
    let hint_short = "→ adopt";
    let min_gap = 3; // breathing room between usage and hint
    let avail_for_hint = width.saturating_sub(1 + usage_left_vis + min_gap + 1); // leading space + content + gap + trailing
    let hint_render = if avail_for_hint >= vis(hint_long) {
        Some((hint_long, vis(hint_long)))
    } else if avail_for_hint >= vis(hint_short) {
        Some((hint_short, vis(hint_short)))
    } else {
        None
    };
    let row1 = match hint_render {
        Some((h, hv)) => {
            let pad = width.saturating_sub(1 + usage_left_vis + hv + 1);
            format!(" {}{}{}", usage_left, " ".repeat(pad), tc(h, HINT_C))
        }
        None => format!(" {}", usage_left),
    };

    let rows = vec![
        Row {
            art: None,
            info: row0,
        },
        Row {
            art: None,
            info: row1,
        },
    ];
    render_rows(out, &rows, 0)
}

/// Format model name + optional context-window size. Shared between both modes.
/// Returns (rendered ansi string, visible width).
fn render_model(model: &Option<String>, color: Rgb) -> (String, usize) {
    match model {
        Some(m) => {
            let m_s = shorten_str(m, 20);
            (tc_bold(&m_s, color), vis(&m_s))
        }
        None => (tc("—", STAT_VAL), 1),
    }
}

/// Format CC version chip. Returns (rendered ansi, visible width). Empty when
/// neither version nor latest_version resolved (e.g. running outside Claude).
fn render_version(data: &StatusInput, dim_c: Rgb) -> (String, usize) {
    if data.version.is_none() && data.latest_version.is_none() {
        return (String::new(), 0);
    }
    if data.update_available {
        let latest = data
            .latest_version
            .as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let s = format!("{} {}", tc_bold("⬆", UPDATE_C), tc(&latest, UPDATE_C));
        (s, 2 + vis(&latest))
    } else {
        let cur = data
            .version
            .as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let s = tc(&cur, dim_c);
        (s, vis(&cur))
    }
}

fn render_full<W: Write>(
    out: &mut W,
    data: &StatusInput,
    pet: &PetState,
    village: &crate::pet::village::Village,
    strategy: Strategy,
    width: usize,
) -> Result<()> {
    const ART_W: usize = 10;
    const ART_THRESHOLD: usize = 90; // drop art column below this width

    // V2: context-pressure dimming. When ctx > 80%, all Tier-3 elements
    // (separators, box-drawing, dim labels) shift one stop dimmer. The chat
    // gets visual priority back; the statusline becomes peripheral signal.
    let dimmed = data.context_pct.is_some_and(|p| p > 0.8);
    let delim_c = if dimmed { DELIM_DIM } else { DELIM };

    let art_visible = width >= ART_THRESHOLD;
    let panel_w = if art_visible {
        width.saturating_sub(ART_W + 2)
    } else {
        width
    };
    let vrgb = village.rgb();

    let cwd_raw = data
        .cwd
        .clone()
        .or_else(current_dir_short)
        .unwrap_or_else(|| "~".to_string());
    let cwd_home = to_home_rel(&cwd_raw);
    let branch_full = git_branch().unwrap_or_else(|| "—".to_string());
    let ab_str = fmt_ahead_behind(git_ahead_behind());

    let box_mid = |content: &str, left: &str, right: &str| -> String {
        let pad = panel_w.saturating_sub(vis(left) + ansi_vis(content) + vis(right));
        format!(
            "{}{}{}{}",
            tc(left, delim_c),
            content,
            " ".repeat(pad),
            tc(right, delim_c)
        )
    };

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // ── Row 0: top border ╭─ model (ctx) ─── cwd ⇡N main ──── Village ──╮
    //
    // V2: village name lives in the top border (saves 1 row vs. the old
    // dedicated divider). Identity chips share the same model→cwd→branch
    // order as render_no_pet — cross-mode anchor.

    let (model_inner, model_inner_vis) = match (&data.model, data.context_window_size) {
        (Some(m), Some(cw)) => {
            let m_s = shorten_str(m, 18);
            let cw_s = fmt_ctx_window(cw);
            let v = vis(&m_s) + 3 + vis(&cw_s); // "m (cw)"
            (
                format!(
                    "{} {}{}{}",
                    tc_bold(&m_s, MODEL_C),
                    tc("(", delim_c),
                    tc(&cw_s, CTX_SZ),
                    tc(")", delim_c),
                ),
                v,
            )
        }
        (Some(m), None) => {
            let m_s = shorten_str(m, 18);
            (tc_bold(&m_s, MODEL_C), vis(&m_s))
        }
        _ => (tc("—", STAT_VAL), 1),
    };

    let vkey = format!("village.{}.name", village.id);
    let vname = t!(&vkey).to_string();
    let vname_short = shorten_str(&vname, 22);
    let vname_vis = vis(&vname_short);

    // Fixed overhead on row 0:
    //   ╭─ (3) + model + " ── " (4) + cwd + ⇡N (ab_vis) + " " (1) + branch
    //   + " ──── " (6) + village + " ──╮" (4) + fill(N)
    let ab_part = if ab_str.is_empty() {
        String::new()
    } else {
        format!(" {}", tc(&ab_str, AHEAD_BEHIND))
    };
    let ab_vis = if ab_str.is_empty() {
        0
    } else {
        1 + vis(&ab_str)
    };
    let fixed0 = 3 + model_inner_vis + 4 + ab_vis + 1 + 6 + vname_vis + 4;
    // cwd budget: leave ≥ 4 for branch
    let cwd_budget = panel_w.saturating_sub(fixed0 + 4).max(8);
    let cwd_display = shorten_path(&cwd_home, cwd_budget);
    let cwd_vis_w = vis(&cwd_display);
    let branch_avail = panel_w.saturating_sub(fixed0 + cwd_vis_w).max(4);
    let branch_display = shorten_str(&branch_full, branch_avail);
    let branch_vis_w = vis(&branch_display);
    let r0_fill = panel_w.saturating_sub(fixed0 + cwd_vis_w + branch_vis_w);

    let row0_info = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        tc("╭─ ", delim_c),
        model_inner,
        tc(" ── ", delim_c),
        tc(&cwd_display, CWD_C),
        ab_part,
        tc(&format!(" {}", branch_display), BRANCH_C),
        tc(" ──── ", delim_c),
        tc_bold(&vname_short, vrgb),
        tcs(format!(" {}", "─".repeat(r0_fill)), delim_c),
        tc("──╮", delim_c),
    );

    // ── Row 1: usage bars inside box │ 5h ▮▯ 5%  7d ▮▮ 39%  ctx ▮▮▮ 54%   │

    let row1_info = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(pct) = data.five_hour_pct {
            parts.push(usage_bare(
                "5h",
                pct,
                data.five_hour_resets_at.map(fmt_remaining),
            ));
        }
        if let Some(pct) = data.seven_day_pct {
            parts.push(usage_bare(
                "7d",
                pct,
                data.seven_day_resets_at.map(fmt_remaining),
            ));
        }
        if let Some(pct) = data.context_pct {
            parts.push(usage_bare("ctx", pct, None));
        }
        let content = parts.join("  ");
        box_mid(&content, "│ ", "│")
    };

    // ── Version chip (used by bottom border) ─────────────────────────────────

    let (ver_str, ver_vis) = render_version(data, delim_c);

    // ── Art helper: pad art line to ART_W, colored with village rgb ──────────
    // When art_visible is false, every helper returns None so render_rows
    // omits the trailing "  art" entirely (info_w = panel_w).

    let art = |i: usize| -> Option<String> {
        if !art_visible {
            return None;
        }
        art_lines.get(i).map(|s| {
            let w = vis(s);
            let pad = if w < ART_W {
                " ".repeat(ART_W - w)
            } else {
                String::new()
            };
            format!("{}{}", tcs(s.to_string(), vrgb), pad)
        })
    };
    let art_s = |i: usize| -> String {
        art(i).unwrap_or_else(|| {
            if art_visible {
                " ".repeat(ART_W)
            } else {
                String::new()
            }
        })
    };

    // ── Rows 2-4: pet stats or speech bubble (V2: was rows 3-5) ──────────────
    //
    // Normal:
    //   │ Ferris Lv.1  ♥ █░░ ...                                    │  art[2]
    //   │ ATK 18  DEF 20  ...  ▸ agg                                 │  art[3]
    //   ╰─ memory ● active ──────────────────────────────── v2.1.105 ╯  art[4]
    //
    // Bubble path renumbered (now rows 2,3,4 instead of 3,4,5):

    if let Some(msg) = data.message.as_deref() {
        render_rows(
            out,
            &[
                Row {
                    art: art(0),
                    info: row0_info,
                },
                Row {
                    art: art(1),
                    info: row1_info,
                },
            ],
            panel_w,
        )?;

        let max_msg = panel_w.saturating_sub(7).max(1);
        let msg_display = shorten_str(msg, max_msg);
        let msg_vis_w = vis(&msg_display);
        let bw = (msg_vis_w + 4).min(panel_w.saturating_sub(3)).max(6);
        let inner_pad = bw.saturating_sub(4 + msg_vis_w);
        let left_fill = panel_w.saturating_sub(2 + bw);

        // Row 2 (was 3): bubble top
        let r2 = format!(
            "{}{}{}{}{}",
            tc("│ ", delim_c),
            " ".repeat(left_fill),
            tc("╭", vrgb),
            tcs("─".repeat(bw.saturating_sub(2)), vrgb),
            tc("╮", vrgb),
        );
        if art_visible {
            writeln!(
                out,
                "{}{} {}",
                pad_to_vis(&r2, panel_w),
                tc("\\", vrgb),
                art_s(2)
            )?;
        } else {
            writeln!(out, "{}", r2)?;
        }

        // Row 3 (was 4): bubble content
        let r3 = format!(
            "{}{}{}{}{}{}{}",
            tc("│ ", delim_c),
            " ".repeat(left_fill),
            tc("│ ", vrgb),
            tc(&msg_display, PET_NAME),
            " ".repeat(inner_pad),
            tc(" ", vrgb),
            tc("│", vrgb),
        );
        if art_visible {
            writeln!(
                out,
                "{} {}{}",
                pad_to_vis(&r3, panel_w),
                tc(">", vrgb),
                art_s(3)
            )?;
        } else {
            writeln!(out, "{}", r3)?;
        }

        // Row 4 (was 5): bottom border with memory status + version
        let r4 = bottom_border(panel_w, delim_c, ver_str.clone(), ver_vis);
        if art_visible {
            writeln!(
                out,
                "{}{} {}",
                pad_to_vis(&r4, panel_w),
                tc("/", vrgb),
                art_s(4)
            )?;
        } else {
            writeln!(out, "{}", r4)?;
        }
    } else {
        // ── Row 2 (was 3): pet HP / XP ──────────────────────────────────────
        // V2: ♥/✦ symbols replace stat.hp / stat.xp text labels.

        let hp_pct = (pet.hp as f64 / 100.0).clamp(0.0, 1.0);
        let hp_filled = (hp_pct * 6.0) as usize;
        let hp_bar = format!(
            "{}{}",
            tcs("█".repeat(hp_filled), hp_rgb(pet.hp)),
            tcs("░".repeat(6 - hp_filled), BAR_EMPTY),
        );
        let xp_filled = ((pet.xp as f64 / pet.xp_to_next as f64).clamp(0.0, 1.0) * 6.0) as usize;
        let xp_bar = format!(
            "{}{}",
            tcs("█".repeat(xp_filled), vrgb),
            tcs("░".repeat(6 - xp_filled), BAR_EMPTY),
        );
        let unlock_suffix = next_unlock(pet.level)
            .map(|u| {
                let name = shorten_str(u.name, 16);
                let lv_label = t!("stat.lv");
                format!(
                    "  {} {}{} {}",
                    tc("→", delim_c),
                    tc(&lv_label, STAT_LBL),
                    tc(&u.required_level.to_string(), PET_LV),
                    tc(&name, STAT_VAL),
                )
            })
            .unwrap_or_default();

        let r2_content = format!(
            "{} {}  {} {} {}  {} {} {}/{}{}",
            tc_bold(&pet.name, PET_NAME),
            tc(&format!("{}{}", t!("stat.lv"), pet.level), PET_LV),
            tc("♥", hp_rgb(pet.hp)),
            hp_bar,
            tc(&pet.hp.to_string(), hp_rgb(pet.hp)),
            tc("✦", vrgb),
            xp_bar,
            tc(&pet.xp.to_string(), vrgb),
            tc(&pet.xp_to_next.to_string(), STAT_VAL),
            unlock_suffix,
        );
        let row2_info = box_mid(&r2_content, "│ ", "│");

        // ── Row 3 (was 4): stats ────────────────────────────────────────────
        // V2: no colons on labels; strategy as "▸ agg" (mode indicator, not k:v).

        let r3_content = format!(
            "{} {}  {} {}  {} {}  {} {}   {} {}",
            tc(&t!("stat.atk"), STAT_LBL),
            tc(&format!("{:2}", pet.atk), STAT_VAL),
            tc(&t!("stat.def"), STAT_LBL),
            tc(&format!("{:2}", pet.def), STAT_VAL),
            tc(&t!("stat.sup"), STAT_LBL),
            tc(&format!("{:2}", pet.sup), STAT_VAL),
            tc(&t!("stat.ver"), STAT_LBL),
            tc(&format!("{:2}", pet.ver), STAT_VAL),
            tc("▸", delim_c),
            tc(strategy.short_tag(), STAT_VAL),
        );
        let row3_info = box_mid(&r3_content, "│ ", "│");

        // ── Row 4 (was 5): bottom border with memory ● active + version ─────

        let row4_info = bottom_border(panel_w, delim_c, ver_str, ver_vis);

        let rows = vec![
            Row {
                art: art(0),
                info: row0_info,
            },
            Row {
                art: art(1),
                info: row1_info,
            },
            Row {
                art: art(2),
                info: row2_info,
            },
            Row {
                art: art(3),
                info: row3_info,
            },
            Row {
                art: art(4),
                info: row4_info,
            },
        ];
        render_rows(out, &rows, panel_w)?;
    }

    Ok(())
}

/// Render the bottom border `╰─ memory ● active ─── fill ─── v2.1.142 ──╯`.
/// V2 style: k9s status-dot pattern (`memory ● active`) replaces the colon-
/// separated `Memory: active` form. Caller passes the version chip + its
/// visible width pre-computed (shared with bubble path).
fn bottom_border(panel_w: usize, delim_c: Rgb, ver_str: String, ver_vis: usize) -> String {
    let mem_label = t!("ui.memory_label").to_string(); // "memory"
    let mem_status = t!("ui.status_active").to_string(); // "active"
                                                         // ╰─ (3) + label + " ● " (3) + status + " " (1) + fill + " " (1) + version + " ──╯" (4)
    let mem_label_vis = vis(&mem_label);
    let mem_status_vis = vis(&mem_status);
    let fixed = 3 + mem_label_vis + 3 + mem_status_vis + 2 + ver_vis + 4;
    let fill = panel_w.saturating_sub(fixed);
    format!(
        "{}{}{}{}{}{}{}{}{}",
        tc("╰─ ", delim_c),
        tc(&mem_label, STAT_LBL),
        tc(" ", delim_c),
        tc("●", MEM_ACT),
        tc(&format!(" {}", mem_status), STAT_VAL),
        tc(" ", delim_c),
        tcs("─".repeat(fill), delim_c),
        tc(&format!(" {}", ver_str), delim_c),
        tc(" ──╯", delim_c),
    )
}
