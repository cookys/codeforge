//! Player-session bookkeeping + Welcome Back Report (spec §3.1).
//!
//! Two pieces glued together:
//!   1. `last_player_seen_at` in the `settings` k/v — bumped on every CLI
//!      entry point (`codeforge statusline`, `codeforge tui`). Used to
//!      measure player absence between invocations.
//!   2. `WelcomeBackSummary::build` — aggregate combat_log + loot_inventory
//!      between last-seen and now, plus current HP%. Returns `None` if
//!      absence is below the 10-minute threshold (intra-session noise).
//!
//! Design notes:
//!   - Absence floor of 10 min keeps statusline (which fires ~every 5s
//!     during an active session) from flashing the report repeatedly.
//!   - We always bump `last_seen` BEFORE returning the summary to the
//!     caller — so even if the caller chooses to suppress the render
//!     (e.g. narrow terminal), the window closes cleanly and we don't
//!     re-report the same absence on a subsequent tick.
//!   - Rendering is intentionally plain-text (`render_lines()` → Vec<String>)
//!     so both the statusline (single compact row) and the TUI panel
//!     (multi-line) can cherry-pick. ANSI-coloring happens at the call
//!     site, keeping this module test-friendly.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

/// Seconds of absence below which we consider the player "still in the
/// same session" and suppress the report. Spec §3 design decision.
pub const ABSENCE_THRESHOLD_SECS: i64 = 10 * 60;

/// Read `last_player_seen_at` from settings. `None` on first-ever run
/// (so callers show a neutral greeting rather than "you've been gone
/// for eternity").
pub fn get_last_seen(conn: &Connection) -> Result<Option<i64>> {
    let row: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'last_player_seen_at'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(row.and_then(|s| s.parse::<i64>().ok()))
}

/// Upsert `last_player_seen_at` to `now_unix`.
pub fn update_last_seen(conn: &Connection, now_unix: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('last_player_seen_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                         updated_at = datetime('now')",
        [now_unix.to_string()],
    )?;
    Ok(())
}

/// Aggregated stats about what happened while the player was away.
/// All counts are capped at `u32::MAX` for defensive saturation — a
/// real absence can't possibly produce overflow-magnitude counts, but
/// test inputs can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeBackSummary {
    pub absence_seconds: i64,
    /// Sorted ascending by kind name for stable rendering.
    pub kills_by_kind: Vec<(String, u32)>,
    pub loot_count: u32,
    /// HP as (hp, hp_max) so the caller can format with/without percent.
    pub hp: Option<(u32, u32)>,
}

impl WelcomeBackSummary {
    /// Construct the summary if the player was away long enough. Pass
    /// `hp_snapshot` as the latest `(hp, hp_max)` from the live state.
    /// Returns `None` when absence is below `ABSENCE_THRESHOLD_SECS`.
    pub fn build(
        conn: &Connection,
        last_seen: i64,
        now_unix: i64,
        hp_snapshot: Option<(u32, u32)>,
    ) -> Result<Option<Self>> {
        let absence = now_unix.saturating_sub(last_seen);
        if absence < ABSENCE_THRESHOLD_SECS {
            return Ok(None);
        }

        let kills_by_kind = count_kills_since(conn, last_seen)?;
        let loot_count = count_loot_since(conn, last_seen)?;

        Ok(Some(Self {
            absence_seconds: absence,
            kills_by_kind,
            loot_count,
            hp: hp_snapshot,
        }))
    }

    /// Return true iff the report has anything useful to say. Pure idle
    /// with no kills / loot / HP info produces an empty bullet list and
    /// the caller may prefer to hide the report altogether rather than
    /// show "Ferris 在巡邏……" every time.
    pub fn has_content(&self) -> bool {
        !self.kills_by_kind.is_empty() || self.loot_count > 0 || self.hp.is_some()
    }

