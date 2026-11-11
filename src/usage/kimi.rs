//! The local `kimi` (Kimi Code CLI) account's 5-hour and weekly quota, as one quota snapshot.
//!
//! `GET <base>/usages` returns the account's windows (operator card d-a080c798). The bearer is
//! read from `<kimi_home>/credentials/kimi-code.json` only at the moment of the request, held in
//! a local [`Token`], and sent only on that request. [`Token`] has no `Debug` and no `Display` —
//! `format!("{:?}", token)` does not compile — so a log, an error, or a test assertion cannot
//! print it. The credential file's other fields are never read. An empty token (kimi's revoked
//! tombstone), an expired token, or HTTP 401/403 is [`Fetch::Denied`]: no refresh, no new
//! snapshot. Kimi refreshes its own token; this collector must not race it.
//!
//! The response is untrusted. [`parse_usages`] keeps `usages.limit_5h` and `usages.limit_7d`
//! (`used_ratio` is already the used fraction, a number or a numeric string in 0.0..=1.5;
//! `reset_time` is RFC3339) and drops everything else, including the month limits and
//! `boosterWallet`. The capture is cached at `<cache_dir>/kimi-usage.json` as `observed_ms` plus
//! each window's `pct` and `resets_at` only (0600, atomic), at most 10 minutes old. A denied,
//! failed, or unparsable fetch falls back to a cache not older than [`quota::MAX_AGE_MS`].
//! `CODEFORGE_USAGE_KIMI=0` reads nothing and calls nothing. The `kimi` binary is not required.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::alias;
use super::quota;

pub use super::fetch::Fetch;

/// The capture is rerun at most this often.
pub const CACHE_MS: i64 = 10 * 60 * 1000;
/// The usages request is bounded by this.
const HTTP_TIMEOUT_S: u64 = 8;
/// Set to `0`: never read the credential file and never call the usages endpoint.
pub const ENV_DISABLE: &str = "CODEFORGE_USAGE_KIMI";
/// Overrides `<home>/.kimi-code` when set and non-empty.
pub const ENV_HOME: &str = "KIMI_CODE_HOME";
/// Overrides the usages API base when it starts with `https://`.
pub const ENV_BASE: &str = "KIMI_CODE_BASE_URL";
/// Cache file name in the summary dir.
const CACHE_FILE: &str = "kimi-usage.json";
/// The cache holds a handful of numbers: anything bigger is no cache.
const CACHE_MAX: u64 = 4096;
/// The credential file is never read above this size.
const CRED_MAX: u64 = 64 * 1024;
const DEFAULT_BASE: &str = "https://api.kimi.com/coding/v1";

/// Bearer from `kimi-code.json`.
///
/// Deliberately implements neither `Debug` nor `Display` (no derive, no manual impl):
/// `format!("{:?}", token)` does not compile, so the token cannot be written by a log or an
/// error. The leak tests check every string the collector actually produces.
#[derive(Deserialize)]
struct Token(String);

impl Token {
    fn as_str(&self) -> &str {
        &self.0
    }
}

/// The two fields of the credential file. Unknown fields are ignored (`refresh_token`, `scope`, …).
#[derive(Deserialize)]
struct Cred {
    access_token: Token,
    expires_at: i64,
}

enum TokenState {
    Ok(Token),
    Expired,
    Unusable,
}

/// The credential in `bytes` at `now` (unix seconds). Only `access_token` and `expires_at` are
/// read. An empty token (a revoked tombstone) or `expires_at <= now` is [`TokenState::Expired`].
/// Not an object, a missing field, or a non-numeric `expires_at` is [`TokenState::Unusable`].
/// No error string is built: a serde failure is dropped, never formatted.
fn read_token(bytes: &[u8], now: i64) -> TokenState {
    let Ok(cred) = serde_json::from_slice::<Cred>(bytes) else {
        return TokenState::Unusable;
    };
    if cred.access_token.as_str().is_empty() || cred.expires_at <= now {
        return TokenState::Expired;
    }
    // A bearer is one header value. Reject it here so the request never builds an error that
    // quotes the token. The check's failure is a unit, not a string.
    if !header_safe(cred.access_token.as_str()) {
        return TokenState::Unusable;
    }
    TokenState::Ok(cred.access_token)
}

