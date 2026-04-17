//! Live read path — P8 of Phase 2a.
//!
//! Statusline and `codeforge pet` pull pet state through this module.
//! It composes two sources:
//!   1. `pet_snapshot` (daemon-authored) — Phase 2a+ authoritative state.
//!      Falls back to Phase 1 `pet` table when no tick has ever run.
//!   2. `event_inbox WHERE seen_at IS NULL` — events queued by hooks or
//!      `pet::xp::award` that haven't been drained yet. Their XP is
//!      overlaid onto the base for real-time display.
//!
//! Why overlay at read time: statusline is invoked fresh on every Claude
//! prompt. Reading `pet_snapshot` alone would show daemon-lag. Overlaying
//! the inbox gives sub-ms reflection of hook events without any IPC wake.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use crate::daemon::events::xp_for_payload;
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;

/// Composed pet state for display.
pub struct LiveState {
    /// Base state with `pending_xp` applied (level-up cascades already ran).
    pub state: PetState,
    /// How many unseen rows are in `event_inbox`. 0 = fully drained.
    /// Exposed for diagnostics (e.g. `codeforge daemon status`) — statusline
    /// does not render it directly, `state` already reflects the overlay.
    #[allow(dead_code)]
    pub pending_events: u32,
    /// Total XP the unseen rows would award on next tick. Same diagnostic
    /// purpose as `pending_events`.
    #[allow(dead_code)]
    pub pending_xp: u32,
    /// Daemon-authored pet speech (e.g. "擊殺 ghost ..."). Statusline uses
    /// this as the speech bubble when the stdin JSON has no "message" field.
    pub last_message: Option<String>,
}

impl LiveState {
    /// Returns true if either `pet_snapshot` or Phase 1 `pet` has a row.
    /// Propagates SQL errors — callers shouldn't conflate "no pet" with
    /// "DB locked / schema missing / IO failure".
    pub fn exists(conn: &Connection) -> Result<bool> {
        let snap: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        if snap > 0 {
            return Ok(true);
        }
        let pet: i64 = conn.query_row("SELECT COUNT(*) FROM pet WHERE id = 1", [], |r| {
            r.get(0)
        })?;
        Ok(pet > 0)
    }

    /// Load the composed live state.
    ///
    /// Returns `Ok(None)` when no pet exists in either source — callers can
    /// then distinguish that from a database error. (Previous behavior was
    /// to return a zero-filled `PetState` whose `xp_to_next=0` hit an
    /// infinite loop on `add_xp`; guarded at that layer now, but making the
    /// absence explicit is cleaner for the two current call sites.)
    pub fn load(conn: &Connection) -> Result<Option<Self>> {
        let Some((mut state, last_message)) = load_base(conn)? else {
            return Ok(None);
        };
        let (pending_events, pending_xp) = sum_unseen_xp(conn)?;
        if pending_xp > 0 {
            state.add_xp(pending_xp);
        }
        Ok(Some(Self {
            state,
            pending_events,
            pending_xp,
            last_message,
        }))
    }
}

/// Prefer daemon snapshot, fall back to Phase 1 pet table.
/// Returns (state, last_message) — last_message only comes from the snapshot.
fn load_base(conn: &Connection) -> Result<Option<(PetState, Option<String>)>> {
    if let Some(pair) = load_from_snapshot(conn)? {
        return Ok(Some(pair));
    }
    Ok(load_from_pet(conn)?.map(|p| (p, None)))
}

fn load_from_snapshot(conn: &Connection) -> Result<Option<(PetState, Option<String>)>> {
    let row = conn
        .query_row(
            "SELECT village, level, hp, xp, xp_to_next, atk, def, sup, ver, last_message
             FROM pet_snapshot WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, u32>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, u32>(6)?,
                    r.get::<_, u32>(7)?,
                    r.get::<_, u32>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(|(village, level, hp, xp, xp_to_next, atk, def, sup, ver, last_message)| {
        let name = VILLAGES
            .iter()
            .find(|v| v.id == village)
            .map(|v| v.pet_name.to_string())
            .unwrap_or_default();
        let state = PetState {
            village,
            name,
            level,
            xp,
            xp_to_next,
            atk,
            hp,
            def,
            sup,
            ver,
        };
        (state, last_message)
    }))
}

