//! Phase 3f — `codeforge snapshot`.
//!
//! Collects a rolling-window view of combat + world state and renders a
//! box-drawing ASCII card suitable for pasting into Slack/Discord.
//!
//! Kept separate from `cli/` so the `collect → render` pipeline stays
//! offline-testable: `render` takes a plain `SnapshotData` struct, no DB
//! access, no I/O; `collect` reads SQLite but does no formatting.
//!
//! Not a cron job — the CLI invokes `collect` each time the user runs
//! `codeforge snapshot`, so the numbers are always fresh against the
//! current `combat_log` and `game_world` state.

pub mod render;

use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

use crate::pet::live_state::LiveState;
use crate::world::{self, ZoneStats};

/// Default rolling window (days) for `codeforge snapshot`. Matches the
/// spec's "本月戰績" framing without introducing calendar-month edge
/// cases at the first-of-the-month boundary.
pub const DEFAULT_WINDOW_DAYS: i64 = 30;

/// A longest-consecutive-kill streak observed inside a single zone within
/// the window. `count == 0` when the combat_log is empty — consumers
/// should suppress the streak line rather than printing "0 kills streak".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreakRecord {
    pub zone_id: String,
    pub mob_kind: String,
    pub count: u32,
}

/// All state the renderer needs. Kept pure-data so `render` can be unit
/// tested without a database.
#[derive(Debug, Clone)]
pub struct SnapshotData {
    /// Pet identity + level. `None` when no pet has been adopted — the
    /// renderer shows a generic placeholder in that case.
    pub pet: Option<PetHeader>,
    /// ISO-8601 date of generation (e.g. "2026-04-18"). Printed in the
    /// card header so the card is self-dating.
    pub generated_date: String,
    /// How many days the window spans (user-configurable via `--days`).
    pub window_days: i64,
    /// Kill count per `mob_kind` inside the window. Missing kinds mean 0
    /// — the renderer looks up known kinds explicitly so the lines are
    /// deterministic.
    pub kills_by_kind: HashMap<String, u32>,
    /// Longest single-zone consecutive-kill streak in the window.
    pub top_streak: StreakRecord,
    /// Zones, as `load_all` orders them (stable 5-village layout).
    pub zones: Vec<ZoneStats>,
}

/// Minimal pet header — only the fields the card prints.
#[derive(Debug, Clone)]
pub struct PetHeader {
    pub name: String,
    pub village_id: String,
    pub level: u32,
}

/// Read the rolling-window snapshot. `days` is clamped to a positive
/// value by the CLI before reaching here.
pub fn collect(conn: &Connection, days: i64) -> Result<SnapshotData> {
    let pet = load_pet_header(conn)?;
    let kills_by_kind = kills_by_kind(conn, days)?;
    let top_streak = longest_streak(conn, days)?;
    let zones = world::load_all(conn)?;
    let generated_date = conn
        .query_row("SELECT date('now')", [], |r| r.get::<_, String>(0))?;
    Ok(SnapshotData {
        pet,
        generated_date,
        window_days: days,
        kills_by_kind,
        top_streak,
        zones,
    })
}

fn load_pet_header(conn: &Connection) -> Result<Option<PetHeader>> {
    let live = match LiveState::load(conn)? {
        Some(l) => l,
        None => return Ok(None),
    };
    Ok(Some(PetHeader {
        name: live.state.name,
        village_id: live.state.village,
        level: live.state.level,
    }))
}

