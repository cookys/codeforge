//! First-Time Events (spec §3.8) — one-shot emotional milestones.
//!
//! Each event fires exactly once per codeforge store, ever. When it fires
//! we:
//!   1. write a row to `first_events` (PRIMARY KEY event_id + INSERT OR
//!      IGNORE — so every write site is naturally idempotent, even across
//!      daemon restarts, crashes, and concurrent writes);
//!   2. return the感性 commentary line so the tick pipeline can drop it
//!      into `LastMessage` for the statusline / TUI to surface.
//!
//! Event catalogue (from spec):
//!   first_zone_entry_<zone_id>   首次進入新語言 Zone
//!   first_boss_kill              首次擊殺 Boss
//!   first_level_10               首次升到 Lv 10
//!   first_legendary_ability      首次解鎖 Legendary Ability (Lv 50)
//!   first_second_pet             首次蒐集到第 2 隻寵物
//!
//! Only the first three are wired into the current tick — legendary and
//! second-pet gates depend on systems (Lv 50 abilities, pet switching)
//! that haven't landed yet; the mechanism is ready for them.

use super::combat::DefeatedMob;
use super::ecs::{GameWorld, LastMessage};
use super::mob::MobKind;
use anyhow::Result;
use rusqlite::Connection;

/// Attempt to trigger an event. Returns `true` iff this write was the first
/// (INSERT affected 1 row). Subsequent calls with the same `event_id`
/// return `false` without side-effects.
///
/// `payload` is optional JSON context (e.g. `{"mob":"src/auth.rs"}` for
/// `first_boss_kill`). Pass `None` for events with no useful context.
pub fn try_trigger(
    conn: &Connection,
    event_id: &str,
    tick_count: u64,
    payload: Option<&str>,
) -> Result<bool> {
    let now_iso = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO first_events (event_id, triggered_at, tick_count, payload)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![event_id, now_iso, tick_count as i64, payload],
    )?;
    Ok(inserted == 1)
}

/// The感性 commentary line for a first-event. CJK-safe truncation per
/// HARD RULE — names from `defeats` can contain paths with any encoding.
/// Returns `None` for event IDs we don't know about (so adding a new
/// event elsewhere without updating this table doesn't write a garbage
/// message).
pub fn commentary_for(event_id: &str) -> Option<&'static str> {
    // Strings intentionally inlined here rather than in locales/*.yaml —
    // these trigger once per codeforge lifetime, translation churn is
    // not worth the i18n ceremony until Phase 5 Nation themes.
    match event_id {
        id if id.starts_with("first_zone_entry_") => Some("這裡的空氣跟之前不一樣……我記住了。"),
        "first_boss_kill" => Some("剛才我感覺到了什麼……原來這就是「完成」的感覺。"),
        "first_level_10" => Some("我……比上週的自己強了一點。謝謝你。"),
        "first_legendary_ability" => Some("這個名字我要記一輩子。"),
        "first_second_pet" => Some("你去了別的地方……帶回了新的夥伴。"),
        _ => None,
    }
}

/// Scan the tick's state for any first-time milestones that should fire,
/// write them durably, and park the first commentary line on `LastMessage`
/// (so statusline + TUI surface it within the TTL window). If multiple
/// milestones fire in the same tick (rare — e.g. a Lv 10 promotion from
/// a first boss kill), only the **first** commentary lands; the others
/// are still persisted to `first_events` so they never re-trigger.
///
/// Called INSIDE the tick transaction so the `first_events` write is
/// atomic with the rest of the tick. Passes through `current_tick` so
/// the LastMessage tick_stamp aligns with the serialize-to-db TTL.
pub fn check_and_trigger(
    gw: &mut GameWorld,
    conn: &Connection,
    defeats: &[DefeatedMob],
    level_before: u32,
    level_after: u32,
    zone_id: &str,
    current_tick: u64,
) -> Result<()> {
    let mut first_line: Option<&'static str> = None;

    // Rule: first zone entry. event_id encodes the zone so each zone
    // triggers independently (Phase 3a's multi-zone world will light up
    // one per zone as the player crosses in).
    let zone_event = format!("first_zone_entry_{zone_id}");
    if try_trigger(conn, &zone_event, current_tick, None)? && first_line.is_none() {
        first_line = commentary_for(&zone_event);
    }

    // Rule: first boss kill. `defeats` is authoritative for this tick.
    if defeats.iter().any(|d| d.kind == MobKind::Boss) {
        // Record the mob name for future forensics; CJK-safe truncation.
        let hero = defeats.iter().find(|d| d.kind == MobKind::Boss).unwrap();
        let short: String = hero.name.chars().take(80).collect();
        let payload = format!("{{\"mob\":\"{}\"}}", json_escape(&short));
        if try_trigger(conn, "first_boss_kill", current_tick, Some(&payload))?
            && first_line.is_none()
        {
            first_line = commentary_for("first_boss_kill");
        }
    }

    // Rule: first Lv 10. Uses level_before vs level_after so multi-level
    // cascades in a single tick still trigger exactly once.
    if level_before < 10
        && level_after >= 10
        && try_trigger(conn, "first_level_10", current_tick, None)?
        && first_line.is_none()
    {
        first_line = commentary_for("first_level_10");
    }

    if let Some(text) = first_line {
        let pet = gw.pet();
        let _ = gw.world_mut().insert_one(
            pet,
            LastMessage {
                text: text.to_string(),
                tick_stamp: current_tick,
            },
        );
    }
    Ok(())
}

