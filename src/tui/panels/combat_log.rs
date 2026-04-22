//! CombatLog panel — Phase 2c P4; Phase 3c P5 adds commentary rows.
//!
//! Reads the last N rows from `combat_log` and (Phase 3c) interleaves
//! them with `commentary_feed` entries, newest-first. Pure function of
//! pre-loaded slices so tests can feed fixtures without hitting SQLite.

use super::pad_to_width;
use super::super::styled::StyledLine;
use crate::commentary::display::CommentaryEntry;
use chrono::{TimeZone, Utc};

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
#[allow(dead_code)] // Superseded by `dispatcher::render_combat_log` but kept for direct panel tests.
pub fn render(rows: &[CombatLogRow], width: usize, max_rows: usize) -> Vec<StyledLine> {
    let mut out: Vec<StyledLine> = Vec::with_capacity(max_rows + 1);
    out.push(StyledLine::plain(pad_to_width("⚔ Combat Log", width)));
    if rows.is_empty() {
        out.push(StyledLine::plain(pad_to_width("  (no kills yet)", width)));
        while out.len() < max_rows {
            out.push(StyledLine::plain(pad_to_width("", width)));
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
        out.push(StyledLine::plain(pad_to_width(&line, width)));
    }
    while out.len() < max_rows {
        out.push(StyledLine::plain(pad_to_width("", width)));
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

/// Phase 3c P5: combined combat + commentary renderer. Caller passes both
/// sources newest-first; the panel merges them by timestamp, keeps the
/// most recent `max_rows - 1` (one line reserved for the header), and
/// prefixes each line with an icon so the two streams are visually
/// distinct (`⚔` combat, `💬` commentary).
pub fn render_mixed(
    combat: &[CombatLogRow],
    commentary: &[CommentaryEntry],
    width: usize,
    max_rows: usize,
) -> Vec<StyledLine> {
    let mut out: Vec<StyledLine> = Vec::with_capacity(max_rows + 1);
    out.push(StyledLine::plain(pad_to_width("⚔ Combat Log", width)));
    if max_rows == 0 {
        return out;
    }

    // Merge newest-first. Both sources are already ordered newest-first
    // individually, so we walk them pairwise using a peek-compare loop —
    // avoids allocating a combined Vec for sorting.
    let merged = merge_newest_first(combat, commentary);
    if merged.is_empty() {
        out.push(StyledLine::plain(pad_to_width("  (no activity yet)", width)));
        while out.len() < max_rows {
            out.push(StyledLine::plain(pad_to_width("", width)));
        }
        return out;
    }

    for entry in merged.into_iter().take(max_rows.saturating_sub(1)) {
        let line = match entry {
            MergedEntry::Combat(row) => format_combat_line(row),
            MergedEntry::Commentary(entry) => format_commentary_line(entry),
        };
        out.push(StyledLine::plain(pad_to_width(&line, width)));
    }
    while out.len() < max_rows {
        out.push(StyledLine::plain(pad_to_width("", width)));
    }
    out
}

enum MergedEntry<'a> {
    Combat(&'a CombatLogRow),
    Commentary(&'a CommentaryEntry),
}

fn merge_newest_first<'a>(
    combat: &'a [CombatLogRow],
    commentary: &'a [CommentaryEntry],
) -> Vec<MergedEntry<'a>> {
    let mut out = Vec::with_capacity(combat.len() + commentary.len());
    let mut i = 0;
    let mut j = 0;
    while i < combat.len() && j < commentary.len() {
        let combat_ts = parse_combat_ts(&combat[i].occurred_at);
        let commentary_ts = commentary[j].created_at;
        if combat_ts >= commentary_ts {
            out.push(MergedEntry::Combat(&combat[i]));
            i += 1;
        } else {
            out.push(MergedEntry::Commentary(&commentary[j]));
            j += 1;
        }
    }
    while i < combat.len() {
        out.push(MergedEntry::Combat(&combat[i]));
        i += 1;
    }
    while j < commentary.len() {
        out.push(MergedEntry::Commentary(&commentary[j]));
        j += 1;
    }
    out
}

fn format_combat_line(row: &CombatLogRow) -> String {
    let time = short_time(&row.occurred_at);
    let loot_tail = row
        .loot
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|l| format!(" · {l}"))
        .unwrap_or_default();
    format!(
        "⚔ [{time}] {kind} — {name} (+{xp}){loot_tail}",
        kind = row.mob_kind,
        name = row.mob_name,
        xp = row.xp_gained,
    )
}

fn format_commentary_line(entry: &CommentaryEntry) -> String {
    let time = short_time_from_unix(entry.created_at);
    format!("💬 [{time}] {}", entry.phrase)
}

fn short_time_from_unix(ts: i64) -> String {
    match Utc.timestamp_opt(ts, 0).single() {
        Some(dt) => dt.format("%H:%M").to_string(),
        // Unrepresentable epochs fall back to the raw number; the panel
        // will render an ugly but non-panicking line.
        None => ts.to_string(),
    }
}

/// Reverse of `short_time`: best-effort parse of a "YYYY-MM-DD HH:MM:SS"
/// timestamp to unix seconds. Malformed input returns `0` so merge stays
/// total — the row will sort as "very old" but nothing panics.
fn parse_combat_ts(ts: &str) -> i64 {
    match chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        Ok(dt) => dt.and_utc().timestamp(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // vis_width no longer needed — StyledLine has its own visible_width().

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
        assert!(lines[0].plain_text().contains("Combat Log"));
    }

    #[test]
    fn empty_rows_show_placeholder() {
        let lines = render(&[], 40, 5);
        assert!(lines[1].plain_text().contains("no kills yet"));
    }

    #[test]
    fn produces_exactly_max_rows_lines() {
        let lines = render(&[row("ghost", "x", 5, None)], 40, 5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn rows_render_time_kind_name_xp() {
        let lines = render(&[row("ghost", "unused@x.rs", 5, None)], 60, 3);
        assert!(lines[1].plain_text().starts_with("[14:23]"));
        assert!(lines[1].plain_text().contains("ghost"));
        assert!(lines[1].plain_text().contains("unused@x.rs"));
        assert!(lines[1].plain_text().contains("+5"));
    }

    #[test]
    fn loot_appended_after_dot() {
        let lines = render(
            &[row("boss", "big", 50, Some("Rare Item"))],
            80,
            3,
        );
        assert!(lines[1].plain_text().contains("· Rare Item"));
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
            assert_eq!(l.visible_width(), 50);
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
        assert!(lines[1].plain_text().contains("ghost"));
    }

    #[test]
    fn long_mob_name_clipped_to_width() {
        let long_name = "a".repeat(500);
        let lines = render(&[row("boss", &long_name, 50, None)], 50, 3);
        assert_eq!(lines[1].visible_width(), 50);
        assert!(lines[1].plain_text().contains('…'));
    }

    // ── Phase 3c P5: render_mixed tests ───────────────────────────

    fn combat_row(time: &str, kind: &str, name: &str) -> CombatLogRow {
        CombatLogRow {
            mob_kind: kind.into(),
            mob_name: name.into(),
            xp_gained: 10,
            loot: None,
            occurred_at: time.into(),
        }
    }

    fn commentary_entry(ts: i64, phrase: &str) -> CommentaryEntry {
        CommentaryEntry {
            phrase: phrase.into(),
            created_at: ts,
            kind: crate::commentary::Kind::BossKill,
        }
    }

    #[test]
    fn mixed_merges_newest_first_across_sources() {
        // 14:20 commentary, 14:25 combat, 14:30 commentary — interleaved
        let combat = vec![combat_row("2026-04-18 14:25:00", "boss", "x")];
        let commentary = vec![
            commentary_entry(ts("2026-04-18 14:30:00"), "newest phrase"),
            commentary_entry(ts("2026-04-18 14:20:00"), "oldest phrase"),
        ];
        let lines = render_mixed(&combat, &commentary, 80, 5);
        // header + 3 entries
        assert!(lines[0].plain_text().contains("Combat Log"));
        assert!(lines[1].plain_text().contains("💬") && lines[1].plain_text().contains("newest phrase"));
        assert!(lines[2].plain_text().contains("⚔") && lines[2].plain_text().contains("boss"));
        assert!(lines[3].plain_text().contains("💬") && lines[3].plain_text().contains("oldest phrase"));
    }

    #[test]
    fn mixed_empty_shows_activity_placeholder() {
        let lines = render_mixed(&[], &[], 40, 5);
        assert!(lines[1].plain_text().contains("no activity yet"));
    }

    #[test]
    fn mixed_commentary_only_renders_with_icon() {
        let commentary = vec![commentary_entry(ts("2026-04-18 10:00:00"), "hello")];
        let lines = render_mixed(&[], &commentary, 60, 3);
        assert!(lines[1].plain_text().starts_with("💬"));
        assert!(lines[1].plain_text().contains("hello"));
    }

    #[test]
    fn mixed_combat_only_gains_icon_prefix() {
        // render_mixed prefixes combat lines with `⚔ `, unlike legacy render
        let combat = vec![combat_row("2026-04-18 14:25:00", "ghost", "x")];
        let lines = render_mixed(&combat, &[], 60, 3);
        assert!(lines[1].plain_text().starts_with("⚔"));
    }

    #[test]
    fn mixed_caps_at_max_rows_minus_header() {
        // 10 commentary entries, max_rows=5 → header + 4 entries
        let commentary: Vec<_> = (0..10)
            .map(|i| commentary_entry(1000 + i, &format!("msg{i}")))
            .collect();
        let lines = render_mixed(&[], &commentary, 40, 5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn mixed_width_enforced() {
        let commentary = vec![commentary_entry(1000, "x")];
        let combat = vec![combat_row("2026-04-18 14:25:00", "ghost", "y")];
        let lines = render_mixed(&combat, &commentary, 55, 5);
        for l in &lines {
            assert_eq!(l.visible_width(), 55);
        }
    }

    fn ts(s: &str) -> i64 {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp()
    }
}
