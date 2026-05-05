//! Phase 3a — World state aggregation.
//!
//! One module per-concept:
//!   * `ZoneRank` — Traveler / Forger / Iron Crafter / Veteran (spec §3.3)
//!   * `rank_for(kill_count)` — pure function, no persistence (spec: "純 aggregation")
//!   * `ZoneStats` — one row view over `game_world` + computed rank
//!   * `load_all` — read side for CLI + TUI
//!   * `refresh_from_l1` — write side: L1 distribution → game_world upsert

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

use crate::memory::l1_stats::language_distribution;
use crate::pet::village::VILLAGES;

/// Minimum concept mentions in the L1 store before a non-home Zone unlocks.
/// Three avoids unlocking on a single passing mention while still letting
/// any pair of paired concepts + one follow-up trip the threshold.
pub const UNLOCK_THRESHOLD: u32 = 3;

/// Zone Mastery ranks — spec §3.3 display thresholds. `as_str` is the
/// canonical localised name (繁中台灣用語) for statusline / world map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneRank {
    Traveler,
    Forger,
    IronCrafter,
    Veteran,
}

impl ZoneRank {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Traveler => "Traveler",
            Self::Forger => "Forger",
            Self::IronCrafter => "Iron Crafter",
            Self::Veteran => "Veteran",
        }
    }

    /// Decorative marker shown next to the rank on the world map.
    /// Traveler is unmarked; Forger → `✦`, Iron Crafter → `✦`, Veteran → `✦✦`.
    /// Kept subtle — the rank text already carries the signal.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Traveler => "",
            Self::Forger => "",
            Self::IronCrafter => "✦",
            Self::Veteran => "✦✦",
        }
    }
}

/// Spec §3.3 threshold table.
pub fn rank_for(kill_count: u32) -> ZoneRank {
    match kill_count {
        0..=49 => ZoneRank::Traveler,
        50..=199 => ZoneRank::Forger,
        200..=499 => ZoneRank::IronCrafter,
        _ => ZoneRank::Veteran,
    }
}

/// Composed view of one Zone. Read-only; callers do not mutate.
#[derive(Debug, Clone)]
pub struct ZoneStats {
    pub village_id: String,
    pub unlocked: bool,
    pub kill_count: u32,
    pub concept_count: u32,
    pub rank: ZoneRank,
}

impl ZoneStats {
    fn from_row(village_id: String, unlocked: bool, kill_count: u32, concept_count: u32) -> Self {
        let rank = rank_for(kill_count);
        Self {
            village_id,
            unlocked,
            kill_count,
            concept_count,
            rank,
        }
    }
}

