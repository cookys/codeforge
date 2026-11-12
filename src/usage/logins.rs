//! Per-root login timeline, the statusline session gate, and session pins (re-login attribution).
//!
//! `<quota_dir parent>/logins-<root hash8>.json` records when the alias on a Claude config
//! dir changed. `<quota_dir parent>/sessions-<root hash8>.json` remembers the rate-limit
//! fingerprint each session last filed, so a session that was already running keeps its
//! pre-switch window off the new account. `<quota_dir parent>/pins-<root hash8>.json` maps a
//! session to the alias a setup-token lane pinned (PM 01M76F3F). None of these files stores
//! an email or an org.

use super::alias::valid_alias;
use super::quota::{self, MAX_AGE_MS};
use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read};
use std::path::Path;

const MAX_LOGINS: usize = 64;
const MAX_SESSIONS: usize = 512;
const LOGIN_MAX_BYTES: u64 = 32 * 1024;
const SESSION_MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoginEntry {
    since_ms: i64,
    alias: String,
}

/// The alias that was logged in on one root, newest last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timeline {
    entries: Vec<LoginEntry>,
}

impl Timeline {
    /// The alias whose `since_ms` is the greatest one that is still `<= t_ms`.
    /// `None` when `t_ms` precedes every entry (or the timeline is empty).
    pub fn alias_at(&self, t_ms: i64) -> Option<&str> {
        let mut best: Option<&LoginEntry> = None;
        for entry in &self.entries {
            if entry.since_ms <= t_ms && best.is_none_or(|b| entry.since_ms >= b.since_ms) {
                best = Some(entry);
            }
        }
        best.map(|e| e.alias.as_str()).filter(|a| valid_alias(a))
    }

    fn to_json(&self) -> Value {
        json!({
            "entries": self.entries.iter().map(|e| json!({
                "since_ms": e.since_ms,
                "alias": e.alias,
            })).collect::<Vec<_>>()
        })
    }
}

/// The timeline as stored, without observing anything (a root with no usable login now).
/// A missing, corrupt or foreign file reads as empty.
pub fn read(dir: &Path, root: &str) -> Result<Timeline> {
    let name = state_file("logins", root)?;
    let empty = || Timeline {
        entries: Vec::new(),
    };
    Ok(match load_bytes(&dir.join(&name), LOGIN_MAX_BYTES)? {
        Loaded::Data(buf) => parse_timeline(&buf).unwrap_or_else(empty),
        Loaded::Missing | Loaded::Foreign => empty(),
    })
}

/// Append `(now_ms, alias)` when the file is empty or the last alias differs.
/// A corrupt or foreign file is treated as empty and rewritten. Only [`valid_alias`]
/// values are stored. At most 64 entries; the oldest (front) ones are dropped.
pub fn observe(dir: &Path, root: &str, alias: &str, now_ms: i64) -> Result<Timeline> {
    let name = state_file("logins", root)?;
    let path = dir.join(&name);
    let (mut timeline, mut dirty) = match load_bytes(&path, LOGIN_MAX_BYTES)? {
        Loaded::Missing => (
            Timeline {
                entries: Vec::new(),
            },
            false,
        ),
        Loaded::Foreign => (
            Timeline {
                entries: Vec::new(),
            },
            true,
        ),
        Loaded::Data(buf) => match parse_timeline(&buf) {
            Some(t) => (t, false),
            None => (
                Timeline {
                    entries: Vec::new(),
                },
                true,
            ),
        },
    };
    if timeline.entries.len() > MAX_LOGINS {
        let drop_n = timeline.entries.len() - MAX_LOGINS;
        timeline.entries.drain(0..drop_n);
        dirty = true;
    }
    if valid_alias(alias) {
        let changed = timeline.entries.last().is_none_or(|e| e.alias != alias);
        if changed {
            timeline.entries.push(LoginEntry {
                since_ms: now_ms,
                alias: alias.to_string(),
            });
            if timeline.entries.len() > MAX_LOGINS {
                let drop_n = timeline.entries.len() - MAX_LOGINS;
                timeline.entries.drain(0..drop_n);
            }
            dirty = true;
        }
    }
    if dirty {
        write_json(dir, &name, &timeline.to_json())?;
    }
    Ok(timeline)
}

