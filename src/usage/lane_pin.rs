//! Token-lane session pin decision and SessionStart pin write.
//!
//! The pin decision used to live only in `statusline::run`. Headless `claude -p`
//! sessions never run the statusline, so their requests were filed under the login
//! file's account (cuda 2026-11-13, twgame token lane). The decision is shared;
//! SessionStart (`memory context --hook`) writes the pin as well.

use super::alias::{self, claude_config_dir, override_from, path_hash8, TOKEN_ENV};
use super::logins::{self, PIN_UNATTRIBUTED};
use super::quota::{self, is_hash8};
use std::path::{Path, PathBuf};

/// A process that is a setup-token lane: config dir to hash as the pin root, and an
/// optional override alias (`CODEFORGE_ACCOUNT_ALIAS` when accepted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanePin {
    pub config_dir: PathBuf,
    pub override_alias: Option<String>,
}

/// Decide whether this process is a token lane.
///
/// Returns `None` when there is no accepted override alias and the token env is
/// absent (or empty). Otherwise returns the Claude config dir and the override
/// alias when set. Only the token env's **presence** is checked; its value is
/// never read. Pure: callers pass env values in so tests never mutate process env.
pub fn lane_pin(
    alias_raw: Option<&str>,
    token_present: bool,
    env_config_dir: Option<&str>,
    home: &Path,
) -> Option<LanePin> {
    let override_alias = override_from(alias_raw);
    if override_alias.is_none() && !token_present {
        return None;
    }
    Some(LanePin {
        config_dir: claude_config_dir(env_config_dir, home),
        override_alias,
    })
}

/// [`lane_pin`] from this process's environment. Not called from tests (env seams
/// race under `cargo test`).
pub fn lane_pin_from_env() -> Option<LanePin> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cfg = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let alias = std::env::var(alias::ACCOUNT_ALIAS_ENV).ok();
    // Only the variable's presence is checked; its value is never read.
    let token_present = std::env::var_os(TOKEN_ENV).is_some_and(|v| !v.is_empty());
    lane_pin(alias.as_deref(), token_present, cfg.as_deref(), &home)
}

/// Pin `raw_session_id` for a token lane, same write as the statusline's pinned path:
/// `pin_session(<quota_dir parent>, path_hash8(config_dir), sanitize_session_id(id),
/// alias or PIN_UNATTRIBUTED, now_ms)` with the same `is_hash8` / `valid_alias` guards.
/// No-op when `raw_session_id` is empty. Never errors out to the caller.
pub fn pin_lane_session(lane: &LanePin, live_base: &Path, raw_session_id: &str, now_ms: i64) {
    if raw_session_id.is_empty() {
        return;
    }
    let session = crate::live::sanitize_session_id(raw_session_id);
    let root = path_hash8(&lane.config_dir);
    if !is_hash8(&root) {
        return;
    }
    let alias = lane.override_alias.as_deref().unwrap_or(PIN_UNATTRIBUTED);
    if !alias::valid_alias(alias) {
        return;
    }
    let quota = quota::quota_dir(live_base);
    let Some(parent) = quota.parent() else {
        return;
    };
    let _ = logins::pin_session(parent, &root, &session, alias, now_ms);
}

