/// Statusline — 兩欄渲染：info（左）+ art（右）
/// info 欄 pad 到 panel_w，後接 2 空格，再寫 art（已 pad 到 ART_W）
/// UX Pro palette: 三層亮度 (Tier1=222-231 身份, Tier2=71-179 狀態, Tier3=236-246 背景)
use anyhow::Result;
use crate::db;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
use owo_colors::OwoColorize;
use unicode_width::UnicodeWidthStr;
use std::io::{self, Write};

pub fn run(ctx: &db::Context) -> Result<()> {
    let data = read_status_input();
    let conn = ctx.open_db()?;
    let has_pet = PetState::exists(&conn).unwrap_or(false);

    let width: usize = std::env::var("CODEFORGE_WIDTH")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(100);

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if !has_pet {
        render_no_pet(&mut out, &data, width)?;
    } else {
        let pet = PetState::load(&conn).unwrap_or_default();
        let village = VILLAGES.iter().find(|v| v.id == pet.village).unwrap_or(&VILLAGES[2]);
        render_full(&mut out, &data, &pet, village, width)?;
    }

    out.flush()?;
    Ok(())
}

// ─── Input ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct StatusInput {
    raw: serde_json::Value,
    model: Option<String>,
    model_date: Option<String>,        // YYYYMMDD from model.id suffix
    cwd: Option<String>,
    version: Option<String>,           // current Claude Code version
    update_available: bool,            // Claude Code has a newer version
    latest_version: Option<String>,    // the newer version string
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

            // Extract 8-digit YYYYMMDD suffix from model.id (e.g. "claude-sonnet-4-6-20251001")
            let model_date = v["model"]["id"].as_str()
                .or_else(|| v["model"].as_str())
                .and_then(|id| {
                    let b = id.as_bytes();
                    let len = b.len();
                    (len >= 8 && b[len-8..].iter().all(|c| c.is_ascii_digit()))
                        .then(|| id[len-8..].to_string())
                });

            let context_pct = v["context_window"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let context_window_size = v["context_window"]["context_window_size"].as_u64();
            let five_hour_pct = v["rate_limits"]["five_hour"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let five_hour_resets_at = v["rate_limits"]["five_hour"]["resets_at"].as_i64();
            let seven_day_pct = v["rate_limits"]["seven_day"]["used_percentage"].as_f64()
                .map(|p| p / 100.0);
            let seven_day_resets_at = v["rate_limits"]["seven_day"]["resets_at"].as_i64();

            // Claude Code update info (several field layouts observed in the wild)
            let update_available = v["update_available"].as_bool()
                .or_else(|| v["claude"]["update_available"].as_bool())
                .or_else(|| v["update"]["available"].as_bool())
                .unwrap_or(false);
            let latest_version = v["latest_version"].as_str()
                .or_else(|| v["claude"]["latest_version"].as_str())
                .or_else(|| v["update"]["version"].as_str())
                .map(|s| s.to_string());

            return StatusInput {
                raw: v.clone(),
                model,
                model_date,
                cwd: v["cwd"].as_str()
                    .or_else(|| v["workspace"]["current_dir"].as_str())
                    .map(|s| s.to_string()),
                version: v["version"].as_str().map(|s| s.to_string()),
                update_available,
                latest_version,
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
const MEM_INACT:Rgb = (0xD7, 0x5F, 0x5F); // 167 — memory inactive
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
        if path.starts_with(h.as_ref()) {
            return format!("~{}", &path[h.len()..]);
        }
    }
    path.to_string()
}

fn fmt_ctx_window(n: u64) -> String {
    if n >= 1_000_000 { format!("{}M", n / 1_000_000) }
    else if n >= 1_000 { format!("{}k", n / 1_000) }
    else { format!("{}", n) }
}

/// Strip ANSI escape sequences (ESC [ ... m) from a string
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for c2 in chars.by_ref() {
                if c2 == 'm' { break; }
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

/// Returns true if the model date string (YYYYMMDD) is older than 90 days
fn model_is_outdated(date_str: &str) -> bool {
    chrono::NaiveDate::parse_from_str(date_str, "%Y%m%d")
        .ok()
        .map(|d| (chrono::Utc::now().date_naive() - d).num_days() > 90)
        .unwrap_or(false)
}

fn fmt_remaining(resets_at: i64) -> String {
    let secs = (resets_at - chrono::Utc::now().timestamp()).max(0) as u64;
    if secs >= 86400 { format!("{}d{}h", secs / 86400, (secs % 86400) / 3600) }
    else if secs >= 3600 { format!("{}h{}m", secs / 3600, (secs % 3600) / 60) }
    else { format!("{}m", secs / 60) }
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

struct UsageSeg {
    ansi: String,
    vis_w: usize,
}

fn usage_seg(label: &str, pct: f64, remain: Option<String>) -> UsageSeg {
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

    let (ansi, vis_w) = seg(&inner, inner_vis);
    UsageSeg { ansi, vis_w }
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
    let cwd_short = shorten_path(&to_home_rel(&cwd_raw), 22);

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // ── Row 0: identity ──────────────────────────────────────────────────────

    // Date suffix in dim DELIM color (e.g. "20251001")
    let (date_str, date_vis) = match &data.model_date {
        Some(d) => (format!(" {}", tc(d, DELIM)), 1 + vis(d)),
        None    => (String::new(), 0),
    };

    let model_inner = match (&data.model, data.context_window_size) {
        (Some(m), Some(cw)) => {
            let m_s = shorten_str(m, 18);
            let cw_s = fmt_ctx_window(cw);
            let inner_vis = vis(&m_s) + date_vis + 3 + vis(&cw_s);
            (format!("{}{} {}{}{}",
                tc_bold(&m_s, MODEL_C),
                date_str,
                tc("(", DELIM),
                tc(&cw_s, CTX_SZ),
                tc(")", DELIM)
            ), inner_vis)
        }
        (Some(m), None) => {
            let m_s = shorten_str(m, 16);
            let inner_vis = vis(&m_s) + date_vis;
            (format!("{}{}", tc_bold(&m_s, MODEL_C), date_str), inner_vis)
        }
        _ => (tc("—", STAT_VAL), 1),
    };

    // Branch gets remaining space after model + cwd segments + gaps
    // seg overhead = 2 per segment (▏▕), gaps between 3 segs = 2+2=4
    let model_seg_vis = model_inner.1 + 2;
    let cwd_seg_vis   = vis(&cwd_short) + 2;
    let branch_budget = panel_w
        .saturating_sub(model_seg_vis + 2 + cwd_seg_vis + 2 + 2) // 2=gap, 2=gap, 2=▏▕
        .max(8);
    let branch_short = shorten_str(&branch, branch_budget);

    let row0_info = format!("{}  {}  {}",
        seg(&model_inner.0, model_inner.1).0,
        seg(&tc(&cwd_short, CWD_C), vis(&cwd_short)).0,
        seg(&tc(&branch_short, BRANCH_C), vis(&branch_short)).0,
    );

    // ── Row 1: usage ─────────────────────────────────────────────────────────

    let row1_info = build_usage_line(data, panel_w);

    // ── Row 2: village divider ────────────────────────────────────────────────

    let vname = village.display_name;
    let fill_vis = panel_w.saturating_sub(4 + vis(vname) + 1);
    let row2_info = format!("{}{}{}{}",
        tc("── ", DELIM),
        tc_bold(vname, vrgb),
        tc(" ", DELIM),
        tcs("─".repeat(fill_vis), DELIM),
    );

    // ── Row 3: pet HP / XP ───────────────────────────────────────────────────

    let hp_pct = (pet.hp as f64 / 100.0).clamp(0.0, 1.0);
    let hp_filled = (hp_pct * 6.0) as usize;
    let hp_bar = format!("{}{}",
        tcs("█".repeat(hp_filled), hp_rgb(pet.hp)),
        tcs("░".repeat(6 - hp_filled), BAR_EMPTY)
    );
    let xp_filled = ((pet.xp as f64 / pet.xp_to_next as f64).clamp(0.0, 1.0) * 6.0) as usize;
    let xp_bar = format!("{}{}",
        tcs("█".repeat(xp_filled), vrgb),
        tcs("░".repeat(6 - xp_filled), BAR_EMPTY)
    );
    let row3_info = format!("{} {}  {} {} {}  {} {} {}/{}",
        tc_bold(&pet.name, PET_NAME),
        tc(&format!("Lv.{}", pet.level), PET_LV),
        tc("HP", STAT_LBL), hp_bar, tc(&pet.hp.to_string(), hp_rgb(pet.hp)),
        tc("XP", STAT_LBL), xp_bar,
        tc(&pet.xp.to_string(), vrgb),
        tc(&pet.xp_to_next.to_string(), STAT_VAL),
    );

    // ── Row 4: stats ─────────────────────────────────────────────────────────

    let row4_info = format!("{} {}  {} {}  {} {}  {} {}",
        tc("ATK:", STAT_LBL), tc(&format!("{:2}", pet.atk), STAT_VAL),
        tc("DEF:", STAT_LBL), tc(&format!("{:2}", pet.def), STAT_VAL),
        tc("SUP:", STAT_LBL), tc(&format!("{:2}", pet.sup), STAT_VAL),
        tc("VER:", STAT_LBL), tc(&format!("{:2}", pet.ver), STAT_VAL),
    );

    // ── Row 5: footer ─────────────────────────────────────────────────────────

    // Claude Code version from JSON (e.g. "1.9.2"), fallback to "—"
    let cc_ver = data.version.as_deref()
        .map(|v| format!("v{}", v))
        .unwrap_or_else(|| "—".to_string());

    // Footer right side: show version; if update available add ⬆ v{latest} in amber
    let ver_right = if data.update_available {
        let latest = data.latest_version.as_deref().unwrap_or("new");
        let upd = format!("v{}", latest);
        format!("{} {} {}",
            tc(&cc_ver, DELIM),
            tc_bold("⬆", UPDATE_C),
            tc_bold(&upd, UPDATE_C),
        )
    } else {
        tc(&cc_ver, DELIM)
    };
    let ver_right_vis = if data.update_available {
        let latest = data.latest_version.as_deref().unwrap_or("new");
        vis(&cc_ver) + 3 + vis(latest) + 1  // "vcur ⬆ vnew"
    } else {
        vis(&cc_ver)
    };

    let mem_vis = vis("Memory:") + 1 + vis("active");
    let pad = panel_w.saturating_sub(mem_vis + ver_right_vis + 2);
    let row5_info = format!("{} {}{}{}",
        tc("Memory:", STAT_LBL),
        tc_bold("active", MEM_ACT),
        " ".repeat(pad.max(1)),
        ver_right,
    );

    // ── 組裝 rows，每行 art pad 到 ART_W 寬 ──────────────────────────────────

    let art = |i: usize| -> Option<String> {
        art_lines.get(i).map(|s| {
            let w = vis(s);
            let pad = if w < ART_W { " ".repeat(ART_W - w) } else { String::new() };
            format!("{}{}", tcs(s.to_string(), vrgb), pad)
        })
    };

    let rows = vec![
        Row { art: art(0), info: row0_info },
        Row { art: art(1), info: row1_info },
        Row { art: art(2), info: row2_info },
        Row { art: art(3), info: row3_info },
        Row { art: art(4), info: row4_info },
        Row { art: art(5), info: row5_info },
    ];

    render_rows(out, &rows, panel_w)?;

    // ── 更新橫幅（主內容下方，全寬） ─────────────────────────────────────────

    if data.update_available {
        let latest = data.latest_version.as_deref().unwrap_or("new version");
        let ver_str = if latest.starts_with('v') { latest.to_string() } else { format!("v{}", latest) };
        let label = format!(" ⬆  Claude Code {} available ", ver_str);
        let hint  = " restart to update ";
        let fill  = width.saturating_sub(vis(&label) + vis(&hint));
        writeln!(out, "{}{}{}",
            tc_bold(&label, UPDATE_C),
            tcs("─".repeat(fill), UPDATE_C),
            tc_bold(&hint, UPDATE_C),
        )?;
    }

    Ok(())
}

// ─── Usage line builder ────────────────────────────────────────────────────────

fn build_usage_line(data: &StatusInput, _panel_w: usize) -> String {
    let mut segs: Vec<String> = Vec::new();

    if let Some(pct) = data.five_hour_pct {
        let remain = data.five_hour_resets_at.map(fmt_remaining);
        let s = usage_seg("5h", pct, remain);
        segs.push(s.ansi);
    }
    if let Some(pct) = data.seven_day_pct {
        let remain = data.seven_day_resets_at.map(fmt_remaining);
        let s = usage_seg("7d", pct, remain);
        segs.push(s.ansi);
    }
    if let Some(pct) = data.context_pct {
        let s = usage_seg("ctx", pct, None);
        segs.push(s.ansi);
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

