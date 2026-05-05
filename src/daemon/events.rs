//! Dispatch drained InboxEvents into GameWorld state changes.
//!
//! Phase 2a P3: basic XP awards per event type. Future phases refine
//! (combat from file_saved, commentary from commit patterns, etc.).
//!
//! Phase 2a P8: the same `xp_for_payload` used by dispatch is also the
//! overlay function used by the CLI live-read path (`pet::live_state`).
//! Keeping one source of truth avoids read/write XP drift.

use super::ecs::GameWorld;
use super::inbox::InboxEvent;
use super::systems;

/// XP awarded per *named* event type. Keep conservative — Phase 2b+ will
/// rebalance. For `xp_award` events the XP is taken from the payload.
pub fn xp_for_event(event_name: &str) -> u32 {
    match event_name {
        "git_commit" => 20,
        "session_end" => 10,
        "session_start" => 3,
        "file_saved" => 1,
        _ => 0,
    }
}

/// Compute XP a single event payload would award.
///
/// Two branches:
/// 1. `xp_award` events carry an explicit `xp` integer field — used by
///    the legacy CLI migration path (`pet::xp::award` for learn/search/ingest).
///    Falls back to 0 if `xp` is missing or not a non-negative integer.
/// 2. All other events map through `xp_for_event`.
///
/// Returns 0 on any parse failure (malformed JSON, missing keys, wrong types,
/// unknown event name). Case-sensitive by design — hooks must use exact names.
pub fn xp_for_payload(payload: &str) -> u32 {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let name = match v.get("event").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return 0,
    };
    if name == "xp_award" {
        return v
            .get("xp")
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;
    }
    xp_for_event(name)
}

/// Dispatch one event. Returns the XP awarded (for logging/testing).
///
/// **Unknown-event behavior**: events whose name doesn't match the
/// known set produce 0 XP — silently. We log a warning to stderr
/// (systemd journal) so misconfigured hooks get caught in `journalctl`
/// rather than vanishing into the void. Match is case-sensitive by
/// design: hook scripts should use the exact names in `xp_for_event`.
pub fn dispatch(gw: &mut GameWorld, event: &InboxEvent) -> u32 {
    let xp = xp_for_payload(&event.payload);
    if xp > 0 {
        systems::apply_xp(gw, xp);
        return xp;
    }
    match parse_event_name(&event.payload) {
        Some(name) if !name.is_empty() => eprintln!(
            "codeforge daemon: unknown event `{}` (id={}) — mapped to 0 XP",
            name, event.id
        ),
        _ => eprintln!(
            "codeforge daemon: event id={} has missing / non-string `event` key — ignored",
            event.id
        ),
    }
    0
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
        InboxEvent {
            id,
            payload: payload.to_string(),
            created_at: 0,
        }
    }

    #[test]
    fn git_commit_awards_xp() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"event":"git_commit","sha":"abc"}"#));
        assert_eq!(awarded, 20);
        let l = gw
            .world()
            .get::<&crate::daemon::ecs::PetLevel>(gw.pet())
            .unwrap();
        assert_eq!(l.xp, 20);
    }

    #[test]
    fn unknown_event_is_noop() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"event":"never_heard_of_it"}"#));
        assert_eq!(awarded, 0);
        let l = gw
            .world()
            .get::<&crate::daemon::ecs::PetLevel>(gw.pet())
            .unwrap();
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
        assert_eq!(dispatch(&mut gw, &ev(2, r#"{"event":"GIT_COMMIT"}"#)), 0);
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

    #[test]
    fn xp_award_event_uses_payload_xp() {
        let mut gw = fresh_world();
        let awarded = dispatch(
            &mut gw,
            &ev(1, r#"{"event":"xp_award","xp":7,"source":"learn"}"#),
        );
        assert_eq!(awarded, 7);
    }

    #[test]
    fn xp_award_without_xp_field_is_zero() {
        let mut gw = fresh_world();
        let awarded = dispatch(&mut gw, &ev(1, r#"{"event":"xp_award","source":"x"}"#));
        assert_eq!(awarded, 0);
    }

    #[test]
    fn xp_for_payload_named_event() {
        assert_eq!(xp_for_payload(r#"{"event":"git_commit"}"#), 20);
        assert_eq!(xp_for_payload(r#"{"event":"session_end"}"#), 10);
        assert_eq!(xp_for_payload(r#"{"event":"file_saved"}"#), 1);
    }

    #[test]
    fn xp_for_payload_xp_award() {
        assert_eq!(xp_for_payload(r#"{"event":"xp_award","xp":12}"#), 12);
        assert_eq!(xp_for_payload(r#"{"event":"xp_award","xp":0}"#), 0);
    }

    #[test]
    fn xp_for_payload_handles_garbage() {
        assert_eq!(xp_for_payload(""), 0);
        assert_eq!(xp_for_payload("not-json"), 0);
        assert_eq!(xp_for_payload("[]"), 0);
        assert_eq!(xp_for_payload(r#"{"event":42}"#), 0);
    }
}
