//! `summary.json` and `codeforge usage push` (G36 plan, phase 3).
//!
//! The summary is the only thing that leaves the machine. Its schema is exact — every key at
//! every depth is listed below — and every string value is constrained (aliases, host hash,
//! model id grammar, dates, enums), so a path, an email or a prompt cannot ride along. Push
//! validates, then uploads exactly the validated bytes under a fixed name.

use super::{alias::valid_alias, collect::Row, price, quota::Snapshot};
use anyhow::{bail, Context as _, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// (day, source, account, model, speed)
type DayKey = (String, String, String, String, String);

pub const SCHEMA: &str = "codeforge.usage-summary/1";
pub const FILE: &str = "summary.json";
pub const UPLOAD_NAME: &str = "usage-summary.json";

// ---- the contract (`codeforge.usage-summary/1`): every rule `validate` applies, as data. The
// forge-stats pane parses the same file; its copy (`USAGE_CONTRACT` in
// plugins/forge-stats/hooks/model.ts) must equal `contract()` — a test compares the two. ----
const MAX_ACCOUNTS: usize = 64;
const MAX_DAYS_ROWS: usize = 10_000;
const TOP_KEYS: [&str; 6] = [
    "schema",
    "generated_at_ms",
    "host",
    "prices_as_of",
    "accounts",
    "days",
];
/// Present only when a local quota snapshot was dropped: reason -> count. The key is optional so
/// an unchanged summary keeps the old exact shape; reasons are a closed set, never an alias.
const DROPPED_KEY: &str = "dropped_accounts";
const DROP_REASONS: [&str; 3] = [
    "written_at_ms_out_of_range",
    "resets_at_out_of_range",
    "pct_out_of_range",
];
const ACCOUNT_KEYS: [&str; 4] = ["alias", "written_at_ms", "five_hour", "seven_day"];
const WINDOW_KEYS: [&str; 2] = ["pct", "resets_at"];
const COUNT_KEYS: [&str; 6] = [
    "input",
    "output",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "requests",
];
const DAY_KEYS: [&str; 12] = [
    "day",
    "source",
    "account",
    "model",
    "speed",
    "input",
    "output",
    "cache_write_5m",
    "cache_write_1h",
    "cache_read",
    "requests",
    "cost_usd",
];
const SOURCES: [&str; 2] = ["claude", "codex"];
const SPEEDS: [&str; 2] = ["standard", "fast"];
const PCT_MAX: f64 = 1.5;
/// Every integer (counts, times, `resets_at`) is within 0..=2^53-1, exact in a JS number.
pub const UINT_MAX: u64 = (1 << 53) - 1;
/// The grammars `valid_alias`, `model_id` and `is_host` implement, as regexes for the pane.
#[cfg(test)]
const ALIAS_RE: &str = "^[a-z0-9-]{1,24}$";
#[cfg(test)]
const MODEL_RE: &str = "^[A-Za-z0-9._-]{1,64}$";
#[cfg(test)]
const HOST_RE: &str = "^host-[0-9a-f]{8}$";

/// The contract as data, compared against the pane's copy.
#[cfg(test)]
pub fn contract() -> Value {
    json!({
        "schema": SCHEMA,
        "top": TOP_KEYS, "top_optional": [DROPPED_KEY], "drop_reasons": DROP_REASONS, "account": ACCOUNT_KEYS, "window": WINDOW_KEYS, "day": DAY_KEYS,
        "counts": COUNT_KEYS, "sources": SOURCES, "speeds": SPEEDS, "pct_max": PCT_MAX,
        "alias": ALIAS_RE, "model": MODEL_RE, "host": HOST_RE, "uint_max": UINT_MAX,
        "max_accounts": MAX_ACCOUNTS, "max_days": MAX_DAYS_ROWS,
    })
}

/// `<live_base>/codeforge/usage`.
pub fn summary_dir(live_base: &Path) -> PathBuf {
    live_base.join(crate::pet::view::DIR).join("usage")
}

/// `host-` + sha8(hostname): never the hostname itself.
pub fn host_alias(hostname: &str) -> String {
    let d = Sha256::digest(hostname.as_bytes());
    format!(
        "host-{}",
        d.iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

#[cfg(unix)]
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer is valid for its length; gethostname NUL-terminates within it or
    // truncates, and we read up to the first NUL.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(not(unix))]
pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

fn window(w: &Option<super::quota::Window>) -> Value {
    match w {
        Some(w) => json!({ "pct": w.pct, "resets_at": w.resets_at }),
        None => Value::Null,
    }
}

/// Build the summary from bucket rows (any days; the caller passes the last 7) and the
/// fresh quota snapshots. Projects are summed away.
pub fn build(rows: &[Row], accounts: &[Snapshot], host: &str, now_ms: i64) -> Value {
    let mut days: BTreeMap<DayKey, (price::Tokens, u64)> = BTreeMap::new();
    for r in rows {
        let e = days
            .entry((
                r.day.clone(),
                r.source.clone(),
                r.account.clone(),
                r.model.clone(),
                r.speed.clone(),
            ))
            .or_default();
        e.0.add(&r.tokens);
        e.1 = e.1.saturating_add(r.requests);
    }
    let days: Vec<Value> = days
        .into_iter()
        .map(|((day, source, account, model, speed), (t, req))| {
            let cost = price::cost(&model, speed == "fast", &t);
            // saturated sums are capped at the contract's integer bound, so the summary validates
            let c = |n: u64| n.min(UINT_MAX);
            json!({
                "day": day, "source": source, "account": account, "model": model, "speed": speed,
                "input": c(t.input), "output": c(t.output), "cache_write_5m": c(t.cache_write_5m),
                "cache_write_1h": c(t.cache_write_1h), "cache_read": c(t.cache_read), "requests": c(req),
                "cost_usd": cost,
            })
        })
        .collect();
    // One snapshot off the contract is dropped and counted, never fatal to the whole summary.
    let mut dropped: BTreeMap<&str, u64> = BTreeMap::new();
    let mut kept: Vec<Value> = Vec::new();
    for s in accounts {
        if let Some(reason) = bad_snapshot(s) {
            *dropped.entry(reason).or_default() += 1;
        } else if kept.len() < MAX_ACCOUNTS {
            kept.push(json!({
                "alias": s.alias, "written_at_ms": s.written_at_ms,
                "five_hour": window(&s.five_hour), "seven_day": window(&s.seven_day),
            }));
        }
    }
    let mut out = json!({
        "schema": SCHEMA, "generated_at_ms": now_ms, "host": host,
        "prices_as_of": price::PRICES_AS_OF, "accounts": kept, "days": days,
    });
    if !dropped.is_empty() {
        out[DROPPED_KEY] = json!(dropped);
    }
    out
}

/// Why a snapshot cannot enter the summary (a `DROP_REASONS` entry), or `None`.
fn bad_snapshot(s: &Snapshot) -> Option<&'static str> {
    if !(0..=UINT_MAX as i64).contains(&s.written_at_ms) {
        return Some(DROP_REASONS[0]);
    }
    for w in [&s.five_hour, &s.seven_day].into_iter().flatten() {
        if !(0..=UINT_MAX as i64).contains(&w.resets_at) {
            return Some(DROP_REASONS[1]);
        }
        if !(w.pct.is_finite() && (0.0..=PCT_MAX).contains(&w.pct)) {
            return Some(DROP_REASONS[2]);
        }
    }
    None
}

/// How many snapshots a built summary dropped.
pub fn dropped_count(v: &Value) -> u64 {
    v[DROPPED_KEY]
        .as_object()
        .map_or(0, |m| m.values().filter_map(Value::as_u64).sum())
}

fn obj<'a>(v: &'a Value, keys: &[&str], at: &str) -> Result<&'a Map<String, Value>> {
    let Some(m) = v.as_object() else {
        bail!("{at}: 不是物件");
    };
    // the unexpected key itself is never echoed: it is exactly what must not leave the machine
    let extra = m.keys().filter(|k| !keys.contains(&k.as_str())).count();
    if extra > 0 {
        bail!("{at}: 有 {extra} 個不允許的欄位");
    }
    for k in keys {
        if !m.contains_key(*k) {
            bail!("{at}: 缺少欄位 `{k}`");
        }
    }
    Ok(m)
}