fn kills_by_kind(conn: &Connection, days: i64) -> Result<HashMap<String, u32>> {
    let mut stmt = conn.prepare(
        "SELECT mob_kind, COUNT(*) FROM combat_log
         WHERE occurred_at >= datetime('now', ?1)
         GROUP BY mob_kind",
    )?;
    let window = format!("-{} days", days);
    let rows = stmt.query_map(rusqlite::params![window], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out: HashMap<String, u32> = HashMap::new();
    for row in rows {
        let (kind, count) = row?;
        out.insert(kind, count.max(0) as u32);
    }
    Ok(out)
}

/// Longest run of consecutive kills inside a single zone within the
/// window. A "kill" is any `combat_log` row (every row represents a
/// defeated mob). The run breaks on zone change — cross-zone streaks
/// would require cross-zone scanner support (future phase).
///
/// Ties: first zone encountered wins. Deterministic thanks to the
/// `ORDER BY occurred_at, id` ordering.
fn longest_streak(conn: &Connection, days: i64) -> Result<StreakRecord> {
    let mut stmt = conn.prepare(
        "SELECT zone_id, mob_kind FROM combat_log
         WHERE occurred_at >= datetime('now', ?1)
         ORDER BY occurred_at, id",
    )?;
    let window = format!("-{} days", days);
    let rows = stmt.query_map(rusqlite::params![window], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;

    let mut best = StreakRecord::default();
    let mut cur_zone: Option<String> = None;
    let mut cur_kind: Option<String> = None;
    let mut cur_count: u32 = 0;
    for row in rows {
        let (zone, kind) = row?;
        if Some(&zone) == cur_zone.as_ref() && Some(&kind) == cur_kind.as_ref() {
            cur_count += 1;
        } else {
            cur_zone = Some(zone);
            cur_kind = Some(kind);
            cur_count = 1;
        }
        if cur_count > best.count {
            best = StreakRecord {
                zone_id: cur_zone.clone().unwrap_or_default(),
                mob_kind: cur_kind.clone().unwrap_or_default(),
                count: cur_count,
            };
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn insert_kill(conn: &Connection, zone: &str, kind: &str, offset_days: i64) {
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_name, mob_kind, xp_gained, occurred_at)
             VALUES (?1, ?2, ?3, 10, datetime('now', ?4))",
            rusqlite::params![zone, format!("{kind}-mob"), kind, format!("-{offset_days} days")],
        )
        .unwrap();
    }

    #[test]
    fn collect_empty_db_returns_zeroed_snapshot() {
        let conn = fresh();
        let snap = collect(&conn, 30).unwrap();
        assert!(snap.pet.is_none());
        assert!(snap.kills_by_kind.is_empty());
        assert_eq!(snap.top_streak, StreakRecord::default());
        assert_eq!(snap.window_days, 30);
        assert_eq!(snap.zones.len(), 5, "load_all always returns 5 villages");
    }

    #[test]
    fn kills_by_kind_groups_within_window() {
        let conn = fresh();
        insert_kill(&conn, "rust", "boss", 1);
        insert_kill(&conn, "rust", "elite", 1);
        insert_kill(&conn, "rust", "elite", 2);
        insert_kill(&conn, "rust", "zombie", 5);
        // Outside the window (35 days ago)
        insert_kill(&conn, "rust", "boss", 35);

        let out = kills_by_kind(&conn, 30).unwrap();
        assert_eq!(out.get("boss"), Some(&1), "35-day-old boss excluded");
        assert_eq!(out.get("elite"), Some(&2));
        assert_eq!(out.get("zombie"), Some(&1));
    }

    #[test]
    fn kills_by_kind_respects_days_parameter() {
        let conn = fresh();
        insert_kill(&conn, "rust", "boss", 1);
        insert_kill(&conn, "rust", "boss", 6);
        insert_kill(&conn, "rust", "boss", 20);

        let out7 = kills_by_kind(&conn, 7).unwrap();
        assert_eq!(out7.get("boss"), Some(&2), "only last 7 days counted");
        let out30 = kills_by_kind(&conn, 30).unwrap();
        assert_eq!(out30.get("boss"), Some(&3));
    }

    #[test]
    fn longest_streak_detects_consecutive_same_zone_same_kind() {
        let conn = fresh();
        // 5 boss kills in rust, consecutive (ordered by occurred_at)
        // Insert from oldest to newest so occurred_at ordering matches insert order.
        for d in (1..=5).rev() {
            insert_kill(&conn, "rust", "boss", d);
        }
        // A python elite in between would break the streak; skip — this is
        // the no-break case.
        let out = longest_streak(&conn, 30).unwrap();
        assert_eq!(out.zone_id, "rust");
        assert_eq!(out.mob_kind, "boss");
        assert_eq!(out.count, 5);
    }

    #[test]
    fn longest_streak_breaks_on_zone_change() {
        let conn = fresh();
        // rust, rust, python, python, rust — streak = 2 (rust-rust or python-python)
        insert_kill(&conn, "rust", "boss", 5);
        insert_kill(&conn, "rust", "boss", 4);
        insert_kill(&conn, "python", "boss", 3);
        insert_kill(&conn, "python", "boss", 2);
        insert_kill(&conn, "rust", "boss", 1);
        let out = longest_streak(&conn, 30).unwrap();
        assert_eq!(out.count, 2, "any same-zone pair caps at 2");
    }

    #[test]
    fn longest_streak_breaks_on_kind_change() {
        let conn = fresh();
        insert_kill(&conn, "rust", "boss", 5);
        insert_kill(&conn, "rust", "boss", 4);
        insert_kill(&conn, "rust", "elite", 3);
        insert_kill(&conn, "rust", "boss", 2);
        let out = longest_streak(&conn, 30).unwrap();
        assert_eq!(out.count, 2);
        assert_eq!(out.mob_kind, "boss");
    }

    #[test]
    fn longest_streak_on_empty_log_returns_default() {
        let conn = fresh();
        let out = longest_streak(&conn, 30).unwrap();
        assert_eq!(out, StreakRecord::default());
        assert_eq!(out.count, 0);
    }

    #[test]
    fn longest_streak_respects_window() {
        let conn = fresh();
        // 4 boss kills — 2 inside 7d window, 2 outside
        insert_kill(&conn, "rust", "boss", 10);
        insert_kill(&conn, "rust", "boss", 9);
        insert_kill(&conn, "rust", "boss", 2);
        insert_kill(&conn, "rust", "boss", 1);
        let out = longest_streak(&conn, 7).unwrap();
        assert_eq!(out.count, 2, "only in-window kills contribute to the streak");
    }

    #[test]
    fn collect_with_seeded_data_populates_fields() {
        let conn = fresh();
        // Seed a pet so PetHeader is Some
        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next,
                              atk, hp, def, sup, ver)
             VALUES (1, 'rust', 'Ferris', 7, 0, 100, 12, 80, 10, 10, 10)",
            [],
        )
        .unwrap();
        insert_kill(&conn, "rust", "boss", 3);
        insert_kill(&conn, "rust", "elite", 2);

        let snap = collect(&conn, 30).unwrap();
        assert!(snap.pet.is_some());
        let pet = snap.pet.unwrap();
        assert_eq!(pet.name, "Ferris");
        assert_eq!(pet.village_id, "rust");
        assert_eq!(pet.level, 7);
        assert_eq!(snap.kills_by_kind.get("boss"), Some(&1));
        assert_eq!(snap.kills_by_kind.get("elite"), Some(&1));
        assert!(!snap.generated_date.is_empty());
    }
}
