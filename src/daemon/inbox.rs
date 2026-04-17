//! Event inbox drain — P4 of Phase 2a.
//!
//! Hooks INSERT rows into `event_inbox` via the `codeforge emit` CLI.
//! The daemon runs this drain loop every ~500ms:
//!   1. SELECT rows WHERE seen_at IS NULL
//!   2. Process each payload (dispatched to game engine — stub in P4)
//!   3. UPDATE seen_at on processed rows
//!
//! Retention: rows older than 7 days (by created_at) and already seen are
//! purged during startup cleanup. Inbox stays bounded for active users.

use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tokio::time::interval;

pub const DRAIN_INTERVAL_MS: u64 = 500;
pub const RETENTION_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

/// One drained event, as parsed from the inbox.
#[derive(Debug, Clone)]
pub struct InboxEvent {
    pub id: i64,
    pub payload: String,
    pub created_at: i64,
}

/// Drain all unseen rows. Returns the processed events (caller dispatches).
/// After this call, processed rows have seen_at set to now.
pub fn drain_once(conn: &Connection) -> Result<Vec<InboxEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, payload, created_at FROM event_inbox
         WHERE seen_at IS NULL ORDER BY id ASC",
    )?;
    let rows: Vec<InboxEvent> = stmt
        .query_map([], |r| {
            Ok(InboxEvent {
                id: r.get(0)?,
                payload: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if rows.is_empty() {
        return Ok(rows);
    }

    let now = now_unix_secs() as i64;
    let placeholders: Vec<String> = (0..rows.len()).map(|i| format!("?{}", i + 2)).collect();
    let sql = format!(
        "UPDATE event_inbox SET seen_at = ?1 WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
    for ev in &rows {
        params.push(Box::new(ev.id));
    }
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_refs.as_slice())?;

    Ok(rows)
}

/// Delete rows older than `retention_secs` seconds AND already processed.
/// Unseen rows are never deleted — daemon-down durability guarantee.
pub fn cleanup_expired(conn: &Connection, retention_secs: u64) -> Result<usize> {
    let cutoff = (now_unix_secs().saturating_sub(retention_secs)) as i64;
    let n = conn.execute(
        "DELETE FROM event_inbox
         WHERE seen_at IS NOT NULL AND created_at < ?1",
        rusqlite::params![cutoff],
    )?;
    Ok(n)
}

/// Run the drain loop until shutdown. Opens its own connection so it can run
/// concurrently with the tick loop (SQLite WAL + busy_timeout handles write
/// serialization at the SQLite layer).
pub async fn run_drain_loop(
    conn: &Connection,
    poll_interval: Duration,
    shutdown: Arc<Notify>,
) -> Result<()> {
    // Opportunistic cleanup on entry
    let _ = cleanup_expired(conn, RETENTION_SECS);

    let mut ticker = interval(poll_interval);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let events = drain_once(conn)?;
                for _ev in events {
                    // P4 scope: drain only. P3+ will dispatch to game engine.
                    // Keeping the channel plumbed end-to-end even with a no-op
                    // handler is deliberate — proves the data path.
                }
            }
            _ = shutdown.notified() => return Ok(()),
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn insert_event(conn: &Connection, payload: &str, created_at: i64) {
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, ?2)",
            rusqlite::params![payload, created_at],
        )
        .unwrap();
    }

    #[test]
    fn drain_empty_is_noop() {
        let conn = fresh_conn();
        let events = drain_once(&conn).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn drain_returns_unseen_in_id_order() {
        let conn = fresh_conn();
        insert_event(&conn, "{\"event\":\"a\"}", 100);
        insert_event(&conn, "{\"event\":\"b\"}", 101);
        insert_event(&conn, "{\"event\":\"c\"}", 102);

        let events = drain_once(&conn).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].payload, "{\"event\":\"a\"}");
        assert_eq!(events[2].payload, "{\"event\":\"c\"}");
        // ids should be monotonic
        assert!(events[0].id < events[1].id);
        assert!(events[1].id < events[2].id);
    }

    #[test]
    fn drained_rows_get_seen_at() {
        let conn = fresh_conn();
        insert_event(&conn, "{\"event\":\"x\"}", 100);
        drain_once(&conn).unwrap();

        let seen_at: Option<i64> = conn
            .query_row(
                "SELECT seen_at FROM event_inbox WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(seen_at.is_some());
    }

    #[test]
    fn second_drain_skips_already_seen() {
        let conn = fresh_conn();
        insert_event(&conn, "{\"event\":\"once\"}", 100);
        let first = drain_once(&conn).unwrap();
        assert_eq!(first.len(), 1);

        let second = drain_once(&conn).unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn drain_batches_hundreds() {
        // Dynamic SQL with N placeholders — stress-test a realistic burst.
        let conn = fresh_conn();
        for i in 0..500 {
            insert_event(&conn, &format!("{{\"n\":{}}}", i), 1000 + i);
        }
        let events = drain_once(&conn).unwrap();
        assert_eq!(events.len(), 500);

        let unseen: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM event_inbox WHERE seen_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unseen, 0);
    }

    #[test]
    fn cleanup_only_deletes_seen_and_old() {
        let conn = fresh_conn();
        let now = now_unix_secs() as i64;
        let long_ago = now - (8 * 24 * 60 * 60); // 8 days ago
        let recent = now - 60;

        // 1. old + seen → should delete
        insert_event(&conn, "old-seen", long_ago);
        conn.execute(
            "UPDATE event_inbox SET seen_at = ?1 WHERE id = 1",
            rusqlite::params![long_ago + 1],
        )
        .unwrap();

        // 2. old + unseen → must preserve (daemon-down durability)
        insert_event(&conn, "old-unseen", long_ago);

        // 3. recent + seen → preserve
        insert_event(&conn, "recent-seen", recent);
        conn.execute(
            "UPDATE event_inbox SET seen_at = ?1 WHERE id = 3",
            rusqlite::params![recent + 1],
        )
        .unwrap();

        // 4. recent + unseen → preserve
        insert_event(&conn, "recent-unseen", recent);

        let deleted = cleanup_expired(&conn, RETENTION_SECS).unwrap();
        assert_eq!(deleted, 1);

        let remaining: Vec<String> = conn
            .prepare("SELECT payload FROM event_inbox ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            remaining,
            vec!["old-unseen", "recent-seen", "recent-unseen"]
        );
    }

    #[tokio::test]
    async fn drain_loop_honors_shutdown() {
        let conn = fresh_conn();
        insert_event(&conn, "{\"event\":\"pending\"}", 100);

        let shutdown = Arc::new(Notify::new());
        let trigger = shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            trigger.notify_one();
        });

        run_drain_loop(&conn, Duration::from_millis(5), shutdown)
            .await
            .unwrap();

        // Event should have been drained within the ~30ms window
        let seen: Option<i64> = conn
            .query_row(
                "SELECT seen_at FROM event_inbox WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(seen.is_some(), "drain loop should have processed the event");
    }
}
