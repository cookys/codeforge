//! G40 quota mirror model: the pure reader of autopilot's `fleet.quota/1` mirror view
//! (parse + section 5.2 merge + token warn + tightest). A port of
//! `plugins/forge-stats/hooks/quota.ts` with identical semantics; no I/O, no rendering.
//!
//! Contract: autopilot `project/plans/2026-11-03-g40-agent-lane-quota.md` 5.2 (merge), 20-22
//! (record, view, refusal, token state); display rules: `doc/plans/2026-11-03-g40-usage-accounts.md` 10.
//! Distinct from `usage::quota` (the G36 `codeforge.quota/1` snapshot). Timestamps are epoch SECONDS.

use serde::Serialize;
use serde_json::{json, Value};

// ---- the contract constants (a test compares them with the block in quota.ts) ----
pub const SCHEMA: &str = "fleet.quota/1";
pub const VIEW_SCHEMA: &str = "fleet.quota-view/1";
pub const NS_SCHEMA: &str = "fleet.quota-ns/1";
pub const KINDS: [&str; 7] = [
    "rolling_5h",
    "weekly",
    "daily",
    "monthly",
    "credits",
    "rpm",
    "tpm",
];
pub const SOURCES: [&str; 4] = ["passive", "endpoint", "probe", "estimated"];
pub const UNITS: [&str; 3] = ["requests", "tokens", "credits"];
pub const REFUSAL_REASONS: [&str; 3] = ["rate_limit", "quota", "other"];
pub const TOKEN_STATES: [&str; 3] = ["ok", "invalid", "unknown"];
pub const VISIBILITY: [&str; 4] = ["L0", "L1", "L2", "L3"];
pub const PROVIDER_PATTERN: &str = "^[a-z0-9-]{1,24}$";
pub const ALIAS_PATTERN: &str = "^[a-z0-9-]{1,24}$";
pub const MAX_AGE_S: i64 = 21600;
pub const FUTURE_SKEW_S: i64 = 300;
pub const TIGHT: f64 = 0.9;
pub const TOKEN_WARN_NOTICE_DAYS: i64 = 30;
pub const TOKEN_WARN_URGENT_DAYS: i64 = 7;
pub const REFUSAL_DEFAULT_COOLDOWN_S: i64 = 1800;
pub const MAX_WINDOWS: usize = 7;
pub const VIEW_MAX_BYTES: usize = 32768;
pub const MAX_ROWS: usize = 256;
pub const EPOCH_S_MAX: i64 = 100_000_000_000;

const DAY_S: i64 = 86400;
/// Field names the contract rejects (never renames); `*_ms` is matched by suffix.
const DRIFT: [&str; 7] = [
    "pct",
    "resetsAt",
    "five_hour",
    "seven_day",
    "namespace",
    "writer_courier",
    "sent_at",
];

/// The contract as data, compared against the pane's copy.
#[cfg_attr(not(test), allow(dead_code))]
pub fn contract() -> Value {
    json!({
        "schema": SCHEMA,
        "view_schema": VIEW_SCHEMA,
        "ns_schema": NS_SCHEMA,
        "kinds": KINDS,
        "sources": SOURCES,
        "units": UNITS,
        "refusal_reasons": REFUSAL_REASONS,
        "token_states": TOKEN_STATES,
        "visibility": VISIBILITY,
        "provider": PROVIDER_PATTERN,
        "alias": ALIAS_PATTERN,
        "max_age_s": MAX_AGE_S,
        "future_skew_s": FUTURE_SKEW_S,
        "tight": TIGHT,
        "token_warn_days": { "notice": TOKEN_WARN_NOTICE_DAYS, "urgent": TOKEN_WARN_URGENT_DAYS },
        "refusal_default_cooldown_s": REFUSAL_DEFAULT_COOLDOWN_S,
        "max_windows": MAX_WINDOWS,
        "view_max_bytes": VIEW_MAX_BYTES,
        "max_rows": MAX_ROWS,
        "epoch_s_max": EPOCH_S_MAX,
    })
}

