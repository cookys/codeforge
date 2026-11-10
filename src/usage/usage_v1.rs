//! `usage.v1` client: fleet's read-only quota view (`GET /v1/usage`), behind `CODEFORGE_USAGE_V1`.
//!
//! The flag value is the source: a file path or an `http(s)://` URL. Parsing validates only the trust
//! boundary (`schema`, `pools` array, input size); a window missing a field or with a wrong type is
//! unknown for that window alone. `reset_passed` / `stale` are re-applied at read time with an
//! injected `now_ms`, and never cleared when the hub set them. `pct` is a 0..1 used fraction
//! (confirmed by the fleet lane: the hub passes the digest value through). capacity fields are 0..100 points and are not read.

use super::quota_view::{countdown, Tr};
use anyhow::{bail, Context as _, Result};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const SCHEMA: &str = "usage.v1";
pub const ENV_SOURCE: &str = "CODEFORGE_USAGE_V1";
pub const ENV_TOKEN_FILE: &str = "CODEFORGE_USAGE_V1_TOKEN_FILE";
const MAX_BYTES: usize = 256 * 1024;
const STALE_AFTER_MS: i64 = 2 * 3_600_000;
const TIMEOUT_S: u64 = 5;

#[derive(Debug, Clone, PartialEq)]
pub enum WinState {
    Fresh {
        pct: f64,
        resets_at_ms: i64,
    },
    Stale {
        pct: f64,
        resets_at_ms: i64,
        age_ms: i64,
    },
    ResetPassed,
    Unknown {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Window {
    pct: f64,
    resets_at_ms: i64,
    seen_at_ms: i64,
    stale: bool,
    reset_passed: bool,
}

#[derive(Debug, Clone)]
struct Pool {
    id: String,
    pool: Option<String>,
    provider: String,
    accounts: Vec<String>,
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    no_source: Map<String, Value>,
}

#[derive(Debug, Clone)]
struct Source {
    name: String,
    seen_at_ms: Option<i64>,
    stale: bool,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    generated_at_ms: Option<i64>,
    sources: Vec<Source>,
    pools: Vec<Pool>,
}

fn window(v: &Value) -> Option<Window> {
    Some(Window {
        pct: v.get("pct")?.as_f64()?,
        resets_at_ms: v.get("resets_at_ms")?.as_i64()?,
        seen_at_ms: v.get("seen_at_ms")?.as_i64()?,
        stale: v.get("stale")?.as_bool()?,
        reset_passed: v.get("reset_passed")?.as_bool()?,
    })
}

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k)?.as_str().map(str::to_string)
}