    /// Render 2-4 lines in 繁中, already newline-stripped. The caller adds
    /// frames/colors. Format follows spec §3.1 example.
    pub fn render_lines(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(4);
        out.push(format!(
            "你不在的 {}：",
            human_duration(self.absence_seconds)
        ));

        for (kind, count) in &self.kills_by_kind {
            out.push(format!("  → 擊殺 {} ×{}", kind, count));
        }
        if self.loot_count > 0 {
            out.push(format!("  → 撿到 Loot ×{}", self.loot_count));
        }
        if let Some((hp, hp_max)) = self.hp {
            let ratio = hp as f32 / hp_max.max(1) as f32;
            let pct = (ratio * 100.0).round() as u32;
            let hint = if ratio < 0.3 {
                "（建議休息一下）"
            } else {
                ""
            };
            out.push(format!("  → HP {}% {}", pct, hint).trim_end().to_string());
        }

        // Pure-idle case: show the巡邏 line so the player doesn't feel the
        // report is broken when genuinely nothing happened.
        if !self.has_content() || (self.kills_by_kind.is_empty() && self.loot_count == 0) {
            out.push("  → Ferris 在這裡巡邏，沒有發現新威脅。".to_string());
        }
        out
    }
}

fn count_kills_since(conn: &Connection, since_unix: i64) -> Result<Vec<(String, u32)>> {
    // combat_log.occurred_at is stored as ISO text (`datetime('now')`)
    // — convert to unix via strftime for a range compare.
    let mut stmt = conn.prepare(
        "SELECT mob_kind, COUNT(*) as n
         FROM combat_log
         WHERE CAST(strftime('%s', occurred_at) AS INTEGER) >= ?1
         GROUP BY mob_kind
         ORDER BY mob_kind ASC",
    )?;
    let rows = stmt.query_map([since_unix], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn count_loot_since(conn: &Connection, since_unix: i64) -> Result<u32> {
    // loot_inventory.last_acquired_at is already unix seconds.
    let sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(quantity), 0) FROM loot_inventory
         WHERE last_acquired_at >= ?1",
        [since_unix],
        |r| r.get(0),
    )?;
    Ok(sum.clamp(0, u32::MAX as i64) as u32)
}

