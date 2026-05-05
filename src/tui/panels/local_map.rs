//! LocalMap panel renderer — Phase 2c P4 + tile-map UX polish.
//!
//! Two display modes, selected via [`LocalMapPanel::display_mode`]:
//! * `List`: original linear `▶ dir    [🧟2]` format ([`render_list`])
//! * `Grid`: tile-grid via `local_map_tile::render_grid`, with automatic
//!   fallback to `List` when `width < MIN_GRID_WIDTH` (grid math can't
//!   fit even one tile)
//!
//! Pure — caller has already done the DB work.

use super::super::local_map::RoomSummary;
use super::super::styled::StyledLine;
use super::local_map_tile::{render_grid, MIN_GRID_WIDTH};
use super::{pad_to_width, vis_width};

/// User-selectable display mode for the LocalMap panel. Toggled at runtime
/// via the `g`/`l` keys; state lives in memory only (Phase 1 scope —
/// cross-session persistence is out of scope per plan).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Linear `▶ dir [🧟N]` list. Default — matches Phase 2c behaviour.
    #[default]
    List,
    /// Tile-grid overhead view. Falls back to list when the panel is too
    /// narrow to fit a single tile.
    Grid,
}

/// Panel state held across frames. Mirrors the `ZoaPanel` shape so the
/// main loop can pass `&mut LocalMapPanel` into `paint_once` and the
/// event handler can flip `display_mode` in place.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalMapPanel {
    pub display_mode: DisplayMode,
}

impl LocalMapPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycle between List and Grid. Called from the main loop on every
    /// `ToggleMapMode` event.
    pub fn toggle(&mut self) {
        self.display_mode = match self.display_mode {
            DisplayMode::List => DisplayMode::Grid,
            DisplayMode::Grid => DisplayMode::List,
        };
    }
}

/// Dispatcher: route to `render_list` or `render_grid` per panel state.
/// Narrow widths force a list fallback even in Grid mode — the grid
/// simply cannot fit below `MIN_GRID_WIDTH` (30 cols).
pub fn render(
    panel: &LocalMapPanel,
    rooms: &[RoomSummary],
    width: usize,
    max_rows: usize,
) -> Vec<StyledLine> {
    match panel.display_mode {
        DisplayMode::List => render_list(rooms, width, max_rows),
        DisplayMode::Grid if width >= MIN_GRID_WIDTH => render_grid(rooms, width, max_rows),
        DisplayMode::Grid => render_list(rooms, width, max_rows),
    }
}

/// Render the LocalMap panel as a linear list. Produces `max_rows` lines
/// padded to `width` visible columns. Exposed so the dispatcher (and
/// tests that pin the old behaviour) can call it directly.
pub fn render_list(rooms: &[RoomSummary], width: usize, max_rows: usize) -> Vec<StyledLine> {
    let mut out: Vec<StyledLine> = Vec::with_capacity(max_rows + 1);
    out.push(StyledLine::plain(pad_to_width("📍 Local Map", width)));

    if rooms.is_empty() {
        out.push(StyledLine::plain(pad_to_width(
            "  (no mobs scanned)",
            width,
        )));
        while out.len() < max_rows {
            out.push(StyledLine::plain(pad_to_width("", width)));
        }
        return out;
    }

    for room in rooms.iter().take(max_rows.saturating_sub(1)) {
        let marker = if room.is_current { "▶ " } else { "  " };
        let badge = badge_for(room);
        // Build "marker dir    badge" where dir takes most of the row and
        // badge sits on the right. Reserve ~8 cols for the badge.
        let badge_w = vis_width(&badge);
        let body_w = width
            .saturating_sub(vis_width(marker))
            .saturating_sub(badge_w)
            .saturating_sub(1); // 1-space gap before badge
        let dir = if body_w == 0 {
            String::new()
        } else {
            pad_to_width(&room.directory, body_w)
        };
        let line = format!("{marker}{dir} {badge}");
        out.push(StyledLine::plain(pad_to_width(&line, width)));
    }
    while out.len() < max_rows {
        out.push(StyledLine::plain(pad_to_width("", width)));
    }
    out
}

