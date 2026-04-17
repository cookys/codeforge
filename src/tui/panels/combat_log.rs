//! CombatLog panel — Phase 2c P4.
//!
//! Reads the last N rows from `combat_log` and formats them as
//! `[HH:MM] kind — name · +XP XP` lines. Pure function of a pre-loaded
//! `Vec<CombatLogRow>` so tests can feed fixtures without hitting SQLite.

use super::{pad_to_width, vis_width};

/// A single combat_log row loaded from DB. The DB loader lives in P5;
/// having the struct here keeps the renderer testable in isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatLogRow {
    pub mob_kind: String,
    pub mob_name: String,
    pub xp_gained: i64,
    /// Loot summary string (already joined). Empty = no loot drop.
    pub loot: Option<String>,
    /// `occurred_at` text from DB (datetime default `YYYY-MM-DD HH:MM:SS`).
    pub occurred_at: String,
}

/// Render the combat-log panel. Rows are expected to be sorted newest-first
/// by the caller (`ORDER BY id DESC`). Produces `max_rows` lines max,
/// padded if there's headroom.
pub fn render(rows: &[CombatLogRow], width: usize, max_rows: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(max_rows + 1);
    out.push(pad_to_width("⚔ Combat Log", width));
    if rows.is_empty() {
        out.push(pad_to_width("  (no kills yet)", width));
        while out.len() < max_rows {
            out.push(pad_to_width("", width));
        }
        return out;
    }
    for row in rows.iter().take(max_rows.saturating_sub(1)) {
        let time = short_time(&row.occurred_at);
        let loot_tail = row
            .loot
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|l| format!(" · {l}"))
            .unwrap_or_default();
        let line = format!(
            "[{time}] {kind} — {name} (+{xp}){loot_tail}",
            kind = row.mob_kind,
            name = row.mob_name,
            xp = row.xp_gained,
        );
        out.push(pad_to_width(&line, width));
    }
    while out.len() < max_rows {
        out.push(pad_to_width("", width));
    }
    out
}

/// Extract `HH:MM` from a `YYYY-MM-DD HH:MM:SS` timestamp; falls back to
/// returning the full string if the shape doesn't match, so malformed
/// data still renders (just wider).
fn short_time(ts: &str) -> String {
    ts.split_whitespace()
        .nth(1)
        .and_then(|hms| hms.get(..5))
        .unwrap_or(ts)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, name: &str, xp: i64, loot: Option<&str>) -> CombatLogRow {
        CombatLogRow {
            mob_kind: kind.to_string(),
            mob_name: name.to_string(),
            xp_gained: xp,
            loot: loot.map(|s| s.to_string()),
            occurred_at: "2026-04-17 14:23:45".to_string(),
        }
    }

    #[test]
    fn header_is_first_line() {
        let lines = render(&[], 40, 5);
        assert!(lines[0].contains("Combat Log"));
    }

    #[test]
    fn empty_rows_show_placeholder() {
        let lines = render(&[], 40, 5);
        assert!(lines[1].contains("no kills yet"));
    }

    #[test]
    fn produces_exactly_max_rows_lines() {
        let lines = render(&[row("ghost", "x", 5, None)], 40, 5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn rows_render_time_kind_name_xp() {
        let lines = render(&[row("ghost", "unused@x.rs", 5, None)], 60, 3);
        assert!(lines[1].starts_with("[14:23]"));
        assert!(lines[1].contains("ghost"));
        assert!(lines[1].contains("unused@x.rs"));
        assert!(lines[1].contains("+5"));
    }

    #[test]
    fn loot_appended_after_dot() {
        let lines = render(
            &[row("boss", "big", 50, Some("Rare Item"))],
            80,
            3,
        );
        assert!(lines[1].contains("· Rare Item"));
    }

    #[test]
    fn all_lines_match_requested_width() {
        let rows = vec![
            row("ghost", "a", 5, None),
            row("zombie", "b", 3, Some("TODO Cleaner")),
            row("boss", "c", 50, Some("Rare Item")),
        ];
        let lines = render(&rows, 50, 5);
        for l in &lines {
            assert_eq!(vis_width(l), 50);
        }
    }

    #[test]
    fn caps_at_max_rows_minus_header() {
        // 10 rows fed, max_rows=5 → 1 header + 4 rows = 5 lines
        let rows: Vec<_> = (0..10).map(|i| row("ghost", "x", i, None)).collect();
        let lines = render(&rows, 40, 5);
        assert_eq!(lines.len(), 5);
        let data_lines = &lines[1..];
        assert_eq!(data_lines.len(), 4);
    }

    #[test]
    fn malformed_timestamp_does_not_panic() {
        let r = CombatLogRow {
            mob_kind: "ghost".to_string(),
            mob_name: "x".to_string(),
            xp_gained: 1,
            loot: None,
            occurred_at: "bad".to_string(),
        };
        let lines = render(&[r], 80, 3);
        assert!(lines[1].contains("ghost"));
    }

    #[test]
    fn long_mob_name_clipped_to_width() {
        let long_name = "a".repeat(500);
        let lines = render(&[row("boss", &long_name, 50, None)], 50, 3);
        assert_eq!(vis_width(&lines[1]), 50);
        assert!(lines[1].contains('…'));
    }
}