/// SessionStart hook path: parse `session_id` from the event JSON in `stdin_text` and
/// pin when `lane` is a token lane. Malformed JSON, missing/`null`/non-string
/// `session_id`, or empty id → nothing written, no panic. Never writes to stdout/stderr.
/// Parameters are explicit so tests do not mutate process env (parallel-safe).
pub fn pin_from_session_start(
    lane: Option<&LanePin>,
    live_base: &Path,
    stdin_text: &str,
    now_ms: i64,
) {
    let Some(lane) = lane else {
        return;
    };
    let sid = match serde_json::from_str::<serde_json::Value>(stdin_text) {
        Ok(v) => match v.get("session_id").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => return,
        },
        Err(_) => return,
    };
    pin_lane_session(lane, live_base, &sid, now_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::logins::PIN_UNATTRIBUTED;

    fn live_with_usage() -> (tempfile::TempDir, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let live = base.path().to_path_buf();
        std::fs::create_dir_all(live.join("codeforge/usage/quota")).unwrap();
        (base, live)
    }

    fn cfg_home() -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let cfg = home.path().join(".claude");
        std::fs::create_dir_all(&cfg).unwrap();
        (home, cfg)
    }

    #[test]
    fn lane_pin_none_when_not_a_token_lane() {
        let home = Path::new("/home/t");
        assert_eq!(lane_pin(None, false, None, home), None);
        assert_eq!(lane_pin(Some(""), false, None, home), None);
        assert_eq!(lane_pin(Some("acct-BAD"), false, None, home), None);
        // empty token presence flag with no alias
        assert_eq!(lane_pin(Some("not-an-acct-hex"), false, None, home), None);
    }

    #[test]
    fn lane_pin_returns_config_dir_and_override() {
        let home = Path::new("/home/t");
        let got = lane_pin(Some("acct-0123abcd"), false, Some("/x/cfg"), home).unwrap();
        assert_eq!(got.config_dir, PathBuf::from("/x/cfg"));
        assert_eq!(got.override_alias.as_deref(), Some("acct-0123abcd"));
        let tok = lane_pin(None, true, None, home).unwrap();
        assert_eq!(tok.config_dir, home.join(".claude"));
        assert_eq!(tok.override_alias, None);
    }

    #[test]
    fn session_start_with_override_alias_pins_that_session() {
        let (home, cfg) = cfg_home();
        let (base, live) = live_with_usage();
        let lane = lane_pin(
            Some("acct-deadbeef"),
            false,
            Some(cfg.to_str().unwrap()),
            home.path(),
        )
        .unwrap();
        let root = path_hash8(&lane.config_dir);
        let now_ms = 1_700_000_000_000i64;
        let stdin = r#"{"session_id":"sess-hook-1","hook_event_name":"SessionStart"}"#;
        pin_from_session_start(Some(&lane), &live, stdin, now_ms);
        let pins: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                base.path()
                    .join(format!("codeforge/usage/pins-{root}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(pins["sess-hook-1"]["alias"], "acct-deadbeef");
        assert_eq!(pins["sess-hook-1"]["at_ms"], now_ms);
    }

    #[test]
    fn session_start_token_env_without_alias_pins_unattributed() {
        let (home, cfg) = cfg_home();
        let (base, live) = live_with_usage();
        let lane = lane_pin(None, true, Some(cfg.to_str().unwrap()), home.path()).unwrap();
        let root = path_hash8(&lane.config_dir);
        let now_ms = 1_700_000_000_001i64;
        pin_from_session_start(Some(&lane), &live, r#"{"session_id":"sess-tok"}"#, now_ms);
        let pins: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                base.path()
                    .join(format!("codeforge/usage/pins-{root}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(pins["sess-tok"]["alias"], PIN_UNATTRIBUTED);
    }

    #[test]
    fn session_start_not_a_token_lane_writes_nothing() {
        let (_base, live) = live_with_usage();
        let usage = live.join("codeforge/usage");
        pin_from_session_start(None, &live, r#"{"session_id":"sess-x"}"#, 1);
        let entries: Vec<_> = std::fs::read_dir(&usage)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            entries
                .iter()
                .all(|n| n != "pins-xxxxxxxx.json" && !n.to_string_lossy().starts_with("pins-")),
            "pins file appeared: {entries:?}"
        );
    }

    #[test]
    fn session_start_malformed_or_missing_session_id_writes_nothing() {
        let (home, cfg) = cfg_home();
        let (base, live) = live_with_usage();
        let lane = lane_pin(
            Some("acct-deadbeef"),
            true,
            Some(cfg.to_str().unwrap()),
            home.path(),
        )
        .unwrap();
        let root = path_hash8(&lane.config_dir);
        let pins_path = base
            .path()
            .join(format!("codeforge/usage/pins-{root}.json"));
        for bad in [
            "",
            "{not json",
            "{}",
            r#"{"session_id":null}"#,
            r#"{"session_id":123}"#,
            r#"{"session_id":""}"#,
            r#"{"other":"sess-x"}"#,
        ] {
            pin_from_session_start(Some(&lane), &live, bad, 99);
            assert!(!pins_path.exists(), "pins written for input {bad:?}");
        }
    }
}