/// Load a `ZoneStats` for every known village. Villages missing from
/// `game_world` surface as locked zero-stat rows so the world map can
/// render a deterministic 5-village layout regardless of daemon state.
pub fn load_all(conn: &Connection) -> Result<Vec<ZoneStats>> {
    let mut out = Vec::with_capacity(VILLAGES.len());
    for village in VILLAGES {
        let row: Option<(i64, i64, i64)> = conn
            .query_row(
                "SELECT unlocked, kill_count, concept_count
                 FROM game_world WHERE zone_id = ?1",
                rusqlite::params![village.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let (unlocked, kill_count, concept_count) = match row {
            Some((u, k, c)) => (u != 0, k.max(0) as u32, c.max(0) as u32),
            None => (false, 0, 0),
        };
        out.push(ZoneStats::from_row(
            village.id.to_string(),
            unlocked,
            kill_count,
            concept_count,
        ));
    }
    Ok(out)
}

/// Write side: scan L1, update concept_count per village, flip unlocked
/// on when either (a) it's the home village or (b) concept_count crosses
/// `UNLOCK_THRESHOLD`.
///
/// Home-village unlock is sticky — once flipped, never reverts, even if
/// concept count later drops (should not happen in practice; defensive
/// against concept pruning in Phase 3f dream cycles).
///
/// `store_dir` is typically `ctx.project_dir.join("store")`.
pub fn refresh_from_l1(conn: &Connection, store_dir: &Path, home_village: &str) -> Result<()> {
    let distribution = language_distribution(store_dir)?;

    for village in VILLAGES {
        let concept_count = distribution.get(village.id).copied().unwrap_or(0);
        let is_home = village.id == home_village;
        let should_unlock = is_home || concept_count >= UNLOCK_THRESHOLD;

        conn.execute(
            "INSERT INTO game_world (zone_id, unlocked, concept_count)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(zone_id) DO UPDATE SET
                 concept_count = excluded.concept_count,
                 -- Sticky unlock: OR with the prior value so a drop in
                 -- concept_count can't relock a zone the player already
                 -- visited. Home zone is forced via the CASE branch.
                 unlocked = CASE WHEN ?4 = 1 THEN 1
                                 ELSE MAX(game_world.unlocked, excluded.unlocked)
                            END",
            rusqlite::params![
                village.id,
                if should_unlock { 1_i64 } else { 0 },
                concept_count as i64,
                if is_home { 1_i64 } else { 0 },
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use tempfile::TempDir;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn write_concept(root: &Path, name: &str, body: &str) {
        let concepts = root.join("concepts");
        fs::create_dir_all(&concepts).unwrap();
        fs::write(concepts.join(format!("{name}.md")), body).unwrap();
    }

    #[test]
    fn rank_boundaries_match_spec() {
        assert_eq!(rank_for(0), ZoneRank::Traveler);
        assert_eq!(rank_for(49), ZoneRank::Traveler);
        assert_eq!(rank_for(50), ZoneRank::Forger);
        assert_eq!(rank_for(199), ZoneRank::Forger);
        assert_eq!(rank_for(200), ZoneRank::IronCrafter);
        assert_eq!(rank_for(499), ZoneRank::IronCrafter);
        assert_eq!(rank_for(500), ZoneRank::Veteran);
        assert_eq!(rank_for(10_000), ZoneRank::Veteran);
    }

    #[test]
    fn rank_glyphs_escalate_with_tier() {
        assert_eq!(ZoneRank::Traveler.glyph(), "");
        assert_eq!(ZoneRank::Forger.glyph(), "");
        assert_eq!(ZoneRank::IronCrafter.glyph(), "✦");
        assert_eq!(ZoneRank::Veteran.glyph(), "✦✦");
    }

    #[test]
    fn load_all_returns_five_village_rows_on_empty_db() {
        let conn = fresh();
        let stats = load_all(&conn).unwrap();
        assert_eq!(stats.len(), 5);
        for s in &stats {
            assert!(!s.unlocked);
            assert_eq!(s.kill_count, 0);
            assert_eq!(s.concept_count, 0);
            assert_eq!(s.rank, ZoneRank::Traveler);
        }
    }

    #[test]
    fn refresh_from_l1_unlocks_home_village_with_zero_concepts() {
        // Home village unlocks even when the L1 store is empty.
        let conn = fresh();
        let store = TempDir::new().unwrap();
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        let stats = load_all(&conn).unwrap();
        let rust = stats.iter().find(|s| s.village_id == "rust").unwrap();
        assert!(rust.unlocked);
        assert_eq!(rust.concept_count, 0);
        // Other zones stay locked.
        let python = stats.iter().find(|s| s.village_id == "python").unwrap();
        assert!(!python.unlocked);
    }

    #[test]
    fn refresh_from_l1_unlocks_on_threshold_boundary() {
        let conn = fresh();
        let store = TempDir::new().unwrap();
        // 3 python concepts → should unlock (UNLOCK_THRESHOLD=3)
        for i in 0..3 {
            write_concept(store.path(), &format!("py-{i}"), "pip install");
        }
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        let stats = load_all(&conn).unwrap();
        let py = stats.iter().find(|s| s.village_id == "python").unwrap();
        assert!(py.unlocked, "3 concepts must unlock python");
        assert_eq!(py.concept_count, 3);
    }

    #[test]
    fn refresh_from_l1_leaves_below_threshold_locked() {
        let conn = fresh();
        let store = TempDir::new().unwrap();
        // 2 python concepts — below threshold
        for i in 0..2 {
            write_concept(store.path(), &format!("py-{i}"), "pip install");
        }
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        let stats = load_all(&conn).unwrap();
        let py = stats.iter().find(|s| s.village_id == "python").unwrap();
        assert!(!py.unlocked);
        assert_eq!(py.concept_count, 2);
    }

    #[test]
    fn refresh_from_l1_unlock_is_sticky_across_concept_drop() {
        let conn = fresh();
        let store = TempDir::new().unwrap();
        // First refresh: 3 python concepts → unlock
        for i in 0..3 {
            write_concept(store.path(), &format!("py-{i}"), "pip install");
        }
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        assert!(
            load_all(&conn)
                .unwrap()
                .iter()
                .find(|s| s.village_id == "python")
                .unwrap()
                .unlocked
        );

        // Now delete the concepts and refresh — unlock must persist.
        for i in 0..3 {
            let p = store.path().join("concepts").join(format!("py-{i}.md"));
            fs::remove_file(&p).unwrap();
        }
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        let py = load_all(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.village_id == "python")
            .unwrap();
        assert!(py.unlocked, "unlock must not revert on concept drop");
        assert_eq!(py.concept_count, 0);
    }

    #[test]
    fn refresh_from_l1_preserves_existing_kill_count() {
        let conn = fresh();
        // Pre-seed game_world with kill data from prior combat.
        conn.execute(
            "INSERT INTO game_world (zone_id, unlocked, kill_count, concept_count)
             VALUES ('rust', 1, 55, 0)",
            [],
        )
        .unwrap();
        let store = TempDir::new().unwrap();
        refresh_from_l1(&conn, store.path(), "rust").unwrap();

        // kill_count untouched by refresh.
        let rust = load_all(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.village_id == "rust")
            .unwrap();
        assert_eq!(rust.kill_count, 55);
        assert_eq!(rust.rank, ZoneRank::Forger);
        assert!(rust.unlocked);
    }

    #[test]
    fn load_all_reports_mastery_rank_from_kill_count() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO game_world (zone_id, unlocked, kill_count) VALUES ('rust', 1, 250)",
            [],
        )
        .unwrap();
        let rust = load_all(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.village_id == "rust")
            .unwrap();
        assert_eq!(rust.rank, ZoneRank::IronCrafter);
    }

    #[test]
    fn refresh_from_l1_is_idempotent() {
        let conn = fresh();
        let store = TempDir::new().unwrap();
        write_concept(store.path(), "a", "rust");
        write_concept(store.path(), "b", "rust");
        write_concept(store.path(), "c", "rust");
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        refresh_from_l1(&conn, store.path(), "rust").unwrap();
        let rust = load_all(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.village_id == "rust")
            .unwrap();
        assert_eq!(rust.concept_count, 3);
        assert!(rust.unlocked);
    }
}
