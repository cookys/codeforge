//! `codeforge world [--refresh]` — Phase 3a CLI surface for the World Map.
//!
//! Reads the 5-village layout from `game_world` via `crate::world::load_all`
//! and paints a 2×3 ASCII grid (5 real villages + a `???` placeholder cell
//! reserved for Phase 5 Nation P2P). With `--refresh`, scans the L1 store
//! first via `refresh_from_l1` so the numbers reflect the current memory
//! distribution before rendering.
//!
//! Rendering is CJK-safe: every cell is padded to a fixed *visible* width
//! via `unicode-width` so double-wide characters (村落名稱 / emoji) land on
//! a clean box boundary regardless of terminal font.

use anyhow::Result;
use crossterm::style::{Color, Stylize};
use std::io::Write;
use unicode_width::UnicodeWidthStr;

use crate::db;
use crate::pet::live_state::LiveState;
use crate::pet::village::VILLAGES;
use crate::world::{self, ZoneRank, ZoneStats};

/// Fixed inner width of each card (columns between the `│` borders,
/// excluding the one-space gutter on each side). 22 fits the longest
/// display name ("Dockside Workshop" = 17) with room for decoration.
const CARD_INNER: usize = 22;

/// Number of cards per row in the grid.
const COLS: usize = 3;

/// Default home village when no pet has been adopted yet. Matches
/// `pet/state.rs`'s seed and the Phase 1 default.
const DEFAULT_HOME: &str = "rust";

pub fn run(ctx: &db::Context, refresh: bool) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    let home_village: String = LiveState::load(&conn)?
        .map(|l| l.state.village)
        .unwrap_or_else(|| DEFAULT_HOME.to_string());

    if refresh {
        let store_dir = ctx.project_dir.join("store");
        world::refresh_from_l1(&conn, &store_dir, &home_village)?;
    }

    let stats = world::load_all(&conn)?;
    let mut out = std::io::stdout();
    render_header(&mut out, &home_village)?;
    render_grid(&mut out, &stats, &home_village)?;
    render_footer(&mut out, &stats, &home_village)?;
    Ok(())
}

fn render_header(out: &mut impl Write, home_village: &str) -> Result<()> {
    writeln!(out)?;
    writeln!(out, "  {}", "🗺  CodeForge World Map".bold())?;

    let home = VILLAGES
        .iter()
        .find(|v| v.id == home_village)
        .unwrap_or(&VILLAGES[2]);
    writeln!(
        out,
        "  {}",
        format!("本命村：{} ({})", home.display_name, home.language).with(home.color)
    )?;
    writeln!(out)?;
    Ok(())
}

fn render_grid(out: &mut impl Write, stats: &[ZoneStats], home_village: &str) -> Result<()> {
    let mut cells: Vec<Cell> = stats
        .iter()
        .map(|s| Cell::from_stats(s, s.village_id == home_village))
        .collect();
    cells.push(Cell::nation_placeholder());

    for chunk in cells.chunks(COLS) {
        render_row(out, chunk)?;
        writeln!(out)?;
    }
    Ok(())
}

fn render_row(out: &mut impl Write, cells: &[Cell]) -> Result<()> {
    // Five text rows (name, language, status, stats) plus top/bottom border.
    let top: String = cells
        .iter()
        .map(|_| format!("┌{}┐", "─".repeat(CARD_INNER)))
        .collect::<Vec<_>>()
        .join(" ");
    let bot: String = cells
        .iter()
        .map(|_| format!("└{}┘", "─".repeat(CARD_INNER)))
        .collect::<Vec<_>>()
        .join(" ");

    write!(out, "  ")?;
    writeln!(out, "{}", top)?;

    for row_idx in 0..4 {
        write!(out, "  ")?;
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                write!(out, " ")?;
            }
            write!(out, "│")?;
            let text = cell.line(row_idx);
            paint_cell_line(out, cell, &text)?;
            write!(out, "│")?;
        }
        writeln!(out)?;
    }

    write!(out, "  ")?;
    writeln!(out, "{}", bot)?;
    Ok(())
}

fn paint_cell_line(out: &mut impl Write, cell: &Cell, text: &str) -> Result<()> {
    let padded = pad_visible(text, CARD_INNER);
    let styled = match cell.tone {
        CellTone::HomeUnlocked(color) => padded.with(color).bold(),
        CellTone::Unlocked(color) => padded.with(color),
        CellTone::Locked => padded.with(Color::AnsiValue(240)),
    };
    write!(out, "{}", styled)?;
    Ok(())
}

/// Count of visibly-open villages. Mirrors the home-village override
/// from `Cell::from_stats` so the footer count can't contradict the
/// grid (a fresh install shows the home village as `★ 本命 · 開放`
/// before `--refresh` flips the DB flag; this counts it as unlocked).
fn unlocked_count(stats: &[ZoneStats], home_village: &str) -> usize {
    stats
        .iter()
        .filter(|s| s.unlocked || s.village_id == home_village)
        .count()
}