/// Minimal JSON string escaping for payload blobs. Avoid pulling serde_json
/// in here just for this — `first_events.payload` is read-only diagnostic
/// context, not a contract.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh() -> (Connection, GameWorld) {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        let world = GameWorld::load_or_init(&conn).unwrap();
        (conn, world)
    }

    #[test]
    fn try_trigger_returns_true_first_time_only() {
        let (conn, _) = fresh();
        assert!(try_trigger(&conn, "first_boss_kill", 1, None).unwrap());
        assert!(!try_trigger(&conn, "first_boss_kill", 2, None).unwrap());
        assert!(!try_trigger(&conn, "first_boss_kill", 99, Some("{}")).unwrap());

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM first_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "PK idempotency must collapse re-triggers");
    }

    #[test]
    fn try_trigger_persists_tick_and_payload() {
        let (conn, _) = fresh();
        try_trigger(&conn, "first_boss_kill", 42, Some(r#"{"mob":"big"}"#)).unwrap();
        let (tick, payload): (i64, Option<String>) = conn
            .query_row(
                "SELECT tick_count, payload FROM first_events WHERE event_id = 'first_boss_kill'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(tick, 42);
        assert_eq!(payload.as_deref(), Some(r#"{"mob":"big"}"#));
    }

    #[test]
    fn commentary_for_covers_documented_events() {
        assert!(commentary_for("first_boss_kill").is_some());
        assert!(commentary_for("first_level_10").is_some());
        assert!(commentary_for("first_zone_entry_rust").is_some());
        assert!(commentary_for("first_legendary_ability").is_some());
        assert!(commentary_for("first_second_pet").is_some());
        assert!(commentary_for("not_a_first_event").is_none());
    }

    #[test]
    fn commentary_is_in_tw_chinese() {
        // Defensive spot-check: spec wants 感性 commentary in 繁體中文.
        // A future edit that accidentally swaps in simplified text should
        // fail CI. Check for a known traditional-only char.
        assert!(commentary_for("first_boss_kill").unwrap().contains("這"));
    }

    #[test]
    fn check_and_trigger_fires_boss_kill_on_first_boss_defeat() {
        let (conn, mut gw) = fresh();
        let defeats = vec![DefeatedMob {
            id: 1,
            kind: MobKind::Boss,
            name: "src/auth.rs".to_string(),
            hp_max: 500,
        }];
        check_and_trigger(&mut gw, &conn, &defeats, 1, 1, "rust", 100).unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_boss_kill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn check_and_trigger_fires_level_10_on_crossing_threshold() {
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 9, 10, "rust", 200).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_level_10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn check_and_trigger_fires_level_10_on_multi_level_cascade() {
        // Big XP drop in a single tick cascades level 5 → 12. The event
        // must still fire exactly once.
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 5, 12, "rust", 300).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_level_10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn check_and_trigger_does_not_fire_level_10_below_threshold() {
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 8, 9, "rust", 400).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_level_10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn check_and_trigger_records_zone_entry_separately_per_zone() {
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 1, 1, "rust", 1).unwrap();
        check_and_trigger(&mut gw, &conn, &[], 1, 1, "python", 2).unwrap();

        let rust: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_zone_entry_rust'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let py: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_zone_entry_python'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rust, 1);
        assert_eq!(py, 1, "each zone is an independent first-entry event");
    }

    #[test]
    fn check_and_trigger_sets_last_message_on_first_fire() {
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 1, 1, "rust", 10).unwrap();
        let msg = gw.world().get::<&LastMessage>(gw.pet()).unwrap();
        assert!(msg.text.contains("記住"), "got: {}", msg.text);
        assert_eq!(msg.tick_stamp, 10);
    }

    #[test]
    fn check_and_trigger_sets_exactly_one_message_when_multiple_fire() {
        // Lv 10 crossing in same tick as the first zone entry — spec
        // intent is one感性 line, not a paragraph. Both events still
        // persist to first_events; only the commentary is single.
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 9, 10, "rust", 20).unwrap();
        let msg = gw.world().get::<&LastMessage>(gw.pet()).unwrap();
        // Exactly one of the two commentary strings, not both concatenated.
        assert!(!msg.text.contains("\n"));
    }

    #[test]
    fn check_and_trigger_is_idempotent_across_calls() {
        // Simulating restart: two ticks in a row where Lv 10 appears to
        // cross (from DB's perspective, daemon state matches). Rule still
        // only writes once.
        let (conn, mut gw) = fresh();
        check_and_trigger(&mut gw, &conn, &[], 9, 10, "rust", 50).unwrap();
        check_and_trigger(&mut gw, &conn, &[], 9, 10, "rust", 51).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM first_events WHERE event_id = 'first_level_10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn json_escape_handles_quotes_and_backslashes() {
        assert_eq!(
            json_escape(r#"path "with" quote"#),
            r#"path \"with\" quote"#
        );
        assert_eq!(json_escape(r"a\b"), r"a\\b");
        assert_eq!(json_escape("a\nb"), "a\\nb");
    }
}
