//! Dispatch drained InboxEvents into GameWorld state changes.
//!
//! Phase 2a P3: basic XP awards per event type. Future phases refine
//! (combat from file_saved, commentary from commit patterns, etc.).

use super::ecs::GameWorld;
use super::inbox::InboxEvent;
use super::systems;

/// XP awarded per event type. Keep conservative — Phase 2b+ will rebalance.
pub fn xp_for_event(event_name: &str) -> u32 {
    match event_name {
        "git_commit" => 20,
        "session_end" => 10,
        "session_start" => 3,
        "file_saved" => 1,
        _ => 0,
    }
}

/// Dispatch one event. Returns the XP awarded (for logging/testing).
///
/// **Unknown-event behavior**: events whose name doesn't match the
/// known set produce 0 XP — silently. We log a warning to stderr
/// (systemd journal) so misconfigured hooks get caught in `journalctl`
/// rather than vanishing into the void. Match is case-sensitive by
/// design: hook scripts should use the exact names in `xp_for_event`.
pub fn dispatch(gw: &mut GameWorld, event: &InboxEvent) -> u32 {
    let name = parse_event_name(&event.payload).unwrap_or_default();
    let xp = xp_for_event(&name);
    if xp > 0 {
        systems::apply_xp(gw, xp);
    } else if !name.is_empty() {
        // Known event name that didn't match, or unknown event — log once per event.
        eprintln!(
            "codeforge daemon: unknown event name `{}` (id={}) — mapped to 0 XP",
            name, event.id
        );
    } else {
        eprintln!(
            "codeforge daemon: event id={} has missing or non-string `event` key — ignored",
            event.id
        );
    }
    xp
}

fn parse_event_name(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("event")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh_world() -> GameWorld {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        GameWorld::load_or_init(&conn).unwrap()
    }

    fn ev(id: i64, payload: &str) -> InboxEvent {
        InboxEvent { id, payload: payload.to_string(), created_at: 0 }
    }

    #[test]
    fn git_commit_awards_xp() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"event":"git_commit","sha":"abc"}"#));
        assert_eq!(awarded, 20);
        let l = gw.world().get::<&crate::daemon::ecs::PetLevel>(gw.pet()).unwrap();
        assert_eq!(l.xp, 20);
    }

    #[test]
    fn unknown_event_is_noop() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"event":"never_heard_of_it"}"#));
        assert_eq!(awarded, 0);
        let l = gw.world().get::<&crate::daemon::ecs::PetLevel>(gw.pet()).unwrap();
        assert_eq!(l.xp, 0);
    }

    #[test]
    fn malformed_payload_is_noop() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, "not-json"));
        assert_eq!(awarded, 0);
    }

    #[test]
    fn missing_event_key_is_noop() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"foo":"bar"}"#));
        assert_eq!(awarded, 0);
    }

    #[test]
    fn wrong_case_event_name_produces_zero_xp() {
        // Locks the "case-sensitive by design" contract. If this behavior
        // ever changes (e.g., to_lowercase), update the test and doc.
        let mut gw = fresh_world();
        assert_eq!(
            dispatch(&mut gw, &ev(1, r#"{"event":"Git_Commit"}"#)),
            0,
            "wrong-case event name should not award XP"
        );
        assert_eq!(
            dispatch(&mut gw, &ev(2, r#"{"event":"GIT_COMMIT"}"#)),
            0
        );
    }

    #[test]
    fn wrong_event_key_case_produces_zero_xp() {
        // parse_event_name hardcodes lowercase "event"
        let mut gw = fresh_world();
        assert_eq!(
            dispatch(&mut gw, &ev(1, r#"{"EVENT":"git_commit"}"#)),
            0,
            "uppercase EVENT key should not match"
        );
    }
}
