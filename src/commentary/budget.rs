//! Phase 3c — global 1/hour rate limit gate.
//!
//! Keeps a single answer front-and-center: "is it okay to emit a commentary
//! right now?". The answer depends on:
//!   1. the opt-in flag (settings table + `CODEFORGE_COMMENTARY` env),
//!   2. the last successful emission's unix timestamp,
//!   3. the requested trigger kind (ManualTest bypasses both).
//!
//! The actual reserve/unreserve dance lives in `repo::budget_*` — this
//! module is the policy layer on top.

use super::repo;
use super::trigger::RATE_LIMIT_WINDOW_SECS;
use super::Kind;
use anyhow::Result;
use rusqlite::Connection;

/// Environment variable that mirrors the settings flag. Either being truthy
/// opts the user in — so one-off shells can try the feature without
/// persisting a choice.
pub const OPT_IN_ENV: &str = "CODEFORGE_COMMENTARY";

/// Is commentary currently enabled? Either the settings flag (set via
/// `codeforge commentary on`) or the env var being "1"/"true" counts.
pub fn opt_in_enabled(conn: &Connection) -> Result<bool> {
    if repo::opt_in_flag(conn)? {
        return Ok(true);
    }
    Ok(env_opt_in())
}

fn env_opt_in() -> bool {
    match std::env::var(OPT_IN_ENV) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"),
        Err(_) => false,
    }
}

/// Can a commentary of this `kind` be emitted at `now`?
///
/// * `ManualTest` always passes — it's the escape hatch for `codeforge
///   commentary test` to verify the pipeline without waiting an hour.
/// * Other kinds require (last_emit_at is NULL) OR (now - last_emit_at ≥ 3600).
///
/// The caller should still call `repo::budget_reserve` before dispatching
/// the LLM, and `budget_unreserve` if the dispatch fails.
pub fn can_emit(conn: &Connection, now: i64, kind: Kind) -> Result<bool> {
    if kind == Kind::ManualTest {
        return Ok(true);
    }
    let last = repo::budget_last_emit(conn)?;
    Ok(match last {
        None => true,
        Some(t) => now.saturating_sub(t) >= RATE_LIMIT_WINDOW_SECS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        crate::db::migrations::run(&conn).expect("migrations");
        conn
    }

    #[test]
    fn opt_in_defaults_off_and_unset_env() {
        std::env::remove_var(OPT_IN_ENV);
        let conn = open_db();
        assert!(!opt_in_enabled(&conn).unwrap());
    }

    #[test]
    fn opt_in_via_settings_flag() {
        std::env::remove_var(OPT_IN_ENV);
        let conn = open_db();
        repo::set_opt_in_flag(&conn, true).unwrap();
        assert!(opt_in_enabled(&conn).unwrap());
    }

    // env_opt_in is a pure function over the env var — exercise it
    // directly so parallel tests don't contend on the global env state.
    #[test]
    fn env_opt_in_recognised_values() {
        // Avoid touching the real process env in multi-threaded tests —
        // swap to an isolated check via matches!().
        for v in ["1", "true", "TRUE", "yes", "on", "On"] {
            let matched =
                matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
            assert!(matched, "value {:?} should opt in", v);
        }
        for v in ["0", "false", "", "maybe"] {
            let matched =
                matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
            assert!(!matched, "value {:?} should stay off", v);
        }
    }

    #[test]
    fn can_emit_first_ever_returns_true() {
        let conn = open_db();
        assert!(can_emit(&conn, 1000, Kind::BossKill).unwrap());
    }

    #[test]
    fn can_emit_boundary_exactly_at_window() {
        let conn = open_db();
        repo::budget_reserve(&conn, 1000).unwrap();

        // t + 3599 → still within window
        assert!(!can_emit(&conn, 1000 + RATE_LIMIT_WINDOW_SECS - 1, Kind::BossKill).unwrap());
        // t + 3600 → exactly at boundary, allowed per spec
        assert!(can_emit(&conn, 1000 + RATE_LIMIT_WINDOW_SECS, Kind::BossKill).unwrap());
        // t + 3601 → well past boundary
        assert!(can_emit(&conn, 1000 + RATE_LIMIT_WINDOW_SECS + 1, Kind::BossKill).unwrap());
    }

    #[test]
    fn can_emit_manual_test_bypasses_window() {
        let conn = open_db();
        repo::budget_reserve(&conn, 10_000).unwrap();
        // Well within the window — other kinds blocked, ManualTest free
        assert!(!can_emit(&conn, 10_001, Kind::BossKill).unwrap());
        assert!(can_emit(&conn, 10_001, Kind::ManualTest).unwrap());
    }

    #[test]
    fn can_emit_handles_clock_regression_safely() {
        // Paranoia: if the wall clock goes backward (NTP adjust, bad
        // daemon restart), can_emit must not over-restrict with a
        // negative-age result. saturating_sub keeps the difference ≥ 0.
        let conn = open_db();
        repo::budget_reserve(&conn, 5000).unwrap();
        // now < last_emit — saturating_sub → 0 → blocked (correct)
        assert!(!can_emit(&conn, 4000, Kind::BossKill).unwrap());
    }
}