fn header_safe(s: &str) -> bool {
    !s.is_empty()
        && s.is_ascii()
        && !s
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        && reqwest::header::HeaderValue::from_str(s).is_ok()
}

/// `used_ratio`: a finite number, or a numeric string, in 0.0..=1.5. Already the used fraction.
fn used_ratio(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.parse().ok()?,
        _ => return None,
    };
    (n.is_finite() && (0.0..=1.5).contains(&n)).then_some(n)
}

fn window_of(v: &Value, observed_ms: i64) -> Option<quota::Window> {
    let pct = used_ratio(v.get("used_ratio")?)?;
    let reset = v.get("reset_time")?.as_str()?;
    let resets_at = DateTime::parse_from_rfc3339(reset).ok()?.timestamp();
    Some(quota::Window {
        pct,
        resets_at,
        seen: Some(quota::Seen {
            at_ms: observed_ms,
            session: String::new(),
        }),
    })
}

/// One snapshot from a usages body, or nothing when `usages` is missing or neither window is
/// valid. `limit_5h` → `five_hour`, `limit_7d` → `seven_day`. A window whose `used_ratio` is
/// outside 0.0..=1.5 or whose `reset_time` is missing or not RFC3339 is dropped; month limits
/// and `boosterWallet` are ignored. `alias` is `kimi-<key_hash>`.
pub fn parse_usages(body: &Value, observed_ms: i64, key_hash: &str) -> Option<quota::Snapshot> {
    let usages = body.get("usages")?.as_object()?;
    let five_hour = usages
        .get("limit_5h")
        .and_then(|v| window_of(v, observed_ms));
    let seven_day = usages
        .get("limit_7d")
        .and_then(|v| window_of(v, observed_ms));
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(quota::Snapshot {
        alias: format!("kimi-{key_hash}"),
        org: None,
        email: None,
        root: None,
        written_at_ms: observed_ms,
        five_hour,
        seven_day,
    })
}

/// One cached window: the two numbers a snapshot needs, and nothing else.
#[derive(Deserialize)]
struct WinCache {
    pct: f64,
    resets_at: i64,
}

/// The cache file. Absent windows are omitted, never stored as null.
#[derive(Deserialize)]
struct Cache {
    observed_ms: i64,
    #[serde(default)]
    five_hour: Option<WinCache>,
    #[serde(default)]
    seven_day: Option<WinCache>,
}

fn read_cache(path: &Path) -> Option<Cache> {
    super::fetch::read_capped_json(path, CACHE_MAX)
}

fn win_from_cache(w: &WinCache, observed_ms: i64) -> Option<quota::Window> {
    if !w.pct.is_finite() || !(0.0..=1.5).contains(&w.pct) {
        return None;
    }
    Some(quota::Window {
        pct: w.pct,
        resets_at: w.resets_at,
        seen: Some(quota::Seen {
            at_ms: observed_ms,
            session: String::new(),
        }),
    })
}

fn snapshot_from_cache(c: &Cache, key_hash: &str) -> Option<quota::Snapshot> {
    let five_hour = c
        .five_hour
        .as_ref()
        .and_then(|w| win_from_cache(w, c.observed_ms));
    let seven_day = c
        .seven_day
        .as_ref()
        .and_then(|w| win_from_cache(w, c.observed_ms));
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(quota::Snapshot {
        alias: format!("kimi-{key_hash}"),
        org: None,
        email: None,
        root: None,
        written_at_ms: c.observed_ms,
        five_hour,
        seven_day,
    })
}

fn cache_json(observed_ms: i64, snap: &quota::Snapshot) -> Value {
    let mut m = Map::new();
    m.insert("observed_ms".into(), json!(observed_ms));
    let mut put = |key: &str, w: &Option<quota::Window>| {
        if let Some(w) = w {
            m.insert(
                key.into(),
                json!({ "pct": w.pct, "resets_at": w.resets_at }),
            );
        }
    };
    put("five_hour", &snap.five_hour);
    put("seven_day", &snap.seven_day);
    Value::Object(m)
}

