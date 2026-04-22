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
use super::super::styled::StyledLine;
use super::{clip_to_width, pad_to_width, vis_width};
use termcolor::Color;
use unicode_width::UnicodeWidthStr;

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
pub fn render_tile(room: &RoomSummary, width: usize, height: usize) -> Vec<StyledLine> {
    if width < 4 || height < 3 {
        return Vec::new();
    }
    let inner = width - 2;

    let top = format!("┌{}┐", "─".repeat(inner));
    let name = format!("│{}│", pad_to_width(&room.directory, inner));
    let badge = badge_for(room);
    let badge_row = compose_badge_row(&badge, room.is_current, inner);

    let mut lines: Vec<StyledLine> = Vec::with_capacity(height);
    // B11 P2: all spans default-fg; P3 replaces the border spans with
    // Some(zone_color(&room.directory)) so borders pick up zone colour
    // while the interior (name / badge / @ marker) stays default fg.
    lines.push(StyledLine::plain(pad_to_width(&top, width)));
    lines.push(StyledLine::plain(pad_to_width(&name, width)));
    lines.push(StyledLine::plain(pad_to_width(&badge_row, width)));
    // Extra rows beyond 3 pad blank — keeps grid alignment if caller
    // passes a taller tile than the default.
    while lines.len() < height {
        lines.push(StyledLine::plain(pad_to_width("", width)));
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

// ---------------------------------------------------------------------------
// Grid flow (P2)
// ---------------------------------------------------------------------------

/// Coordinate of a tile within the grid. `col` grows right, `row` grows down,
/// both zero-indexed. Used by callers that need to know where a tile was
/// placed (e.g. future hit-testing for mouse events).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileCoord {
    pub col: usize,
    pub row: usize,
}

/// Grid dimensions derived from panel size. `capacity = cols * rows` —
/// rooms past this index are dropped and surface as the overflow count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLayout {
    pub cols: usize,
    pub rows: usize,
    pub capacity: usize,
}

/// Pack `rooms` into a grid sized for a `panel_w × panel_h` viewport.
///
/// Returns the layout, a row-major placement list of (coord, room), and
/// the overflow count (`rooms.len() - capacity`, saturating). Rooms
/// beyond capacity are silently dropped — caller renders `…+N more`
/// using the returned overflow.
///
/// Degenerate panels (< TILE_WIDTH cols or < TILE_HEIGHT rows) yield
/// `capacity = 0`; all rooms overflow and the caller should fall back
/// to list render.
pub fn compute_grid(
    rooms: &[RoomSummary],
    panel_w: usize,
    panel_h: usize,
) -> (GridLayout, Vec<(TileCoord, &RoomSummary)>, usize) {
    let cols = panel_w / TILE_WIDTH;
    let rows = panel_h / TILE_HEIGHT;
    let capacity = cols * rows;
    let layout = GridLayout {
        cols,
        rows,
        capacity,
    };

    let placed: Vec<(TileCoord, &RoomSummary)> = rooms
        .iter()
        .take(capacity)
        .enumerate()
        .map(|(i, r)| {
            // Row-major: left-to-right across a row, then wrap down.
            let coord = TileCoord {
                col: i % cols.max(1),
                row: i / cols.max(1),
            };
            (coord, r)
        })
        .collect();

    let overflow = rooms.len().saturating_sub(capacity);
    (layout, placed, overflow)
}

