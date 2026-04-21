//! Tile primitive for the tile-grid Local Map — Phase 2c UX polish.
//!
//! Produces a compact box-drawn tile for one `RoomSummary`:
//! ```text
//! ┌────────┐
//! │ daemon │   ← dir name (CJK-safe, clipped to inner width)
//! │🧟3   @ │   ← mob badge + optional `@` for current room
//! ```
//! Default size `TILE_WIDTH × TILE_HEIGHT` = 10 × 3. Lines are bare
//! strings (no ANSI) per the `panels` convention — colorization happens
//! at the paint layer; use [`zone_color`] to derive the border colour
//! from the directory name.
//!
//! CJK-safe via `clip_to_width` / `pad_to_width` / `vis_width`. A 10-col
//! tile has 8 inner cols, fitting 4 CJK chars or 8 ASCII chars.

use super::super::local_map::RoomSummary;
use super::{clip_to_width, pad_to_width, vis_width};
use termcolor::Color;

/// Default tile width. Grid math uses `cols = floor(panel_w / TILE_WIDTH)`.
pub const TILE_WIDTH: usize = 10;

/// Default tile height. Grid math uses `rows = floor(panel_h / TILE_HEIGHT)`.
pub const TILE_HEIGHT: usize = 3;

/// Minimum panel width for grid mode. Below this the caller must fall
/// back to list render — a single tile plus 1-col gutter cannot fit.
pub const MIN_GRID_WIDTH: usize = 30;

/// Render a single tile as `height` lines, each exactly `width` visible
/// columns wide.
///
/// For the default 10 × 3 tile:
/// * Row 0: `┌────────┐` top border
/// * Row 1: `│ name.. │` directory name (clipped CJK-safe)
/// * Row 2: `│🧟3   @ │` badge + optional `@` overlay if `room.is_current`
///
/// Width must be ≥ 4 (2 border cols + 2 content cols); height must be
/// ≥ 3. Degenerate dimensions return an empty `Vec` rather than panic —
/// the caller is expected to pre-check via `MIN_GRID_WIDTH` or fall back
/// to list render.
pub fn render_tile(room: &RoomSummary, width: usize, height: usize) -> Vec<String> {
    if width < 4 || height < 3 {
        return Vec::new();
    }
    let inner = width - 2;

    let top = format!("┌{}┐", "─".repeat(inner));
    let name = format!("│{}│", pad_to_width(&room.directory, inner));
    let badge = badge_for(room);
    let badge_row = compose_badge_row(&badge, room.is_current, inner);

    let mut lines = Vec::with_capacity(height);
    lines.push(pad_to_width(&top, width));
    lines.push(pad_to_width(&name, width));
    lines.push(pad_to_width(&badge_row, width));
    // Extra rows beyond 3 pad blank — keeps grid alignment if caller
    // passes a taller tile than the default.
    while lines.len() < height {
        lines.push(pad_to_width("", width));
    }
    lines
}

/// Assemble the badge row: `│<badge>  <marker>│` where marker is `@`
/// when current, space otherwise. Marker reserves 1 col; gap absorbs
/// whatever width the badge didn't use. CJK-safe.
fn compose_badge_row(badge: &str, is_current: bool, inner: usize) -> String {
    let marker = if is_current { "@" } else { " " };
    // Reserve 1 col for the marker; rest goes to the badge.
    let badge_budget = inner.saturating_sub(2); // 1 col marker + ≥ 1 col gap
    let badge_clipped = clip_to_width(badge, badge_budget);
    let badge_w = vis_width(&badge_clipped);
    let marker_w = vis_width(marker);
    let gap = inner.saturating_sub(badge_w + marker_w);
    let body = format!("{badge_clipped}{}{marker}", " ".repeat(gap));
    // Defensive: if the arithmetic above slightly mispredicted (shouldn't
    // happen, but clip_to_width may not exactly hit the budget when adding
    // `…`), pad_to_width normalizes to inner cols.
    let padded = pad_to_width(&body, inner);
    format!("│{padded}│")
}

/// Right-side badge content (no surrounding `[]` — borders are the box).
/// `🧟N` for alive, `✓` when all defeated, `—` when nothing recorded.
fn badge_for(room: &RoomSummary) -> String {
    if room.alive > 0 {
        format!("🧟{}", room.alive)
    } else if room.defeated > 0 {
        "✓".to_string()
    } else {
        "—".to_string()
    }
}

