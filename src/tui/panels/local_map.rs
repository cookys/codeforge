//! LocalMap panel renderer — Phase 2c P4.
//!
//! Consumes `Vec<RoomSummary>` from `super::super::local_map::compute`
//! and formats as `▶ dir    [🧟2]` / `  dir    [✓]` lines. Pure —
//! caller has already done the DB work.

use super::super::local_map::RoomSummary;
use super::{pad_to_width, vis_width};

/// Render the LocalMap panel. Produces `max_rows` lines padded to
/// `width` visible columns.
pub fn render(rooms: &[RoomSummary], width: usize, max_rows: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(max_rows + 1);
    out.push(pad_to_width("📍 Local Map", width));

    if rooms.is_empty() {
        out.push(pad_to_width("  (no mobs scanned)", width));
        while out.len() < max_rows {
            out.push(pad_to_width("", width));
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
            let padded = pad_to_width(&room.directory, body_w);
            padded
        };
        let line = format!("{marker}{dir} {badge}");
        out.push(pad_to_width(&line, width));
    }
    while out.len() < max_rows {
        out.push(pad_to_width("", width));
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
        let lines = render(&[], 30, 5);
        assert!(lines[0].contains("Local Map"));
    }

    #[test]
    fn empty_rooms_show_placeholder() {
        let lines = render(&[], 30, 5);
        assert!(lines[1].contains("no mobs"));
    }

    #[test]
    fn current_row_marked_with_arrow() {
        let rooms = vec![room("src", 2, 0, true), room("doc", 0, 0, false)];
        let lines = render(&rooms, 40, 5);
        assert!(lines[1].contains("▶"));
        assert!(!lines[2].contains("▶"));
    }

    #[test]
    fn alive_count_shown_in_badge() {
        let rooms = vec![room("src", 7, 0, false)];
        let lines = render(&rooms, 40, 3);
        assert!(lines[1].contains("🧟7"));
    }

    #[test]
    fn all_defeated_shows_check() {
        let rooms = vec![room("target", 0, 5, false)];
        let lines = render(&rooms, 40, 3);
        assert!(lines[1].contains("✓"));
    }

    #[test]
    fn renders_exactly_max_rows_lines() {
        let rooms = (0..3).map(|i| room(&format!("d{i}"), 1, 0, false)).collect::<Vec<_>>();
        let lines = render(&rooms, 30, 5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn all_lines_match_requested_width() {
        let rooms = vec![
            room("src", 3, 0, true),
            room("doc", 0, 2, false),
            room("(unknown)", 1, 0, false),
        ];
        let lines = render(&rooms, 35, 6);
        for l in &lines {
            assert_eq!(vis_width(l), 35);
        }
    }

    #[test]
    fn caps_rooms_at_max_rows_minus_header() {
        let rooms: Vec<_> = (0..20).map(|i| room(&format!("d{i}"), 1, 0, false)).collect();
        let lines = render(&rooms, 30, 5);
        assert_eq!(lines.len(), 5);
        // 1 header + 4 rooms = 5
    }

    #[test]
    fn narrow_width_clips_directory_name() {
        let rooms = vec![room("very-long-directory-name", 1, 0, false)];
        let lines = render(&rooms, 20, 3);
        assert_eq!(vis_width(&lines[1]), 20);
    }

    #[test]
    fn cjk_directory_name_renders_width_correctly() {
        let rooms = vec![room("原始碼目錄", 2, 0, true)];
        let lines = render(&rooms, 30, 3);
        assert_eq!(vis_width(&lines[1]), 30);
        assert!(lines[1].contains("▶"));
    }
}