pub fn parse(bytes: &[u8]) -> Result<Snapshot> {
    if bytes.len() > MAX_BYTES {
        bail!("usage.v1 回應超過 {} KiB，不讀", MAX_BYTES / 1024);
    }
    let v: Value = serde_json::from_slice(bytes).context("usage.v1 不是合法的 JSON")?;
    if v.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        bail!("usage.v1 的 schema 不是 \"{SCHEMA}\"");
    }
    let pools = v
        .get("pools")
        .and_then(Value::as_array)
        .context("usage.v1 的 pools 不是陣列")?;
    let sources = v
        .get("sources")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|s| Source {
                    name: text(s, "name").unwrap_or_else(|| "?".into()),
                    seen_at_ms: s.get("seen_at_ms").and_then(Value::as_i64),
                    stale: s.get("stale").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    let pools = pools
        .iter()
        .filter(|p| p.is_object())
        .map(|p| Pool {
            id: text(p, "id").unwrap_or_else(|| "?".into()),
            pool: text(p, "pool"),
            provider: text(p, "provider").unwrap_or_else(|| "?".into()),
            accounts: p
                .get("accounts")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            five_hour: p.get("five_hour").and_then(window),
            seven_day: p.get("seven_day").and_then(window),
            no_source: p
                .get("no_source")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    Ok(Snapshot {
        generated_at_ms: v.get("generated_at_ms").and_then(Value::as_i64),
        sources,
        pools,
    })
}

fn state_of(w: Option<&Window>, reason: Option<&Value>, now_ms: i64) -> WinState {
    let Some(w) = w else {
        return WinState::Unknown {
            reason: reason.and_then(Value::as_str).map(str::to_string),
        };
    };
    if w.reset_passed || w.resets_at_ms <= now_ms {
        return WinState::ResetPassed;
    }
    let age_ms = (now_ms - w.seen_at_ms).max(0);
    if w.stale || age_ms > STALE_AFTER_MS {
        WinState::Stale {
            pct: w.pct,
            resets_at_ms: w.resets_at_ms,
            age_ms,
        }
    } else {
        WinState::Fresh {
            pct: w.pct,
            resets_at_ms: w.resets_at_ms,
        }
    }
}

impl Pool {
    fn states(&self, now_ms: i64) -> [(&'static str, WinState); 2] {
        [
            (
                "five_hour",
                state_of(
                    self.five_hour.as_ref(),
                    self.no_source.get("five_hour"),
                    now_ms,
                ),
            ),
            (
                "seven_day",
                state_of(
                    self.seven_day.as_ref(),
                    self.no_source.get("seven_day"),
                    now_ms,
                ),
            ),
        ]
    }
}

fn pct_text(p: f64) -> String {
    format!("{}%", (p * 100.0).round() as i64)
}

/// `3h`, `45m`, `2d` — coarse age for the old-reading marker.
fn age_text(ms: i64) -> String {
    let m = ms / 60_000;
    if m < 60 {
        format!("{m}m")
    } else if m < 48 * 60 {
        format!("{}h", m / 60)
    } else {
        format!("{}d", m / 1440)
    }
}

fn win_text(label: &str, st: &WinState, now_ms: i64, tr: &Tr) -> String {
    match st {
        WinState::Fresh { pct, resets_at_ms } => {
            let at = chrono::DateTime::from_timestamp_millis(*resets_at_ms)
                .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            let left = countdown(*resets_at_ms as f64 / 1000.0, now_ms as f64 / 1000.0);
            tr.t(
                "usage_v1.fresh",
                &[
                    ("w", label),
                    ("pct", &pct_text(*pct)),
                    ("at", &at),
                    ("in", &left),
                ],
            )
        }
        WinState::Stale { pct, age_ms, .. } => tr.t(
            "usage_v1.stale",
            &[
                ("w", label),
                ("pct", &pct_text(*pct)),
                ("age", &age_text(*age_ms)),
            ],
        ),
        WinState::ResetPassed => tr.t("usage_v1.reset", &[("w", label)]),
        WinState::Unknown { reason: None } => format!("{label} —"),
        WinState::Unknown { reason: Some(r) } => {
            let r: String = r.chars().take(60).collect();
            format!("{label} — ({r})")
        }
    }
}

pub fn render_lines(s: &Snapshot, now_ms: i64, tr: &Tr) -> Vec<String> {
    let mut out = Vec::new();
    for p in &s.pools {
        let [(_, five), (_, seven)] = p.states(now_ms);
        let name = p.pool.as_deref().unwrap_or(&p.id);
        out.push(format!(
            "{name}  {}  {}  {}",
            p.provider,
            win_text("5h", &five, now_ms, tr),
            win_text("7d", &seven, now_ms, tr)
        ));
    }
    for src in &s.sources {
        let age = src.seen_at_ms.map(|t| (now_ms - t).max(0));
        let old = src.stale || age.is_some_and(|a| a > STALE_AFTER_MS);
        if old {
            let a = age.map(age_text).unwrap_or_else(|| "?".into());
            out.push(tr.t("usage_v1.source_old", &[("name", &src.name), ("age", &a)]));
        }
    }
    out
}

fn window_json(st: &WinState, w: Option<&Window>, now_ms: i64) -> Value {
    let (state, pct, resets) = match st {
        WinState::Fresh { pct, resets_at_ms } => ("fresh", Some(*pct), Some(*resets_at_ms)),
        WinState::Stale {
            pct, resets_at_ms, ..
        } => ("stale", Some(*pct), Some(*resets_at_ms)),
        WinState::ResetPassed => ("reset_passed", None, None),
        WinState::Unknown { .. } => ("unknown", None, None),
    };
    json!({
        "state": state,
        "pct": pct,
        "resets_at_ms": resets,
        "stale": matches!(st, WinState::Stale { .. }),
        "reset_passed": matches!(st, WinState::ResetPassed),
        "age_ms": w.map(|w| (now_ms - w.seen_at_ms).max(0)),
        "reason": match st { WinState::Unknown { reason } => json!(reason), _ => Value::Null },
    })
}

pub fn to_json(s: &Snapshot, now_ms: i64) -> Value {
    let pools: Vec<Value> = s
        .pools
        .iter()
        .map(|p| {
            let [(_, five), (_, seven)] = p.states(now_ms);
            json!({
                "id": p.id,
                "pool": p.pool,
                "provider": p.provider,
                "accounts": p.accounts,
                "five_hour": window_json(&five, p.five_hour.as_ref(), now_ms),
                "seven_day": window_json(&seven, p.seven_day.as_ref(), now_ms),
            })
        })
        .collect();
    let sources: Vec<Value> = s
        .sources
        .iter()
        .map(|x| {
            let age = x.seen_at_ms.map(|t| (now_ms - t).max(0));
            json!({
                "name": x.name,
                "age_ms": age,
                "stale": x.stale || age.is_some_and(|a| a > STALE_AFTER_MS),
            })
        })
        .collect();
    json!({"schema": SCHEMA, "generated_at_ms": s.generated_at_ms, "sources": sources, "pools": pools})
}

/// The flag: unset or empty is off; otherwise the value is the source.
pub fn configured(v: Option<&str>) -> Option<String> {
    v.filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn off_message(tr: &Tr) -> String {
    tr.t("usage_v1.off", &[])
}

/// Read the raw bytes from a file path or an `http(s)://` URL (GET, bearer token from a file).
pub fn fetch(src: &str, token_file: Option<&Path>) -> Result<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        let token = match token_file {
            Some(p) => Some(
                std::fs::read_to_string(p)
                    .with_context(|| format!("讀不到 token 檔 {}", p.display()))?
                    .trim()
                    .to_string(),
            ),
            None => None,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(TIMEOUT_S))
                .no_proxy()
                .build()?;
            let mut req = client.get(src);
            if let Some(t) = token.filter(|t| !t.is_empty()) {
                req = req.bearer_auth(t);
            }
            let r = req.send().await.context("連不上 usage.v1 端點")?;
            let status = r.status();
            if !status.is_success() {
                bail!("usage.v1 端點回應 HTTP {}", status.as_u16());
            }
            let b = r.bytes().await.context("讀取 usage.v1 回應失敗")?;
            if b.len() > MAX_BYTES {
                bail!("usage.v1 回應超過 {} KiB，不讀", MAX_BYTES / 1024);
            }
            Ok(b.to_vec())
        })
    } else {
        let len = std::fs::metadata(src)
            .with_context(|| format!("讀不到 usage.v1 檔 {src}"))?
            .len();
        if len as usize > MAX_BYTES {
            bail!("usage.v1 檔超過 {} KiB，不讀", MAX_BYTES / 1024);
        }
        std::fs::read(src).with_context(|| format!("讀不到 usage.v1 檔 {src}"))
    }
}

/// `codeforge usage pools`: env-driven entry for the CLI.
pub fn run(json: bool, now_ms: i64) -> Result<()> {
    let tr = Tr::current();
    let Some(src) = configured(std::env::var(ENV_SOURCE).ok().as_deref()) else {
        println!("{}", off_message(&tr));
        return Ok(());
    };
    let token_file = std::env::var(ENV_TOKEN_FILE)
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let snap = parse(&fetch(&src, token_file.as_deref())?)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&to_json(&snap, now_ms))?);
    } else {
        for l in render_lines(&snap, now_ms, &tr) {
            println!("{l}");
        }
    }
    Ok(())
}