/// Directory-name heuristic → border colour. Kept simple (no DB join)
/// per plan §Risks: avoids a per-paint round-trip against the world
/// table. Zone-kind mapping:
///
/// | Dir                         | Color       |
/// |-----------------------------|-------------|
/// | `src`, `rust`               | Red         |
/// | `doc`, `docs`, `memory`, `.claude` | Magenta     |
/// | `daemon`                    | White       |
/// | `tui`, `ui`                 | Cyan        |
/// | `db`, `data`                | Yellow      |
/// | else (`(unknown)`, `target`, …) | Ansi256(8) (dark grey) |
///
/// Dark-grey uses `Ansi256(8)` (ANSI "bright black") since `termcolor`
/// has no `DarkGrey` variant; every ANSI-capable terminal renders it.
pub fn zone_color(directory: &str) -> Color {
    match directory {
        "src" | "rust" => Color::Red,
        "doc" | "docs" | "memory" | ".claude" => Color::Magenta,
        "daemon" => Color::White,
        "tui" | "ui" => Color::Cyan,
        "db" | "data" => Color::Yellow,
        _ => Color::Ansi256(8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(dir: &str, alive: u32, defeated: u32, current: bool) -> RoomSummary {
        RoomSummary {
            directory: dir.to_string(),
            alive,
            defeated,
            is_current: current,
        }
    }

    // ------------------------------------------------------------------
    // Shape / dimensions
    // ------------------------------------------------------------------

    #[test]
    fn default_tile_is_three_rows() {
        let lines = render_tile(&room("src", 0, 0, false), TILE_WIDTH, TILE_HEIGHT);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn every_line_hits_requested_width() {
        let lines = render_tile(&room("daemon", 2, 0, true), TILE_WIDTH, TILE_HEIGHT);
        for (i, l) in lines.iter().enumerate() {
            assert_eq!(vis_width(l), TILE_WIDTH, "row {i} width mismatch: {l:?}");
        }
    }

    #[test]
    fn invalid_width_returns_empty() {
        assert!(render_tile(&room("src", 0, 0, false), 3, 3).is_empty());
    }

    #[test]
    fn invalid_height_returns_empty() {
        assert!(render_tile(&room("src", 0, 0, false), 10, 2).is_empty());
    }

    #[test]
    fn taller_height_pads_with_blank_rows() {
        let lines = render_tile(&room("src", 0, 0, false), 10, 5);
        assert_eq!(lines.len(), 5);
        // Last two rows are blank padded (no border chars).
        assert_eq!(vis_width(&lines[3]), 10);
        assert_eq!(vis_width(&lines[4]), 10);
        assert!(!lines[3].contains('│'));
        assert!(!lines[4].contains('│'));
    }

    // ------------------------------------------------------------------
    // Border / name / badge content
    // ------------------------------------------------------------------

    #[test]
    fn top_border_uses_box_drawing() {
        let lines = render_tile(&room("src", 0, 0, false), 10, 3);
        assert!(lines[0].starts_with('┌'));
        assert!(lines[0].trim_end().ends_with('┐'));
        assert!(lines[0].contains('─'));
    }

    #[test]
    fn name_row_contains_directory() {
        let lines = render_tile(&room("daemon", 0, 0, false), 10, 3);
        assert!(lines[1].contains("daemon"));
        assert!(lines[1].starts_with('│'));
    }

    #[test]
    fn badge_row_shows_alive_zombies() {
        let lines = render_tile(&room("src", 7, 0, false), 10, 3);
        assert!(lines[2].contains("🧟7"));
    }

    #[test]
    fn badge_row_shows_check_when_all_defeated() {
        let lines = render_tile(&room("target", 0, 3, false), 10, 3);
        assert!(lines[2].contains('✓'));
    }

    #[test]
    fn badge_row_shows_dash_when_empty() {
        let lines = render_tile(&room("noise", 0, 0, false), 10, 3);
        assert!(lines[2].contains('—'));
    }

    // ------------------------------------------------------------------
    // @ overlay (is_current)
    // ------------------------------------------------------------------

    #[test]
    fn current_room_shows_at_marker() {
        let lines = render_tile(&room("src", 2, 0, true), 10, 3);
        assert!(lines[2].contains('@'));
    }

    #[test]
    fn non_current_room_omits_at_marker() {
        let lines = render_tile(&room("src", 2, 0, false), 10, 3);
        assert!(!lines[2].contains('@'));
    }

    // ------------------------------------------------------------------
    // CJK safety
    // ------------------------------------------------------------------

    #[test]
    fn cjk_name_fits_within_inner_width() {
        // 「後端」= 2 CJK chars = 4 visible cols; inner = 8 → fits with padding.
        let lines = render_tile(&room("後端", 0, 0, false), 10, 3);
        assert_eq!(vis_width(&lines[1]), 10);
        assert!(lines[1].contains("後端"));
    }

    #[test]
    fn long_cjk_name_clipped_with_ellipsis() {
        // 「代號七七七」= 5 CJK = 10 cols, inner=8 → must clip + ….
        let lines = render_tile(&room("代號七七七", 0, 0, false), 10, 3);
        assert_eq!(vis_width(&lines[1]), 10);
        assert!(lines[1].contains('…'));
    }

    #[test]
    fn cjk_name_with_current_marker_maintains_width() {
        let lines = render_tile(&room("前端目錄", 3, 0, true), 10, 3);
        for l in &lines {
            assert_eq!(vis_width(l), 10);
        }
        assert!(lines[2].contains('@'));
    }

    // ------------------------------------------------------------------
    // zone_color heuristic
    // ------------------------------------------------------------------

    #[test]
    fn zone_color_src_maps_red() {
        assert_eq!(zone_color("src"), Color::Red);
    }

    #[test]
    fn zone_color_doc_maps_magenta() {
        assert_eq!(zone_color("doc"), Color::Magenta);
        assert_eq!(zone_color("docs"), Color::Magenta);
        assert_eq!(zone_color("memory"), Color::Magenta);
    }

    #[test]
    fn zone_color_daemon_maps_white() {
        assert_eq!(zone_color("daemon"), Color::White);
    }

    #[test]
    fn zone_color_tui_maps_cyan() {
        assert_eq!(zone_color("tui"), Color::Cyan);
    }

    #[test]
    fn zone_color_db_maps_yellow() {
        assert_eq!(zone_color("db"), Color::Yellow);
    }

    #[test]
    fn zone_color_unknown_maps_dark_grey() {
        assert_eq!(zone_color("(unknown)"), Color::Ansi256(8));
        assert_eq!(zone_color("target"), Color::Ansi256(8));
        assert_eq!(zone_color(".git"), Color::Ansi256(8));
    }
}