fn render_footer(out: &mut impl Write, stats: &[ZoneStats], home_village: &str) -> Result<()> {
    let unlocked = unlocked_count(stats, home_village);
    let total = stats.len();
    writeln!(
        out,
        "  {}",
        format!(
            "已解鎖 {}/{} 村落 · 門檻：{} concepts · `codeforge world --refresh` 重新掃描 L1",
            unlocked,
            total,
            world::UNLOCK_THRESHOLD,
        )
        .with(Color::AnsiValue(244))
    )?;
    writeln!(out)?;
    Ok(())
}

/// Visual state of one card — drives colour selection in `paint_cell_line`.
#[derive(Debug, Clone, Copy)]
enum CellTone {
    HomeUnlocked(Color),
    Unlocked(Color),
    Locked,
}

/// One grid cell. Stores pre-formatted lines so paint + layout stay pure.
#[derive(Debug, Clone)]
struct Cell {
    name: String,
    language: String,
    status: String,
    stats: String,
    tone: CellTone,
}

impl Cell {
    fn from_stats(s: &ZoneStats, is_home: bool) -> Self {
        let village = VILLAGES.iter().find(|v| v.id == s.village_id);
        let name = village
            .map(|v| v.display_name.to_string())
            .unwrap_or_else(|| s.village_id.clone());
        let language = village.map(|v| v.language.to_string()).unwrap_or_default();

        // Home village always displays as unlocked even if the daemon /
        // refresh hasn't yet flipped the DB flag (e.g. brand-new install,
        // `codeforge world` invoked before any `--refresh`). Keeps rank
        // and status in sync with the "home is always yours" semantic.
        let display_unlocked = s.unlocked || is_home;

        let status = match (display_unlocked, is_home) {
            (_, true) => " ★ 本命 · 開放".to_string(),
            (true, false) => " ─ 開放".to_string(),
            (false, _) => " 🔒 未解鎖".to_string(),
        };

        let stats = if display_unlocked {
            format!(
                " {} · ⚔ {} · 📖 {}",
                rank_label(s.rank),
                s.kill_count,
                s.concept_count
            )
        } else {
            format!(" — · ⚔ {} · 📖 {}", s.kill_count, s.concept_count)
        };

        let color = village.map(|v| v.color).unwrap_or(Color::White);
        let tone = if is_home {
            CellTone::HomeUnlocked(color)
        } else if display_unlocked {
            CellTone::Unlocked(color)
        } else {
            CellTone::Locked
        };

        Self {
            name: format!(" {}", name),
            language: format!(" {}", language),
            status,
            stats,
            tone,
        }
    }

    fn nation_placeholder() -> Self {
        Self {
            name: " ???".to_string(),
            language: " Nation P2P".to_string(),
            status: " 🔒 未探索".to_string(),
            stats: " — · ⚔ — · 📖 —".to_string(),
            tone: CellTone::Locked,
        }
    }

    /// Which line of the card to render. Index in 0..4.
    fn line(&self, idx: usize) -> String {
        match idx {
            0 => self.name.clone(),
            1 => self.language.clone(),
            2 => self.status.clone(),
            3 => self.stats.clone(),
            _ => String::new(),
        }
    }
}

fn rank_label(rank: ZoneRank) -> String {
    let glyph = rank.glyph();
    if glyph.is_empty() {
        rank.as_str().to_string()
    } else {
        format!("{} {}", glyph, rank.as_str())
    }
}

/// Pad `s` with spaces on the right so its visible width (unicode-width)
/// is exactly `cols`. Clips with `…` if `s` already overflows. This is the
/// CJK-safe analog of `format!("{:<cols}", s)` — `format!`'s width counts
/// `char`s, which misaligns double-wide characters in the grid.
fn pad_visible(s: &str, cols: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w == cols {
        return s.to_string();
    }
    if w > cols {
        return clip_visible(s, cols);
    }
    format!("{}{}", s, " ".repeat(cols - w))
}