/// Render `rooms` as a tile grid fitting `panel_w × panel_h`. Output is
/// exactly `panel_h` lines each `panel_w` visible cols wide.
///
/// Overflow (rooms past `capacity`) surfaces as `…+N more` overlaid on
/// the bottom-right of the last row. This intentionally covers whatever
/// tile content was there — the signal matters more than the last tile.
///
/// For `capacity == 0` (panel too narrow) the frame is blank; callers
/// should have routed to list render before reaching this path, but the
/// function stays well-defined rather than panicking.
pub fn render_grid(rooms: &[RoomSummary], panel_w: usize, panel_h: usize) -> Vec<StyledLine> {
    let (layout, placed, overflow) = compute_grid(rooms, panel_w, panel_h);

    // We build a scratch `Vec<String>` first (one row per panel line),
    // then materialise StyledLine::plain at the end. This keeps the
    // horizontal concatenation and overflow-overlay arithmetic
    // unchanged from P2 pre-B11 — P3 will reshape render_tile into a
    // per-span styled form and then this function will compose at
    // the span level instead of via string concatenation.
    let mut rows: Vec<String> = Vec::with_capacity(panel_h);

    if layout.capacity == 0 {
        while rows.len() < panel_h {
            rows.push(pad_to_width("", panel_w));
        }
        let rows = finalize_with_overflow(rows, overflow, panel_w);
        return rows.into_iter().map(StyledLine::plain).collect();
    }

    // Group placed tiles by row for easy horizontal concatenation.
    let mut rows_of_tiles: Vec<Vec<&RoomSummary>> = vec![Vec::new(); layout.rows];
    for (coord, room) in placed {
        rows_of_tiles[coord.row].push(room);
    }

    for row_tiles in rows_of_tiles {
        // Pre-render each tile in this row, then flatten StyledLine back
        // to plain strings for horizontal concatenation. Lossy only in
        // that we drop fg info — P2 emits default-fg everywhere so this
        // is a no-op; P3 will rewrite this loop against spans directly.
        let rendered: Vec<Vec<String>> = row_tiles
            .iter()
            .map(|r| {
                render_tile(r, TILE_WIDTH, TILE_HEIGHT)
                    .into_iter()
                    .map(|sl| sl.plain_text())
                    .collect()
            })
            .collect();
        for i in 0..TILE_HEIGHT {
            let mut line = String::new();
            for tile in &rendered {
                if let Some(piece) = tile.get(i) {
                    line.push_str(piece);
                }
            }
            rows.push(pad_to_width(&line, panel_w));
        }
    }
    while rows.len() < panel_h {
        rows.push(pad_to_width("", panel_w));
    }

    let rows = finalize_with_overflow(rows, overflow, panel_w);
    rows.into_iter().map(StyledLine::plain).collect()
}

/// Overlay the `…+N more` indicator onto the bottom-right of `frame`
/// when `overflow > 0`. No-op otherwise. Operates on raw Strings —
/// kept as-is from pre-B11 because the overlay math is width-based
/// and doesn't need span awareness.
fn finalize_with_overflow(mut frame: Vec<String>, overflow: usize, panel_w: usize) -> Vec<String> {
    if overflow == 0 || frame.is_empty() {
        return frame;
    }
    let msg = format!("…+{overflow} more");
    let msg_w = vis_width(&msg);
    let last = frame.len() - 1;
    if msg_w >= panel_w {
        frame[last] = pad_to_width(&clip_to_width(&msg, panel_w), panel_w);
    } else {
        let head_w = panel_w - msg_w;
        let head = take_cols(&frame[last], head_w);
        let head_padded = pad_to_width(&head, head_w);
        frame[last] = format!("{head_padded}{msg}");
    }
    frame
}