// ---- parsed shapes ----
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QWindow {
    pub kind: &'static str,
    pub utilization: Option<f64>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub unit: Option<&'static str>,
    pub resets_at: Option<i64>,
    pub source: &'static str,
    pub observed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QRefusal {
    pub observed_at: i64,
    pub retry_at: Option<i64>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QRecord {
    pub visibility: &'static str,
    pub windows: Vec<QWindow>,
    pub refusal: Option<QRefusal>,
    pub token_state: &'static str,
    pub token_state_at: Option<i64>,
    pub host_role: String,
    pub token_issued_at: Option<i64>,
    pub token_expires_at: Option<i64>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QRow {
    pub writer_courier: String,
    pub host_role: String,
    pub observed_at_hub: Option<i64>,
    pub skewed: bool,
    pub record: QRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuotaView {
    pub namespace: Option<String>,
    pub hub_offset: f64,
    pub rows: Vec<QRow>,
    pub rejected: usize,
}

// ---- validation helpers ----
type Obj = serde_json::Map<String, Value>;

fn as_obj(v: &Value) -> Option<&Obj> {
    v.as_object()
}
fn finite(v: &Value) -> Option<f64> {
    v.as_f64().filter(|f| f.is_finite())
}
/// Integer epoch seconds in `0..=EPOCH_S_MAX`.
fn sec(v: &Value) -> Option<i64> {
    let f = finite(v)?;
    (f.fract() == 0.0 && (0.0..=EPOCH_S_MAX as f64).contains(&f)).then_some(f as i64)
}
/// Absent or null reads as null; invalid is `None`.
fn sec_or_null(v: Option<&Value>) -> Option<Option<i64>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(x) => sec(x).map(Some),
    }
}
fn num_or_null(v: Option<&Value>) -> Option<Option<f64>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(x) => finite(x).filter(|f| *f >= 0.0).map(Some),
    }
}
fn in_set(v: Option<&Value>, set: &'static [&'static str]) -> Option<&'static str> {
    let s = v?.as_str()?;
    set.iter().copied().find(|x| *x == s)
}
fn only_keys(o: &Obj, required: &[&str], optional: &[&str]) -> bool {
    required.iter().all(|k| o.contains_key(*k))
        && o.keys()
            .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str()))
}
fn has_drift(o: &Obj) -> bool {
    o.keys()
        .any(|k| DRIFT.contains(&k.as_str()) || k.ends_with("_ms"))
}
fn charset_ok(s: &str, max: usize, extra: &str) -> bool {
    !s.is_empty()
        && s.chars().count() <= max
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}
/// `^[a-z0-9-]{1,24}$` (PROVIDER_PATTERN / ALIAS_PATTERN).
fn slug_ok(s: &str) -> bool {
    !s.is_empty()
        && s.chars().count() <= 24
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
fn courier_ok(s: &str) -> bool {
    charset_ok(s, 128, "._:@/-")
}
fn role_ok(s: &str) -> bool {
    charset_ok(s, 64, "._-")
}

fn parse_window(v: &Value) -> Option<QWindow> {
    let o = as_obj(v)?;
    if has_drift(o)
        || !only_keys(
            o,
            &["kind", "utilization", "source", "observed_at"],
            &["used", "limit", "unit", "remaining", "resets_at", "clamped"],
        )
    {
        return None;
    }
    let kind = in_set(o.get("kind"), &KINDS)?;
    let source = in_set(o.get("source"), &SOURCES)?;
    let observed_at = sec(o.get("observed_at")?)?;
    if let Some(c) = o.get("clamped") {
        c.as_bool()?;
    }
    let resets_at = sec_or_null(o.get("resets_at"))?;
    let used = num_or_null(o.get("used"))?;
    let limit = num_or_null(o.get("limit"))?;
    let remaining = num_or_null(o.get("remaining"))?;
    let unit = match o.get("unit") {
        None | Some(Value::Null) => None,
        u => Some(in_set(u, &UNITS)?),
    };
    if used.is_none() != limit.is_none() {
        return None;
    }
    if (used.is_some() || remaining.is_some()) && unit.is_none() {
        return None;
    }
    if let Some(r) = remaining {
        if kind != "credits" {
            return None;
        }
        if let (Some(u), Some(l)) = (used, limit) {
            if (r - (l - u)).abs() > 1e-9 {
                return None;
            }
        }
    }
    let remaining_only = remaining.is_some() && used.is_none();
    let utilization = match o.get("utilization")? {
        // null is legal only on `credits`, remaining-only shape (21.2)
        Value::Null => {
            if kind != "credits" || !remaining_only {
                return None;
            }
            None
        }
        u => {
            let f = finite(u)?;
            if !(0.0..=1.0).contains(&f) || remaining_only {
                return None;
            }
            Some(f)
        }
    };
    Some(QWindow {
        kind,
        utilization,
        used,
        limit,
        remaining,
        unit,
        resets_at,
        source,
        observed_at,
    })
}

fn parse_record(v: &Value, provider: &str, alias: &str) -> Option<QRecord> {
    let o = as_obj(v)?;
    if has_drift(o)
        || !only_keys(
            o,
            &[
                "schema",
                "provider",
                "alias",
                "visibility",
                "windows",
                "host_role",
            ],
            &[
                "refusal",
                "token_state",
                "token_state_at",
                "token_issued_at",
                "token_expires_at",
                "source",
            ],
        )
    {
        return None;
    }
    if o["schema"].as_str() != Some(SCHEMA)
        || o["provider"].as_str() != Some(provider)
        || o["alias"].as_str() != Some(alias)
    {
        return None;
    }
    let visibility = in_set(o.get("visibility"), &VISIBILITY)?;
    let host_role = o["host_role"].as_str().filter(|s| role_ok(s))?;
    let wins = o["windows"].as_array().filter(|a| a.len() <= MAX_WINDOWS)?;
    let mut windows: Vec<QWindow> = Vec::new();
    for w in wins {
        let p = parse_window(w)?;
        if windows.iter().any(|x| x.kind == p.kind) {
            return None;
        }
        windows.push(p);
    }
    let mut refusal = None;
    match o.get("refusal") {
        None | Some(Value::Null) => {}
        Some(r) => {
            let r = as_obj(r)?;
            if has_drift(r) || !only_keys(r, &["observed_at", "retry_at", "reason"], &[]) {
                return None;
            }
            refusal = Some(QRefusal {
                observed_at: sec(&r["observed_at"])?,
                retry_at: sec_or_null(r.get("retry_at"))?,
                reason: in_set(r.get("reason"), &REFUSAL_REASONS)?,
            });
        }
    }
    let token_state = match o.get("token_state") {
        None => "unknown",
        t => in_set(t, &TOKEN_STATES)?,
    };
    let token_state_at = sec_or_null(o.get("token_state_at"))?;
    let token_issued_at = sec_or_null(o.get("token_issued_at"))?;
    let token_expires_at = sec_or_null(o.get("token_expires_at"))?;
    if (token_state == "unknown") != token_state_at.is_none() {
        return None;
    }
    let has_source = o.contains_key("source");
    if has_source && o["source"].as_str() != Some("registry") {
        return None;
    }
    if refusal.is_some() && visibility == "L0" {
        return None;
    }
    // 20.4: L0 shows nothing, L1 only a refusal; a record carrying windows at either level is malformed
    if !windows.is_empty() && (visibility == "L0" || visibility == "L1") {
        return None;
    }
    let mut placeholder = false;
    if windows.is_empty() {
        // 21.6: empty windows are valid only as (a) placeholder, (b) refusal-only, (c) credential-state-only
        if visibility == "L2" || visibility == "L3" {
            return None;
        }
        if refusal.is_some() {
            if visibility != "L1" || has_source {
                return None;
            }
        } else if token_state != "unknown" {
            if has_source {
                return None;
            }
        } else {
            if !has_source || visibility != "L0" {
                return None;
            }
            placeholder = true;
        }
    } else if has_source {
        return None;
    }
    Some(QRecord {
        visibility,
        windows,
        refusal,
        token_state,
        token_state_at,
        host_role: host_role.to_string(),
        token_issued_at,
        token_expires_at,
        placeholder,
    })
}

fn parse_row(v: &Value, provider: &str, alias: &str) -> Option<QRow> {
    let o = as_obj(v)?;
    if !only_keys(
        o,
        &[
            "writer_courier",
            "host_role",
            "observed_at_hub",
            "skewed",
            "record",
        ],
        &[],
    ) {
        return None;
    }
    let writer = o["writer_courier"].as_str().filter(|s| courier_ok(s))?;
    o["host_role"].as_str().filter(|s| role_ok(s))?;
    let skewed = o["skewed"].as_bool()?;
    let oah = match &o["observed_at_hub"] {
        Value::Null => None,
        x => Some(sec(x)?),
    };
    let record = parse_record(&o["record"], provider, alias)?;
    // row-level host_role is the hub's copy; the record's value is the one a reader uses (22.2)
    Some(QRow {
        writer_courier: writer.to_string(),
        host_role: record.host_role.clone(),
        observed_at_hub: oah,
        skewed,
        record,
    })
}

/// Parse one mirror view file (`<live_dir>/quota/<provider>/<alias>.json`). A bad wrapper is `Err(reason)`;
/// a bad row is dropped and counted.
pub fn parse_quota_view(text: &str, provider: &str, alias: &str) -> Result<QuotaView, String> {
    let bad = |r: &str| Err(r.to_string());
    if !slug_ok(provider) || !slug_ok(alias) {
        return bad("path");
    }
    if text.len() > VIEW_MAX_BYTES {
        return bad("size");
    }
    let Ok(raw) = serde_json::from_str::<Value>(text) else {
        return bad("json");
    };
    let Some(o) = as_obj(&raw) else {
        return bad("schema");
    };
    if !only_keys(o, &["schema", "namespace", "hub_offset", "rows"], &[])
        || o["schema"].as_str() != Some(VIEW_SCHEMA)
    {
        return bad("schema");
    }
    let namespace = match &o["namespace"] {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        _ => return bad("namespace"),
    };
    let Some(hub_offset) = finite(&o["hub_offset"]).filter(|f| f.abs() <= EPOCH_S_MAX as f64)
    else {
        return bad("hub_offset");
    };
    let Some(raw_rows) = o["rows"].as_array().filter(|a| a.len() <= MAX_ROWS) else {
        return bad("rows");
    };
    let mut rows: Vec<QRow> = Vec::new();
    let mut rejected = 0;
    for r in raw_rows {
        match parse_row(r, provider, alias) {
            // one row per writer_courier: a repeat is a bad row
            Some(p) if !rows.iter().any(|x| x.writer_courier == p.writer_courier) => rows.push(p),
            _ => rejected += 1,
        }
    }
    Ok(QuotaView {
        namespace,
        hub_offset,
        rows,
        rejected,
    })
}

// ---- merge ----
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergedWindow {
    pub kind: &'static str,
    pub utilization: Option<f64>,
    pub remaining: Option<f64>,
    pub unit: Option<&'static str>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    /// The reset time on the LOCAL clock (record `resets_at` minus `hub_offset`): what a countdown against the local clock needs.
    pub resets_at_local: Option<f64>,
    pub observed_at: i64,
    pub age_s: f64,
    pub writer_courier: String,
    pub host_role: String,
    pub source: &'static str,
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergedRefusal {
    /// On the local clock.
    pub end_s: f64,
    pub reason: &'static str,
    pub age_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergedQuota {
    pub windows: Vec<MergedWindow>,
    pub refusal: Option<MergedRefusal>,
    pub token_state: &'static str,
    pub token_expires_at: Option<i64>,
    pub placeholder: bool,
    pub had_observation: bool,
    /// True when any usable row's record carried a window, even if the merge then dropped them all
    /// (false for placeholder, refusal-only and token-state-only, 21.6 shape c).
    pub had_window: bool,
    pub level: &'static str,
    pub hosts: Vec<String>,
    pub multi_host: bool,
}

struct Cand<'a> {
    w: &'a QWindow,
    row: &'a QRow,
    age: f64,
    local: bool,
}
fn util(c: &Cand) -> f64 {
    c.w.utilization.unwrap_or(-1.0)
}
/// Smaller age wins; an exact tie goes to the local row, a tie between peers to the higher utilization (5.2 rule 3).
fn better<'a>(a: Cand<'a>, b: Cand<'a>) -> Cand<'a> {
    if a.age != b.age {
        return if a.age < b.age { a } else { b };
    }
    if a.local != b.local {
        return if a.local { a } else { b };
    }
    if util(&b) > util(&a) {
        b
    } else {
        a
    }
}

/// Merge one view by contract 5.2. `now_s` is the local clock in epoch seconds; `ns` is `quota/namespace.json`'s value (or None).
pub fn merge_quota(view: &QuotaView, ns: Option<&str>, now_s: i64) -> MergedQuota {
    let peers_ok = matches!((view.namespace.as_deref(), ns), (Some(a), Some(b)) if a == b);
    // a row is local when its observed_at_hub is null (22.2); a peer row is used only past the namespace guard
    let rows = view
        .rows
        .iter()
        .filter(|r| r.observed_at_hub.is_none() || peers_ok);
    let now_l = now_s as f64;
    let now_c = now_l + view.hub_offset;

    let mut measured: Vec<Option<Cand>> = KINDS.iter().map(|_| None).collect();
    let mut estimated: Vec<Option<Cand>> = KINDS.iter().map(|_| None).collect();
    let mut refusal: Option<MergedRefusal> = None;
    let mut had_observation = false;
    let mut placeholder = false;
    let mut had_window = false;
    let mut live_rows: Vec<(String, String)> = Vec::new();
    let mut mark_live = |w: &str, role: &str| match live_rows.iter_mut().find(|(k, _)| k == w) {
        Some(e) => e.1 = role.to_string(),
        None => live_rows.push((w.to_string(), role.to_string())),
    };
    let mut state_cands: Vec<(&'static str, f64)> = Vec::new();
    let mut expiry: Option<(f64, i64)> = None;

    for row in rows {
        let rec = &row.record;
        let local = row.observed_at_hub.is_none();
        let ref_now = if local { now_l } else { now_c };
        let mut stamps: Vec<i64> = rec.windows.iter().map(|w| w.observed_at).collect();
        if let Some(r) = &rec.refusal {
            stamps.push(r.observed_at);
        }
        if let Some(t) = rec.token_state_at {
            stamps.push(t);
        }
        if !rec.windows.is_empty() {
            had_window = true;
        }
        if !stamps.is_empty() {
            had_observation = true;
        } else if rec.placeholder {
            placeholder = true;
        }
        let basis = stamps.iter().copied().max().unwrap_or(0);
        // 5.2 rule 1: a peer stamp x has hub time t = observed_at_hub - (B - x); a local stamp is its own time
        let t_of = |x: i64| -> f64 {
            match row.observed_at_hub {
                None => x as f64,
                Some(h) => (h - (basis - x)) as f64,
            }
        };
        let age_of = |x: i64| ref_now - t_of(x);

        for w in &rec.windows {
            let age = age_of(w.observed_at);
            if age > MAX_AGE_S as f64 || age < -(FUTURE_SKEW_S as f64) {
                continue;
            }
            if w.resets_at.is_some_and(|r| r as f64 <= now_c) {
                continue;
            }
            mark_live(&row.writer_courier, &rec.host_role);
            let c = Cand { w, row, age, local };
            let i = KINDS.iter().position(|k| *k == w.kind).unwrap();
            let map = if w.source == "estimated" {
                &mut estimated
            } else {
                &mut measured
            };
            map[i] = Some(match map[i].take() {
                Some(cur) => better(cur, c),
                None => c,
            });
        }

        if let Some(r) = &rec.refusal {
            let t = t_of(r.observed_at);
            let age = ref_now - t;
            let cooldown = r
                .retry_at
                .map_or(REFUSAL_DEFAULT_COOLDOWN_S, |ra| ra - r.observed_at);
            let end = t + cooldown as f64;
            let expires = end.min(t + MAX_AGE_S as f64);
            if age >= -(FUTURE_SKEW_S as f64) && ref_now < expires {
                mark_live(&row.writer_courier, &rec.host_role);
                if refusal.as_ref().is_none_or(|cur| age < cur.age_s) {
                    refusal = Some(MergedRefusal {
                        end_s: end - (ref_now - now_l),
                        reason: r.reason,
                        age_s: age,
                    });
                }
            }
        }

        // token_state is a state, not a measurement: the 6 h age rule never retires it (21.1)
        if let Some(t) = rec.token_state_at {
            state_cands.push((rec.token_state, age_of(t)));
            mark_live(&row.writer_courier, &rec.host_role);
        }
        if let Some(at) = rec.token_expires_at {
            let age = if stamps.is_empty() {
                f64::INFINITY
            } else {
                age_of(basis)
            };
            if expiry.is_none_or(|(a, _)| age < a) {
                expiry = Some((age, at));
            }
        }
    }

    let mut out: Vec<MergedWindow> = Vec::new();
    for (i, kind) in KINDS.iter().enumerate() {
        let (m, e) = (measured[i].as_ref(), estimated[i].as_ref());
        let mut pick = m.or(e);
        let mut is_est = m.is_none() && e.is_some();
        // an estimate can only raise (max(measured, estimated)), never lower or replace (5.2 rule 4)
        if let (Some(m), Some(e)) = (m, e) {
            if let (Some(eu), Some(mu)) = (e.w.utilization, m.w.utilization) {
                if eu > mu {
                    pick = Some(e);
                    is_est = true;
                }
            }
        }
        let Some(p) = pick else { continue };
        let w = p.w;
        out.push(MergedWindow {
            kind,
            utilization: w.utilization,
            remaining: w.remaining,
            unit: w.unit,
            used: w.used,
            limit: w.limit,
            resets_at_local: w.resets_at.map(|r| r as f64 - view.hub_offset),
            observed_at: w.observed_at,
            age_s: p.age,
            writer_courier: p.row.writer_courier.clone(),
            host_role: p.row.record.host_role.clone(),
            source: w.source,
            estimated: is_est,
        });
    }

    // newest state change wins; an exact tie goes to `invalid` (the dangerous direction)
    let mut token_state = "unknown";
    let mut best: Option<(&'static str, f64)> = None;
    for s in &state_cands {
        if best.is_none_or(|b| s.1 < b.1 || (s.1 == b.1 && s.0 == "invalid")) {
            best = Some(*s);
        }
    }
    if let Some(b) = best {
        token_state = b.0;
    }

    let level = if !out.is_empty() {
        if out.iter().all(|w| w.resets_at_local.is_some()) {
            "L3"
        } else {
            "L2"
        }
    } else if refusal.is_some() {
        "L1"
    } else {
        "L0"
    };
    MergedQuota {
        windows: out,
        refusal,
        token_state,
        token_expires_at: expiry.map(|e| e.1),
        placeholder: placeholder && !had_observation,
        had_observation,
        had_window,
        level,
        multi_host: live_rows.len() >= 2,
        hosts: live_rows.into_iter().map(|e| e.1).collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TokenWarn {
    pub level: &'static str,
    pub days: i64,
}

/// Token expiry warn level (21.4): at or under `notice` days warns, at or under `urgent` warns harder, past the date is `expired`.
pub fn token_warn(expires_at: Option<i64>, now_s: i64) -> Option<TokenWarn> {
    let left = expires_at? - now_s;
    if left <= 0 {
        return Some(TokenWarn {
            level: "expired",
            days: left.div_euclid(DAY_S),
        });
    }
    let days = (left + DAY_S - 1) / DAY_S;
    if left <= TOKEN_WARN_URGENT_DAYS * DAY_S {
        return Some(TokenWarn {
            level: "urgent",
            days,
        });
    }
    if left <= TOKEN_WARN_NOTICE_DAYS * DAY_S {
        return Some(TokenWarn {
            level: "notice",
            days,
        });
    }
    None
}

const TIGHT_KINDS: [&str; 5] = ["rolling_5h", "weekly", "daily", "monthly", "credits"];
/// The window with the largest utilization among the gauge kinds; null-utilization credits and rpm/tpm never count.
pub fn tightest(merged: &MergedQuota) -> Option<&MergedWindow> {
    let mut best: Option<&MergedWindow> = None;
    for w in &merged.windows {
        let Some(u) = w.utilization else { continue };
        if !TIGHT_KINDS.contains(&w.kind) {
            continue;
        }
        if best.is_none_or(|b| u > b.utilization.unwrap_or(f64::MIN)) {
            best = Some(w);
        }
    }
    best
}

pub fn is_tight(utilization: f64) -> bool {
    utilization >= TIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: i64 = 1_793_700_000;
    const DAY: i64 = 86400;

    fn with(base: Value, patch: Value) -> Value {
        let mut b = base;
        for (k, v) in patch.as_object().unwrap() {
            b[k.as_str()] = v.clone();
        }
        b
    }
    fn without(mut v: Value, keys: &[&str]) -> Value {
        for k in keys {
            v.as_object_mut().unwrap().remove(*k);
        }
        v
    }
    fn win(kind: &str, util: Value, observed_at: i64, o: Value) -> Value {
        with(
            json!({"kind": kind, "utilization": util, "used": null, "limit": null, "unit": null, "resets_at": N + 7500, "source": "passive", "observed_at": observed_at}),
            o,
        )
    }
    fn rec(o: Value) -> Value {
        recp(o, "claude", "work")
    }
    fn recp(o: Value, provider: &str, alias: &str) -> Value {
        with(
            json!({"schema": "fleet.quota/1", "provider": provider, "alias": alias, "visibility": "L3", "windows": [], "refusal": null, "token_state": "unknown", "token_state_at": null, "host_role": "agent", "token_issued_at": null, "token_expires_at": null}),
            o,
        )
    }
    fn row(record: Value) -> Value {
        rowo(record, json!({}))
    }
    fn rowo(record: Value, o: Value) -> Value {
        with(
            json!({"writer_courier": "local", "host_role": "agent", "observed_at_hub": null, "skewed": false, "record": record}),
            o,
        )
    }
    fn peer_row(record: Value, oah: i64) -> Value {
        rowo(
            record,
            json!({"writer_courier": "cookys/desk/peer", "host_role": "desk", "observed_at_hub": oah}),
        )
    }
    fn view(rows: Vec<Value>) -> String {
        viewo(rows, json!({}))
    }
    fn viewo(rows: Vec<Value>, o: Value) -> String {
        with(json!({"schema": "fleet.quota-view/1", "namespace": "cookys", "hub_offset": 0, "rows": rows}), o).to_string()
    }
    fn ok(text: &str) -> QuotaView {
        parse_quota_view(text, "claude", "work").unwrap_or_else(|e| panic!("bad view: {e}"))
    }
    fn merged(rows: Vec<Value>) -> MergedQuota {
        mergedx(rows, json!({}), Some("cookys"), N)
    }
    fn mergedx(rows: Vec<Value>, o: Value, ns: Option<&str>, now: i64) -> MergedQuota {
        let r = rows
            .first()
            .map(|r| r["record"].clone())
            .unwrap_or(json!({}));
        let p = r["provider"].as_str().unwrap_or("claude").to_string();
        let a = r["alias"].as_str().unwrap_or("work").to_string();
        let v =
            parse_quota_view(&viewo(rows, o), &p, &a).unwrap_or_else(|e| panic!("bad view: {e}"));
        merge_quota(&v, ns, now)
    }
    fn d1() -> Value {
        rec(json!({
            "windows": [win("rolling_5h", json!(0.62), 1793699940, json!({"resets_at": 1793707500})), win("weekly", json!(0.31), 1793699940, json!({"resets_at": 1793973600}))],
            "token_state": "ok", "token_state_at": 1793600000, "token_issued_at": 1780740000, "token_expires_at": 1812276000
        }))
    }
    fn desc(m: &MergedQuota) -> Vec<(&str, Option<f64>, &str, f64)> {
        m.windows
            .iter()
            .map(|w| (w.kind, w.utilization, w.writer_courier.as_str(), w.age_s))
            .collect()
    }

    #[test]
    fn d1_claude_work_5h_7d_is_l3_aged_from_the_local_clock_token_ok() {
        let m = merged(vec![row(d1())]);
        let got: Vec<_> = m
            .windows
            .iter()
            .map(|w| {
                (
                    w.kind,
                    w.utilization,
                    w.age_s,
                    w.writer_courier.as_str(),
                    w.source,
                    w.estimated,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("rolling_5h", Some(0.62), 60.0, "local", "passive", false),
                ("weekly", Some(0.31), 60.0, "local", "passive", false)
            ]
        );
        assert_eq!(m.windows[0].observed_at, 1793699940);
        assert_eq!(m.level, "L3");
        assert_eq!(m.token_state, "ok");
        assert_eq!(m.token_expires_at, Some(1812276000));
        assert_eq!(m.hosts, vec!["agent"]);
        assert!(!m.multi_host && !m.placeholder && m.had_observation && m.refusal.is_none());
        assert_eq!(tightest(&m).unwrap().kind, "rolling_5h");
    }

    #[test]
    fn d2_codex_credits_remaining_only_text_only_l2_never_tightest() {
        let credits = win(
            "credits",
            Value::Null,
            1793699000,
            json!({"remaining": 12.5, "unit": "credits", "resets_at": null, "source": "endpoint"}),
        );
        let m = merged(vec![row(recp(
            json!({"visibility": "L2", "windows": [credits.clone()]}),
            "codex",
            "main",
        ))]);
        assert_eq!(m.windows.len(), 1);
        let w = &m.windows[0];
        assert_eq!(
            (
                w.kind,
                w.utilization,
                w.remaining,
                w.unit,
                w.resets_at_local,
                w.age_s
            ),
            ("credits", None, Some(12.5), Some("credits"), None, 1000.0)
        );
        assert_eq!(m.level, "L2");
        assert!(tightest(&m).is_none());
        let weekly = win("weekly", json!(0.2), 1793699000, json!({"resets_at": null}));
        let m2 = merged(vec![row(recp(
            json!({"visibility": "L2", "windows": [credits, weekly]}),
            "codex",
            "main",
        ))]);
        assert_eq!(tightest(&m2).unwrap().kind, "weekly");
    }

    #[test]
    fn d2_parse_accepts_full_used_limit_remaining_and_checks_the_sum() {
        let good = win(
            "credits",
            json!(0.5),
            N - 10,
            json!({"used": 5, "limit": 10, "remaining": 5, "unit": "credits"}),
        );
        assert_eq!(
            ok(&view(vec![row(rec(json!({"windows": [good]})))]))
                .rows
                .len(),
            1
        );
        let wrong = win(
            "credits",
            json!(0.5),
            N - 10,
            json!({"used": 5, "limit": 10, "remaining": 4, "unit": "credits"}),
        );
        assert_eq!(
            ok(&view(vec![row(rec(json!({"windows": [wrong]})))])).rejected,
            1
        );
    }

    #[test]
    fn d3_placeholder_l0_never_observed_not_retired_by_age() {
        let p = without(
            recp(
                json!({"visibility": "L0", "source": "registry"}),
                "claude",
                "spare",
            ),
            &["refusal", "token_state_at"],
        );
        let m = mergedx(vec![row(p)], json!({}), Some("cookys"), N + 10 * DAY);
        assert_eq!(m.level, "L0");
        assert!(m.placeholder && !m.had_observation);
        assert!(m.windows.is_empty() && m.hosts.is_empty());
    }

    #[test]
    fn d4_stale_or_passed_window_is_dropped_aged_window_carries_its_age() {
        let stale = recp(
            json!({"windows": [win("rolling_5h", json!(0.4), N - 25200, json!({"resets_at": N + 3000}))]}),
            "minimax",
            "main",
        );
        let m = merged(vec![row(stale)]);
        assert!(m.windows.is_empty());
        assert_eq!(m.level, "L0");
        assert!(m.had_observation && !m.placeholder);
        let passed = rec(
            json!({"windows": [win("rolling_5h", json!(0.4), N - 60, json!({"resets_at": N - 1}))]}),
        );
        assert!(merged(vec![row(passed)]).windows.is_empty());
        let aged = rec(json!({"windows": [win("rolling_5h", json!(0.4), N - 8000, json!({}))]}));
        assert_eq!(merged(vec![row(aged)]).windows[0].age_s, 8000.0);
    }

    #[test]
    fn d5_estimated_window_alone_is_used_and_marked_l2() {
        let e = recp(
            json!({"visibility": "L2", "windows": [win("daily", json!(0.41), 1793699700, json!({"used": 410, "limit": 1000, "unit": "requests", "resets_at": null, "source": "estimated"}))]}),
            "glm",
            "main",
        );
        let m = merged(vec![row(e)]);
        let w = &m.windows[0];
        assert_eq!(
            (
                w.kind,
                w.utilization,
                w.used,
                w.limit,
                w.unit,
                w.estimated,
                w.source,
                w.age_s
            ),
            (
                "daily",
                Some(0.41),
                Some(410.0),
                Some(1000.0),
                Some("requests"),
                true,
                "estimated",
                300.0
            )
        );
        assert_eq!(m.level, "L2");
    }

    #[test]
    fn d6_refusal_only_l1_end_age_default_cooldown_and_expiry() {
        let r = |retry_at: Value| {
            recp(
                json!({"visibility": "L1", "refusal": {"observed_at": 1793699880, "retry_at": retry_at, "reason": "rate_limit"}}),
                "kimi",
                "main",
            )
        };
        let m = merged(vec![row(r(json!(1793704800)))]);
        assert!(m.windows.is_empty());
        assert_eq!(
            m.refusal,
            Some(MergedRefusal {
                end_s: 1793704800.0,
                reason: "rate_limit",
                age_s: 120.0
            })
        );
        assert_eq!(m.level, "L1");
        assert!(m.had_observation);
        assert_eq!(
            merged(vec![row(r(Value::Null))]).refusal.unwrap().end_s,
            (1793699880 + REFUSAL_DEFAULT_COOLDOWN_S) as f64
        );
        assert!(mergedx(
            vec![row(r(json!(1793704800)))],
            json!({}),
            Some("cookys"),
            1793704800
        )
        .refusal
        .is_none());
        assert!(mergedx(
            vec![row(r(json!(1793704800)))],
            json!({}),
            Some("cookys"),
            1793704799
        )
        .refusal
        .is_some());
        let far = rec(
            json!({"visibility": "L1", "refusal": {"observed_at": N - 100, "retry_at": N + 10 * DAY, "reason": "quota"}}),
        );
        assert_eq!(
            merged(vec![row(far.clone())]).refusal.unwrap().end_s,
            (N + 10 * DAY) as f64
        );
        assert!(mergedx(
            vec![row(far.clone())],
            json!({}),
            Some("cookys"),
            N - 100 + MAX_AGE_S
        )
        .refusal
        .is_none());
        assert!(mergedx(
            vec![row(far)],
            json!({}),
            Some("cookys"),
            N - 100 + MAX_AGE_S - 1
        )
        .refusal
        .is_some());
    }

    #[test]
    fn d7_token_near_expiry_warns_urgent_and_invalid_beats_fresh_low_windows() {
        assert_eq!(
            token_warn(Some(1794218400), N),
            Some(TokenWarn {
                level: "urgent",
                days: 6
            })
        );
        let inv = with(
            d1(),
            json!({"token_state": "invalid", "token_state_at": 1793690000}),
        );
        let m = merged(vec![row(inv)]);
        assert_eq!(m.token_state, "invalid");
        assert_eq!(m.windows[0].utilization, Some(0.62));
        assert_eq!(m.level, "L3");
    }

    #[test]
    fn an_invalid_token_is_not_retired_by_age_a_newer_ok_clears_it() {
        let inv = rec(
            json!({"visibility": "L0", "token_state": "invalid", "token_state_at": N - 10 * DAY}),
        );
        assert_eq!(merged(vec![row(inv.clone())]).token_state, "invalid");
        let ok_peer =
            rec(json!({"visibility": "L0", "token_state": "ok", "token_state_at": N - 100}));
        let m = merged(vec![
            row(inv),
            rowo(
                ok_peer,
                json!({"writer_courier": "cookys/desk/peer", "host_role": "desk", "observed_at_hub": N - 100}),
            ),
        ]);
        assert_eq!(m.token_state, "ok");
    }

    #[test]
    fn d8_two_hosts_local_wins_by_age_both_hosts_listed() {
        let peer = rec(json!({
            "host_role": "desk",
            "windows": [win("rolling_5h", json!(0.58), 1793699000, json!({"resets_at": 1793707500})), win("weekly", json!(0.3), 1793699000, json!({"resets_at": 1793973600, "source": "probe"}))],
            "token_state": "ok", "token_state_at": 1793600000
        }));
        let m = mergedx(
            vec![row(d1()), peer_row(peer.clone(), 1793699100)],
            json!({"hub_offset": 3}),
            Some("cookys"),
            N,
        );
        assert_eq!(
            desc(&m),
            vec![
                ("rolling_5h", Some(0.62), "local", 60.0),
                ("weekly", Some(0.31), "local", 60.0)
            ]
        );
        assert_eq!(m.hosts, vec!["agent", "desk"]);
        assert!(m.multi_host);
        // the peer's age is (N + 3) - 1793699100 = 903 s when it is the only row
        let only = mergedx(
            vec![peer_row(peer.clone(), 1793699100)],
            json!({"hub_offset": 3}),
            Some("cookys"),
            N,
        );
        let w = &only.windows[0];
        assert_eq!(
            (
                w.writer_courier.as_str(),
                w.host_role.as_str(),
                w.age_s,
                w.utilization,
                w.observed_at
            ),
            ("cookys/desk/peer", "desk", 903.0, Some(0.58), 1793699000)
        );
        assert_eq!(only.windows[1].source, "probe");
        let fresh = merged(vec![row(d1()), peer_row(peer, N - 5)]);
        let w = &fresh.windows[0];
        assert_eq!(
            (w.writer_courier.as_str(), w.utilization, w.age_s),
            ("cookys/desk/peer", Some(0.58), 5.0)
        );
    }

    #[test]
    fn exact_age_tie_local_wins_two_peers_tie_higher_utilization() {
        let a = rec(json!({"windows": [win("weekly", json!(0.2), N - 100, json!({}))]}));
        let b = rec(
            json!({"host_role": "desk", "windows": [win("weekly", json!(0.9), 777, json!({}))]}),
        );
        let m = merged(vec![row(a), peer_row(b.clone(), N - 100)]);
        assert_eq!(
            (
                m.windows[0].writer_courier.as_str(),
                m.windows[0].utilization
            ),
            ("local", Some(0.2))
        );
        let c =
            rec(json!({"host_role": "lap", "windows": [win("weekly", json!(0.5), 5, json!({}))]}));
        let m2 = merged(vec![
            peer_row(b, N - 100),
            rowo(
                c,
                json!({"writer_courier": "cookys/lap/peer", "host_role": "lap", "observed_at_hub": N - 100}),
            ),
        ]);
        assert_eq!(
            (
                m2.windows[0].writer_courier.as_str(),
                m2.windows[0].utilization
            ),
            ("cookys/desk/peer", Some(0.9))
        );
    }

    // ---- N80: one clock domain ----
    fn hub_stamp(off: i64, age: i64) -> i64 {
        N + off - age
    }
    fn n80_windows(off: i64) -> Vec<Value> {
        vec![
            row(rec(
                json!({"windows": [win("rolling_5h", json!(0.2), N - 120, json!({"resets_at": null}))]}),
            )),
            // the peer writer's own clock is arbitrary (123456789): only observed_at_hub counts
            peer_row(
                rec(
                    json!({"host_role": "desk", "windows": [win("rolling_5h", json!(0.95), 123456789, json!({"resets_at": null}))]}),
                ),
                hub_stamp(off, 30),
            ),
        ]
    }
    fn n80_refusals(off: i64) -> Vec<Value> {
        vec![
            row(rec(
                json!({"visibility": "L1", "refusal": {"observed_at": N - 600, "retry_at": N + 60, "reason": "rate_limit"}}),
            )),
            peer_row(
                rec(
                    json!({"host_role": "desk", "visibility": "L1", "refusal": {"observed_at": 5000, "retry_at": 5000 + 1230, "reason": "quota"}}),
                ),
                hub_stamp(off, 30),
            ),
        ]
    }
    /// The planted wrong comparator: raw `t` of each row's refusal / 5h window, compared as if one clock.
    fn naive_raw(rows: &[Value], window: bool) -> usize {
        let ts: Vec<i64> = rows
            .iter()
            .map(|r| {
                let record = &r["record"];
                let stamp = if window {
                    record["windows"][0]["observed_at"].as_i64().unwrap()
                } else {
                    record["refusal"]["observed_at"].as_i64().unwrap()
                };
                match r["observed_at_hub"].as_i64() {
                    None => stamp,
                    Some(h) => h, // the TS `observed_at_hub - (max(stamps) - stamp)` with one stamp is just the hub stamp
                }
            })
            .collect();
        let max = *ts.iter().max().unwrap();
        ts.iter().position(|t| *t == max).unwrap()
    }

    #[test]
    fn n80_windows_refusals_and_resets_under_both_hub_offsets() {
        for off in [600_i64, -600] {
            let m = mergedx(
                n80_windows(off),
                json!({"hub_offset": off}),
                Some("cookys"),
                N,
            );
            let w = &m.windows[0];
            assert_eq!(
                (w.writer_courier.as_str(), w.utilization, w.age_s),
                ("cookys/desk/peer", Some(0.95), 30.0),
                "windows off {off}"
            );
            let m = mergedx(
                n80_refusals(off),
                json!({"hub_offset": off}),
                Some("cookys"),
                N,
            );
            assert_eq!(
                m.refusal,
                Some(MergedRefusal {
                    end_s: (N + 1200) as f64,
                    reason: "quota",
                    age_s: 30.0
                }),
                "refusals off {off}"
            );
            let mk = |resets: i64| {
                vec![
                    row(rec(
                        json!({"windows": [win("weekly", json!(0.2), N - 10, json!({"resets_at": resets}))]}),
                    )),
                    peer_row(
                        rec(
                            json!({"host_role": "desk", "windows": [win("rolling_5h", json!(0.3), 1, json!({"resets_at": resets}))]}),
                        ),
                        hub_stamp(off, 10),
                    ),
                ]
            };
            assert!(
                mergedx(mk(N + off), json!({"hub_offset": off}), Some("cookys"), N)
                    .windows
                    .is_empty(),
                "resets off {off}"
            );
            let kinds: Vec<_> = mergedx(
                mk(N + off + 1),
                json!({"hub_offset": off}),
                Some("cookys"),
                N,
            )
            .windows
            .iter()
            .map(|w| w.kind)
            .collect();
            assert_eq!(kinds, vec!["rolling_5h", "weekly"], "resets+1 off {off}");
        }
    }

    #[test]
    fn n80_the_planted_raw_t_comparator_picks_the_wrong_row_under_minus_600() {
        // +600: raw t happens to agree with the right answer; -600: it picks the older local row
        assert_eq!(naive_raw(&n80_windows(600), true), 1);
        assert_eq!(naive_raw(&n80_windows(-600), true), 0);
        assert_eq!(naive_raw(&n80_refusals(600), false), 1);
        assert_eq!(naive_raw(&n80_refusals(-600), false), 0);
        let right = mergedx(
            n80_windows(-600),
            json!({"hub_offset": -600}),
            Some("cookys"),
            N,
        )
        .windows[0]
            .writer_courier
            .clone();
        assert_eq!(right, "cookys/desk/peer");
        assert!((naive_raw(&n80_windows(-600), true) == 1) != (right == "cookys/desk/peer"));
    }

    #[test]
    fn age_21600_kept_21601_dropped_300s_future_kept_301_dropped() {
        let at = |x: i64| {
            merged(vec![row(rec(
                json!({"windows": [win("weekly", json!(0.3), x, json!({"resets_at": null}))]}),
            ))])
            .windows
            .len()
        };
        assert_eq!(at(N - MAX_AGE_S), 1);
        assert_eq!(at(N - MAX_AGE_S - 1), 0);
        assert_eq!(at(N + FUTURE_SKEW_S), 1);
        assert_eq!(at(N + FUTURE_SKEW_S + 1), 0);
    }

    #[test]
    fn resets_at_equal_to_now_plus_hub_offset_is_dropped_one_second_later_lives() {
        let at = |r: Value| {
            merged(vec![row(rec(
                json!({"windows": [win("weekly", json!(0.3), N - 5, json!({"resets_at": r}))]}),
            ))])
            .windows
            .len()
        };
        assert_eq!(at(json!(N)), 0);
        assert_eq!(at(json!(N + 1)), 1);
        assert_eq!(at(Value::Null), 1);
    }

    #[test]
    fn an_estimate_never_lowers_or_replaces_a_measured_window_it_can_only_raise() {
        let meas = |u: f64| win("daily", json!(u), N - 100, json!({"resets_at": null}));
        let est = |u: f64| {
            win(
                "daily",
                json!(u),
                N - 5,
                json!({"resets_at": null, "source": "estimated", "used": 1, "limit": 2, "unit": "requests"}),
            )
        };
        let lower = merged(vec![
            row(rec(json!({"windows": [meas(0.8)]}))),
            peer_row(
                rec(json!({"host_role": "desk", "windows": [est(0.3)]})),
                N - 5,
            ),
        ]);
        assert_eq!(
            (
                lower.windows[0].utilization,
                lower.windows[0].estimated,
                lower.windows[0].source
            ),
            (Some(0.8), false, "passive")
        );
        let higher = merged(vec![
            row(rec(json!({"windows": [meas(0.3)]}))),
            peer_row(
                rec(json!({"host_role": "desk", "windows": [est(0.8)]})),
                N - 5,
            ),
        ]);
        assert_eq!(
            (
                higher.windows[0].utilization,
                higher.windows[0].estimated,
                higher.windows[0].source
            ),
            (Some(0.8), true, "estimated")
        );
        let alone = merged(vec![row(rec(json!({"windows": [est(0.3)]})))]);
        assert_eq!(
            (alone.windows[0].utilization, alone.windows[0].estimated),
            (Some(0.3), true)
        );
    }

    #[test]
    fn namespace_mismatch_or_a_null_side_ignores_peer_rows() {
        let rows = vec![
            row(d1()),
            peer_row(
                rec(
                    json!({"host_role": "desk", "windows": [win("rolling_5h", json!(0.99), 1, json!({}))]}),
                ),
                N - 1,
            ),
        ];
        for ns in [Some("other"), None] {
            let m = mergedx(rows.clone(), json!({}), ns, N);
            assert_eq!(
                (
                    m.windows[0].writer_courier.as_str(),
                    m.windows[0].utilization
                ),
                ("local", Some(0.62))
            );
            assert_eq!(m.hosts, vec!["agent"]);
            assert!(!m.multi_host);
        }
        assert_eq!(
            mergedx(rows.clone(), json!({"namespace": null}), Some("cookys"), N).hosts,
            vec!["agent"]
        );
        assert_eq!(merged(rows.clone()).hosts, vec!["agent", "desk"]);
        let peer_only = mergedx(vec![rows[1].clone()], json!({}), Some("other"), N);
        assert!(peer_only.windows.is_empty() && !peer_only.had_observation);
    }

    #[test]
    fn token_warn_boundaries() {
        assert_eq!(token_warn(None, N), None);
        assert_eq!(token_warn(Some(N + 30 * DAY + 1), N), None);
        assert_eq!(
            token_warn(Some(N + 30 * DAY), N),
            Some(TokenWarn {
                level: "notice",
                days: 30
            })
        );
        assert_eq!(
            token_warn(Some(N + 7 * DAY + 1), N).unwrap().level,
            "notice"
        );
        assert_eq!(
            token_warn(Some(N + 7 * DAY), N),
            Some(TokenWarn {
                level: "urgent",
                days: 7
            })
        );
        assert_eq!(
            token_warn(Some(N + 1), N),
            Some(TokenWarn {
                level: "urgent",
                days: 1
            })
        );
        assert_eq!(token_warn(Some(N), N).unwrap().level, "expired");
        assert_eq!(
            token_warn(Some(N - 3 * DAY), N),
            Some(TokenWarn {
                level: "expired",
                days: -3
            })
        );
    }

    #[test]
    fn tightest_takes_largest_utilization_over_gauge_kinds_never_rpm_tpm_or_null_credits() {
        let m = merged(vec![row(rec(json!({"windows": [
            win("rpm", json!(0.99), N - 5, json!({"resets_at": null})),
            win("weekly", json!(0.4), N - 5, json!({})),
            win("daily", json!(0.7), N - 5, json!({"resets_at": null})),
            win("credits", Value::Null, N - 5, json!({"remaining": 1, "unit": "credits"}))
        ]})))]);
        assert_eq!(tightest(&m).unwrap().kind, "daily");
        assert!(tightest(&merged(vec![row(rec(
            json!({"windows": [win("tpm", json!(0.9), N - 5, json!({}))]})
        ))]))
        .is_none());
        assert!(tightest(&merged(vec![])).is_none());
        assert!(is_tight(0.9) && !is_tight(0.89));
    }

    #[test]
    fn a_merged_window_carries_resets_at_on_the_local_clock() {
        let at = |off: i64, resets: i64| {
            mergedx(vec![row(rec(json!({"windows": [win("rolling_5h", json!(0.5), N - 60, json!({"resets_at": resets}))]})))], json!({"hub_offset": off}), Some("cookys"), N).windows[0].resets_at_local
        };
        assert_eq!(at(600, N + 660), Some((N + 60) as f64));
        assert_eq!(at(-600, N + 60), Some((N + 660) as f64));
        assert_eq!(at(0, N + 60), Some((N + 60) as f64));
    }

    // ---- rejects ----
    fn good_win() -> Value {
        win("rolling_5h", json!(0.5), N - 60, json!({}))
    }
    fn with_rec(patch: Value) -> Value {
        row(with(rec(json!({"windows": [good_win()]})), patch))
    }
    fn with_win(patch: Value) -> Value {
        with_rec(json!({"windows": [with(good_win(), patch)]}))
    }

    #[test]
    fn every_reject_drops_exactly_one_row() {
        let rejects: Vec<(&str, Value)> = vec![
            ("drift pct in a window", with_win(json!({"pct": 0.5}))),
            (
                "drift resetsAt in a window",
                with_win(json!({"resetsAt": 5})),
            ),
            (
                "drift five_hour in the record",
                with_rec(json!({"five_hour": {}})),
            ),
            (
                "drift seven_day in the record",
                with_rec(json!({"seven_day": {}})),
            ),
            (
                "an *_ms key in the record",
                with_rec(json!({"written_at_ms": 1})),
            ),
            (
                "an *_ms key in a window",
                with_win(json!({"observed_at_ms": 1})),
            ),
            (
                "utilization above 1",
                with_win(json!({"utilization": 1.01})),
            ),
            (
                "utilization below 0",
                with_win(json!({"utilization": -0.01})),
            ),
            (
                "null utilization on a non-credits window",
                with_win(json!({"utilization": null})),
            ),
            (
                "null utilization on credits without remaining",
                with_win(json!({"kind": "credits", "utilization": null})),
            ),
            (
                "a millisecond-sized observed_at",
                with_win(json!({"observed_at": 1793699940000_i64})),
            ),
            (
                "a millisecond-sized resets_at",
                with_win(json!({"resets_at": 1793707500000_i64})),
            ),
            (
                "a fractional observed_at",
                with_win(json!({"observed_at": N as f64 - 0.5})),
            ),
            ("provider mismatch", with_rec(json!({"provider": "codex"}))),
            ("alias mismatch", with_rec(json!({"alias": "other"}))),
            (
                "a duplicate kind",
                with_rec(json!({"windows": [good_win(), good_win()]})),
            ),
            ("an unknown kind", with_win(json!({"kind": "hourly"}))),
            ("window source 429", with_win(json!({"source": "429"}))),
            (
                "window source statusline",
                with_win(json!({"source": "statusline"})),
            ),
            ("used without limit", with_win(json!({"used": 5}))),
            (
                "limit without used",
                with_win(json!({"limit": 5, "unit": "requests"})),
            ),
            (
                "used and limit without unit",
                with_win(json!({"used": 1, "limit": 5})),
            ),
            (
                "a unit outside the enum",
                with_win(json!({"used": 1, "limit": 5, "unit": "bytes"})),
            ),
            (
                "remaining on a non-credits window",
                with_win(json!({"remaining": 1, "unit": "credits"})),
            ),
            (
                "more than 7 windows",
                with_rec(json!({"windows": (0..8).map(|_| good_win()).collect::<Vec<_>>()})),
            ),
            (
                "a record carrying writer_courier",
                with_rec(json!({"writer_courier": "local"})),
            ),
            (
                "a record carrying namespace",
                with_rec(json!({"namespace": "cookys"})),
            ),
            ("a record carrying sent_at", with_rec(json!({"sent_at": N}))),
            ("a bad visibility", with_rec(json!({"visibility": "L9"}))),
            (
                "windows with visibility L0",
                with_rec(json!({"visibility": "L0"})),
            ),
            (
                "windows with visibility L1",
                with_rec(json!({"visibility": "L1"})),
            ),
            ("a bad schema", with_rec(json!({"schema": "fleet.quota/2"}))),
            (
                "a bad refusal reason",
                with_rec(
                    json!({"visibility": "L1", "windows": [], "refusal": {"observed_at": N, "retry_at": null, "reason": "busy"}}),
                ),
            ),
            (
                "a refusal with an extra key",
                with_rec(
                    json!({"visibility": "L1", "windows": [], "refusal": {"observed_at": N, "retry_at": null, "reason": "quota", "kind": "weekly"}}),
                ),
            ),
            (
                "a refusal with L0",
                with_rec(
                    json!({"visibility": "L0", "refusal": {"observed_at": N, "retry_at": null, "reason": "quota"}}),
                ),
            ),
            (
                "a bad token_state",
                with_rec(json!({"token_state": "revoked"})),
            ),
            (
                "token_state_at set while unknown",
                with_rec(json!({"token_state_at": N - 1})),
            ),
            (
                "token_state ok without token_state_at",
                with_rec(json!({"token_state": "ok"})),
            ),
            (
                "record source on a record with windows",
                with_rec(json!({"source": "registry"})),
            ),
            (
                "an unknown record source",
                with_rec(json!({"visibility": "L0", "windows": [], "source": "manual"})),
            ),
            (
                "L3 with no windows",
                with_rec(json!({"visibility": "L3", "windows": [], "source": "registry"})),
            ),
            (
                "empty windows with no placeholder source, refusal or state",
                with_rec(json!({"visibility": "L0", "windows": []})),
            ),
            (
                "a refusal-only record that also carries a source",
                with_rec(
                    json!({"visibility": "L1", "windows": [], "source": "registry", "refusal": {"observed_at": N, "retry_at": null, "reason": "quota"}}),
                ),
            ),
            ("an unknown extra record key", with_rec(json!({"extra": 1}))),
            (
                "a row with a bad observed_at_hub",
                rowo(
                    rec(json!({"windows": [good_win()]})),
                    json!({"observed_at_hub": 1793699940000_i64}),
                ),
            ),
            (
                "a row with a non-boolean skewed",
                rowo(
                    rec(json!({"windows": [good_win()]})),
                    json!({"skewed": "no"}),
                ),
            ),
        ];
        assert_eq!(rejects.len(), 46);
        for (name, bad) in rejects {
            let v = ok(&view(vec![bad]));
            assert!(v.rows.is_empty(), "reject: {name}");
            assert_eq!(v.rejected, 1, "reject: {name}");
        }
    }

    #[test]
    fn a_bad_row_is_dropped_and_counted_a_repeated_writer_courier_is_a_bad_row() {
        let v = ok(&view(vec![
            row(d1()),
            with_win(json!({"utilization": 2})),
            row(d1()),
        ]));
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rejected, 2);
    }

    #[test]
    fn absent_optional_fields_read_as_null_absent_token_state_reads_as_unknown() {
        let bare = json!({"schema": "fleet.quota/1", "provider": "claude", "alias": "work", "visibility": "L2", "host_role": "agent",
            "windows": [{"kind": "weekly", "utilization": 0.2, "source": "passive", "observed_at": N - 5}]});
        let v = ok(&view(vec![row(bare)]));
        let r = &v.rows[0].record;
        let w = &r.windows[0];
        assert_eq!(
            (w.used, w.limit, w.unit, w.remaining, w.resets_at),
            (None, None, None, None, None)
        );
        assert_eq!(
            (
                r.refusal.is_none(),
                r.token_state,
                r.token_state_at,
                r.token_issued_at,
                r.token_expires_at
            ),
            (true, "unknown", None, None, None)
        );
    }

    #[test]
    fn a_bad_view_wrapper_is_bad() {
        let bad = |t: &str, p: &str, a: &str| parse_quota_view(t, p, a).is_err();
        assert!(bad("{", "claude", "work"));
        assert!(bad(
            &viewo(vec![], json!({"schema": "fleet.quota/1"})),
            "claude",
            "work"
        ));
        assert!(bad(&viewo(vec![], json!({"extra": 1})), "claude", "work"));
        assert!(bad(&viewo(vec![], json!({"rows": {}})), "claude", "work"));
        assert!(bad(
            &viewo(vec![], json!({"hub_offset": "x"})),
            "claude",
            "work"
        ));
        assert!(bad(
            &viewo(vec![], json!({"namespace": 5})),
            "claude",
            "work"
        ));
        let pad = json!({"schema": "fleet.quota-view/1", "namespace": "c", "hub_offset": 0, "rows": [], "pad": "x".repeat(VIEW_MAX_BYTES)}).to_string();
        assert!(bad(&pad, "claude", "work"));
        assert!(bad(&view(vec![]), "Claude", "work"));
        assert!(bad(&view(vec![]), "claude", "../x"));
        assert!(!bad(&view(vec![]), "claude", "work"));
        assert_eq!(
            ok(&viewo(vec![], json!({"namespace": null}))).namespace,
            None
        );
        assert_eq!(
            ok(&viewo(vec![], json!({"hub_offset": -600}))).hub_offset,
            -600.0
        );
    }

    #[test]
    fn the_pane_parses_by_the_same_contract() {
        // plugins/forge-stats/hooks/quota.ts carries the contract between two markers as strict JSON
        let ts = include_str!("../../plugins/forge-stats/hooks/quota.ts");
        let block = ts
            .split("// contract:begin quota")
            .nth(1)
            .and_then(|r| r.split("// contract:end").next())
            .expect("contract block in quota.ts");
        let json = &block[block.find('{').unwrap()..=block.rfind('}').unwrap()];
        let pane: Value = serde_json::from_str(json).expect("the block is strict JSON");
        assert_eq!(pane, contract());
    }
}