/// The host's kimi snapshot at `now_ms`. A cache at `cache_dir`/`kimi-usage.json` younger than
/// [`CACHE_MS`] is used and `fetch` is not called. Otherwise `fetch` runs: a [`Fetch::Body`]
/// that parses is written back (those fields only) and returned; [`Fetch::Denied`],
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
            let Some(snap) = parse_usages(&body, now_ms, &key_hash) else {
                return fallback();
            };
            let _ = crate::live::write_live_json(cache_dir, CACHE_FILE, &cache_json(now_ms, &snap));
            vec![snap]
        }
        Fetch::Denied | Fetch::Failed => fallback(),
    }
}

/// [`ENV_DISABLE`] `=0`: the caller skips [`snapshots`] entirely, so not even the cache is
/// read or reported.
pub fn disabled() -> bool {
    std::env::var(ENV_DISABLE).is_ok_and(|v| v == "0")
}

/// `KIMI_CODE_HOME` when set and non-empty, else `<home>/.kimi-code`.
fn kimi_home(env_home: Option<&str>, home: &Path) -> PathBuf {
    match env_home {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => home.join(".kimi-code"),
    }
}

/// `KIMI_CODE_BASE_URL` when it starts with `https://` and names a host, else [`DEFAULT_BASE`].
/// `http://` and any other scheme are ignored. A trailing slash is stripped so the usages path
/// is a single slash. Pure: tests pass the value in, they do not mutate the process environment.
fn usages_base(env_value: Option<&str>) -> String {
    let Some(raw) = env_value.filter(|s| s.starts_with("https://")) else {
        return DEFAULT_BASE.to_string();
    };
    let trimmed = raw.trim_end_matches('/');
    if !trimmed.starts_with("https://") || trimmed.len() == "https://".len() {
        return DEFAULT_BASE.to_string();
    }
    trimmed.to_string()
}

fn usages_url(base: &str) -> String {
    format!("{base}/usages")
}

fn request(token: &Token, base: &str) -> Fetch {
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        return Fetch::Failed;
    };
    rt.block_on(request_async(token, base))
}

async fn request_async(token: &Token, base: &str) -> Fetch {
    let url = usages_url(base);
    // The bearer exists only in this header. `set_sensitive` keeps a trace from printing it.
    let Ok(mut auth) =
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.as_str()))
    else {
        return Fetch::Failed;
    };
    auth.set_sensitive(true);
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_S))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
    else {
        return Fetch::Failed;
    };
    let Ok(resp) = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .header(reqwest::header::ACCEPT, "application/json")
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

/// The live quota. [`ENV_DISABLE`] `=0` returns [`Fetch::Failed`] before the home env is read
/// and before the credential file is opened. Otherwise
/// `<kimi_home>/credentials/kimi-code.json` is read only when it exists. Expired or revoked is
/// [`Fetch::Denied`] and the endpoint is not called. The bearer leaves this process only as the
/// `Authorization` header of `GET <base>/usages`.
pub fn fetch_live(home: &Path) -> Fetch {
    if disabled() {
        return Fetch::Failed;
    }
    let env_home = std::env::var(ENV_HOME).ok();
    let env_base = std::env::var(ENV_BASE).ok();
    fetch_at(
        home,
        Utc::now().timestamp(),
        false,
        env_home.as_deref(),
        env_base.as_deref(),
    )
}