/// What `record_quota` should do with this session's `rate_limits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionGate {
    /// Same fingerprint as the pre-switch entry: do not merge or write the snapshot.
    Carry,
    /// No entry, same alias, or a new fingerprint: merge as usual.
    Merge,
}

/// The four raw rate-limit fields (5h pct, 5h resets_at, 7d pct, 7d resets_at) in one string.
/// The same stdin reading always formats the same; the string never contains `@`.
pub fn rate_fingerprint(
    five_pct: Option<f64>,
    five_resets_at: Option<i64>,
    seven_pct: Option<f64>,
    seven_resets_at: Option<i64>,
) -> String {
    fn pct(v: Option<f64>) -> String {
        match v {
            Some(n) if n.is_finite() => format!("{n}"),
            _ => "-".to_string(),
        }
    }
    fn at(v: Option<i64>) -> String {
        match v {
            Some(n) => n.to_string(),
            None => "-".to_string(),
        }
    }
    format!(
        "{}|{}|{}|{}",
        pct(five_pct),
        at(five_resets_at),
        pct(seven_pct),
        at(seven_resets_at)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionSeen {
    alias: String,
    fp: String,
    at_ms: i64,
}

/// Decide the session gate and, on [`SessionGate::Merge`], record `{alias, fp, at_ms}`
/// when that entry changed. An empty or unsanitized session is not gated (merge, no file
/// change). A read or write error is returned so the statusline can fall back to merging.
pub fn session_gate(
    dir: &Path,
    root: &str,
    session: &str,
    alias: &str,
    fp: &str,
    now_ms: i64,
) -> Result<SessionGate> {
    if !usable_session(session) || !valid_alias(alias) || !clean_fp(fp) {
        return Ok(SessionGate::Merge);
    }
    let name = state_file("sessions", root)?;
    let path = dir.join(&name);
    // a missing, corrupt or foreign file is an empty map: the next merge rewrites it clean
    let mut sessions = match load_bytes(&path, SESSION_MAX_BYTES)? {
        Loaded::Data(buf) => parse_sessions(&buf).unwrap_or_default(),
        Loaded::Missing | Loaded::Foreign => BTreeMap::new(),
    };
    if let Some(prev) = sessions.get(session) {
        if prev.alias != alias && prev.fp == fp {
            return Ok(SessionGate::Carry);
        }
    }
    // same alias and fingerprint is not a change: leave `at_ms` and skip the write
    let changed = !matches!(
        sessions.get(session),
        Some(prev) if prev.alias == alias && prev.fp == fp
    );
    if changed {
        sessions.insert(
            session.to_string(),
            SessionSeen {
                alias: alias.to_string(),
                fp: fp.to_string(),
                at_ms: now_ms,
            },
        );
        prune_sessions(&mut sessions, now_ms, session);
        write_json(dir, &name, &sessions_json(&sessions))?;
    }
    Ok(SessionGate::Merge)
}

/// The pin of a token lane without an override alias. `collect` files its requests as
/// [`crate::usage::collect::UNATTRIBUTED`]; it is a valid alias only so the pins file keeps one shape.
pub const PIN_UNATTRIBUTED: &str = "unattributed";

#[derive(Clone, Debug, PartialEq, Eq)]
struct PinSeen {
    alias: String,
    at_ms: i64,
}

/// The pinned alias of each session on this root. A missing, corrupt or foreign file
/// (including any `@`) is an empty map. An I/O error other than those is returned.
pub fn pins(dir: &Path, root: &str) -> Result<BTreeMap<String, String>> {
    let name = state_file("pins", root)?;
    let map = match load_bytes(&dir.join(&name), SESSION_MAX_BYTES)? {
        Loaded::Data(buf) => parse_pins(&buf).unwrap_or_default(),
        Loaded::Missing | Loaded::Foreign => BTreeMap::new(),
    };
    Ok(map.into_iter().map(|(k, e)| (k, e.alias)).collect())
}

/// Record `{alias, at_ms}` for `session` in `pins-<root hash8>.json`. An unusable session
/// id or an alias that is not [`valid_alias`] is ignored. The file is rewritten only when
/// the stored alias changed, or when the previous file was corrupt or foreign. On write,
/// entries older than [`MAX_AGE_MS`] are dropped and the map is capped at 512.
pub fn pin_session(dir: &Path, root: &str, session: &str, alias: &str, now_ms: i64) -> Result<()> {
    if !usable_session(session) || !valid_alias(alias) {
        return Ok(());
    }
    let name = state_file("pins", root)?;
    let path = dir.join(&name);
    let (mut pins, mut dirty) = match load_bytes(&path, SESSION_MAX_BYTES)? {
        Loaded::Missing => (BTreeMap::new(), false),
        Loaded::Foreign => (BTreeMap::new(), true),
        Loaded::Data(buf) => match parse_pins(&buf) {
            Some(m) => (m, false),
            None => (BTreeMap::new(), true),
        },
    };
    // same alias is not a change: leave `at_ms` and skip the write
    let changed = !matches!(pins.get(session), Some(prev) if prev.alias == alias);
    if changed {
        pins.insert(
            session.to_string(),
            PinSeen {
                alias: alias.to_string(),
                at_ms: now_ms,
            },
        );
        dirty = true;
    }
    if dirty {
        prune_pins(&mut pins, now_ms, session);
        write_json(dir, &name, &pins_json(&pins))?;
    }
    Ok(())
}

fn clean_fp(fp: &str) -> bool {
    !fp.is_empty() && fp.len() <= 256 && !fp.contains('@')
}

/// A session id as `sanitize_session_id` leaves it: 1–64 of `[A-Za-z0-9_-]`.
pub(crate) fn usable_session(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn prune_pins(map: &mut BTreeMap<String, PinSeen>, now_ms: i64, keep: &str) {
    map.retain(|_, e| now_ms.saturating_sub(e.at_ms) <= MAX_AGE_MS);
    while map.len() > MAX_SESSIONS {
        let oldest = map
            .iter()
            .filter(|(k, _)| k.as_str() != keep)
            .min_by(|a, b| a.1.at_ms.cmp(&b.1.at_ms).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                map.remove(&k);
            }
            None => break,
        }
    }
}

fn pins_json(map: &BTreeMap<String, PinSeen>) -> Value {
    let mut pins = Map::new();
    for (k, e) in map {
        if !usable_session(k) || !valid_alias(&e.alias) {
            continue;
        }
        pins.insert(k.clone(), json!({"alias": e.alias, "at_ms": e.at_ms}));
    }
    Value::Object(pins)
}

fn parse_pins(buf: &[u8]) -> Option<BTreeMap<String, PinSeen>> {
    let v: Value = serde_json::from_slice(buf).ok()?;
    let obj = v.as_object()?;
    let mut map = BTreeMap::new();
    for (k, item) in obj {
        if !usable_session(k) {
            return None;
        }
        let o = item.as_object()?;
        if o.len() != 2 || !o.contains_key("alias") || !o.contains_key("at_ms") {
            return None;
        }
        let alias = o.get("alias")?.as_str()?;
        let at_ms = o.get("at_ms")?.as_i64()?;
        if !valid_alias(alias) {
            return None;
        }
        map.insert(
            k.clone(),
            PinSeen {
                alias: alias.to_string(),
                at_ms,
            },
        );
    }
    Some(map)
}

fn prune_sessions(map: &mut BTreeMap<String, SessionSeen>, now_ms: i64, keep: &str) {
    map.retain(|_, e| now_ms.saturating_sub(e.at_ms) <= MAX_AGE_MS);
    while map.len() > MAX_SESSIONS {
        let oldest = map
            .iter()
            .filter(|(k, _)| k.as_str() != keep)
            .min_by(|a, b| a.1.at_ms.cmp(&b.1.at_ms).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                map.remove(&k);
            }
            None => break,
        }
    }
}

fn sessions_json(map: &BTreeMap<String, SessionSeen>) -> Value {
    let mut sessions = Map::new();
    for (k, e) in map {
        if !valid_alias(&e.alias) || !clean_fp(&e.fp) {
            continue;
        }
        sessions.insert(
            k.clone(),
            json!({"alias": e.alias, "fp": e.fp, "at_ms": e.at_ms}),
        );
    }
    json!({"sessions": sessions})
}

fn state_file(prefix: &str, root: &str) -> Result<String> {
    if !quota::is_hash8(root) {
        anyhow::bail!("登入時間軸的 root 必須是 8 碼雜湊");
    }
    Ok(format!("{prefix}-{root}.json"))
}

enum Loaded {
    Missing,
    Data(Vec<u8>),
    Foreign,
}

fn load_bytes(path: &Path, max: u64) -> std::io::Result<Loaded> {
    let file = match quota::open_no_follow(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Ok(Loaded::Foreign),
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if !meta.file_type().is_file() || meta.len() > max {
        return Ok(Loaded::Foreign);
    }
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > max || buf.contains(&b'@') {
        return Ok(Loaded::Foreign);
    }
    Ok(Loaded::Data(buf))
}

fn write_json(dir: &Path, name: &str, value: &Value) -> Result<()> {
    crate::live::write_live_json(dir, name, value)
        .map_err(|e| anyhow!("無法寫入{name}（{}）", e.kind()))
}

fn parse_timeline(buf: &[u8]) -> Option<Timeline> {
    let v: Value = serde_json::from_slice(buf).ok()?;
    let obj = v.as_object()?;
    if obj.len() != 1 || !obj.contains_key("entries") {
        return None;
    }
    let arr = obj.get("entries")?.as_array()?;
    let mut entries = Vec::with_capacity(arr.len());
    for item in arr {
        let o = item.as_object()?;
        if o.len() != 2 || !o.contains_key("since_ms") || !o.contains_key("alias") {
            return None;
        }
        let since_ms = o.get("since_ms")?.as_i64()?;
        let alias = o.get("alias")?.as_str()?;
        if !valid_alias(alias) {
            return None;
        }
        entries.push(LoginEntry {
            since_ms,
            alias: alias.to_string(),
        });
    }
    Some(Timeline { entries })
}

fn parse_sessions(buf: &[u8]) -> Option<BTreeMap<String, SessionSeen>> {
    let v: Value = serde_json::from_slice(buf).ok()?;
    let obj = v.as_object()?;
    if obj.len() != 1 || !obj.contains_key("sessions") {
        return None;
    }
    let sessions = obj.get("sessions")?.as_object()?;
    let mut map = BTreeMap::new();
    for (k, item) in sessions {
        if !usable_session(k) {
            return None;
        }
        let o = item.as_object()?;
        if o.len() != 3
            || !o.contains_key("alias")
            || !o.contains_key("fp")
            || !o.contains_key("at_ms")
        {
            return None;
        }
        let alias = o.get("alias")?.as_str()?;
        let fp = o.get("fp")?.as_str()?;
        let at_ms = o.get("at_ms")?.as_i64()?;
        if !valid_alias(alias) || !clean_fp(fp) {
            return None;
        }
        map.insert(
            k.clone(),
            SessionSeen {
                alias: alias.to_string(),
                fp: fp.to_string(),
                at_ms,
            },
        );
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "1a2b3c4d";

    fn no_at(path: &Path) {
        let body = std::fs::read_to_string(path).unwrap();
        assert!(!body.contains('@'), "{body}");
        assert!(!body.contains("email"), "{body}");
        assert!(!body.contains("org"), "{body}");
    }

    #[test]
    fn observe_appends_only_when_the_alias_changes_and_alias_at_respects_the_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let t = observe(dir.path(), ROOT, "acct-aaaaaaaa", 1_000).unwrap();
        assert_eq!(t.alias_at(999), None);
        assert_eq!(t.alias_at(1_000), Some("acct-aaaaaaaa"));
        assert_eq!(t.alias_at(1_500), Some("acct-aaaaaaaa"));
        let path = dir.path().join("logins-1a2b3c4d.json");
        let bytes = std::fs::read(&path).unwrap();
        // same alias, later clock: not an append, file untouched
        let again = observe(dir.path(), ROOT, "acct-aaaaaaaa", 1_500).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(again.entries.len(), 1);
        assert_eq!(again.alias_at(1_500), Some("acct-aaaaaaaa"));
        let switched = observe(dir.path(), ROOT, "acct-bbbbbbbb", 2_000).unwrap();
        assert_eq!(switched.alias_at(1_999), Some("acct-aaaaaaaa"));
        assert_eq!(switched.alias_at(2_000), Some("acct-bbbbbbbb"));
        assert_eq!(switched.alias_at(9_000), Some("acct-bbbbbbbb"));
        no_at(&path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn observe_keeps_at_most_64_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut last = None;
        for i in 0..65 {
            last = Some(observe(dir.path(), ROOT, &format!("a{i:02}"), i).unwrap());
        }
        let t = last.unwrap();
        assert_eq!(t.entries.len(), 64);
        assert_eq!(t.entries[0].alias, "a01");
        assert_eq!(t.entries[63].alias, "a64");
        assert_eq!(t.alias_at(0), None);
        assert_eq!(t.alias_at(1), Some("a01"));
        assert_eq!(t.alias_at(64), Some("a64"));
        no_at(&dir.path().join("logins-1a2b3c4d.json"));
    }

    #[test]
    fn a_corrupt_or_foreign_timeline_is_treated_as_empty_and_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logins-1a2b3c4d.json");
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(&path, b"not-json").unwrap();
        let t = observe(dir.path(), ROOT, "acct-aaaaaaaa", 10).unwrap();
        assert_eq!(t.alias_at(9), None);
        assert_eq!(t.alias_at(10), Some("acct-aaaaaaaa"));
        no_at(&path);
        // an email (or any `@`) makes the file foreign: rewritten without it
        std::fs::write(&path, br#"{"email":"me@example.com","entries":[]}"#).unwrap();
        let t = observe(dir.path(), ROOT, "acct-bbbbbbbb", 20).unwrap();
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.alias_at(20), Some("acct-bbbbbbbb"));
        no_at(&path);
        // an alias that is not valid_alias is not stored, and does not create a file
        let bare = tempfile::tempdir().unwrap();
        let skipped = observe(bare.path(), ROOT, "me@example.com", 1).unwrap();
        assert!(skipped.entries.is_empty());
        assert!(!bare.path().join("logins-1a2b3c4d.json").exists());
    }

    #[test]
    fn session_gate_carries_a_pre_switch_fingerprint_and_records_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let fp = rate_fingerprint(Some(0.5), Some(100), Some(0.96), Some(200));
        assert!(!fp.contains('@'));
        assert_eq!(
            session_gate(dir.path(), ROOT, "sess-s", "acct-aaaaaaaa", &fp, 1_000).unwrap(),
            SessionGate::Merge
        );
        // same fingerprint, new alias: the pre-switch response, entry unchanged
        assert_eq!(
            session_gate(dir.path(), ROOT, "sess-s", "acct-bbbbbbbb", &fp, 2_000).unwrap(),
            SessionGate::Carry
        );
        let path = dir.path().join("sessions-1a2b3c4d.json");
        let body: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["sessions"]["sess-s"]["alias"], "acct-aaaaaaaa");
        assert_eq!(body["sessions"]["sess-s"]["at_ms"], 1_000);
        no_at(&path);
        // a new fingerprint is a real response after the switch
        let fp2 = rate_fingerprint(Some(0.5), Some(100), Some(0.07), Some(300));
        assert_eq!(
            session_gate(dir.path(), ROOT, "sess-s", "acct-bbbbbbbb", &fp2, 3_000).unwrap(),
            SessionGate::Merge
        );
        let body: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["sessions"]["sess-s"]["alias"], "acct-bbbbbbbb");
        assert_eq!(body["sessions"]["sess-s"]["at_ms"], 3_000);
        // empty session: no gate, file unchanged
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            session_gate(dir.path(), ROOT, "", "acct-bbbbbbbb", &fp2, 4_000).unwrap(),
            SessionGate::Merge
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn session_entries_past_max_age_are_pruned_and_the_file_caps_at_512() {
        let dir = tempfile::tempdir().unwrap();
        let later = 10_000_000_000;
        let edge = later - MAX_AGE_MS;
        session_gate(
            dir.path(),
            ROOT,
            "edge-session",
            "acct-aaaaaaaa",
            "1|0|0|0",
            edge,
        )
        .unwrap();
        session_gate(
            dir.path(),
            ROOT,
            "old-session",
            "acct-aaaaaaaa",
            "2|0|0|0",
            edge - 1,
        )
        .unwrap();
        session_gate(
            dir.path(),
            ROOT,
            "new-session",
            "acct-aaaaaaaa",
            "3|0|0|0",
            later,
        )
        .unwrap();
        let body = std::fs::read_to_string(dir.path().join("sessions-1a2b3c4d.json")).unwrap();
        assert!(!body.contains("old-session"), "{body}");
        assert!(body.contains("edge-session"), "{body}");
        assert!(body.contains("new-session"), "{body}");
        assert!(!body.contains('@'));

        let capped = tempfile::tempdir().unwrap();
        for i in 0..513 {
            session_gate(
                capped.path(),
                ROOT,
                &format!("s{i:04}"),
                "acct-aaaaaaaa",
                &format!("{i}|0|0|0"),
                1_000 + i,
            )
            .unwrap();
        }
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(capped.path().join("sessions-1a2b3c4d.json")).unwrap(),
        )
        .unwrap();
        let sessions = v["sessions"].as_object().unwrap();
        assert_eq!(sessions.len(), 512);
        assert!(sessions.get("s0000").is_none());
        assert!(sessions.get("s0001").is_some());
        assert!(sessions.get("s0512").is_some());
    }

    #[test]
    fn pin_session_writes_only_a_changed_alias_and_a_bad_file_reads_empty() {
        let dir = tempfile::tempdir().unwrap();
        pin_session(dir.path(), ROOT, "", "acct-bbbbbbbb", 1).unwrap();
        pin_session(dir.path(), ROOT, "bad/id", "acct-bbbbbbbb", 1).unwrap();
        pin_session(dir.path(), ROOT, "sess-a", "Not-Alias", 1).unwrap();
        assert!(pins(dir.path(), ROOT).unwrap().is_empty());
        assert!(!dir.path().join("pins-1a2b3c4d.json").exists());

        pin_session(dir.path(), ROOT, "sess-a", "acct-bbbbbbbb", 1_000).unwrap();
        let path = dir.path().join("pins-1a2b3c4d.json");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            pins(dir.path(), ROOT)
                .unwrap()
                .get("sess-a")
                .map(String::as_str),
            Some("acct-bbbbbbbb")
        );
        pin_session(dir.path(), ROOT, "sess-a", "acct-bbbbbbbb", 2_000).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        no_at(&path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let bare = tempfile::tempdir().unwrap();
        assert!(pins(bare.path(), ROOT).unwrap().is_empty());
        std::fs::write(&path, b"not-json").unwrap();
        assert!(pins(dir.path(), ROOT).unwrap().is_empty());
        std::fs::write(&path, br#"{"sess-a":{"alias":"me@x.com","at_ms":1}}"#).unwrap();
        assert!(pins(dir.path(), ROOT).unwrap().is_empty());
        pin_session(dir.path(), ROOT, "sess-a", "acct-bbbbbbbb", 3_000).unwrap();
        assert_eq!(
            pins(dir.path(), ROOT)
                .unwrap()
                .get("sess-a")
                .map(String::as_str),
            Some("acct-bbbbbbbb")
        );
        no_at(&path);
    }

    #[test]
    fn pin_entries_past_max_age_are_pruned_and_the_file_caps_at_512() {
        let dir = tempfile::tempdir().unwrap();
        let later = 10_000_000_000;
        let edge = later - MAX_AGE_MS;
        pin_session(dir.path(), ROOT, "edge-session", "acct-aaaaaaaa", edge).unwrap();
        pin_session(dir.path(), ROOT, "old-session", "acct-aaaaaaaa", edge - 1).unwrap();
        pin_session(dir.path(), ROOT, "new-session", "acct-aaaaaaaa", later).unwrap();
        let body = std::fs::read_to_string(dir.path().join("pins-1a2b3c4d.json")).unwrap();
        assert!(!body.contains("old-session"), "{body}");
        assert!(body.contains("edge-session"), "{body}");
        assert!(body.contains("new-session"), "{body}");
        assert!(!body.contains('@'));

        let capped = tempfile::tempdir().unwrap();
        for i in 0..513 {
            pin_session(
                capped.path(),
                ROOT,
                &format!("s{i:04}"),
                "acct-aaaaaaaa",
                1_000 + i,
            )
            .unwrap();
        }
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(capped.path().join("pins-1a2b3c4d.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v.as_object().unwrap().len(), 512);
        assert!(v.get("s0000").is_none());
        assert!(v.get("s0001").is_some());
        assert!(v.get("s0512").is_some());
    }
}