fn load_from_pet(conn: &Connection) -> Result<Option<PetState>> {
    if !PetState::exists(conn)? {
        return Ok(None);
    }
    Ok(Some(PetState::load(conn)?))
}

fn sum_unseen_xp(conn: &Connection) -> Result<(u32, u32)> {
    let mut stmt =
        conn.prepare("SELECT payload FROM event_inbox WHERE seen_at IS NULL")?;
    let mut rows = stmt.query([])?;
    let mut count: u32 = 0;
    let mut total: u32 = 0;
    while let Some(row) = rows.next()? {
        count = count.saturating_add(1);
        let payload: String = row.get(0)?;
        total = total.saturating_add(xp_for_payload(&payload));
    }
    Ok((count, total))
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

    fn seed_pet_phase1(conn: &Connection, xp: u32) {
        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next,
                              atk, hp, def, sup, ver)
             VALUES (1, 'rust', 'Ferris', 3, ?1, 200, 15, 30, 12, 10, 11)",
            rusqlite::params![xp],
        )
        .unwrap();
    }

    fn seed_snapshot(conn: &Connection, xp: u32, level: u32) {
        conn.execute(
            "INSERT INTO pet_snapshot
               (id, village, level, hp, hp_max, xp, xp_to_next,
                atk, def, sup, ver, last_message, updated_at)
             VALUES (1, 'python', ?1, 50, 60, ?2, 100,
                     20, 18, 15, 14, NULL, datetime('now'))",
            rusqlite::params![level, xp],
        )
        .unwrap();
    }

    fn seed_snapshot_with_message(conn: &Connection, msg: &str) {
        conn.execute(
            "INSERT INTO pet_snapshot
               (id, village, level, hp, hp_max, xp, xp_to_next,
                atk, def, sup, ver, last_message, updated_at)
             VALUES (1, 'rust', 1, 50, 60, 0, 100,
                     20, 18, 15, 14, ?1, datetime('now'))",
            rusqlite::params![msg],
        )
        .unwrap();
    }

    fn insert_event(conn: &Connection, payload: &str) {
        conn.execute(
            "INSERT INTO event_inbox (payload, created_at) VALUES (?1, 1000)",
            rusqlite::params![payload],
        )
        .unwrap();
    }

    #[test]
    fn exists_is_false_for_empty_db() {
        let conn = fresh();
        assert!(!LiveState::exists(&conn).unwrap());
    }

    #[test]
    fn exists_detects_phase1_pet() {
        let conn = fresh();
        seed_pet_phase1(&conn, 42);
        assert!(LiveState::exists(&conn).unwrap());
    }

    #[test]
    fn exists_detects_pet_snapshot() {
        let conn = fresh();
        seed_snapshot(&conn, 10, 2);
        assert!(LiveState::exists(&conn).unwrap());
    }

    #[test]
    fn snapshot_takes_precedence_over_pet_table() {
        let conn = fresh();
        // Phase 1 pet says XP=50. Daemon snapshot says XP=200 (newer).
        seed_pet_phase1(&conn, 50);
        seed_snapshot(&conn, 200, 5);

        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.state.xp, 200);
        assert_eq!(live.state.level, 5);
        assert_eq!(live.state.village, "python");
    }

    #[test]
    fn falls_back_to_pet_when_no_snapshot() {
        let conn = fresh();
        seed_pet_phase1(&conn, 75);
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.state.xp, 75);
        assert_eq!(live.state.level, 3);
        assert_eq!(live.state.village, "rust");
    }

    #[test]
    fn overlays_pending_xp_from_named_events() {
        let conn = fresh();
        seed_snapshot(&conn, 10, 1);
        insert_event(&conn, r#"{"event":"git_commit"}"#); // +20
        insert_event(&conn, r#"{"event":"session_end"}"#); // +10

        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_events, 2);
        assert_eq!(live.pending_xp, 30);
        // base xp 10 + pending 30 = 40 (no level-up at xp_to_next=100)
        assert_eq!(live.state.xp, 40);
        assert_eq!(live.state.level, 1);
    }

    #[test]
    fn overlay_triggers_level_up_cascade() {
        let conn = fresh();
        // base xp=90, xp_to_next=100 — 2 commits (20+20=40) should cross threshold
        seed_snapshot(&conn, 90, 1);
        insert_event(&conn, r#"{"event":"git_commit"}"#);
        insert_event(&conn, r#"{"event":"git_commit"}"#);

        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_xp, 40);
        // Level up happened during overlay
        assert_eq!(live.state.level, 2);
        assert_eq!(live.state.xp, 30); // (90+40) - 100 = 30
    }

    #[test]
    fn overlays_xp_award_events() {
        let conn = fresh();
        seed_snapshot(&conn, 5, 1);
        // Legacy CLI migration path: xp::award emits this shape
        insert_event(
            &conn,
            r#"{"event":"xp_award","xp":7,"source":"learn","detail":"note"}"#,
        );
        insert_event(
            &conn,
            r#"{"event":"xp_award","xp":3,"source":"search"}"#,
        );
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_xp, 10);
        assert_eq!(live.state.xp, 15);
    }

    #[test]
    fn unknown_and_malformed_events_contribute_zero() {
        let conn = fresh();
        seed_snapshot(&conn, 50, 1);
        insert_event(&conn, r#"{"event":"unknown_event"}"#);
        insert_event(&conn, "not-json");
        insert_event(&conn, r#"{"no_event_key":"x"}"#);

        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_events, 3);
        assert_eq!(live.pending_xp, 0);
        assert_eq!(live.state.xp, 50);
    }

    #[test]
    fn seen_events_are_not_overlaid() {
        let conn = fresh();
        seed_snapshot(&conn, 10, 1);
        insert_event(&conn, r#"{"event":"git_commit"}"#);
        conn.execute(
            "UPDATE event_inbox SET seen_at = 9999 WHERE id = 1",
            [],
        )
        .unwrap();

        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_events, 0);
        assert_eq!(live.pending_xp, 0);
        assert_eq!(live.state.xp, 10);
    }

    #[test]
    fn empty_db_returns_none() {
        let conn = fresh();
        let live = LiveState::load(&conn).unwrap();
        assert!(live.is_none(), "no pet anywhere → None, not zero-filled state");
    }

    #[test]
    fn empty_db_overlay_never_hangs_on_infinite_loop() {
        // Regression: default PetState has xp_to_next=0, which caused
        // add_xp's while-loop to spin forever. We now return None before
        // overlay runs when there's no base pet — this just guards the
        // guarantee with an explicit test.
        let conn = fresh();
        insert_event(&conn, r#"{"event":"git_commit"}"#);
        let live = LiveState::load(&conn).unwrap();
        assert!(live.is_none());
    }

    #[test]
    fn load_base_none_skips_overlay_even_with_pending_events() {
        // If someone adds a pet after events are already queued, the events
        // are still preserved in the inbox (daemon will drain them); load
        // just returns None since there's no pet to apply them to yet.
        let conn = fresh();
        insert_event(&conn, r#"{"event":"xp_award","xp":5}"#);
        insert_event(&conn, r#"{"event":"git_commit"}"#);
        assert!(LiveState::load(&conn).unwrap().is_none());

        // Once a pet exists, the overlay picks them up (not lost).
        seed_snapshot(&conn, 0, 1);
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(live.pending_xp, 25);
    }

    #[test]
    fn last_message_round_trips_from_snapshot() {
        let conn = fresh();
        seed_snapshot_with_message(&conn, "擊殺 ghost 「dead code」（+5 XP）");
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert_eq!(
            live.last_message.as_deref(),
            Some("擊殺 ghost 「dead code」（+5 XP）")
        );
    }

    #[test]
    fn last_message_none_when_absent() {
        let conn = fresh();
        seed_snapshot(&conn, 0, 1); // seeds NULL last_message
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert!(live.last_message.is_none());
    }

    #[test]
    fn last_message_none_for_phase1_fallback() {
        // Phase 1 pet table has no last_message column — fallback path
        // returns None for the field regardless of snapshot state.
        let conn = fresh();
        seed_pet_phase1(&conn, 42);
        let live = LiveState::load(&conn).unwrap().unwrap();
        assert!(live.last_message.is_none());
    }

    #[test]
    fn exists_propagates_sql_errors() {
        // With no schema, the COUNT query errors — should propagate, not
        // silently report "no pet".
        let conn = Connection::open_in_memory().unwrap();
        let result = LiveState::exists(&conn);
        assert!(result.is_err(), "missing table must surface, not return false");
    }
}