/// "8 小時" / "3 天 12 小時" / "1 分鐘" etc. Coarse granularity on purpose
/// — the report is vibe, not precise analytics.
fn human_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let days = s / 86_400;
    let hours = (s % 86_400) / 3600;
    let minutes = (s % 3600) / 60;
    if days > 0 {
        if hours > 0 {
            format!("{days} 天 {hours} 小時")
        } else {
            format!("{days} 天")
        }
    } else if hours > 0 {
        // Drop sub-10-minute remainders ("1 小時 3 分" reads noisy); keep
        // ≥10 so "1 小時 45 分" still carries useful precision.
        if minutes >= 10 {
            format!("{hours} 小時 {minutes} 分")
        } else {
            format!("{hours} 小時")
        }
    } else {
        format!("{minutes} 分鐘")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn get_last_seen_none_on_fresh_install() {
        let conn = fresh();
        assert!(get_last_seen(&conn).unwrap().is_none());
    }

    #[test]
    fn update_last_seen_round_trips() {
        let conn = fresh();
        update_last_seen(&conn, 1_700_000_000).unwrap();
        assert_eq!(get_last_seen(&conn).unwrap(), Some(1_700_000_000));
    }

    #[test]
    fn update_last_seen_upserts_not_duplicates() {
        let conn = fresh();
        update_last_seen(&conn, 1).unwrap();
        update_last_seen(&conn, 2).unwrap();
        update_last_seen(&conn, 3).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'last_player_seen_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(get_last_seen(&conn).unwrap(), Some(3));
    }

    #[test]
    fn build_returns_none_below_threshold() {
        let conn = fresh();
        let now = 1_000_000;
        // 5 min absence < 10 min threshold
        let summary = WelcomeBackSummary::build(&conn, now - 300, now, None).unwrap();
        assert!(summary.is_none());
    }

    #[test]
    fn build_returns_some_at_threshold_exactly() {
        let conn = fresh();
        let now = 1_000_000;
        let summary =
            WelcomeBackSummary::build(&conn, now - ABSENCE_THRESHOLD_SECS, now, None).unwrap();
        assert!(summary.is_some());
    }

    #[test]
    fn build_aggregates_kills_by_kind() {
        let conn = fresh();
        let now: i64 = 1_700_000_000;
        let since = now - 3600;
        // Recent ISO (mid-2023) — sits *after* `since`.
        let recent_iso = "2023-12-01 00:00:00";
        for (kind, count) in [("boss", 2), ("zombie", 5), ("ghost", 1)] {
            for _ in 0..count {
                conn.execute(
                    "INSERT INTO combat_log (zone_id, mob_name, mob_kind, xp_gained, occurred_at)
                     VALUES ('rust', 'x', ?1, 5, ?2)",
                    rusqlite::params![kind, recent_iso],
                )
                .unwrap();
            }
        }
        let summary = WelcomeBackSummary::build(&conn, since, now, Some((50, 100)))
            .unwrap()
            .unwrap();
        assert_eq!(
            summary.kills_by_kind,
            vec![
                ("boss".into(), 2),
                ("ghost".into(), 1),
                ("zombie".into(), 5)
            ],
            "sorted ascending by kind"
        );
    }

    #[test]
    fn build_skips_kills_before_last_seen() {
        let conn = fresh();
        // Use a modern `now` so '2020-...' can sit *before* `last_seen`.
        let now: i64 = 1_700_000_000; // 2023-ish
        let since = now - 3600;
        // An "old" kill from 2020-01-01 — epoch seconds 1577836800, well
        // before `since`.
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_name, mob_kind, xp_gained, occurred_at)
             VALUES ('rust', 'x', 'boss', 5, '2020-01-01 00:00:00')",
            [],
        )
        .unwrap();
        let summary = WelcomeBackSummary::build(&conn, since, now, None)
            .unwrap()
            .unwrap();
        assert!(
            summary.kills_by_kind.is_empty(),
            "kill before last_seen must be excluded"
        );
    }

    #[test]
    fn build_sums_loot_quantity_since_last_seen() {
        let conn = fresh();
        let now = 1_000_000;
        let since = now - 3600;
        // Only the "new" row counts.
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('item', 'Rare', 3, ?1, ?1)",
            [now - 60],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO loot_inventory (kind, name, quantity, first_acquired_at, last_acquired_at)
             VALUES ('item', 'Older', 99, ?1, ?1)",
            [since - 3600],
        )
        .unwrap();
        let summary = WelcomeBackSummary::build(&conn, since, now, None)
            .unwrap()
            .unwrap();
        assert_eq!(summary.loot_count, 3);
    }

    #[test]
    fn render_lines_includes_patrol_line_on_pure_idle() {
        let s = WelcomeBackSummary {
            absence_seconds: 8 * 3600,
            kills_by_kind: vec![],
            loot_count: 0,
            hp: None,
        };
        let lines = s.render_lines();
        assert!(lines.iter().any(|l| l.contains("巡邏")));
    }

    #[test]
    fn render_lines_low_hp_includes_rest_hint() {
        let s = WelcomeBackSummary {
            absence_seconds: 3600,
            kills_by_kind: vec![("zombie".into(), 3)],
            loot_count: 1,
            hp: Some((2, 10)),
        };
        let lines = s.render_lines();
        assert!(
            lines.iter().any(|l| l.contains("建議休息")),
            "HP<30% should trigger the rest hint"
        );
    }

    #[test]
    fn render_lines_high_hp_omits_rest_hint() {
        let s = WelcomeBackSummary {
            absence_seconds: 3600,
            kills_by_kind: vec![],
            loot_count: 0,
            hp: Some((9, 10)),
        };
        let lines = s.render_lines();
        assert!(!lines.iter().any(|l| l.contains("建議休息")));
    }

    #[test]
    fn human_duration_scales_by_magnitude() {
        assert_eq!(human_duration(600), "10 分鐘");
        assert_eq!(human_duration(3600), "1 小時");
        assert_eq!(human_duration(3900), "1 小時"); // <10 min remainder dropped
        assert_eq!(human_duration(4500), "1 小時 15 分");
        assert_eq!(human_duration(86_400 + 3600), "1 天 1 小時");
        assert_eq!(human_duration(3 * 86_400), "3 天");
    }

    #[test]
    fn human_duration_never_panics_on_negative() {
        // A clock jump backwards shouldn't panic.
        assert_eq!(human_duration(-100), "0 分鐘");
    }
}