fn uint(v: &Value, at: &str) -> Result<()> {
    if v.as_u64().is_none_or(|n| n > UINT_MAX) {
        bail!("{at}: 不是 0..=2^53-1 的整數");
    }
    Ok(())
}

fn is_day(s: &str) -> bool {
    s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

fn is_host(s: &str) -> bool {
    s.strip_prefix("host-").is_some_and(super::quota::is_hash8)
}

fn win(v: &Value, at: &str) -> Result<()> {
    if v.is_null() {
        return Ok(());
    }
    let m = obj(v, &WINDOW_KEYS, at)?;
    let pct = m["pct"]
        .as_f64()
        .filter(|p| p.is_finite() && (0.0..=PCT_MAX).contains(p));
    if pct.is_none() || m["resets_at"].as_u64().is_none_or(|n| n > UINT_MAX) {
        bail!("{at}: 值不合法");
    }
    Ok(())
}

/// Keys, types and values, recursively. Anything else is refused.
pub fn validate(v: &Value) -> Result<()> {
    // the optional drop record is checked on its own, then the rest must be the exact old shape
    let mut rest = v.clone();
    if let Some(d) = rest.as_object_mut().and_then(|m| m.remove(DROPPED_KEY)) {
        let m = d
            .as_object()
            .context("summary.dropped_accounts: 不是物件")?;
        for (k, n) in m {
            if !DROP_REASONS.contains(&k.as_str()) {
                bail!("summary.dropped_accounts: 有不允許的原因");
            }
            uint(n, "summary.dropped_accounts 的計數")?;
        }
    }
    let top = obj(&rest, &TOP_KEYS, "summary")?;
    if top["schema"] != SCHEMA {
        bail!("summary.schema: 不是 {SCHEMA}");
    }
    uint(&top["generated_at_ms"], "summary.generated_at_ms")?;
    if !top["host"].as_str().is_some_and(is_host) {
        bail!("summary.host: 不是 host-<8 hex>");
    }
    if !top["prices_as_of"].as_str().is_some_and(is_day) {
        bail!("summary.prices_as_of: 不是日期");
    }
    let accounts = top["accounts"]
        .as_array()
        .context("summary.accounts: 不是陣列")?;
    if accounts.len() > MAX_ACCOUNTS {
        bail!("summary.accounts: 超過 {MAX_ACCOUNTS} 筆");
    }
    for (i, a) in accounts.iter().enumerate() {
        let at = format!("summary.accounts[{i}]");
        let m = obj(a, &ACCOUNT_KEYS, &at)?;
        if !m["alias"].as_str().is_some_and(valid_alias) {
            bail!("{at}.alias: 不是合法別名");
        }
        uint(&m["written_at_ms"], &format!("{at}.written_at_ms"))?;
        win(&m["five_hour"], &format!("{at}.five_hour"))?;
        win(&m["seven_day"], &format!("{at}.seven_day"))?;
    }
    let days = top["days"].as_array().context("summary.days: 不是陣列")?;
    if days.len() > MAX_DAYS_ROWS {
        bail!("summary.days: 超過 {MAX_DAYS_ROWS} 筆");
    }
    for (i, d) in days.iter().enumerate() {
        let at = format!("summary.days[{i}]");
        let m = obj(d, &DAY_KEYS, &at)?;
        if !m["day"].as_str().is_some_and(is_day) {
            bail!("{at}.day: 不是日期");
        }
        if !m["source"].as_str().is_some_and(|s| SOURCES.contains(&s)) {
            bail!("{at}.source: 不是 claude/codex");
        }
        if !m["account"].as_str().is_some_and(valid_alias) {
            bail!("{at}.account: 不是合法別名");
        }
        let model_ok = match m["model"].as_str() {
            Some(s) => super::collect::model_id(Some(s)) == s,
            None => false,
        };
        if !model_ok {
            bail!("{at}.model: 不符合 model id 格式");
        }
        if !m["speed"].as_str().is_some_and(|s| SPEEDS.contains(&s)) {
            bail!("{at}.speed: 不是 standard/fast");
        }
        for k in COUNT_KEYS {
            uint(&m[k], &format!("{at}.{k}"))?;
        }
        let c = &m["cost_usd"];
        if !(c.is_null() || c.as_f64().is_some_and(|x| x.is_finite() && x >= 0.0)) {
            bail!("{at}.cost_usd: 不是非負數或 null");
        }
    }
    Ok(())
}

/// Write `summary.json` (0600, dir 0700, tmp + rename). Errors name the file, never its path,
/// and carry no source (a source's text may hold the path).
pub fn write(dir: &Path, v: &Value) -> Result<PathBuf> {
    crate::live::write_live_json(dir, FILE, v)
        .map_err(|_| anyhow::anyhow!("無法寫入 summary.json"))?;
    Ok(dir.join(FILE))
}

/// Read the summary back, validate it, and upload exactly the validated bytes (copied to a
/// private 0600 file) as `usage-summary.json`: the local path is never the uploaded name.
/// Returns the blob sha the fleet printed.
pub fn push(dir: &Path, fleet_bin: &str) -> Result<String> {
    let path = dir.join(FILE);
    let bytes = std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("讀不到 summary.json（{:?}）", e.kind()))?;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("summary.json 不是合法 JSON"))?;
    validate(&v).context("summary.json 未通過檢查，拒絕上傳")?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".push.")
        .suffix(".json")
        .tempfile_in(dir)
        .map_err(|e| anyhow::anyhow!("無法建立上傳用的暫存檔（{:?}）", e.kind()))?;
    std::io::Write::write_all(&mut tmp, &bytes)
        .map_err(|e| anyhow::anyhow!("無法寫入上傳用的暫存檔（{:?}）", e.kind()))?;
    let out = std::process::Command::new(fleet_bin)
        .arg("upload")
        .arg(tmp.path())
        .args([
            "--name",
            UPLOAD_NAME,
            "--mime",
            "application/json",
            "--summary",
            "codeforge usage summary",
        ])
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow::anyhow!("找不到 fleet 指令；無法上傳"),
            k => anyhow::anyhow!("無法執行 fleet upload（{k:?}）"),
        })?;
    // fleet's stderr is not echoed (it may name local paths): the exit code says enough
    if !out.status.success() {
        let code = out
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |c| c.to_string());
        bail!("fleet upload 失敗（exit {code}）");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split(|c: char| !c.is_ascii_hexdigit())
        .rev()
        .find(|w| w.len() == 64)
        .map(str::to_string)
        .context("fleet upload 沒有印出 sha256")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::quota::Window;

    fn row(model: &str, speed: &str, project: &str) -> Row {
        Row {
            day: "2026-11-03".into(),
            source: "claude".into(),
            account: "acct-a".into(),
            model: model.into(),
            project: project.into(),
            speed: speed.into(),
            tokens: price::Tokens {
                input: 1_000_000,
                ..Default::default()
            },
            requests: 2,
        }
    }

    fn good() -> Value {
        let snap = Snapshot {
            alias: "acct-a".into(),
            root: Some("1a2b3c4d".into()),
            written_at_ms: 5,
            five_hour: Some(Window {
                pct: 0.3,
                resets_at: 9,
                seen: None,
            }),
            seven_day: None,
        };
        build(
            &[
                row("claude-opus-5-5", "standard", "MARKERPROJ"),
                row("claude-opus-5-5", "fast", "x"),
                row("gpt-6.1-sol", "standard", "y"),
            ],
            &[snap],
            &host_alias("MARKERHOST.example"),
            7,
        )
    }

    #[test]
    fn built_summary_is_exact_and_carries_no_project_root_or_hostname() {
        let v = good();
        validate(&v).unwrap();
        let text = v.to_string();
        for m in ["MARKERPROJ", "MARKERHOST", "1a2b3c4d", "project", "root"] {
            assert!(!text.contains(m), "{m} in summary");
        }
        let days = v["days"].as_array().unwrap();
        assert_eq!(
            days.len(),
            3,
            "projects are summed away, speed and model kept apart"
        );
        let cost = |model: &str, speed: &str| {
            days.iter()
                .find(|d| d["model"] == model && d["speed"] == speed)
                .unwrap()["cost_usd"]
                .clone()
        };
        assert_eq!(cost("claude-opus-5-5", "standard"), json!(4.0));
        assert_eq!(cost("claude-opus-5-5", "fast"), json!(8.0));
        assert_eq!(cost("gpt-6.1-sol", "standard"), Value::Null);
    }

    #[test]
    fn validate_refuses_extra_keys_and_bad_values_at_every_depth() {
        let mut cases: Vec<(&str, Value)> = Vec::new();
        let mut v = good();
        v["extra"] = json!(1);
        cases.push(("top-level key", v));
        let mut v = good();
        v["accounts"][0]["email"] = json!("me@example.com");
        cases.push(("key in accounts", v));
        let mut v = good();
        v["days"][0]["project"] = json!("repo");
        cases.push(("key in days", v));
        let mut v = good();
        v["days"][0]["model"] = json!("/home/me/secret");
        cases.push(("path in model", v));
        let mut v = good();
        v["days"][0]["model"] = json!("me@example.com");
        cases.push(("email in model", v));
        let mut v = good();
        v["accounts"][0]["alias"] = json!("me@example.com");
        cases.push(("email in alias", v));
        let mut v = good();
        v["days"][0]["account"] = json!("/home/me");
        cases.push(("path in account", v));
        let mut v = good();
        v["host"] = json!("my-laptop");
        cases.push(("hostname", v));
        let mut v = good();
        v["days"][0]["input"] = json!(-1);
        cases.push(("negative count", v));
        let mut v = good();
        v["days"][0]["speed"] = json!("turbo");
        cases.push(("speed enum", v));
        let mut v = good();
        v["accounts"][0]["five_hour"]["note"] = json!("x");
        cases.push(("key in a window", v));
        let mut v = good();
        v["days"][0]["input"] = json!(UINT_MAX + 1);
        cases.push(("count past 2^53-1", v));
        let mut v = good();
        v["accounts"][0]["five_hour"]["resets_at"] = json!(UINT_MAX + 1);
        cases.push(("resets_at past 2^53-1", v));
        let mut v = good();
        v["accounts"][0]["five_hour"]["resets_at"] = json!(-1);
        cases.push(("negative resets_at", v));
        for (what, v) in cases {
            assert!(validate(&v).is_err(), "{what} passed");
        }
    }

    #[cfg(unix)]
    #[test]
    fn push_uploads_the_validated_bytes_under_a_fixed_name() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("MARKERDIR/usage");
        let path = write(&dir, &good()).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // a fake fleet: records its argv and the uploaded bytes, prints a sha
        let fake = base.path().join("fleet");
        let log = base.path().join("argv");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}'\ncp \"$2\" '{log}.bytes'\necho \"uploaded usage-summary.json; fetch: fleet fetch {}\"\n",
                "ab".repeat(32),
                log = log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let sha = push(&dir, fake.to_str().unwrap()).unwrap();
        assert_eq!(sha, "ab".repeat(32));
        let argv = std::fs::read_to_string(&log).unwrap();
        assert!(argv.contains("--name\nusage-summary.json\n"), "{argv}");
        assert_eq!(
            std::fs::read(log.with_extension("bytes")).unwrap(),
            std::fs::read(&path).unwrap()
        );
        // an invalid summary on disk is refused before fleet runs
        std::fs::remove_file(&log).unwrap();
        let mut bad = good();
        bad["days"][0]["model"] = json!("/home/me");
        crate::live::write_live_json(&dir, FILE, &bad).unwrap();
        assert!(push(&dir, fake.to_str().unwrap()).is_err());
        assert!(!log.exists(), "fleet ran on an invalid summary");
        // no fleet binary: a clear error
        let err = push(&base.path().join("nope").join(".."), "/nonexistent/fleet")
            .unwrap_err()
            .to_string();
        assert!(!err.is_empty());
    }

    #[test]
    fn host_alias_is_a_hash() {
        let h = host_alias("box.example");
        assert!(is_host(&h) && !h.contains("box"));
    }

    #[cfg(unix)]
    #[test]
    fn errors_name_files_and_fields_never_local_paths_or_raw_values() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("MARKERDIR/usage");
        let home = dirs::home_dir().unwrap();
        let clean = |e: anyhow::Error| {
            let full = format!("{e:?}");
            for bad in [
                "MARKER",
                "me@example.com",
                home.to_str().unwrap(),
                base.path().to_str().unwrap(),
            ] {
                assert!(!full.contains(bad), "{bad} in error: {full}");
            }
            full
        };
        // no summary yet
        std::fs::create_dir_all(&dir).unwrap();
        clean(push(&dir, "fleet").unwrap_err());
        // an unexpected key is counted, not echoed
        let mut v = good();
        v["accounts"][0]["me@example.com"] = json!(1);
        let full = clean(validate(&v).unwrap_err());
        assert!(full.contains("1 個不允許的欄位"), "{full}");
        // fleet missing, and fleet failing with a path on its stderr
        write(&dir, &good()).unwrap();
        clean(push(&dir, base.path().join("MARKERBIN/fleet").to_str().unwrap()).unwrap_err());
        let fake = base.path().join("fleet-fail");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\necho 'cannot read {}/MARKERSECRET' >&2\nexit 3\n",
                home.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let full = clean(push(&dir, fake.to_str().unwrap()).unwrap_err());
        assert!(full.contains("exit 3"), "{full}");
        // writing where a file stands in for the dir
        let blocked = base.path().join("MARKERFILE");
        std::fs::write(&blocked, b"").unwrap();
        clean(write(&blocked.join("usage"), &good()).unwrap_err());
    }

    fn snap_with(alias: &str, written_at_ms: i64, pct: f64, resets_at: i64) -> Snapshot {
        Snapshot {
            alias: alias.into(),
            root: None,
            written_at_ms,
            five_hour: Some(Window {
                pct,
                resets_at,
                seen: None,
            }),
            seven_day: None,
        }
    }

    #[test]
    fn one_bad_snapshot_is_dropped_and_counted_not_fatal() {
        let max = UINT_MAX as i64;
        let snaps = [
            snap_with("acct-a", 5, 0.3, 9),
            snap_with("acct-bad1", 5, 0.3, max + 1),
            snap_with("acct-bad2", max + 1, 0.3, 9),
            snap_with("acct-bad3", -1, 0.3, 9),
            snap_with("acct-bad4", 5, 0.3, -1),
            snap_with("acct-bad5", 5, 9.0, 9),
            snap_with("acct-z", 6, 0.1, 10),
        ];
        let v = build(
            &[row("claude-opus-5-5", "standard", "p")],
            &snaps,
            &host_alias("h"),
            7,
        );
        validate(&v).expect("a bad snapshot must not fail the whole summary");
        let aliases: Vec<&str> = v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["alias"].as_str().unwrap())
            .collect();
        assert_eq!(aliases, ["acct-a", "acct-z"]);
        assert_eq!(v["days"].as_array().unwrap().len(), 1);
        // not silent: reasons are an enum, counts are numbers, no alias leaks
        assert_eq!(
            v["dropped_accounts"],
            json!({
                "written_at_ms_out_of_range": 2,
                "resets_at_out_of_range": 2,
                "pct_out_of_range": 1,
            })
        );
        assert!(!v.to_string().contains("acct-bad"));
        // nothing dropped: the key is absent, so the shape is the old one
        assert!(good().get("dropped_accounts").is_none());
    }

    #[test]
    fn validate_refuses_a_malformed_dropped_record() {
        let ok = |d: Value| {
            let mut v = good();
            v["dropped_accounts"] = d;
            validate(&v)
        };
        ok(json!({"resets_at_out_of_range": 1})).unwrap();
        assert!(ok(json!({"acct-a": 1})).is_err());
        assert!(ok(json!({"resets_at_out_of_range": -1})).is_err());
        assert!(ok(json!({"resets_at_out_of_range": "x"})).is_err());
        assert!(ok(json!([])).is_err());
    }

    #[test]
    fn the_pane_parses_by_the_same_contract() {
        // plugins/forge-stats/hooks/model.ts carries the contract between two markers as strict JSON
        let ts = include_str!("../../plugins/forge-stats/hooks/model.ts");
        let block = ts
            .split("// contract:begin")
            .nth(1)
            .and_then(|r| r.split("// contract:end").next())
            .expect("contract block in model.ts");
        let json = &block[block.find('{').unwrap()..=block.rfind('}').unwrap()];
        let pane: Value = serde_json::from_str(json).expect("the block is strict JSON");
        assert_eq!(pane, contract());
    }

    #[test]
    fn the_contract_regexes_describe_the_grammar_functions() {
        // no regex crate here: the samples pin each grammar's edges, the pane uses the regexes
        assert_eq!(ALIAS_RE, "^[a-z0-9-]{1,24}$");
        for (s, ok) in [
            ("acct-1a2b3c4d", true),
            ("a", true),
            (&"a".repeat(24)[..], true),
            (&"a".repeat(25)[..], false),
            ("Acct", false),
            ("a_b", false),
            ("", false),
        ] {
            assert_eq!(valid_alias(s), ok, "{s}");
        }
        assert_eq!(MODEL_RE, "^[A-Za-z0-9._-]{1,64}$");
        for (s, ok) in [
            ("claude-opus-5-5", true),
            ("gpt-6.1_x", true),
            (&"a".repeat(64)[..], true),
            (&"a".repeat(65)[..], false),
            ("a/b", false),
            ("a b", false),
        ] {
            assert_eq!(super::super::collect::model_id(Some(s)) == s, ok, "{s}");
        }
        assert_eq!(HOST_RE, "^host-[0-9a-f]{8}$");
        for (s, ok) in [
            ("host-1a2b3c4d", true),
            ("host-1A2B3C4D", false),
            ("host-1a2b3c4", false),
            ("1a2b3c4d", false),
        ] {
            assert_eq!(is_host(s), ok, "{s}");
        }
    }
}
