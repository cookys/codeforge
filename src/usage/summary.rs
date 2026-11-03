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
const MAX_ACCOUNTS: usize = 64;
const MAX_DAYS_ROWS: usize = 10_000;

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
        e.0.input += r.tokens.input;
        e.0.output += r.tokens.output;
        e.0.cache_write_5m += r.tokens.cache_write_5m;
        e.0.cache_write_1h += r.tokens.cache_write_1h;
        e.0.cache_read += r.tokens.cache_read;
        e.1 += r.requests;
    }
    let days: Vec<Value> = days
        .into_iter()
        .map(|((day, source, account, model, speed), (t, req))| {
            let cost = price::cost(&model, speed == "fast", &t);
            json!({
                "day": day, "source": source, "account": account, "model": model, "speed": speed,
                "input": t.input, "output": t.output, "cache_write_5m": t.cache_write_5m,
                "cache_write_1h": t.cache_write_1h, "cache_read": t.cache_read, "requests": req,
                "cost_usd": cost,
            })
        })
        .collect();
    let accounts: Vec<Value> = accounts
        .iter()
        .take(MAX_ACCOUNTS)
        .map(|s| {
            json!({
                "alias": s.alias, "written_at_ms": s.written_at_ms,
                "five_hour": window(&s.five_hour), "seven_day": window(&s.seven_day),
            })
        })
        .collect();
    json!({
        "schema": SCHEMA, "generated_at_ms": now_ms, "host": host,
        "prices_as_of": price::PRICES_AS_OF, "accounts": accounts, "days": days,
    })
}

fn obj<'a>(v: &'a Value, keys: &[&str], at: &str) -> Result<&'a Map<String, Value>> {
    let Some(m) = v.as_object() else {
        bail!("{at}: 不是物件");
    };
    for k in m.keys() {
        if !keys.contains(&k.as_str()) {
            bail!("{at}: 不允許的欄位 `{k}`");
        }
    }
    for k in keys {
        if !m.contains_key(*k) {
            bail!("{at}: 缺少欄位 `{k}`");
        }
    }
    Ok(m)
}

fn uint(v: &Value, at: &str) -> Result<()> {
    if v.as_u64().is_none() {
        bail!("{at}: 不是非負整數");
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
    let m = obj(v, &["pct", "resets_at"], at)?;
    let pct = m["pct"]
        .as_f64()
        .filter(|p| p.is_finite() && (0.0..=1.5).contains(p));
    if pct.is_none() || m["resets_at"].as_i64().is_none() {
        bail!("{at}: 值不合法");
    }
    Ok(())
}

/// Keys, types and values, recursively. Anything else is refused.
pub fn validate(v: &Value) -> Result<()> {
    let top = obj(
        v,
        &[
            "schema",
            "generated_at_ms",
            "host",
            "prices_as_of",
            "accounts",
            "days",
        ],
        "summary",
    )?;
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
        let m = obj(
            a,
            &["alias", "written_at_ms", "five_hour", "seven_day"],
            &at,
        )?;
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
        let m = obj(
            d,
            &[
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
            ],
            &at,
        )?;
        if !m["day"].as_str().is_some_and(is_day) {
            bail!("{at}.day: 不是日期");
        }
        if !matches!(m["source"].as_str(), Some("claude" | "codex")) {
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
        if !matches!(m["speed"].as_str(), Some("standard" | "fast")) {
            bail!("{at}.speed: 不是 standard/fast");
        }
        for k in [
            "input",
            "output",
            "cache_write_5m",
            "cache_write_1h",
            "cache_read",
            "requests",
        ] {
            uint(&m[k], &format!("{at}.{k}"))?;
        }
        let c = &m["cost_usd"];
        if !(c.is_null() || c.as_f64().is_some_and(|x| x.is_finite() && x >= 0.0)) {
            bail!("{at}.cost_usd: 不是非負數或 null");
        }
    }
    Ok(())
}

/// Write `summary.json` (0600, dir 0700, tmp + rename).
pub fn write(dir: &Path, v: &Value) -> Result<PathBuf> {
    crate::live::write_live_json(dir, FILE, v)
        .with_context(|| format!("無法寫入 {}", dir.join(FILE).display()))?;
    Ok(dir.join(FILE))
}

/// Read the summary back, validate it, and upload exactly the validated bytes (copied to a
/// private 0600 file) as `usage-summary.json`: the local path is never the uploaded name.
/// Returns the blob sha the fleet printed.
pub fn push(dir: &Path, fleet_bin: &str) -> Result<String> {
    let path = dir.join(FILE);
    let bytes = std::fs::read(&path).with_context(|| format!("讀不到 {}", path.display()))?;
    let v: Value = serde_json::from_slice(&bytes).context("summary.json 不是合法 JSON")?;
    validate(&v).context("summary.json 未通過檢查，拒絕上傳")?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".push.")
        .suffix(".json")
        .tempfile_in(dir)?;
    std::io::Write::write_all(&mut tmp, &bytes)?;
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
            std::io::ErrorKind::NotFound => {
                anyhow::anyhow!("找不到 fleet 指令（{fleet_bin}）；無法上傳")
            }
            _ => anyhow::anyhow!("執行 fleet upload 失敗：{e}"),
        })?;
    if !out.status.success() {
        bail!(
            "fleet upload 失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
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
}
