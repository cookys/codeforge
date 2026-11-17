//! Phase 3c P5 — display helpers for statusline + TUI.
//!
//! Pure-ish read paths. Given an open DB and a clock, return what the UI
//! should show. Separated from `cli/statusline.rs` and `tui/panels/` so
//! both consumers share one consistent source of truth (latest feed row,
//! 1-hour freshness window, deterministic 5% roll).

use super::Kind;
use anyhow::Result;
use rusqlite::Connection;

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
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, kind_s))
    })?;
    let mut out = Vec::with_capacity(max);
    for row in rows {
        let (phrase, created_at, kind_s) = row?;
        let kind = Kind::from_str(&kind_s).unwrap_or(Kind::ManualTest);
        out.push(CommentaryEntry {
            phrase,
            created_at,
            kind,
        });
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
}