/// Truncate `s` so its visible width is at most `max_cols`, adding `…`.
/// Mirrors `src/tui/panels/mod.rs::clip_to_width` with the same logic —
/// duplicated to keep this CLI module independent of the TUI renderer.
fn clip_visible(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let mut cols = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]));
        if cols + w > max_cols.saturating_sub(1) {
            break;
        }
        cols += w;
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    #[test]
    fn pad_visible_ascii_pads_to_width() {
        assert_eq!(pad_visible("abc", 6), "abc   ");
        assert_eq!(pad_visible("abc", 3), "abc");
    }

    #[test]
    fn pad_visible_cjk_counts_display_columns() {
        // Each CJK char is 2 columns wide; "本命村" = 6 cols.
        let s = "本命村";
        assert_eq!(UnicodeWidthStr::width(s), 6);
        let padded = pad_visible(s, 10);
        assert_eq!(UnicodeWidthStr::width(padded.as_str()), 10);
    }

    #[test]
    fn clip_visible_adds_ellipsis_when_overflowing() {
        let out = clip_visible("abcdefg", 4);
        assert!(out.ends_with('…'));
        assert!(UnicodeWidthStr::width(out.as_str()) <= 4);
    }

    #[test]
    fn rank_label_omits_glyph_for_traveler() {
        assert_eq!(rank_label(ZoneRank::Traveler), "Traveler");
        assert!(rank_label(ZoneRank::Forger).contains("Forger"));
        assert!(rank_label(ZoneRank::IronCrafter).contains("✦"));
        assert!(rank_label(ZoneRank::Veteran).contains("✦✦"));
    }

    #[test]
    fn cell_from_stats_home_marks_tone() {
        let s = ZoneStats {
            village_id: "rust".to_string(),
            unlocked: true,
            kill_count: 0,
            concept_count: 0,
            rank: ZoneRank::Traveler,
        };
        let cell = Cell::from_stats(&s, true);
        matches!(cell.tone, CellTone::HomeUnlocked(_));
        assert!(cell.status.contains("本命"));
    }

    #[test]
    fn cell_from_stats_locked_uses_locked_tone() {
        let s = ZoneStats {
            village_id: "python".to_string(),
            unlocked: false,
            kill_count: 0,
            concept_count: 0,
            rank: ZoneRank::Traveler,
        };
        let cell = Cell::from_stats(&s, false);
        assert!(matches!(cell.tone, CellTone::Locked));
        assert!(cell.status.contains("未解鎖"));
    }

    #[test]
    fn cell_nation_placeholder_is_locked() {
        let cell = Cell::nation_placeholder();
        assert!(matches!(cell.tone, CellTone::Locked));
        assert!(cell.name.contains("???"));
        assert!(cell.language.contains("Nation"));
    }

    #[test]
    fn unlocked_count_respects_home_override_on_fresh_db() {
        // Regression: render_footer previously used raw `s.unlocked`, so a
        // fresh DB (home flag still 0) rendered `★ 本命 · 開放` in the grid
        // next to a contradictory `已解鎖 0/5 村落` footer.
        let stats = vec![
            ZoneStats {
                village_id: "python".to_string(),
                unlocked: false,
                kill_count: 0,
                concept_count: 0,
                rank: ZoneRank::Traveler,
            },
            ZoneStats {
                village_id: "rust".to_string(),
                unlocked: false, // daemon/refresh hasn't flipped this yet
                kill_count: 0,
                concept_count: 0,
                rank: ZoneRank::Traveler,
            },
        ];
        assert_eq!(unlocked_count(&stats, "rust"), 1);
        assert_eq!(unlocked_count(&stats, "python"), 1);
        assert_eq!(unlocked_count(&stats, "nonexistent"), 0);
    }

    #[test]
    fn unlocked_count_sums_both_flagged_and_home() {
        let stats = vec![
            ZoneStats {
                village_id: "python".to_string(),
                unlocked: true,
                kill_count: 0,
                concept_count: 0,
                rank: ZoneRank::Traveler,
            },
            ZoneStats {
                village_id: "rust".to_string(),
                unlocked: false,
                kill_count: 0,
                concept_count: 0,
                rank: ZoneRank::Traveler,
            },
            ZoneStats {
                village_id: "go".to_string(),
                unlocked: true,
                kill_count: 0,
                concept_count: 0,
                rank: ZoneRank::Traveler,
            },
        ];
        // python + go flagged + rust home override = 3
        assert_eq!(unlocked_count(&stats, "rust"), 3);
    }

    #[test]
    fn run_refresh_without_pet_uses_default_home() {
        // Fresh DB (no LiveState) + --refresh must not crash and must
        // flip the default home village ('rust') to unlocked.
        let temp = tempfile::TempDir::new().unwrap();
        let project_dir = temp.path().join(".codeforge");
        std::fs::create_dir_all(project_dir.join("store").join("concepts")).unwrap();
        let db_path = temp.path().join("state.db");
        let ctx = db::Context {
            project_dir: project_dir.clone(),
            brain_dir: temp.path().join("brain"),
            db_path,
        };
        // open_db + migrate once so the Connection we open below shares schema.
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        drop(conn);

        // refresh_from_l1 directly — don't invoke `run` because it writes
        // to stdout; the grid render is covered by render unit tests and
        // CLI smoke via manual test.
        let conn = ctx.open_db().unwrap();
        world::refresh_from_l1(&conn, &project_dir.join("store"), DEFAULT_HOME).unwrap();
        let stats = world::load_all(&conn).unwrap();
        let rust = stats.iter().find(|s| s.village_id == "rust").unwrap();
        assert!(rust.unlocked, "default home must unlock after refresh");
    }
}