/// Take at most `max_cols` visible columns from the start of `s`, CJK-safe.
/// No ellipsis — returns the raw prefix. Private helper for overlay math;
/// `clip_to_width` differs by appending `…` on truncation.
fn take_cols(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let mut acc = 0usize;
    let mut out = String::new();
    let mut buf = [0u8; 4];
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut buf));
        if acc + w > max_cols {
            break;
        }
        acc += w;
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------------------
// Zone colour
// ---------------------------------------------------------------------------

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
///
/// NOTE (2026-04-21): mapping is unit-tested; paint-layer integration
/// (per-tile border styling) is tracked as `B11 — Tile-Map Zone Color:
/// Paint-Layer Integration` in `doc/BACKLOG.md`. Requires `paint` to
/// handle `Vec<StyledSpan>` instead of whole-row `Print(&text)`, which
/// is a separate refactor. `#[allow(dead_code)]` holds until B11 lands.
#[allow(dead_code)]
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
            assert_eq!(l.visible_width(), TILE_WIDTH, "row {i} width mismatch: {l:?}");
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
        assert_eq!(lines[3].visible_width(), 10);
        assert_eq!(lines[4].visible_width(), 10);
        assert!(!lines[3].plain_text().contains('│'));
        assert!(!lines[4].plain_text().contains('│'));
    }

    // ------------------------------------------------------------------
    // Border / name / badge content
    // ------------------------------------------------------------------

    #[test]
    fn top_border_uses_box_drawing() {
        let lines = render_tile(&room("src", 0, 0, false), 10, 3);
        assert!(lines[0].plain_text().starts_with('┌'));
        assert!(lines[0].plain_text().trim_end().ends_with('┐'));
        assert!(lines[0].plain_text().contains('─'));
    }

    #[test]
    fn name_row_contains_directory() {
        let lines = render_tile(&room("daemon", 0, 0, false), 10, 3);
        assert!(lines[1].plain_text().contains("daemon"));
        assert!(lines[1].plain_text().starts_with('│'));
    }

    #[test]
    fn badge_row_shows_alive_zombies() {
        let lines = render_tile(&room("src", 7, 0, false), 10, 3);
        assert!(lines[2].plain_text().contains("🧟7"));
    }

    #[test]
    fn badge_row_shows_check_when_all_defeated() {
        let lines = render_tile(&room("target", 0, 3, false), 10, 3);
        assert!(lines[2].plain_text().contains('✓'));
    }

    #[test]
    fn badge_row_shows_dash_when_empty() {
        let lines = render_tile(&room("noise", 0, 0, false), 10, 3);
        assert!(lines[2].plain_text().contains('—'));
    }

    // ------------------------------------------------------------------
    // @ overlay (is_current)
    // ------------------------------------------------------------------

    #[test]
    fn current_room_shows_at_marker() {
        let lines = render_tile(&room("src", 2, 0, true), 10, 3);
        assert!(lines[2].plain_text().contains('@'));
    }

    #[test]
    fn non_current_room_omits_at_marker() {
        let lines = render_tile(&room("src", 2, 0, false), 10, 3);
        assert!(!lines[2].plain_text().contains('@'));
    }

    // ------------------------------------------------------------------
    // CJK safety
    // ------------------------------------------------------------------

    #[test]
    fn cjk_name_fits_within_inner_width() {
        // 「後端」= 2 CJK chars = 4 visible cols; inner = 8 → fits with padding.
        let lines = render_tile(&room("後端", 0, 0, false), 10, 3);
        assert_eq!(lines[1].visible_width(), 10);
        assert!(lines[1].plain_text().contains("後端"));
    }

    #[test]
    fn long_cjk_name_clipped_with_ellipsis() {
        // 「代號七七七」= 5 CJK = 10 cols, inner=8 → must clip + ….
        let lines = render_tile(&room("代號七七七", 0, 0, false), 10, 3);
        assert_eq!(lines[1].visible_width(), 10);
        assert!(lines[1].plain_text().contains('…'));
    }

    #[test]
    fn cjk_name_with_current_marker_maintains_width() {
        let lines = render_tile(&room("前端目錄", 3, 0, true), 10, 3);
        for l in &lines {
            assert_eq!(l.visible_width(), 10);
        }
        assert!(lines[2].plain_text().contains('@'));
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

    // ------------------------------------------------------------------
    // P2 — compute_grid
    // ------------------------------------------------------------------

    #[test]
    fn compute_grid_40x20_yields_4x6() {
        let rooms = vec![];
        let (layout, _, _) = compute_grid(&rooms, 40, 20);
        assert_eq!(layout.cols, 4);
        assert_eq!(layout.rows, 6);
        assert_eq!(layout.capacity, 24);
    }

    #[test]
    fn compute_grid_narrow_panel_has_zero_cols() {
        let rs = [room("src", 0, 0, false)];
        let (layout, placed, overflow) = compute_grid(&rs, 8, 20);
        assert_eq!(layout.cols, 0);
        assert_eq!(layout.capacity, 0);
        assert!(placed.is_empty());
        assert_eq!(overflow, 1);
    }

    #[test]
    fn compute_grid_short_panel_has_zero_rows() {
        let (layout, _, _) = compute_grid(&[], 40, 2);
        assert_eq!(layout.rows, 0);
        assert_eq!(layout.capacity, 0);
    }

    #[test]
    fn compute_grid_places_row_major() {
        let rs: Vec<_> = (0..5).map(|i| room(&format!("d{i}"), 0, 0, false)).collect();
        // 30×3 → cols=3, rows=1, capacity=3. Rooms 0/1/2 placed; 3/4 overflow.
        let (layout, placed, overflow) = compute_grid(&rs, 30, 3);
        assert_eq!(layout.capacity, 3);
        assert_eq!(placed.len(), 3);
        assert_eq!(placed[0].0, TileCoord { col: 0, row: 0 });
        assert_eq!(placed[1].0, TileCoord { col: 1, row: 0 });
        assert_eq!(placed[2].0, TileCoord { col: 2, row: 0 });
        assert_eq!(overflow, 2);
    }

    #[test]
    fn compute_grid_wraps_into_second_row() {
        let rs: Vec<_> = (0..5).map(|i| room(&format!("d{i}"), 0, 0, false)).collect();
        // 30×6 → cols=3, rows=2. Rooms 3/4 wrap to row=1.
        let (_, placed, _) = compute_grid(&rs, 30, 6);
        assert_eq!(placed[3].0, TileCoord { col: 0, row: 1 });
        assert_eq!(placed[4].0, TileCoord { col: 1, row: 1 });
    }

    #[test]
    fn compute_grid_fits_exact_capacity_no_overflow() {
        let rs: Vec<_> = (0..24).map(|i| room(&format!("d{i}"), 0, 0, false)).collect();
        let (_, placed, overflow) = compute_grid(&rs, 40, 20);
        assert_eq!(placed.len(), 24);
        assert_eq!(overflow, 0);
    }

    // ------------------------------------------------------------------
    // P2 — render_grid
    // ------------------------------------------------------------------

    #[test]
    fn render_grid_produces_exact_panel_height() {
        let rooms = vec![room("src", 1, 0, false), room("doc", 0, 0, false)];
        let frame = render_grid(&rooms, 40, 20);
        assert_eq!(frame.len(), 20);
    }

    #[test]
    fn render_grid_every_row_matches_panel_width() {
        let rooms: Vec<_> = (0..8).map(|i| room(&format!("d{i}"), 1, 0, false)).collect();
        let frame = render_grid(&rooms, 40, 12);
        for (i, l) in frame.iter().enumerate() {
            assert_eq!(l.visible_width(), 40, "row {i} width mismatch: {l:?}");
        }
    }

    #[test]
    fn render_grid_first_tile_anchored_top_left() {
        let rooms = vec![room("src", 3, 0, false)];
        let frame = render_grid(&rooms, 40, 12);
        // Top border of tile 0 should begin at column 0 of row 0.
        assert!(frame[0].plain_text().starts_with('┌'));
        assert!(frame[1].plain_text().contains("src"));
        assert!(frame[2].plain_text().contains("🧟3"));
    }

    #[test]
    fn render_grid_cjk_name_preserves_alignment() {
        let rooms = vec![
            room("前端", 1, 0, false),
            room("後端", 0, 2, true),
        ];
        let frame = render_grid(&rooms, 40, 12);
        for (i, l) in frame.iter().enumerate() {
            assert_eq!(l.visible_width(), 40, "row {i} width mismatch");
        }
        assert!(frame[1].plain_text().contains("前端"));
        assert!(frame[1].plain_text().contains("後端"));
    }

    #[test]
    fn render_grid_overflow_shows_indicator() {
        let rooms: Vec<_> = (0..30).map(|i| room(&format!("d{i}"), 1, 0, false)).collect();
        // 40×6 → cols=4, rows=2, capacity=8 → 22 overflow.
        let frame = render_grid(&rooms, 40, 6);
        let last = frame.last().unwrap();
        assert!(last.plain_text().contains("…+22 more"), "expected overflow indicator, got {last:?}");
    }

    #[test]
    fn render_grid_no_overflow_omits_indicator() {
        let rooms: Vec<_> = (0..4).map(|i| room(&format!("d{i}"), 0, 0, false)).collect();
        let frame = render_grid(&rooms, 40, 6);
        let last = frame.last().unwrap();
        assert!(!last.plain_text().contains("more"));
    }

    #[test]
    fn render_grid_empty_rooms_still_well_formed() {
        let frame = render_grid(&[], 40, 12);
        assert_eq!(frame.len(), 12);
        for l in &frame {
            assert_eq!(l.visible_width(), 40);
            assert!(!l.plain_text().contains('┌'));
        }
    }

    #[test]
    fn render_grid_overflow_indicator_keeps_row_width() {
        let rooms: Vec<_> = (0..50).map(|i| room(&format!("d{i}"), 1, 0, false)).collect();
        let frame = render_grid(&rooms, 40, 9);
        let last = frame.last().unwrap();
        assert_eq!(last.visible_width(), 40);
        assert!(last.plain_text().contains("more"));
    }

    #[test]
    fn render_grid_narrow_panel_returns_blank_frame_plus_overflow() {
        // panel_w=8 < TILE_WIDTH → capacity=0. All rooms overflow.
        let rooms = vec![room("src", 1, 0, false), room("doc", 0, 0, false)];
        let frame = render_grid(&rooms, 8, 6);
        assert_eq!(frame.len(), 6);
        for l in &frame {
            assert_eq!(l.visible_width(), 8);
        }
        // Overflow indicator truncated by clip_to_width (8 cols can fit "…+2 mor" + …)
        assert!(frame.last().unwrap().plain_text().contains("…"));
    }

    // ------------------------------------------------------------------
    // P2 — take_cols helper (CJK-safe slicing)
    // ------------------------------------------------------------------

    #[test]
    fn take_cols_ascii_truncates_cleanly() {
        assert_eq!(take_cols("hello world", 5), "hello");
    }

    #[test]
    fn take_cols_cjk_respects_double_width() {
        // Each CJK char = 2 cols; budget 5 → 2 chars (4 cols), the 3rd
        // would overflow so stop.
        let out = take_cols("前端後端", 5);
        assert_eq!(vis_width(&out), 4);
    }

    #[test]
    fn take_cols_zero_returns_empty() {
        assert_eq!(take_cols("anything", 0), "");
    }
}
