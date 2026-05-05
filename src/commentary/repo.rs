//! Phase 3c repo — CRUD for the three commentary tables.
//!
//! All writes expect to run inside the daemon's per-tick transaction so they
//! atomically commit with the rest of tick state. Reads (`recent_feed`,
//! `seen_within`, `budget_last_emit`) take a plain `&Connection` and may be
//! called from the read-only CLI path.

use super::{FeedRow, Kind, Source, Tone};
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// Parameters for a new feed insertion. Keeps the call sites compact.
#[derive(Debug, Clone)]
pub struct InsertFeed<'a> {
    pub created_at: i64,
    pub tick_count: u64,
    pub kind: Kind,
    pub tone: &'a Tone,
    pub phrase: &'a str,
    pub phrase_hash: &'a str,
    pub source: Source,
}

/// Append a new row to `commentary_feed`. Returns the autoincremented id.
pub fn insert_feed(conn: &Connection, row: InsertFeed<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO commentary_feed
             (created_at, tick_count, kind, tone, phrase, phrase_hash, source, displayed_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![
            row.created_at,
            row.tick_count as i64,
            row.kind.as_str(),
            row.tone.composite(),
            row.phrase,
            row.phrase_hash,
            row.source.as_str(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Most-recent N rows from the feed, ordered newest-first.
pub fn recent_feed(conn: &Connection, limit: u32) -> Result<Vec<FeedRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at, tick_count, kind, tone, phrase, phrase_hash,
                source, displayed_count
         FROM commentary_feed
         ORDER BY id DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        let kind_s: String = r.get(3)?;
        let source_s: String = r.get(7)?;
        Ok(FeedRowRaw {
            id: r.get(0)?,
            created_at: r.get(1)?,
            tick_count: r.get::<_, i64>(2)? as u64,
            kind: kind_s,
            tone: r.get(4)?,
            phrase: r.get(5)?,
            phrase_hash: r.get(6)?,
            source: source_s,
            displayed_count: r.get::<_, i64>(8)? as u32,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let raw = row?;
        out.push(feed_row_from_raw(raw)?);
    }
    Ok(out)
}

/// Has this phrase (hash + kind pair) been used within the last `window_seconds`?
/// A match returns `true` and the caller is expected to pick an alternate
/// phrase (or skip emission entirely, depending on strategy).
pub fn seen_within(
    conn: &Connection,
    phrase_hash: &str,
    kind: Kind,
    now: i64,
    window_seconds: i64,
) -> Result<bool> {
    let threshold = now.saturating_sub(window_seconds);
    let last: Option<i64> = conn
        .query_row(
            "SELECT last_used_at FROM commentary_history
             WHERE phrase_hash = ?1 AND kind = ?2",
            params![phrase_hash, kind.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(matches!(last, Some(t) if t >= threshold))
}

/// Record a phrase emission in history. First-time insertions seed
/// `first_used_at`; repeats bump `last_used_at` + increment `use_count`.
pub fn record_history(conn: &Connection, phrase_hash: &str, kind: Kind, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO commentary_history
             (phrase_hash, kind, first_used_at, last_used_at, use_count)
         VALUES (?1, ?2, ?3, ?3, 1)
         ON CONFLICT(phrase_hash, kind) DO UPDATE SET
             last_used_at = excluded.last_used_at,
             use_count    = commentary_history.use_count + 1",
        params![phrase_hash, kind.as_str(), now],
    )?;
    Ok(())
}

/// Budget read: last successful emission time, or `None` if never emitted.
pub fn budget_last_emit(conn: &Connection) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT last_emit_at FROM commentary_budget WHERE id = 1",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )
    .map_err(|e| anyhow!("commentary_budget read failed: {e}"))
}

/// Budget write: reserve `now` as the new last-emit timestamp. Returns the
/// prior value so the caller can `unreserve` on LLM failure.
pub fn budget_reserve(conn: &Connection, now: i64) -> Result<Option<i64>> {
    let prev = budget_last_emit(conn)?;
    conn.execute(
        "UPDATE commentary_budget SET last_emit_at = ?1, consecutive_skip = 0 WHERE id = 1",
        params![now],
    )?;
    Ok(prev)
}

/// Restore the prior `last_emit_at`. Kept despite the optimistic-reserve
/// design (no current caller) because:
///   (a) repo_reserve's return value is documented as "prior for unreserve",
///   (b) tests exercise it to verify the round-trip,
///   (c) a future Haiku-retry policy may want to unreserve on double-failure.
/// Audit target for Phase 3f+; remove if still unused then.
#[allow(dead_code)]
pub fn budget_unreserve(conn: &Connection, prev: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE commentary_budget SET last_emit_at = ?1 WHERE id = 1",
        params![prev],
    )?;
    Ok(())
}

/// Increment the skip telemetry counter. Called when a trigger fired but
/// the budget rate-limited it.
pub fn budget_incr_skip(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE commentary_budget SET consecutive_skip = consecutive_skip + 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// Settings flag read. Returns `true` iff `commentary_opt_in = '1'`. Anything
/// else (NULL, '0', malformed) is treated as off.
pub fn opt_in_flag(conn: &Connection) -> Result<bool> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'commentary_opt_in'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(matches!(v.as_deref(), Some("1")))
}

/// Settings flag write.
pub fn set_opt_in_flag(conn: &Connection, enabled: bool) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('commentary_opt_in', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                         updated_at = datetime('now')",
        params![if enabled { "1" } else { "0" }],
    )?;
    Ok(())
}

// ── internals ───────────────────────────────────────────────────────

struct FeedRowRaw {
    id: i64,
    created_at: i64,
    tick_count: u64,
    kind: String,
    tone: String,
    phrase: String,
    phrase_hash: String,
    source: String,
    displayed_count: u32,
}

fn feed_row_from_raw(raw: FeedRowRaw) -> Result<FeedRow> {
    let kind = Kind::from_str(&raw.kind)
        .ok_or_else(|| anyhow!("unknown commentary kind in DB: {}", raw.kind))?;
    let source = Source::from_str(&raw.source)
        .ok_or_else(|| anyhow!("unknown commentary source in DB: {}", raw.source))?;
    Ok(FeedRow {
        id: Some(raw.id),
        created_at: raw.created_at,
        tick_count: raw.tick_count,
        kind,
        tone: raw.tone,
        phrase: raw.phrase,
        phrase_hash: raw.phrase_hash,
        source,
        displayed_count: raw.displayed_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::{phrase_hash_hex, Tone};
    use rusqlite::Connection;

    fn open_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        crate::db::migrations::run(&conn).expect("migrations");
        conn
    }

    #[test]
    fn insert_and_recent_roundtrip() {
        let conn = open_db();
        let tone = Tone::new("rust", "aggressive");
        let phrase = "好耶";
        let hash = phrase_hash_hex(phrase);
        let id = insert_feed(
            &conn,
            InsertFeed {
                created_at: 1000,
                tick_count: 42,
                kind: Kind::BossKill,
                tone: &tone,
                phrase,
                phrase_hash: &hash,
                source: Source::Rule,
            },
        )
        .unwrap();
        assert!(id > 0);

        let rows = recent_feed(&conn, 5).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.id, Some(id));
        assert_eq!(r.kind, Kind::BossKill);
        assert_eq!(r.source, Source::Rule);
        assert_eq!(r.phrase, phrase);
        assert_eq!(r.phrase_hash, hash);
        assert_eq!(r.tone, "rust:aggressive");
        assert_eq!(r.tick_count, 42);
    }

    #[test]
    fn recent_feed_orders_newest_first() {
        let conn = open_db();
        let tone = Tone::new("rust", "explorer");
        for (i, p) in ["a", "b", "c"].iter().enumerate() {
            let hash = phrase_hash_hex(p);
            insert_feed(
                &conn,
                InsertFeed {
                    created_at: 1000 + i as i64,
                    tick_count: i as u64,
                    kind: Kind::LevelUp,
                    tone: &tone,
                    phrase: p,
                    phrase_hash: &hash,
                    source: Source::Rule,
                },
            )
            .unwrap();
        }
        let rows = recent_feed(&conn, 10).unwrap();
        let phrases: Vec<_> = rows.iter().map(|r| r.phrase.as_str()).collect();
        assert_eq!(phrases, vec!["c", "b", "a"]);
    }

    #[test]
    fn seen_within_detects_inside_window_and_ignores_outside() {
        let conn = open_db();
        let hash = phrase_hash_hex("test");
        record_history(&conn, &hash, Kind::BossKill, 1000).unwrap();

        // Same second — should be seen (window > 0)
        assert!(seen_within(&conn, &hash, Kind::BossKill, 1000, 3600).unwrap());
        // Within 1h window
        assert!(seen_within(&conn, &hash, Kind::BossKill, 4000, 3600).unwrap());
        // Just past the window boundary
        assert!(!seen_within(&conn, &hash, Kind::BossKill, 4601, 3600).unwrap());
        // Different kind — same hash, independent bucket
        assert!(!seen_within(&conn, &hash, Kind::LevelUp, 1500, 3600).unwrap());
    }

    #[test]
    fn seen_within_30_day_window_matches_spec() {
        let conn = open_db();
        let hash = phrase_hash_hex("phrase");
        let thirty_days = 30 * 86_400;
        record_history(&conn, &hash, Kind::LevelUp, 0).unwrap();
        assert!(seen_within(&conn, &hash, Kind::LevelUp, thirty_days, thirty_days).unwrap());
        assert!(!seen_within(&conn, &hash, Kind::LevelUp, thirty_days + 1, thirty_days).unwrap());
    }

    #[test]
    fn record_history_increments_use_count_on_repeat() {
        let conn = open_db();
        let hash = phrase_hash_hex("repeat");
        record_history(&conn, &hash, Kind::LongIdle, 1000).unwrap();
        record_history(&conn, &hash, Kind::LongIdle, 2000).unwrap();
        record_history(&conn, &hash, Kind::LongIdle, 3000).unwrap();
        let (first, last, count): (i64, i64, i64) = conn
            .query_row(
                "SELECT first_used_at, last_used_at, use_count
                 FROM commentary_history
                 WHERE phrase_hash = ?1 AND kind = 'long_idle'",
                params![hash],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(first, 1000);
        assert_eq!(last, 3000);
        assert_eq!(count, 3);
    }

    #[test]
    fn budget_reserve_and_unreserve_restore_prior() {
        let conn = open_db();
        assert_eq!(budget_last_emit(&conn).unwrap(), None);

        let prev = budget_reserve(&conn, 1000).unwrap();
        assert_eq!(prev, None);
        assert_eq!(budget_last_emit(&conn).unwrap(), Some(1000));

        let prev2 = budget_reserve(&conn, 5000).unwrap();
        assert_eq!(prev2, Some(1000));
        assert_eq!(budget_last_emit(&conn).unwrap(), Some(5000));

        // Simulate LLM failure → unreserve back to 1000
        budget_unreserve(&conn, prev2).unwrap();
        assert_eq!(budget_last_emit(&conn).unwrap(), Some(1000));

        // Unreserve to None is valid
        budget_unreserve(&conn, None).unwrap();
        assert_eq!(budget_last_emit(&conn).unwrap(), None);
    }

    #[test]
    fn budget_skip_counter_increments_and_reserve_resets() {
        let conn = open_db();
        budget_incr_skip(&conn).unwrap();
        budget_incr_skip(&conn).unwrap();
        budget_incr_skip(&conn).unwrap();
        let skips: i64 = conn
            .query_row(
                "SELECT consecutive_skip FROM commentary_budget WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(skips, 3);

        budget_reserve(&conn, 1000).unwrap();
        let skips_after: i64 = conn
            .query_row(
                "SELECT consecutive_skip FROM commentary_budget WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(skips_after, 0, "reserve must reset skip counter");
    }

    #[test]
    fn opt_in_default_is_false() {
        let conn = open_db();
        assert!(!opt_in_flag(&conn).unwrap());
    }

    #[test]
    fn opt_in_set_roundtrip() {
        let conn = open_db();
        set_opt_in_flag(&conn, true).unwrap();
        assert!(opt_in_flag(&conn).unwrap());
        set_opt_in_flag(&conn, false).unwrap();
        assert!(!opt_in_flag(&conn).unwrap());
    }

    #[test]
    fn budget_single_row_enforced() {
        let conn = open_db();
        // CHECK (id = 1) — any other id must error
        let err = conn.execute(
            "INSERT INTO commentary_budget (id, last_emit_at) VALUES (2, 1000)",
            [],
        );
        assert!(
            err.is_err(),
            "CHECK constraint should prevent multi-row budget"
        );
    }
}