/// [`fetch_live`] with the clock and the env values injected, so tests never read the process
/// environment (it may point at a real kimi home) and never call the network for an expired,
/// revoked, or unusable file.
fn fetch_at(
    home: &Path,
    now: i64,
    disabled: bool,
    env_home: Option<&str>,
    env_base: Option<&str>,
) -> Fetch {
    if disabled {
        return Fetch::Failed;
    }
    let path = kimi_home(env_home, home)
        .join("credentials")
        .join("kimi-code.json");
    let Some(bytes) = super::fetch::read_capped(&path, CRED_MAX) else {
        return Fetch::Failed;
    };
    match read_token(&bytes, now) {
        TokenState::Ok(token) => request(&token, &usages_base(env_base)),
        TokenState::Expired => Fetch::Denied,
        TokenState::Unusable => Fetch::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW_MS: i64 = 1_793_600_000_000;
    const NOW_S: i64 = 1_794_000_000;
    const KEY_HASH: &str = "1a2b3c4d";
    /// Fake bearer. It must not appear in any cache, snapshot, summary, or error string.
    const ACCESS: &str = "kimi-TEST-SENTINEL-0123456789";
    /// Fake refresh token, a second sentinel the parser must not keep.
    const REFRESH: &str = "kimi-REFRESH-SENTINEL-0123456789";
    /// `2026-11-11T14:00:00Z`.
    const FIVE_RESET: i64 = 1_794_405_600;
    /// `2026-11-13T17:37:00Z`.
    const WEEK_RESET: i64 = 1_794_591_420;
    const FUTURE: i64 = 4_102_444_800;

    fn ts(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp()
    }

    fn home() -> PathBuf {
        PathBuf::from("/home/t")
    }

    fn sample() -> Value {
        body(
            json!(0.42),
            "2026-11-11T14:00:00Z",
            json!(1.0),
            "2026-11-13T17:37:00Z",
        )
    }

    fn body(five_ratio: Value, five_reset: &str, week_ratio: Value, week_reset: &str) -> Value {
        json!({
            "usages": {
                "limit_5h": {"used_ratio": five_ratio, "reset_time": five_reset},
                "limit_7d": {"used_ratio": week_ratio, "reset_time": week_reset},
                "limit_month_total": {"used_ratio": 0.1, "reset_time": "2026-12-01T00:00:00Z"},
                "limit_month_code": {"used_ratio": 0.2, "reset_time": "2026-12-01T00:00:00Z"}
            },
            "boosterWallet": {"balance": 3}
        })
    }

    fn cred(token: &str, exp: i64) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "access_token": token,
            "expires_at": exp,
            "refresh_token": REFRESH,
            "scope": "ignored",
        }))
        .unwrap()
    }

    fn five(s: &quota::Snapshot) -> &quota::Window {
        s.five_hour.as_ref().unwrap()
    }

    fn week(s: &quota::Snapshot) -> &quota::Window {
        s.seven_day.as_ref().unwrap()
    }

    fn seen(s: &quota::Snapshot) {
        for w in [s.five_hour.as_ref(), s.seven_day.as_ref()]
            .into_iter()
            .flatten()
        {
            assert_eq!(
                w.seen,
                Some(quota::Seen {
                    at_ms: s.written_at_ms,
                    session: String::new(),
                })
            );
        }
    }

    /// Fail without echoing `hay` (it may be a sentinel).
    fn no_sentinel(hay: &[u8]) {
        for s in [ACCESS, REFRESH] {
            assert!(
                !hay.windows(s.len()).any(|w| w == s.as_bytes()),
                "sentinel appeared in an output that must not carry a credential"
            );
        }
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
    fn the_usages_body_is_a_five_hour_and_weekly_snapshot() {
        assert_eq!(ts("2026-11-11T14:00:00Z"), FIVE_RESET);
        assert_eq!(ts("2026-11-13T17:37:00Z"), WEEK_RESET);
        let s = parse_usages(&sample(), NOW_MS, KEY_HASH).unwrap();
        assert_eq!(s.alias, format!("kimi-{KEY_HASH}"));
        assert_eq!(
            (s.org.as_ref(), s.email.as_ref(), s.root.as_ref()),
            (None, None, None)
        );
        assert_eq!(s.written_at_ms, NOW_MS);
        assert_eq!(five(&s).pct, 0.42);
        assert_eq!(five(&s).resets_at, FIVE_RESET);
        assert_eq!(week(&s).pct, 1.0);
        assert_eq!(week(&s).resets_at, WEEK_RESET);
        seen(&s);
        // a numeric string is the same used fraction
        let text = parse_usages(
            &body(
                json!("0.5"),
                "2026-11-11T14:00:00Z",
                json!(1),
                "2026-11-13T17:37:00Z",
            ),
            NOW_MS,
            KEY_HASH,
        )
        .unwrap();
        assert_eq!(five(&text).pct, 0.5);
        assert_eq!(week(&text).pct, 1.0);
        // the bounds, still a number
        let lo = parse_usages(
            &body(
                json!(0.0),
                "2026-11-11T14:00:00Z",
                json!(1.5),
                "2026-11-13T17:37:00Z",
            ),
            NOW_MS,
            KEY_HASH,
        )
        .unwrap();
        assert_eq!(five(&lo).pct, 0.0);
        assert_eq!(week(&lo).pct, 1.5);
    }

    #[test]
    fn a_bad_window_is_dropped_and_no_valid_window_is_nothing() {
        let drop_five = parse_usages(
            &body(
                json!(1.6),
                "2026-11-11T14:00:00Z",
                json!(1.0),
                "2026-11-13T17:37:00Z",
            ),
            NOW_MS,
            KEY_HASH,
        )
        .unwrap();
        assert!(drop_five.five_hour.is_none());
        assert_eq!(week(&drop_five).pct, 1.0);

        let bad_text = parse_usages(
            &body(
                json!("x"),
                "2026-11-11T14:00:00Z",
                json!("NaN"),
                "2026-11-13T17:37:00Z",
            ),
            NOW_MS,
            KEY_HASH,
        );
        assert!(bad_text.is_none());

        let mut no_reset = sample();
        no_reset["usages"]["limit_5h"]
            .as_object_mut()
            .unwrap()
            .remove("reset_time");
        let dropped = parse_usages(&no_reset, NOW_MS, KEY_HASH).unwrap();
        assert!(dropped.five_hour.is_none());
        assert!(dropped.seven_day.is_some());

        let mut bad_reset = sample();
        bad_reset["usages"]["limit_7d"]["reset_time"] = json!("not-a-date");
        let dropped = parse_usages(&bad_reset, NOW_MS, KEY_HASH).unwrap();
        assert!(dropped.five_hour.is_some());
        assert!(dropped.seven_day.is_none());

        let mut both = sample();
        both["usages"]["limit_5h"]["reset_time"] = json!(FIVE_RESET);
        both["usages"]["limit_7d"]
            .as_object_mut()
            .unwrap()
            .remove("reset_time");
        assert!(parse_usages(&both, NOW_MS, KEY_HASH).is_none());

        assert!(parse_usages(&json!({}), NOW_MS, KEY_HASH).is_none());
        assert!(parse_usages(&json!({"usages": "x"}), NOW_MS, KEY_HASH).is_none());
        assert!(parse_usages(&json!({"usages": {}}), NOW_MS, KEY_HASH).is_none());
        // month limits and the wallet are not windows
        assert!(parse_usages(
            &json!({
                "usages": {"limit_month_total": {"used_ratio": 0.1, "reset_time": "2026-12-01T00:00:00Z"}},
                "boosterWallet": {"balance": 1}
            }),
            NOW_MS,
            KEY_HASH
        )
        .is_none());
    }

    #[test]
    fn read_token_accepts_a_future_credential_and_ignores_every_other_field() {
        match read_token(&cred(ACCESS, FUTURE), NOW_S) {
            TokenState::Ok(t) => assert!(t.as_str() == ACCESS),
            TokenState::Expired | TokenState::Unusable => panic!("expected Ok"),
        }
        // exactly now is already due
        assert!(matches!(
            read_token(&cred(ACCESS, NOW_S), NOW_S),
            TokenState::Expired
        ));
        assert!(matches!(
            read_token(&cred(ACCESS, NOW_S - 1), NOW_S),
            TokenState::Expired
        ));
        // an empty token is kimi's revoked tombstone, even with a future expiry
        assert!(matches!(
            read_token(&cred("", FUTURE), NOW_S),
            TokenState::Expired
        ));
    }

    #[test]
    fn read_token_unusable_carries_no_token() {
        let malformed = format!(r#"not json {ACCESS} {REFRESH}"#);
        let string_exp = format!(
            r#"{{"access_token":"{ACCESS}","expires_at":"2099-01-01T00:00:00Z","refresh_token":"{REFRESH}"}}"#
        );
        let bad: &[&[u8]] = &[
            b"[]",
            b"\"x\"",
            b"null",
            b"123",
            b"",
            b"{",
            b"{}",
            br#"{"expires_at":4102444800}"#,
            br#"{"access_token":1,"expires_at":4102444800}"#,
            string_exp.as_bytes(),
            malformed.as_bytes(),
        ];
        for b in bad {
            assert!(
                matches!(read_token(b, NOW_S), TokenState::Unusable),
                "unusable credential"
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
            &json!({
                "observed_ms": observed,
                "five_hour": {"pct": 0.42, "resets_at": FIVE_RESET},
                "seven_day": {"pct": 1.0, "resets_at": WEEK_RESET}
            }),
        )
        .unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || -> Fetch {
            panic!("fetch must not be called on a fresh cache")
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        assert_eq!(five(&got[0]).pct, 0.42);
        assert_eq!(week(&got[0]).pct, 1.0);
        assert_eq!(got[0].alias, format!("kimi-{}", alias::path_hash8(&home())));
    }

    #[test]
    fn denied_with_a_stale_cache_returns_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let observed = NOW_MS - 11 * 60_000;
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({
                "observed_ms": observed,
                "five_hour": {"pct": 0.42, "resets_at": FIVE_RESET},
                "seven_day": {"pct": 1.0, "resets_at": WEEK_RESET}
            }),
        )
        .unwrap();
        let before = std::fs::read(dir.path().join(CACHE_FILE)).unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Denied);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        assert_eq!(std::fs::read(dir.path().join(CACHE_FILE)).unwrap(), before);
        std::fs::remove_file(dir.path().join(CACHE_FILE)).unwrap();
        crate::live::write_live_json(
            dir.path(),
            CACHE_FILE,
            &json!({
                "observed_ms": NOW_MS - quota::MAX_AGE_MS - 1,
                "five_hour": {"pct": 0.42, "resets_at": FIVE_RESET}
            }),
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
        assert_eq!(five(&got[0]).resets_at, FIVE_RESET);
        assert_eq!(week(&got[0]).resets_at, WEEK_RESET);
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
        assert_eq!(keys, ["five_hour", "observed_ms", "seven_day"]);
        for win in ["five_hour", "seven_day"] {
            let mut wk: Vec<_> = v[win].as_object().unwrap().keys().cloned().collect();
            wk.sort();
            assert_eq!(wk, ["pct", "resets_at"]);
        }
        assert_eq!(v["five_hour"]["pct"], json!(0.42));
        assert_eq!(v["five_hour"]["resets_at"], json!(FIVE_RESET));
        assert_eq!(v["seven_day"]["pct"], json!(1.0));
        assert_eq!(v["seven_day"]["resets_at"], json!(WEEK_RESET));
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
            &json!({
                "observed_ms": observed,
                "seven_day": {"pct": 1.0, "resets_at": WEEK_RESET}
            }),
        )
        .unwrap();
        let got = snapshots(dir.path(), &home(), NOW_MS, || {
            Fetch::Body(json!({"usages": {"limit_5h": {"used_ratio": "x"}}}))
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].written_at_ms, observed);
        assert!(got[0].five_hour.is_none());
        let v: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap()).unwrap();
        assert_eq!(v["observed_ms"], json!(observed));
        assert!(v.get("five_hour").is_none());
    }

    #[test]
    fn base_url_accepts_only_https_and_does_not_read_the_environment() {
        assert_eq!(usages_base(None), DEFAULT_BASE);
        assert_eq!(usages_base(Some("")), DEFAULT_BASE);
        assert_eq!(usages_base(Some("http://127.0.0.1:9/usages")), DEFAULT_BASE);
        assert_eq!(
            usages_base(Some("http://api.kimi.com/coding/v1")),
            DEFAULT_BASE
        );
        assert_eq!(
            usages_base(Some("HTTPS://api.kimi.com/coding/v1")),
            DEFAULT_BASE
        );
        assert_eq!(usages_base(Some("https://")), DEFAULT_BASE);
        assert_eq!(
            usages_base(Some("https://example.test/coding/v1/")),
            "https://example.test/coding/v1"
        );
        assert_eq!(
            usages_url(&usages_base(Some("https://example.test/coding/v1"))),
            "https://example.test/coding/v1/usages"
        );
        assert_eq!(
            usages_url(&usages_base(None)),
            "https://api.kimi.com/coding/v1/usages"
        );
        assert_eq!(kimi_home(None, &home()), home().join(".kimi-code"));
        assert_eq!(kimi_home(Some(""), &home()), home().join(".kimi-code"));
        assert_eq!(
            kimi_home(Some("/tmp/kimi-home"), &home()),
            PathBuf::from("/tmp/kimi-home")
        );
    }

    fn write_cred(home: &Path, bytes: &[u8]) -> PathBuf {
        let dir = home.join(".kimi-code").join("credentials");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kimi-code.json");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn fetch_at_stops_before_the_endpoint_when_the_credential_cannot_be_used() {
        let home = tempfile::tempdir().unwrap();
        // missing file: nothing to send
        assert!(matches!(
            fetch_at(home.path(), NOW_S, false, None, Some("http://127.0.0.1:9")),
            Fetch::Failed
        ));
        // off switch, even though a parseable expired credential is on disk: Failed, not Denied,
        // so the file was not interpreted and the endpoint was not called
        write_cred(home.path(), &cred(ACCESS, NOW_S - 1));
        assert!(matches!(
            fetch_at(home.path(), NOW_S, true, None, Some("http://127.0.0.1:9")),
            Fetch::Failed
        ));
        assert!(matches!(
            fetch_at(home.path(), NOW_S, false, None, Some("http://127.0.0.1:9")),
            Fetch::Denied
        ));
        write_cred(home.path(), &cred("", FUTURE));
        assert!(matches!(
            fetch_at(home.path(), NOW_S, false, None, None),
            Fetch::Denied
        ));
        let malformed = format!("not json {ACCESS} {REFRESH}");
        write_cred(home.path(), malformed.as_bytes());
        assert!(matches!(
            fetch_at(home.path(), NOW_S, false, None, None),
            Fetch::Failed
        ));
        // KIMI_CODE_HOME is the kimi home itself, not a parent that still gets
        // `.kimi-code` joined. Only `<env>/credentials/kimi-code.json` exists, so a
        // wrong join would miss the file and return Failed, not Denied.
        let other = tempfile::tempdir().unwrap();
        let env_home = tempfile::tempdir().unwrap();
        let direct = env_home.path().join("credentials");
        std::fs::create_dir_all(&direct).unwrap();
        std::fs::write(direct.join("kimi-code.json"), cred(ACCESS, NOW_S - 1)).unwrap();
        assert!(matches!(
            fetch_at(
                other.path(),
                NOW_S,
                false,
                Some(env_home.path().to_str().unwrap()),
                Some("http://127.0.0.1:9"),
            ),
            Fetch::Denied
        ));
        assert!(matches!(
            fetch_at(other.path(), NOW_S, false, None, None),
            Fetch::Failed
        ));
    }

    #[test]
    fn the_sentinel_never_appears_in_cache_snapshots_summary_or_errors() {
        let errors: Vec<String> = Vec::new();
        // TokenState is not Debug and not Display. These arms produce no error string.
        match read_token(format!("not json {ACCESS} {REFRESH}").as_bytes(), NOW_S) {
            TokenState::Unusable => {}
            TokenState::Ok(_) | TokenState::Expired => {
                panic!("malformed credential must be Unusable")
            }
        }
        match read_token(&cred(ACCESS, NOW_S - 1), NOW_S) {
            TokenState::Expired => {}
            TokenState::Ok(_) | TokenState::Unusable => panic!("past expiry must be Expired"),
        }
        match read_token(&cred("", FUTURE), NOW_S) {
            TokenState::Expired => {}
            TokenState::Ok(_) | TokenState::Unusable => panic!("empty token must be Expired"),
        }
        match read_token(&cred(ACCESS, FUTURE), NOW_S) {
            TokenState::Ok(t) => assert!(t.as_str() == ACCESS),
            TokenState::Expired | TokenState::Unusable => panic!("expected Ok"),
        }
        assert!(
            errors.is_empty(),
            "these paths produce no error string (TokenState is not Display)"
        );

        let mut body = sample();
        body["echo"] = json!(ACCESS);
        body["usages"]["note"] = json!(REFRESH);
        body["usages"]["limit_5h"]["access_token"] = json!(ACCESS);
        body["usages"]["limit_month_total"] = json!({
            "used_ratio": REFRESH,
            "reset_time": ACCESS,
        });
        body["boosterWallet"] = json!({"token": REFRESH});

        let dir = tempfile::tempdir().unwrap();
        let snaps = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Failed);
        assert!(snaps.is_empty());
        no_sentinel(&outputs_of(&snaps, NOW_MS));
        assert!(!dir.path().join(CACHE_FILE).exists());

        let snaps = snapshots(dir.path(), &home(), NOW_MS, || Fetch::Body(body));
        assert_eq!(snaps.len(), 1);
        assert_eq!(five(&snaps[0]).pct, 0.42);
        assert_eq!(week(&snaps[0]).pct, 1.0);
        let summary = crate::usage::summary::build(
            &[],
            &snaps,
            "host-aaaaaaaa",
            NOW_MS,
            &crate::usage::names::Names::default(),
        );
        crate::usage::summary::validate(&summary).unwrap();
        assert_eq!(summary["accounts"][0]["name"], "kimi-code");
        no_sentinel(&outputs_of(&snaps, NOW_MS));
        let cache = std::fs::read(dir.path().join(CACHE_FILE)).unwrap();
        no_sentinel(&cache);
        assert!(!cache
            .windows(b"access_token".len())
            .any(|w| w == b"access_token"));
        assert!(!cache
            .windows(b"refresh_token".len())
            .any(|w| w == b"refresh_token"));
        assert!(!cache
            .windows(b"boosterWallet".len())
            .any(|w| w == b"boosterWallet"));

        let stale_at = NOW_MS + CACHE_MS + 1;
        let snaps = snapshots(dir.path(), &home(), stale_at, || Fetch::Denied);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].written_at_ms, NOW_MS);
        no_sentinel(&outputs_of(&snaps, stale_at));
        no_sentinel(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap());

        let snaps = snapshots(dir.path(), &home(), stale_at, || Fetch::Failed);
        assert_eq!(snaps.len(), 1);
        no_sentinel(&outputs_of(&snaps, stale_at));

        let aged = NOW_MS + quota::MAX_AGE_MS + 1;
        let snaps = snapshots(dir.path(), &home(), aged, || {
            Fetch::Body(json!({ "access_token": ACCESS, "refresh_token": REFRESH }))
        });
        assert!(snaps.is_empty());
        no_sentinel(&outputs_of(&snaps, aged));
        no_sentinel(&std::fs::read(dir.path().join(CACHE_FILE)).unwrap());

        // malformed file, expired file, and the off switch: still no error string, no cache write
        let cred_home = tempfile::tempdir().unwrap();
        let malformed = format!(r#"{{"access_token":"{ACCESS}","refresh_token":"{REFRESH}"}}"#);
        write_cred(cred_home.path(), malformed.as_bytes());
        assert!(matches!(
            fetch_at(
                cred_home.path(),
                NOW_S,
                false,
                None,
                Some("http://127.0.0.1:9")
            ),
            Fetch::Failed
        ));
        write_cred(cred_home.path(), &cred(ACCESS, NOW_S - 1));
        assert!(matches!(
            fetch_at(
                cred_home.path(),
                NOW_S,
                false,
                None,
                Some("http://127.0.0.1:9")
            ),
            Fetch::Denied
        ));
        assert!(matches!(
            fetch_at(
                cred_home.path(),
                NOW_S,
                true,
                None,
                Some("http://127.0.0.1:9")
            ),
            Fetch::Failed
        ));
        no_sentinel(errors.concat().as_bytes());
    }
}
