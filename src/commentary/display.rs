//! Phase 3c P5 — display helpers for statusline + TUI.
//!
//! Pure-ish read paths. Given an open DB and a clock, return what the UI
//! should show. Separated from `cli/statusline.rs` and `tui/panels/` so
//! both consumers share one consistent source of truth (latest feed row,
//! 1-hour freshness window, deterministic 5% roll).

use super::Kind;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

/// How long after emission a commentary stays eligible to appear on the
/// statusline bubble. 1 hour lines up with the global rate-limit window —
/// a phrase is "current" for exactly as long as the next one takes to
/// qualify for emission.
pub const STATUSLINE_FRESHNESS_SECS: i64 = 3600;

/// Statusline bubble integration. Returns `Some(phrase)` iff:
///   - The 5% deterministic roll on `now` lands on "show"
///   - The feed has at least one row within `STATUSLINE_FRESHNESS_SECS`
///
/// The caller overlays this on top of the existing `message` fallback —
/// i.e. explicit stdin "message" wins, then commentary (here), then
/// `last_message` (daemon combat narration).
pub fn statusline_override(conn: &Connection, now: i64) -> Result<Option<String>> {
    if !should_show(now) {
        return Ok(None);
    }
    let latest = conn
        .query_row(
            "SELECT phrase FROM commentary_feed
             WHERE created_at >= ?1
             ORDER BY id DESC
             LIMIT 1",
            rusqlite::params![now - STATUSLINE_FRESHNESS_SECS],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(latest)
}

/// Splitmix-style mix of `now` into a 1-in-20 bucket. Deterministic, so a
/// statusline call at a given second always returns the same choice across
/// processes — but adjacent seconds don't cluster (pure `now % 20` would
/// miss whenever the user refreshes at the "wrong" second).
pub fn should_show(now: i64) -> bool {
    let mut x = (now as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x.is_multiple_of(20)
}

/// One row from the commentary feed, stripped for TUI consumption. Unlike
/// `FeedRow` we don't need the hash or internal IDs — the TUI only shows
/// time + phrase, with the `kind` available for an icon badge if desired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentaryEntry {
    pub phrase: String,
    pub created_at: i64,
    pub kind: Kind,
}

/// Load the `max` most recent commentary rows (newest-first) for the TUI
/// panel. Empty on fresh install, never panics — statusline/TUI render
/// paths must stay cheap and correct even on an uninitialised DB.
pub fn recent_commentary(conn: &Connection, max: usize) -> Result<Vec<CommentaryEntry>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT phrase, created_at, kind FROM commentary_feed
         ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![max as i64], |r| {
        let kind_s: String = r.get(2)?;
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            kind_s,
        ))
    })?;
    let mut out = Vec::with_capacity(max);
    for row in rows {
        let (phrase, created_at, kind_s) = row?;
        let kind = Kind::from_str(&kind_s).unwrap_or(Kind::ManualTest);
        out.push(CommentaryEntry { phrase, created_at, kind });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::{phrase_hash_hex, repo::InsertFeed, Source, Tone};
    use rusqlite::Connection;

    fn open_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        crate::db::migrations::run(&conn).expect("migrations");
        conn
    }

    #[test]
    fn should_show_returns_true_for_known_hit_seeds() {
        // Probe a small range so we're sure the function isn't always
        // false (e.g. if someone swaps the constants to something that
        // breaks the modulo distribution).
        let hits = (0..400).filter(|&n| should_show(n)).count();
        assert!(hits > 0, "expected at least one hit in 400 seeds");
        assert!(hits < 400, "not every seed should hit");
    }

    #[test]
    fn should_show_is_deterministic() {
        // Same seed → same answer, always.
        for n in [0_i64, 42, 1_000_000, 1_700_000_000] {
            assert_eq!(should_show(n), should_show(n));
        }
    }

    #[test]
    fn should_show_probability_roughly_5_percent() {
        // Not a tight statistical claim — just a sanity band for the
        // splitmix mixing. Across 10k samples we expect ~500 hits; accept
        // 250..800 so the test won't flake on edge-case distributions.
        let hits = (0..10_000).filter(|&n| should_show(n as i64)).count();
        assert!(
            (250..=800).contains(&hits),
            "expected ~500 hits in 10k seeds; got {hits}"
        );
    }

    #[test]
    fn statusline_override_empty_feed_returns_none() {
        let conn = open_db();
        // Force a "show" seed.
        let now = find_show_seed();
        assert!(statusline_override(&conn, now).unwrap().is_none());
    }

    #[test]
    fn statusline_override_picks_latest_in_window() {
        let conn = open_db();
        let tone = Tone::new("rust", "explorer");
        // Insert older (2h ago — out of window) and newer (10min ago) rows.
        let now = find_show_seed();
        let old_hash = phrase_hash_hex("old phrase");
        crate::commentary::repo::insert_feed(
            &conn,
            InsertFeed {
                created_at: now - 7200,
                tick_count: 1,
                kind: Kind::BossKill,
                tone: &tone,
                phrase: "old phrase",
                phrase_hash: &old_hash,
                source: Source::Rule,
            },
        )
        .unwrap();
        let new_hash = phrase_hash_hex("new phrase");
        crate::commentary::repo::insert_feed(
            &conn,
            InsertFeed {
                created_at: now - 600,
                tick_count: 2,
                kind: Kind::LevelUp,
                tone: &tone,
                phrase: "new phrase",
                phrase_hash: &new_hash,
                source: Source::Rule,
            },
        )
        .unwrap();
        let got = statusline_override(&conn, now).unwrap();
        assert_eq!(got.as_deref(), Some("new phrase"));
    }

    #[test]
    fn statusline_override_excludes_stale_row() {
        let conn = open_db();
        let tone = Tone::new("rust", "explorer");
        let now = find_show_seed();
        let hash = phrase_hash_hex("old phrase");
        // 70 min ago — outside the 1h window.
        crate::commentary::repo::insert_feed(
            &conn,
            InsertFeed {
                created_at: now - 4200,
                tick_count: 1,
                kind: Kind::BossKill,
                tone: &tone,
                phrase: "old phrase",
                phrase_hash: &hash,
                source: Source::Rule,
            },
        )
        .unwrap();
        assert!(statusline_override(&conn, now).unwrap().is_none());
    }

    #[test]
    fn statusline_override_returns_none_on_miss_roll() {
        // Seed where should_show() == false: any seed not in the hit set.
        let miss = find_miss_seed();
        assert!(!should_show(miss));
        let conn = open_db();
        let tone = Tone::new("rust", "explorer");
        let hash = phrase_hash_hex("fresh phrase");
        crate::commentary::repo::insert_feed(
            &conn,
            InsertFeed {
                created_at: miss,
                tick_count: 1,
                kind: Kind::BossKill,
                tone: &tone,
                phrase: "fresh phrase",
                phrase_hash: &hash,
                source: Source::Rule,
            },
        )
        .unwrap();
        assert!(statusline_override(&conn, miss).unwrap().is_none());
    }

    #[test]
    fn recent_commentary_orders_newest_first() {
        let conn = open_db();
        let tone = Tone::new("rust", "aggressive");
        for (i, p) in ["a", "b", "c"].iter().enumerate() {
            let h = phrase_hash_hex(p);
            crate::commentary::repo::insert_feed(
                &conn,
                InsertFeed {
                    created_at: 1000 + i as i64,
                    tick_count: i as u64,
                    kind: Kind::BossKill,
                    tone: &tone,
                    phrase: p,
                    phrase_hash: &h,
                    source: Source::Rule,
                },
            )
            .unwrap();
        }
        let rows = recent_commentary(&conn, 5).unwrap();
        let phrases: Vec<_> = rows.iter().map(|r| r.phrase.as_str()).collect();
        assert_eq!(phrases, vec!["c", "b", "a"]);
        // kind round-trips through the TEXT column.
        assert_eq!(rows[0].kind, Kind::BossKill);
    }

    #[test]
    fn recent_commentary_max_zero_returns_empty() {
        let conn = open_db();
        assert!(recent_commentary(&conn, 0).unwrap().is_empty());
    }

    // Probe helpers — find first seed where should_show is {true,false}
    // so the other tests don't have to hard-code magic numbers that might
    // drift if the mixing constants ever change.
    fn find_show_seed() -> i64 {
        (0..1_000_000).find(|&n| should_show(n)).expect("at least one hit in 1M seeds")
    }
    fn find_miss_seed() -> i64 {
        (0..1_000_000).find(|&n| !should_show(n)).expect("at least one miss in 1M seeds")
    }
}