/// Compact right-side badge: `[🧟N]` when alive mobs remain, `[✓]` when
/// all defeated, `[—]` when no mobs ever recorded for this dir (defensive
/// — caller shouldn't produce such rows, but don't panic).
fn badge_for(room: &RoomSummary) -> String {
    if room.alive > 0 {
        format!("[🧟{}]", room.alive)
    } else if room.defeated > 0 {
        "[✓]".to_string()
    } else {
        "[—]".to_string()
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

    #[test]
    fn header_is_first_line() {
        let lines = render_list(&[], 30, 5);
        assert!(lines[0].plain_text().contains("Local Map"));
    }

    #[test]
    fn empty_rooms_show_placeholder() {
        let lines = render_list(&[], 30, 5);
        assert!(lines[1].plain_text().contains("no mobs"));
    }

    #[test]
    fn current_row_marked_with_arrow() {
        let rooms = vec![room("src", 2, 0, true), room("doc", 0, 0, false)];
        let lines = render_list(&rooms, 40, 5);
        assert!(lines[1].plain_text().contains("▶"));
        assert!(!lines[2].plain_text().contains("▶"));
    }

    #[test]
    fn alive_count_shown_in_badge() {
        let rooms = vec![room("src", 7, 0, false)];
        let lines = render_list(&rooms, 40, 3);
        assert!(lines[1].plain_text().contains("🧟7"));
    }

    #[test]
    fn all_defeated_shows_check() {
        let rooms = vec![room("target", 0, 5, false)];
        let lines = render_list(&rooms, 40, 3);
        assert!(lines[1].plain_text().contains("✓"));
    }

    #[test]
    fn renders_exactly_max_rows_lines() {
        let rooms = (0..3)
            .map(|i| room(&format!("d{i}"), 1, 0, false))
            .collect::<Vec<_>>();
        let lines = render_list(&rooms, 30, 5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn all_lines_match_requested_width() {
        let rooms = vec![
            room("src", 3, 0, true),
            room("doc", 0, 2, false),
            room("(unknown)", 1, 0, false),
        ];
        let lines = render_list(&rooms, 35, 6);
        for l in &lines {
            assert_eq!(l.visible_width(), 35);
        }
    }

    #[test]
    fn caps_rooms_at_max_rows_minus_header() {
        let rooms: Vec<_> = (0..20)
            .map(|i| room(&format!("d{i}"), 1, 0, false))
            .collect();
        let lines = render_list(&rooms, 30, 5);
        assert_eq!(lines.len(), 5);
        // 1 header + 4 rooms = 5
    }

    #[test]
    fn narrow_width_clips_directory_name() {
        let rooms = vec![room("very-long-directory-name", 1, 0, false)];
        let lines = render_list(&rooms, 20, 3);
        assert_eq!(lines[1].visible_width(), 20);
    }

    #[test]
    fn cjk_directory_name_renders_width_correctly() {
        let rooms = vec![room("原始碼目錄", 2, 0, true)];
        let lines = render_list(&rooms, 30, 3);
        assert_eq!(lines[1].visible_width(), 30);
        assert!(lines[1].plain_text().contains("▶"));
    }

    // ------------------------------------------------------------------
    // P3 — LocalMapPanel + dispatcher
    // ------------------------------------------------------------------

    #[test]
    fn panel_defaults_to_list_mode() {
        let panel = LocalMapPanel::new();
        assert_eq!(panel.display_mode, DisplayMode::List);
    }

    #[test]
    fn toggle_cycles_between_list_and_grid() {
        let mut panel = LocalMapPanel::new();
        assert_eq!(panel.display_mode, DisplayMode::List);
        panel.toggle();
        assert_eq!(panel.display_mode, DisplayMode::Grid);
        panel.toggle();
        assert_eq!(panel.display_mode, DisplayMode::List);
    }

    #[test]
    fn dispatcher_list_mode_renders_old_format() {
        let panel = LocalMapPanel {
            display_mode: DisplayMode::List,
        };
        let rooms = vec![room("src", 2, 0, true)];
        let lines = render(&panel, &rooms, 40, 5);
        // List-mode header comes through.
        assert!(lines[0].plain_text().contains("Local Map"));
        // ▶ marker is a list-mode-only signal.
        assert!(lines[1].plain_text().contains("▶"));
    }

    #[test]
    fn dispatcher_grid_mode_renders_tile_borders() {
        let panel = LocalMapPanel {
            display_mode: DisplayMode::Grid,
        };
        let rooms = vec![room("src", 2, 0, false)];
        let lines = render(&panel, &rooms, 40, 6);
        // Grid mode: first row should be a tile top border.
        assert!(lines[0].plain_text().starts_with('┌'));
        // And no "Local Map" header (grid has no header row).
        assert!(!lines.iter().any(|l| l.plain_text().contains("📍")));
    }

    #[test]
    fn dispatcher_grid_mode_falls_back_to_list_below_min_width() {
        let panel = LocalMapPanel {
            display_mode: DisplayMode::Grid,
        };
        let rooms = vec![room("src", 2, 0, false)];
        // 28 cols < MIN_GRID_WIDTH (30) → fallback to list.
        let lines = render(&panel, &rooms, 28, 6);
        assert!(
            lines[0].plain_text().contains("Local Map"),
            "expected list header as fallback"
        );
    }

    #[test]
    fn dispatcher_preserves_width_in_both_modes() {
        let rooms = vec![room("src", 2, 0, true), room("doc", 0, 0, false)];
        for mode in [DisplayMode::List, DisplayMode::Grid] {
            let panel = LocalMapPanel { display_mode: mode };
            let lines = render(&panel, &rooms, 40, 6);
            for l in &lines {
                assert_eq!(l.visible_width(), 40, "mode {mode:?} width mismatch");
            }
        }
    }
}
