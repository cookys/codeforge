//! The local `grok` CLI account's weekly quota, as one quota snapshot for the summary.
//!
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` returns the unified credit
//! balance (operator card d-8bbff214). The bearer is read from `~/.grok/auth.json` only at the
//! moment of the request, held in a local [`Key`], and sent only on that request. [`Key`] has
//! no `Debug` and no `Display` — `format!("{:?}", key)` does not compile — so a log, an error,
//! or a test assertion cannot print it. The login file's other fields are never read. An
//! expired login or HTTP 401/403 is [`Fetch::Denied`]: no refresh, no new snapshot.
//!
//! The response is untrusted. [`parse_billing`] keeps `config.creditUsagePercent` (a number in
//! 0..=150, divided by 100) and `config.currentPeriod.end` (RFC3339, as unix seconds) and drops
//! everything else. The capture is cached at `<cache_dir>/grok-usage.json` as
//! `{"observed_ms","pct","resets_at"}` only (0600, atomic), at most 10 minutes old. A denied,
//! failed, or unparsable fetch falls back to a cache not older than [`quota::MAX_AGE_MS`].
//! `CODEFORGE_USAGE_GROK=0` reads nothing and calls nothing.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use super::alias;
use super::quota;

/// The `grok` binary (resolved on PATH; no `grok` on PATH is no capture, not an error).
const BIN: &str = "grok";
/// The capture is rerun at most this often.
pub const CACHE_MS: i64 = 10 * 60 * 1000;
/// `grok --version` is killed after this.
const VERSION_TIMEOUT_S: u64 = 5;
/// The billing request is bounded by this.
const HTTP_TIMEOUT_S: u64 = 10;
/// Set to `0`: never read the login file and never call the billing endpoint.
pub const ENV_DISABLE: &str = "CODEFORGE_USAGE_GROK";
/// Cache file name in the summary dir.
const CACHE_FILE: &str = "grok-usage.json";
/// The cache holds three numbers: anything bigger is no cache.
const CACHE_MAX: usize = 4096;
/// The login file is never read above this size.
const AUTH_MAX: u64 = 64 * 1024;
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

/// What one attempt to read the live quota produced. Never carries the bearer.
pub enum Fetch {
    /// A billing JSON body. Untrusted; only [`parse_billing`] decides what is kept.
    Body(Value),
    /// HTTP 401/403, or the login's `expires_at` is already due. Do not refresh.
    Denied,
    /// Off switch, no binary, transport error, or a login file that cannot be used.
    Failed,
}

/// Bearer from `auth.json`.
///
/// Deliberately implements neither `Debug` nor `Display` (no derive, no manual impl):
/// `format!("{:?}", key)` does not compile, so the key cannot be written by a log or an
/// error. The leak tests check every string the collector actually produces.
struct Key(String);

impl Key {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        // A bearer is one visible ASCII token. The error text is fixed and never echoes `s`.
        if s.is_empty()
            || !s.is_ascii()
            || s.bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            || reqwest::header::HeaderValue::from_str(&s).is_err()
        {
            return Err(serde::de::Error::custom("unusable"));
        }
        Ok(Key(s))
    }
}

/// The two fields of the login entry. Unknown fields are ignored (`refresh_token`, `email`, …).
#[derive(Deserialize)]
struct Cred {
    key: Key,
    expires_at: String,
}

enum KeyState {
    Ok(Key),
    Expired,
    Unusable,
}

/// The login in `auth_json_bytes` at `now`. The file is one JSON object with exactly one entry;
/// the entry's name is ignored and its value must be an object. Only `key` and `expires_at`
/// are read. `expires_at <= now` is [`KeyState::Expired`]. Anything else off that shape is
/// [`KeyState::Unusable`]. No error string is built: a serde failure is dropped, never formatted.
fn read_key(auth_json_bytes: &[u8], now: DateTime<Utc>) -> KeyState {
    let Ok(map) = serde_json::from_slice::<serde_json::Map<String, Value>>(auth_json_bytes) else {
        return KeyState::Unusable;
    };
    if map.len() != 1 {
        return KeyState::Unusable;
    }
    let (_, value) = map.into_iter().next().unwrap();
    let Ok(cred) = serde_json::from_value::<Cred>(value) else {
        return KeyState::Unusable;
    };
    let Ok(exp) = DateTime::parse_from_rfc3339(&cred.expires_at) else {
        return KeyState::Unusable;
    };
    if exp.with_timezone(&Utc) <= now {
        return KeyState::Expired;
    }
    KeyState::Ok(cred.key)
}

/// One weekly snapshot from a billing body, or nothing when a field is missing, the wrong type,
/// the percent is outside 0..=150, or the reset time is not RFC3339. `pct` is the used fraction
/// (`creditUsagePercent / 100`). `five_hour` is always `None`.
pub fn parse_billing(body: &Value, observed_ms: i64, key_hash: &str) -> Option<quota::Snapshot> {
    let config = body.get("config")?;
    let raw = config.get("creditUsagePercent")?.as_f64()?;
    if !raw.is_finite() || !(0.0..=150.0).contains(&raw) {
        return None;
    }
    let end = config.get("currentPeriod")?.get("end")?.as_str()?;
    let resets_at = DateTime::parse_from_rfc3339(end).ok()?.timestamp();
    Some(quota::Snapshot {
        alias: format!("grok-{key_hash}"),
        org: None,
        email: None,
        root: None,
        written_at_ms: observed_ms,
        five_hour: None,
        seven_day: Some(quota::Window {
            pct: raw / 100.0,
            resets_at,
            seen: Some(quota::Seen {
                at_ms: observed_ms,
                session: String::new(),
            }),
        }),
    })
}

/// The cache file: the three numbers a snapshot needs, and nothing else.
#[derive(Deserialize)]
struct Cache {
    observed_ms: i64,
    pct: f64,
    resets_at: i64,
}

fn read_cache(path: &Path) -> Option<Cache> {
    let file = quota::open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > CACHE_MAX as u64 {
        return None;
    }
    let mut buf = Vec::new();
    file.take(CACHE_MAX as u64 + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > CACHE_MAX as u64 {
        return None;
    }
    serde_json::from_slice(&buf).ok()
}

fn snapshot_from_cache(c: &Cache, key_hash: &str) -> Option<quota::Snapshot> {
    if !c.pct.is_finite() || !(0.0..=1.5).contains(&c.pct) {
        return None;
    }
    Some(quota::Snapshot {
        alias: format!("grok-{key_hash}"),
        org: None,
        email: None,
        root: None,
        written_at_ms: c.observed_ms,
        five_hour: None,
        seven_day: Some(quota::Window {
            pct: c.pct,
            resets_at: c.resets_at,
            seen: Some(quota::Seen {
                at_ms: c.observed_ms,
                session: String::new(),
            }),
        }),
    })
}

/// The host's grok snapshot at `now_ms`. A cache at `cache_dir`/`grok-usage.json` younger than
/// [`CACHE_MS`] is used and `fetch` is not called. Otherwise `fetch` runs: a [`Fetch::Body`]
/// that parses is written back (those three fields only) and returned; [`Fetch::Denied`],
/// [`Fetch::Failed`], and an unparsable body fall back to a cache younger than
/// [`quota::MAX_AGE_MS`], else nothing. The key is the `path_hash8` of `home`.
pub fn snapshots(
    cache_dir: &Path,
    home: &Path,
    now_ms: i64,
    fetch: impl FnOnce() -> Fetch,
) -> Vec<quota::Snapshot> {
    let key_hash = alias::path_hash8(home);
    let cache = read_cache(&cache_dir.join(CACHE_FILE));
    let within =
        |at_ms: i64, limit: i64| (0..limit).contains(&now_ms.checked_sub(at_ms).unwrap_or(limit));
    if let Some(c) = cache.as_ref().filter(|c| within(c.observed_ms, CACHE_MS)) {
        return snapshot_from_cache(c, &key_hash).into_iter().collect();
    }
    let fallback = || {
        cache
            .as_ref()
            .filter(|c| within(c.observed_ms, quota::MAX_AGE_MS))
            .and_then(|c| snapshot_from_cache(c, &key_hash))
            .into_iter()
            .collect()
    };
    match fetch() {
        Fetch::Body(body) => {
            let Some(snap) = parse_billing(&body, now_ms, &key_hash) else {
                return fallback();
            };
            // `parse_billing`'s Some always carries the weekly window.
            let (pct, resets_at) = {
                let w = snap.seven_day.as_ref().unwrap();
                (w.pct, w.resets_at)
            };
            let _ = crate::live::write_live_json(
                cache_dir,
                CACHE_FILE,
                &json!({ "observed_ms": now_ms, "pct": pct, "resets_at": resets_at }),
            );
            vec![snap]
        }
        Fetch::Denied | Fetch::Failed => fallback(),
    }
}

/// The live quota. [`ENV_DISABLE`] `=0` returns [`Fetch::Failed`] before PATH is read and
/// before the login file is opened. Otherwise `grok` must resolve on PATH and `grok --version`
/// (killed after [`VERSION_TIMEOUT_S`]) must print `grok <version> …`; then
/// `home/.grok/auth.json` is read. Expired is [`Fetch::Denied`] and the endpoint is not called.
/// The bearer leaves this process only as the `Authorization` header of [`BILLING_URL`].
pub fn fetch_live(home: &Path) -> Fetch {
    if std::env::var(ENV_DISABLE).is_ok_and(|v| v == "0") {
        return Fetch::Failed;
    }
    let Some(path) = std::env::var_os("PATH") else {
        return Fetch::Failed;
    };
    let Some(bin) = find_on_path(BIN, &path) else {
        return Fetch::Failed;
    };
    let Some(version) = grok_version(&bin) else {
        return Fetch::Failed;
    };
    let Some(bytes) = read_auth(&home.join(".grok").join("auth.json")) else {
        return Fetch::Failed;
    };
    match read_key(&bytes, Utc::now()) {
        KeyState::Ok(key) => request(&key, &version),
        KeyState::Expired => Fetch::Denied,
        KeyState::Unusable => Fetch::Failed,
    }
}

fn read_auth(path: &Path) -> Option<Vec<u8>> {
    let file = quota::open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > AUTH_MAX {
        return None;
    }
    let mut buf = Vec::new();
    file.take(AUTH_MAX + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > AUTH_MAX {
        return None;
    }
    Some(buf)
}

/// `grok 1.0.70 (f50904eaee6c)` → `1.0.70`. Anything else is no version.
fn parse_version(text: &str) -> Option<String> {
    let mut parts = text.split_whitespace();
    if parts.next()? != "grok" {
        return None;
    }
    let ver = parts.next()?;
    let ok = ver.contains('.')
        && ver
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    ok.then(|| ver.to_string())
}

fn grok_version(bin: &Path) -> Option<String> {
    let bin = bin.to_str()?;
    let child = crate::platform::low_priority_command(&[bin, "--version"], VERSION_TIMEOUT_S)
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let _guard = crate::platform::guard_timeout(child.id(), VERSION_TIMEOUT_S);
    let out = child.wait_with_output().ok()?;
    drop(_guard);
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

fn request(key: &Key, version: &str) -> Fetch {
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        return Fetch::Failed;
    };
    rt.block_on(request_async(key, version))
}

async fn request_async(key: &Key, version: &str) -> Fetch {
    let token = key.as_str();
    let Ok(mut auth) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) else {
        return Fetch::Failed;
    };
    auth.set_sensitive(true);
    let Ok(ua) = reqwest::header::HeaderValue::from_str(&format!("grok/{version}")) else {
        return Fetch::Failed;
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_S))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
    else {
        return Fetch::Failed;
    };
    let Ok(resp) = client
        .get(BILLING_URL)
        .header(reqwest::header::AUTHORIZATION, auth)
        .header(reqwest::header::USER_AGENT, ua)
        .send()
        .await
    else {
        return Fetch::Failed;
    };
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Fetch::Denied;
    }
    if !status.is_success() {
        return Fetch::Failed;
    }
    match resp.json().await {
        Ok(v) => Fetch::Body(v),
        Err(_) => Fetch::Failed,
    }
}

/// The first executable `name` in an absolute directory of `path` (empty and relative entries
/// are skipped). On Windows the `.exe`, `.cmd` and `.bat` forms are tried.
fn find_on_path(name: &str, path: &std::ffi::OsStr) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    let names: Vec<String> = ["exe", "cmd", "bat"]
        .iter()
        .map(|e| format!("{name}.{e}"))
        .collect();
    #[cfg(not(windows))]
    let names: Vec<String> = vec![name.to_string()];
    std::env::split_paths(path)
        .filter(|d| d.is_absolute())
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW_MS: i64 = 1_793_600_000_000;
    const KEY_HASH: &str = "1a2b3c4d";
    /// Fake bearer. It must not appear in any cache, snapshot, summary, or error string.
    const SENTINEL: &str = "sk-TEST-SENTINEL-0123456789";
    /// `2026-11-12T03:42:18Z`.
    const RESETS_AT: i64 = 1_794_454_938;

    fn ts(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn home() -> std::path::PathBuf {
        std::path::PathBuf::from("/home/t")
    }

    fn sample() -> Value {
        billing(json!(34.0), "2026-11-12T03:42:18.768071+00:00")
    }

    fn billing(percent: Value, end: &str) -> Value {
        json!({
            "config": {
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-11-05T03:42:18.768071+00:00",
                    "end": end
                },
                "creditUsagePercent": percent,
                "onDemandCap": {"val": 0},
                "productUsage": [{"product": "GrokBuild", "usagePercent": 34.0}],
                "isUnifiedBillingUser": true
            }
        })
    }

    fn auth(key: &str, exp: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "ignored-entry-name": {
                "key": key,
                "expires_at": exp,
                "refresh_token": "refresh-must-not-be-read",
                "email": "nope@example.com",
                "user_id": "user-must-not-be-read"
            }
        }))
        .unwrap()
    }

    fn window(s: &quota::Snapshot) -> &quota::Window {
        s.seven_day.as_ref().unwrap()
    }

    /// Fail without echoing `hay` (it may be the sentinel).
    fn no_sentinel(hay: &[u8]) {
        assert!(
            !hay.windows(SENTINEL.len())
                .any(|w| w == SENTINEL.as_bytes()),
            "sentinel appeared in an output that must not carry the login key"
        );
    }

    fn outputs_of(snaps: &[quota::Snapshot], now_ms: i64) -> Vec<u8> {
        let summary = crate::usage::summary::build(
            &[],
            snaps,
            "host-aaaaaaaa",
            now_ms,
            &crate::usage::names::Names::default(),
        );
        let mut buf = format!("{snaps:?}").into_bytes();
        for s in snaps {
            buf.extend(s.to_json().to_string().into_bytes());
        }
        buf.extend(serde_json::to_vec(&summary).unwrap());
        buf
    }

    #[test]
    fn the_billing_body_is_a_weekly_snapshot() {
        assert_eq!(ts("2026-11-12T03:42:18Z"), RESETS_AT);
        let s = parse_billing(&sample(), NOW_MS, KEY_HASH).unwrap();
        assert_eq!(s.alias, format!("grok-{KEY_HASH}"));
        assert_eq!(
            (s.org.as_ref(), s.email.as_ref(), s.root.as_ref()),
            (None, None, None)
        );
        assert_eq!(s.written_at_ms, NOW_MS);
        assert!(s.five_hour.is_none());
        let w = window(&s);
        assert_eq!(w.pct, 0.34);
        assert_eq!(w.resets_at, RESETS_AT);
        assert_eq!(
            w.seen,
            Some(quota::Seen {
                at_ms: NOW_MS,
                session: String::new(),
            })
        );
        // the bounds, still a number (an integer JSON number is a number)
        let lo = parse_billing(&billing(json!(0), "2026-11-12T03:42:18Z"), NOW_MS, KEY_HASH);
        assert_eq!(window(&lo.unwrap()).pct, 0.0);
        let hi = parse_billing(
            &billing(json!(150), "2026-11-12T03:42:18Z"),
            NOW_MS,
            KEY_HASH,
        );
        assert_eq!(window(&hi.unwrap()).pct, 1.5);
    }

    #[test]
    fn a_body_off_the_shape_is_no_snapshot() {
        assert!(parse_billing(&json!({}), NOW_MS, KEY_HASH).is_none());
        assert!(parse_billing(&json!({"config": "x"}), NOW_MS, KEY_HASH).is_none());
        assert!(parse_billing(
            &billing(json!("34"), "2026-11-12T03:42:18Z"),
            NOW_MS,
            KEY_HASH
        )
        .is_none());
        assert!(parse_billing(
            &billing(json!(151), "2026-11-12T03:42:18Z"),
            NOW_MS,
            KEY_HASH
        )
        .is_none());
        assert!(parse_billing(
            &billing(json!(-1), "2026-11-12T03:42:18Z"),
            NOW_MS,
            KEY_HASH
        )
        .is_none());
        assert!(parse_billing(&billing(json!(34.0), "not-a-date"), NOW_MS, KEY_HASH).is_none());
        let mut no_end = sample();
        no_end["config"]["currentPeriod"]
            .as_object_mut()
            .unwrap()
            .remove("end");
        assert!(parse_billing(&no_end, NOW_MS, KEY_HASH).is_none());
    }

    #[test]
    fn read_key_accepts_a_future_login_and_ignores_every_other_field() {
        let now = at("2026-11-11T12:00:00Z");
        let bytes = auth(SENTINEL, "2099-01-01T00:00:00.000000000Z");
        match read_key(&bytes, now) {
            KeyState::Ok(k) => assert!(k.as_str() == SENTINEL),
            KeyState::Expired | KeyState::Unusable => panic!("expected Ok"),
        }
        // the nanosecond form from the login file, still in the future
        let later = at("2026-11-11T15:27:38.257878021Z");
        match read_key(&auth(SENTINEL, "2026-11-11T15:27:38.257878022Z"), later) {
            KeyState::Ok(k) => assert!(k.as_str() == SENTINEL),
            KeyState::Expired | KeyState::Unusable => panic!("expected Ok"),
        }
    }

    #[test]
    fn read_key_expired_and_unusable_carry_no_key() {
        let now = at("2026-11-11T15:27:38.257878022Z");
        assert!(matches!(
            read_key(&auth(SENTINEL, "2026-11-11T15:27:38.257878022Z"), now),
            KeyState::Expired
        ));
        assert!(matches!(
            read_key(&auth(SENTINEL, "2020-01-01T00:00:00.000000000Z"), now),
            KeyState::Expired
        ));
        let malformed = format!("not json {SENTINEL}");
        let bad: &[&[u8]] = &[
            b"[]",
            b"\"x\"",
            b"null",
            b"123",
            b"",
            b"{",
            b"{}",
            br#"{"a":{"expires_at":"2099-01-01T00:00:00Z"},"b":{"key":"k","expires_at":"2099-01-01T00:00:00Z"}}"#,
            br#"{"acct":{"expires_at":"2099-01-01T00:00:00Z"}}"#,
            br#"{"acct":{"key":1,"expires_at":"2099-01-01T00:00:00Z"}}"#,
            br#"{"acct":"sk-TEST-SENTINEL-0123456789"}"#,
            malformed.as_bytes(),
        ];
        for b in bad {
            assert!(
                matches!(read_key(b, now), KeyState::Unusable),
                "unusable login"
            );
        }
    }

    #[test]
    fn a_fresh_cache_is_used_without_calling_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let observed = NOW_MS - 60_000;
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({ "observed_ms": observed, "pct": 0.34, "resets_at": RESETS_AT }),
        )
        .unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || -> Fetch {
            panic!("fetch must not be called on a fresh cache")
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        assert_eq!(window(&got[0]).pct, 0.34);
        assert_eq!(got[0].alias, format!("grok-{}", alias::path_hash8(&home())));
        assert!(got[0].five_hour.is_none());
    }

    #[test]
    fn denied_with_a_stale_cache_returns_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let observed = NOW_MS - 11 * 60_000;
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({ "observed_ms": observed, "pct": 0.34, "resets_at": RESETS_AT }),
        )
        .unwrap();
        let before = std::fs::read(dir.path().join(CACHE_FILE)).unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Denied);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        assert_eq!(std::fs::read(dir.path().join(CACHE_FILE)).unwrap(), before);
        // an over-aged cache is not returned
        std::fs::remove_file(dir.path().join(CACHE_FILE)).unwrap();
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({ "observed_ms": NOW_MS - 7 * 3_600_000, "pct": 0.34, "resets_at": RESETS_AT }),
        )
        .unwrap();
        assert!(snapshots(dir.path(), &home(), NOW_MS, || Fetch::Denied).is_empty());
    }

    #[test]
    fn failed_with_no_cache_is_empty_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(snapshots(dir.path(), &home(), NOW_MS, || Fetch::Failed).is_empty());
        assert!(!dir.path().join(CACHE_FILE).exists());
        assert!(snapshots(dir.path(), &home(), NOW_MS, || Fetch::Denied).is_empty());
        assert!(snapshots(dir.path(), &home(), NOW_MS, || Fetch::Body(json!({}))).is_empty());
        assert!(!dir.path().join(CACHE_FILE).exists());
    }

    #[test]
    fn a_body_writes_the_cache_and_the_next_call_does_not_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Body(sample()));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, NOW_MS);
        assert_eq!(window(&got[0]).resets_at, RESETS_AT);
        let path = dir.path().join(CACHE_FILE);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let raw = std::fs::read(&path).unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["observed_ms", "pct", "resets_at"]);
        assert_eq!(v["pct"], json!(0.34));
        assert_eq!(v["resets_at"], json!(RESETS_AT));
        let again = snapshots(dir.path(), &home(), NOW_MS + 60_000, || -> Fetch {
            panic!("fetch must not be called on a fresh cache")
        });
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].written_at_ms, NOW_MS);
    }

    #[test]
    fn an_unparsable_body_falls_back_to_the_stale_cache_without_rewriting_it() {
        let dir = tempfile::tempdir().unwrap();
        let observed = NOW_MS - 11 * 60_000;
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({ "observed_ms": observed, "pct": 0.34, "resets_at": RESETS_AT }),
        )
        .unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || {
            Fetch::Body(billing(json!("34"), "2026-11-12T03:42:18Z"))
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        let v: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap()).unwrap();
        assert_eq!(v["observed_ms"], json!(observed));
    }

    #[test]
    fn version_line_keeps_only_the_number() {
        assert_eq!(
            parse_version("grok 1.0.70 (f50904eaee6c)\n").as_deref(),
            Some("1.0.70")
        );
        assert_eq!(parse_version("grok\n"), None);
        assert_eq!(parse_version("other 1.0.70\n"), None);
        assert_eq!(parse_version("grok 1\n"), None);
        assert_eq!(parse_version("grok 1.0.70-dev\n"), None);
    }

    #[test]
    fn the_off_switch_returns_failed_without_a_sendable_login() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".grok");
        std::fs::create_dir_all(&dir).unwrap();
        // malformed on purpose: even a bug that skipped the off switch must not obtain a bearer
        std::fs::write(dir.join("auth.json"), format!("not json {SENTINEL}")).unwrap();
        struct ClearEnv(&'static str);
        impl Drop for ClearEnv {
            fn drop(&mut self) {
                std::env::remove_var(self.0);
            }
        }
        std::env::set_var(ENV_DISABLE, "0");
        let _guard = ClearEnv(ENV_DISABLE);
        // Failed is a unit variant: it has no string to carry the login.
        assert!(matches!(fetch_live(home.path()), Fetch::Failed));
    }

    #[test]
    fn the_sentinel_never_appears_in_cache_snapshots_summary_or_errors() {
        let now = at("2026-11-11T12:00:00Z");
        let errors: Vec<String> = Vec::new();
        // KeyState is not Debug and not Display. These arms produce no error string.
        match read_key(format!("not json {SENTINEL}").as_bytes(), now) {
            KeyState::Unusable => {}
            KeyState::Ok(_) | KeyState::Expired => panic!("malformed auth must be Unusable"),
        }
        match read_key(&auth(SENTINEL, "2020-01-01T00:00:00Z"), now) {
            KeyState::Expired => {}
            KeyState::Ok(_) | KeyState::Unusable => panic!("past expiry must be Expired"),
        }
        match read_key(&auth(SENTINEL, "2099-01-01T00:00:00.000000000Z"), now) {
            KeyState::Ok(k) => assert!(k.as_str() == SENTINEL),
            KeyState::Expired | KeyState::Unusable => panic!("expected Ok"),
        }
        assert!(
            errors.is_empty(),
            "these paths produce no error string (KeyState is not Display)"
        );

        let mut body = sample();
        body["echo"] = json!(SENTINEL);
        body["config"]["note"] = json!(SENTINEL);

        // Failed, no cache.
        let dir = tempfile::tempdir().unwrap();
        let snaps = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Failed);
        assert!(snaps.is_empty());
        no_sentinel(&outputs_of(&snaps, NOW_MS));
        assert!(!dir.path().join(CACHE_FILE).exists());

        // Ok body that also carries the sentinel in fields we must not keep.
        let snaps = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Body(body));
        assert_eq!(snaps.len(), 1);
        assert_eq!(window(&snaps[0]).pct, 0.34);
        let summary = crate::usage::summary::build(
            &[],
            &snaps,
            "host-aaaaaaaa",
            NOW_MS,
            &crate::usage::names::Names::default(),
        );
        crate::usage::summary::validate(&summary).unwrap();
        assert_eq!(summary["accounts"][0]["name"], "grok-cli");
        assert!(summary["accounts"][0]["five_hour"].is_null());
        no_sentinel(&outputs_of(&snaps, NOW_MS));
        let cache = std::fs::read(dir.path().join(CACHE_FILE)).unwrap();
        no_sentinel(&cache);
        assert!(!cache.windows(b"echo".len()).any(|w| w == b"echo"));

        // Denied against that cache (now stale): still no sentinel, cache unchanged.
        let stale_at = NOW_MS + CACHE_MS + 1;
        let snaps = snapshots(dir.path(), &home(), stale_at, || Fetch::Denied);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].written_at_ms, NOW_MS);
        no_sentinel(&outputs_of(&snaps, stale_at));
        no_sentinel(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap());

        // Failed with the cache still inside the max age.
        let snaps = snapshots(dir.path(), &home(), stale_at, || Fetch::Failed);
        assert_eq!(snaps.len(), 1);
        no_sentinel(&outputs_of(&snaps, stale_at));

        // malformed body after the cache has aged out
        let aged = NOW_MS + quota::MAX_AGE_MS + 1;
        let snaps = snapshots(dir.path(), &home(), aged, || {
            Fetch::Body(json!({ "key": SENTINEL }))
        });
        assert!(snaps.is_empty());
        no_sentinel(&outputs_of(&snaps, aged));
        // the old cache is left in place and still has no sentinel
        no_sentinel(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn find_on_path_takes_only_an_executable_in_an_absolute_dir() {
        use std::os::unix::fs::PermissionsExt;
        let none = tempfile::tempdir().unwrap();
        let plain = tempfile::tempdir().unwrap();
        let exec = tempfile::tempdir().unwrap();
        std::fs::write(plain.path().join("grok"), "").unwrap();
        let bin = exec.path().join("grok");
        std::fs::write(&bin, "").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
        assert_eq!(
            find_on_path("grok", &path(&[none.path(), plain.path(), exec.path()])),
            Some(bin.clone())
        );
        assert_eq!(
            find_on_path("grok", &path(&[none.path(), plain.path()])),
            None
        );
        assert_eq!(find_on_path("grok", std::ffi::OsStr::new(".:")), None);
    }
}
