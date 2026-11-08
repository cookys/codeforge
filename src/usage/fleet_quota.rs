//! `codeforge usage fleet`: one quota table over many hosts' `codeforge.usage-summary/1` files.
//!
//! The dir holds one `*.json` per host (file stem = the host label, named by the collector). Every
//! file goes through `summary::validate`; a bad one is skipped and counted. Merge, evaluation and
//! rendering are pure (`now_ms` is injected). Drawing reuses the G40 view (`quota_view`): each merged
//! alias is mapped to a `QEntry` so the window lines, age, countdown and `緊` come from one renderer.

use super::quota::MAX_AGE_MS;
use super::quota_mirror::{MergedQuota, MergedWindow};
use super::quota_mirror_read::QEntry;
use super::quota_view::{self as qv, Line, Part, Style, Tr};
use super::summary;
use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

pub const SCHEMA: &str = "codeforge.fleet-quota/1";
pub const DEFAULT_USE_TARGET: f64 = 0.85;
/// Every non-Claude provider, each listed even with no account: without quota data its row is
/// `no_source` with the reason. codex has windows when its rollouts carried `rate_limits`.
pub const PROVIDERS: [(&str, SourceNote); 10] = [
    ("kimi", SourceNote::WebOnly),
    ("glm", SourceNote::NeedsToken),
    ("qoder", SourceNote::WebOnly),
    ("qodercn", SourceNote::WebOnly),
    ("minimax", SourceNote::NeedsToken),
    ("codex", SourceNote::NoWindow),
    ("agy", SourceNote::WebOnly),
    ("grok", SourceNote::WebOnly),
    ("cursor", SourceNote::WebOnly),
    ("opencode", SourceNote::WebOnly),
];
const WEEK_S: i64 = 7 * 86400;
const WARMUP_S: i64 = 6 * 3600;
const WASTE_WITHIN_S: i64 = 48 * 3600;
const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 4 << 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Win {
    pub pct: f64,
    pub resets_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warn {
    Waste,
    Short,
}

/// Why a `no_source` row has no quota data (fleet-quota/1 `source_note`, a closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceNote {
    /// the provider shows its quota only on its web page
    WebOnly,
    /// readable with an API token, which autopilot does not mirror yet
    NeedsToken,
    /// the account carries no rate-limit window (codex without `rate_limits`)
    NoWindow,
}
impl SourceNote {
    pub fn name(self) -> &'static str {
        match self {
            SourceNote::WebOnly => "web_only",
            SourceNote::NeedsToken => "needs_token",
            SourceNote::NoWindow => "no_window",
        }
    }
    fn text(self, tr: &Tr) -> String {
        tr.t(&format!("quota.fleet_src_{}", self.name()), &[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// at least one window is drawn
    Ok,
    /// observed within 6 h, but every window has already reset
    Stale,
    NoSource,
    /// last observation older than 6 h
    Expired,
}
impl State {
    fn name(self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Stale => "stale",
            State::NoSource => "no_source",
            State::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weekly {
    pub win: Win,
    pub projected: Option<f64>,
    pub warn: Option<Warn>,
    /// unix seconds
    pub exhaust_at: Option<i64>,
    /// The window's `resets_at` has passed: shown as reset, never warned on.
    pub reset: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Internal key only: never drawn (see [`super::names`]).
    pub alias: String,
    /// What the screens show: the operator's name for the account, else the org short form.
    pub name: String,
    /// `personal` or a sanitized org name from the newest observation that carried one.
    pub org: Option<String>,
    /// The login email from the newest observation that carried one; the alias stays the key.
    pub email: Option<String>,
    pub provider: String,
    pub hosts: Vec<String>,
    /// Hosts whose data for this alias lost the merge: an older capture that disagrees with the
    /// chosen one (subset of `hosts`, sorted).
    pub old_hosts: Vec<String>,
    /// Capture time of the newest chosen window (written_at_ms when no window).
    pub observed_at_ms: Option<i64>,
    pub state: State,
    pub weekly: Option<Weekly>,
    pub five_hour: Option<Win>,
    /// `five_hour` has already reset.
    pub five_reset: bool,
    /// Why there is no source; only on a `no_source` row of a [`PROVIDERS`] provider.
    pub source_note: Option<SourceNote>,
    /// The operator's note ([`attach_notes`]); local only.
    pub note: Option<String>,
    /// The operator's weekly reset rule ([`attach_resets`]) and its next occurrence (epoch s);
    /// operator-provided, low confidence, and never a replacement for a real window's reset.
    pub note_reset_rule: Option<String>,
    pub note_resets_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fleet {
    pub rows: Vec<Row>,
    pub skipped: BTreeMap<&'static str, u64>,
    pub use_target: f64,
}

/// Weekly projection and warning at `now_s` for a window that has not reset yet.
pub fn weekly(win: Win, now_s: i64, use_target: f64) -> Weekly {
    let remaining = win.resets_at - now_s;
    let elapsed = WEEK_S - remaining;
    let mut w = Weekly {
        win,
        projected: None,
        warn: None,
        exhaust_at: None,
        reset: false,
    };
    if elapsed >= WARMUP_S {
        let rate = win.pct / elapsed as f64; // fraction per second
        let projected = win.pct * WEEK_S as f64 / elapsed as f64;
        w.projected = Some(projected);
        if projected >= 1.0 && win.pct < 1.0 {
            w.warn = Some(Warn::Short);
            w.exhaust_at = Some(now_s + ((1.0 - win.pct) / rate).round() as i64);
        } else if remaining < WASTE_WITHIN_S && win.pct < use_target {
            w.warn = Some(Warn::Waste);
        }
    }
    w
}

/// Read every `*.json` in `dir`: (file stem, parsed value) of those that pass `summary::validate`,
/// plus the skip counts by reason. Never aborts on a bad file.
pub type Skipped = BTreeMap<&'static str, u64>;

pub fn load(dir: &Path) -> Result<(Vec<(String, Value)>, Skipped)> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| "讀不了目錄")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    let mut ok = Vec::new();
    let mut skipped: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut bump = |k: &'static str| *skipped.entry(k).or_default() += 1;
    for (i, p) in names.iter().enumerate() {
        if i >= MAX_FILES {
            bump("too_many");
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            bump("bad_name");
            continue;
        };
        let meta = match std::fs::metadata(p) {
            Ok(m) if m.is_file() => m,
            _ => {
                bump("unreadable");
                continue;
            }
        };
        if meta.len() > MAX_BYTES {
            bump("too_large");
            continue;
        }
        let Ok(bytes) = std::fs::read(p) else {
            bump("unreadable");
            continue;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            bump("not_json");
            continue;
        };
        if summary::validate(&v).is_err() {
            bump("invalid");
            continue;
        }
        ok.push((stem.to_string(), v));
    }
    Ok((ok, skipped))
}

/// One host's reading of an alias: each window with its capture time (`seen_at_ms`, else the
/// account's `written_at_ms`).
struct Ob {
    host: String,
    written_at: i64,
    five: Option<(Win, i64)>,
    seven: Option<(Win, i64)>,
}

struct Acc {
    provider: String,
    hosts: BTreeSet<String>,
    /// org and email, each from the newest observation that carries one (`written_at_ms`)
    org: Option<(i64, String)>,
    email: Option<(i64, String)>,
    /// newest `written_at_ms` seen
    newest_written: Option<i64>,
    obs: Vec<Ob>,
}

/// Keep `(at, v)` when it is newer than what `slot` holds.
fn newer(slot: &mut Option<(i64, String)>, at: i64, v: Option<&str>) {
    if let Some(v) = v {
        if slot.as_ref().is_none_or(|(n, _)| at > *n) {
            *slot = Some((at, v.to_string()));
        }
    }
}

fn win_of(v: &Value, written_at: i64) -> Option<(Win, i64)> {
    let w = Win {
        pct: v.get("pct")?.as_f64()?,
        resets_at: v.get("resets_at")?.as_i64()?,
    };
    let at = v
        .get("seen_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(written_at);
    Some((w, at))
}

/// The observation with the newest capture time; a tie goes to the newer file, then the smaller label.
fn pick(obs: &[Ob], of: fn(&Ob) -> Option<(Win, i64)>) -> Option<(String, Win, i64)> {
    let mut best: Option<(String, Win, i64, i64)> = None;
    for o in obs {
        let Some((w, at)) = of(o) else { continue };
        let better = best.as_ref().is_none_or(|(h, _, bat, bw)| {
            (at, o.written_at) > (*bat, *bw) || ((at, o.written_at) == (*bat, *bw) && o.host < *h)
        });
        if better {
            best = Some((o.host.clone(), w, at, o.written_at));
        }
    }
    best.map(|(h, w, at, _)| (h, w, at))
}

/// A host whose reading of this window is older than the chosen one and says something else.
fn lost(o: &Ob, of: fn(&Ob) -> Option<(Win, i64)>, chosen: &Option<(String, Win, i64)>) -> bool {
    match (of(o), chosen) {
        (Some((w, at)), Some((_, cw, cat))) => {
            at < *cat && (w.pct != cw.pct || w.resets_at != cw.resets_at)
        }
        _ => false,
    }
}

/// Merge validated summaries into rows at `now_ms`, sorted: warnings first. Each row is named by
/// `names` (else its org or email); an account with neither (an identity from before the
/// login-derived alias) is dropped and counted under `skipped.no_identity`.
pub fn merge(
    files: &[(String, Value)],
    mut skipped: BTreeMap<&'static str, u64>,
    now_ms: i64,
    use_target: f64,
    names: &super::names::Names,
) -> Fleet {
    let mut by: BTreeMap<String, Acc> = BTreeMap::new();
    let new = |provider: &str| Acc {
        provider: provider.to_string(),
        hosts: BTreeSet::new(),
        org: None,
        email: None,
        newest_written: None,
        obs: Vec::new(),
    };
    for (stem, v) in files {
        for a in v["accounts"].as_array().into_iter().flatten() {
            let (Some(alias), Some(at)) = (a["alias"].as_str(), a["written_at_ms"].as_i64()) else {
                continue;
            };
            // a `codex-<hex8>` key is a codex account carrying its rollouts' windows
            let provider = super::names::key_provider(alias).unwrap_or("claude");
            let e = by.entry(alias.to_string()).or_insert_with(|| new(provider));
            e.provider = provider.into();
            e.hosts.insert(stem.clone());
            // a pushed blob has no email: an older copy that has one still names the account
            newer(&mut e.org, at, a.get("org").and_then(Value::as_str));
            newer(&mut e.email, at, a.get("email").and_then(Value::as_str));
            if e.newest_written.is_none_or(|n| at > n) {
                e.newest_written = Some(at);
            }
            e.obs.push(Ob {
                host: stem.clone(),
                written_at: at,
                five: win_of(&a["five_hour"], at),
                seven: win_of(&a["seven_day"], at),
            });
        }
        for d in v["days"].as_array().into_iter().flatten() {
            let (Some(alias), Some(src)) = (d["account"].as_str(), d["source"].as_str()) else {
                continue;
            };
            by.entry(alias.to_string())
                .or_insert_with(|| new(src))
                .hosts
                .insert(stem.clone());
        }
    }
    for (p, _) in PROVIDERS {
        if !by.values().any(|a| a.provider == p) {
            by.entry(p.to_string()).or_insert_with(|| new(p));
        }
    }
    let now_s = now_ms.div_euclid(1000);
    let rows: Vec<Row> = by
        .into_iter()
        .map(|(alias, acc)| {
            let five = pick(&acc.obs, |o| o.five);
            let seven = pick(&acc.obs, |o| o.seven);
            let mut old_hosts: BTreeSet<String> = BTreeSet::new();
            for o in &acc.obs {
                if lost(o, |o| o.five, &five) || lost(o, |o| o.seven, &seven) {
                    old_hosts.insert(o.host.clone());
                }
            }
            // the capture time of the newest chosen window; the newest file when it has no window
            let at = [&five, &seven]
                .into_iter()
                .flatten()
                .map(|c| c.2)
                .max()
                .or(acc.newest_written);
            let mut r = Row {
                alias,
                name: String::new(),
                org: acc.org.map(|(_, v)| v),
                email: acc.email.map(|(_, v)| v),
                provider: acc.provider,
                hosts: acc.hosts.into_iter().collect(),
                old_hosts: old_hosts.into_iter().collect(),
                observed_at_ms: at,
                state: State::NoSource,
                weekly: None,
                five_hour: None,
                five_reset: false,
                source_note: None,
                note: None,
                note_reset_rule: None,
                note_resets_at: None,
            };
            if let Some(at) = at {
                if now_ms - at >= MAX_AGE_MS {
                    r.state = State::Expired;
                } else {
                    let five = five.map(|c| c.1);
                    let seven = seven.map(|c| c.1);
                    let live = |w: &Option<Win>| w.is_some_and(|w| w.resets_at > now_s);
                    if live(&five) || live(&seven) {
                        // a window past its reset is shown as reset and never warned on
                        r.five_reset = five.is_some_and(|w| w.resets_at <= now_s);
                        r.five_hour = five;
                        r.weekly = seven.map(|w| {
                            if w.resets_at > now_s {
                                weekly(w, now_s, use_target)
                            } else {
                                Weekly {
                                    win: w,
                                    projected: None,
                                    warn: None,
                                    exhaust_at: None,
                                    reset: true,
                                }
                            }
                        });
                        r.state = State::Ok;
                    } else {
                        r.state = State::Stale;
                    }
                }
            }
            if r.state == State::NoSource {
                r.source_note = PROVIDERS
                    .iter()
                    .find(|(p, _)| *p == r.provider)
                    .map(|(_, n)| *n);
            }
            r
        })
        .collect();
    let who: Vec<super::names::Who> = rows
        .iter()
        .map(|r| super::names::Who {
            key: &r.alias,
            org: r.org.as_deref(),
            email: r.email.as_deref(),
        })
        .collect();
    let shown = super::names::assign(&who, names);
    let before = rows.len();
    let mut rows: Vec<Row> = rows
        .into_iter()
        .zip(shown)
        .filter_map(|(r, n)| n.map(|name| Row { name, ..r }))
        .collect();
    if rows.len() < before {
        *skipped.entry("no_identity").or_default() += (before - rows.len()) as u64;
    }
    let key = |r: &Row| {
        let w = r.weekly.as_ref();
        let rank = match w.and_then(|w| w.warn) {
            Some(Warn::Waste) => 0,
            Some(Warn::Short) => 1,
            None => 2,
        };
        let at = if rank < 2 {
            w.map_or(i64::MAX, |w| w.win.resets_at)
        } else {
            0
        };
        (rank, at)
    };
    rows.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.alias.cmp(&b.alias)));
    Fleet {
        rows,
        skipped,
        use_target,
    }
}

/// `claude` or a [`PROVIDERS`] name: a notes key that labels a provider, never an account.
pub fn is_provider_label(s: &str) -> bool {
    s == "claude" || PROVIDERS.iter().any(|(p, _)| *p == s)
}

/// The notes of accounts that have no data (`usage note set --new`): a key that is a valid
/// display name, not a provider label, and not `taken` by an account.
pub fn note_only<'a>(
    notes: &'a super::names::Notes,
    taken: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    notes
        .0
        .iter()
        .filter(move |(k, _)| !is_provider_label(k) && super::names::valid_name(k) && !taken(k))
        .map(|(k, v)| (k.as_str(), v.as_str()))
}

/// Attach the operator's notes: an account key's note, else its provider's. A note whose key
/// matches no row adds a `no_source` Claude row named by the key; when another account already
/// shows that name, that account wins and the note is dropped.
pub fn attach_notes(f: &mut Fleet, notes: &super::names::Notes) {
    for r in &mut f.rows {
        r.note = notes.for_row(&r.alias, &r.provider).map(str::to_string);
    }
    let extra: Vec<Row> = note_only(notes, |k| {
        f.rows.iter().any(|r| r.alias == k || r.name == k)
    })
    .map(|(k, text)| Row {
        alias: k.to_string(),
        name: k.to_string(),
        org: None,
        email: None,
        provider: "claude".to_string(),
        hosts: Vec::new(),
        old_hosts: Vec::new(),
        observed_at_ms: None,
        state: State::NoSource,
        weekly: None,
        five_hour: None,
        five_reset: false,
        source_note: None,
        note: Some(text.to_string()),
        note_reset_rule: None,
        note_resets_at: None,
    })
    .collect();
    f.rows.extend(extra);
}

/// Attach the operator's weekly reset rules: an account key's rule, else its provider's; the
/// next occurrence strictly after `now_ms`. Call after [`attach_notes`] (it adds the no-data rows).
pub fn attach_resets(f: &mut Fleet, resets: &super::names::Resets, now_ms: i64) {
    for r in &mut f.rows {
        let rule = resets.for_row(&r.alias, &r.provider);
        r.note_resets_at = rule
            .and_then(super::reset_rule::parse)
            .map(|x| x.next_after(now_ms.div_euclid(1000)));
        r.note_reset_rule = rule.map(str::to_string);
    }
}

fn warn_name(w: Option<Warn>) -> Value {
    match w {
        Some(Warn::Waste) => json!("waste"),
        Some(Warn::Short) => json!("short"),
        None => Value::Null,
    }
}

/// `codeforge.fleet-quota/1`.
pub fn to_json(f: &Fleet, now_ms: i64) -> Value {
    let accounts: Vec<Value> = f
        .rows
        .iter()
        .map(|r| {
            let mut a = json!({
                "alias": r.alias,
                "name": r.name,
                "org": r.org,
                "email": r.email,
                "provider": r.provider,
                "hosts": r.hosts,
                "old_hosts": r.old_hosts,
                "observed_at_ms": r.observed_at_ms,
                "age_s": r.observed_at_ms.map(|t| ((now_ms - t) / 1000).max(0)),
                "state": r.state.name(),
                "weekly": r.weekly.as_ref().map(|w| json!({
                    "pct": w.win.pct,
                    "resets_at": w.win.resets_at,
                    "projected": w.projected,
                    "warn": warn_name(w.warn),
                    "exhaust_at": w.exhaust_at,
                    "reset": w.reset,
                })),
                "five_hour": r.five_hour.map(|w| json!({"pct": w.pct, "resets_at": w.resets_at, "reset": r.five_reset})),
            });
            // optional keys, present only when set (an older reader's file shape is unchanged)
            if let Some(n) = r.source_note {
                a["source_note"] = json!(n.name());
            }
            if let Some(n) = &r.note {
                a["note"] = json!(n);
            }
            if let (Some(rule), Some(at)) = (&r.note_reset_rule, r.note_resets_at) {
                a["note_reset_rule"] = json!(rule);
                a["note_resets_at"] = json!(at);
            }
            a
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "generated_at_ms": now_ms,
        "use_target": f.use_target,
        "skipped": f.skipped,
        "accounts": accounts,
    })
}

/// Write `v` to `path` atomically: temp file in the same dir, then rename. 0600.
pub fn write_atomic(path: &Path, v: &Value) -> Result<()> {
    use std::io::Write as _;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let res = (|| -> Result<()> {
        let mut f = o.open(&tmp).context("開不了暫存檔")?;
        f.write_all(serde_json::to_string_pretty(v)?.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        std::fs::rename(&tmp, path).context("改名失敗")
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

// ---- text ----

fn level_of(provider: &str) -> &'static str {
    match provider {
        "claude" => "claude",
        "codex" => "codex",
        _ => "—",
    }
}

/// The host labels as drawn: the chosen hosts first, then the old ones, each with the 舊 / old marker.
fn hosts_shown(r: &Row, tr: &Tr) -> Vec<String> {
    let (old, new): (Vec<&String>, Vec<&String>) =
        r.hosts.iter().partition(|h| r.old_hosts.contains(h));
    new.into_iter()
        .cloned()
        .chain(old.into_iter().map(|h| tr.t("quota.host_old", &[("h", h)])))
        .collect()
}

fn entry(r: &Row, now_ms: i64, tr: &Tr) -> QEntry {
    let at = r.observed_at_ms.unwrap_or(0);
    let mk = |kind: &'static str, w: Win| MergedWindow {
        kind,
        utilization: Some(w.pct),
        remaining: None,
        unit: None,
        used: None,
        limit: None,
        resets_at_local: Some(w.resets_at as f64),
        observed_at: at.div_euclid(1000),
        age_s: ((now_ms - at) as f64 / 1000.0).max(0.0),
        writer_courier: String::new(),
        host_role: String::new(),
        source: "fleet",
        estimated: false,
    };
    let mut windows = Vec::new();
    // a window that has already reset is not drawn as a gauge: `window_lines_of` words it
    if let Some(w) = r.five_hour.filter(|_| !r.five_reset) {
        windows.push(mk("rolling_5h", w));
    }
    if let Some(w) = r.weekly.as_ref().filter(|w| !w.reset) {
        windows.push(mk("weekly", w.win));
    }
    QEntry {
        provider: r.provider.clone(),
        alias: r.name.clone(),
        key: r.name.clone(),
        m: MergedQuota {
            windows,
            refusal: None,
            token_state: "unknown",
            token_expires_at: None,
            placeholder: r.state == State::NoSource,
            had_observation: r.state != State::NoSource,
            had_window: true,
            level: level_of(&r.provider),
            hosts: hosts_shown(r, tr),
            multi_host: !r.hosts.is_empty(),
        },
    }
}

fn p(text: impl Into<String>, style: Style) -> Part {
    Part {
        text: text.into(),
        style,
    }
}

fn pct_of(u: f64) -> String {
    format!("{}%", (u * 100.0).round() as i64)
}

fn warn_line(r: &Row, f: &Fleet, now_s: i64, tr: &Tr) -> Option<Line> {
    let w = r.weekly.as_ref()?;
    let warn = w.warn?;
    let dim = |s: String| p(s, Style::Dim);
    let dot = || dim(" · ".into());
    let label = match warn {
        Warn::Waste => tr.t("quota.fleet_waste", &[]),
        Warn::Short => tr.t("quota.fleet_short", &[]),
    };
    let mut l: Line = vec![
        p(label, Style::Warn),
        dot(),
        p(label_of(r), Style::Bold),
        dot(),
        p(
            format!("{} {}", tr.t("quota.w_weekly", &[]), pct_of(w.win.pct)),
            Style::Plain,
        ),
    ];
    if let (Warn::Short, Some(x)) = (warn, w.exhaust_at) {
        l.push(dot());
        l.push(p(
            tr.t(
                "quota.fleet_runout",
                &[("t", &qv::countdown(x as f64, now_s as f64))],
            ),
            Style::Plain,
        ));
    }
    l.push(dot());
    l.push(dim(format!(
        "{} ◷ {}",
        tr.t("quota.fleet_reset", &[]),
        qv::countdown(w.win.resets_at as f64, now_s as f64)
    )));
    if warn == Warn::Waste {
        l.push(dot());
        l.push(dim(
            tr.t("quota.fleet_target", &[("p", &pct_of(f.use_target))])
        ));
    }
    Some(l)
}

/// The email after the name (the first part of a header/compact line), cut to `cap` columns;
/// nothing when unknown.
fn email_shown(r: &Row, cap: usize) -> Option<String> {
    let e = r.email.as_ref()?;
    Some(qv::plain(&qv::clip(&[p(e.clone(), Style::Dim)], cap)))
}

/// Display columns an email may take on a header or warn line.
const EMAIL_CELLS: usize = 32;
/// Display columns the name may take (names are at most 24 chars).
const NAME_CELLS: usize = 24;

/// The account's name cut to `cap` columns; never the internal alias.
fn name_shown(r: &Row, cap: usize) -> String {
    qv::plain(&qv::clip(&[p(r.name.clone(), Style::Plain)], cap))
}

/// The name, then the email when known: `name · email`.
fn label_of(r: &Row) -> String {
    match email_shown(r, EMAIL_CELLS) {
        Some(e) => format!("{} · {e}", name_shown(r, NAME_CELLS)),
        None => name_shown(r, NAME_CELLS),
    }
}

/// Columns the name may take on a line of `cols`: the numbers of a compact line come first.
fn name_cells(cols: usize, compact: bool) -> usize {
    if compact {
        cols.saturating_sub(18).clamp(8, NAME_CELLS)
    } else {
        NAME_CELLS
    }
}

fn add_email(line: &mut Line, r: &Row, sep: &str) {
    if let Some(shown) = email_shown(r, EMAIL_CELLS) {
        line.insert(1, p(format!("{sep}{shown}"), Style::Dim));
    }
}

/// `週 已重置` for the reset windows of a row (compact line); `None` when none has reset.
fn reset_note(r: &Row, tr: &Tr) -> Option<String> {
    let mut v = Vec::new();
    if r.five_reset {
        v.push(tr.t("quota.w_5h", &[]));
    }
    if r.weekly.as_ref().is_some_and(|w| w.reset) {
        v.push(tr.t("quota.w_weekly", &[]));
    }
    (!v.is_empty()).then(|| format!("{} {}", v.join("/"), tr.t("quota.fleet_win_reset", &[])))
}

/// One line per window in 5h, week order: the live ones from the G40 renderer, a reset one as
/// `週 已重置` (its old pct is not shown as current).
fn window_lines_of(r: &Row, e: &QEntry, cols: usize, now_f: f64, tr: &Tr) -> Vec<Line> {
    let mut live = qv::window_lines(&e.m, cols, now_f, tr).into_iter();
    let mut out = Vec::new();
    let mut one = |present: bool, reset: bool, label: &str| {
        if !present {
            return;
        }
        if reset {
            out.push(vec![p(
                format!("  {label} {}", tr.t("quota.fleet_win_reset", &[])),
                Style::Idle,
            )]);
        } else if let Some(l) = live.next() {
            out.push(l);
        }
    };
    one(
        r.five_hour.is_some(),
        r.five_reset,
        &tr.t("quota.w_5h", &[]),
    );
    one(
        r.weekly.is_some(),
        r.weekly.as_ref().is_some_and(|w| w.reset),
        &tr.t("quota.w_weekly", &[]),
    );
    out
}

/// A note shorter than this needs only its own width; a longer one is cut, but not below this.
const NOTE_MIN_CELLS: usize = 8;

/// The operator's tail for a row: the note, then `◷ 2d3h` to the next operator-rule reset. When
/// both do not fit in `room` the note is cut, never the countdown; `None` when too little room.
fn tail_text(r: &Row, now_s: i64, room: usize) -> Option<String> {
    let cd = r
        .note_resets_at
        .map(|at| format!("◷ {}", qv::countdown(at as f64, now_s as f64)));
    let w = |s: &str| UnicodeWidthStr::width(s);
    match (&r.note, cd) {
        (None, None) => None,
        (None, Some(cd)) => (w(&cd) <= room).then_some(cd),
        (Some(n), None) => (room >= NOTE_MIN_CELLS.min(w(n))).then(|| super::names::clip(n, room)),
        (Some(n), Some(cd)) => {
            let full = format!("{n} {cd}");
            if w(&full) <= room {
                return Some(full);
            }
            let note_room = room.saturating_sub(w(&cd) + 1);
            if note_room >= NOTE_MIN_CELLS.min(w(n)) {
                Some(format!("{} {cd}", super::names::clip(n, note_room)))
            } else {
                (w(&cd) <= room).then_some(cd)
            }
        }
    }
}

/// `line` with the operator's tail appended dim after two spaces; `None` when too little room.
fn with_tail(line: &Line, r: &Row, now_s: i64, cols: usize) -> Option<Line> {
    let room = cols.checked_sub(qv::width(line) + 2)?;
    let text = tail_text(r, now_s, room)?;
    let mut l = line.clone();
    l.push(p("  ", Style::Plain));
    l.push(p(text, Style::Dim));
    Some(l)
}

/// The whole screen, `cols` <= 100.
pub fn text(f: &Fleet, cols: usize, now_ms: i64, tr: &Tr, color: bool) -> String {
    let now_s = now_ms.div_euclid(1000);
    let mut lines: Vec<Line> = Vec::new();
    let title = tr.t("quota.fleet_title", &[]);
    let count = tr.t("quota.fleet_count", &[("n", &f.rows.len().to_string())]);
    let gap = cols.saturating_sub(
        UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(count.as_str()),
    );
    if gap >= 2 {
        lines.push(vec![
            p(title, Style::Bold),
            p(" ".repeat(gap), Style::Plain),
            p(count, Style::Dim),
        ]);
    } else {
        lines.push(vec![p(title, Style::Bold)]);
    }
    lines.push(vec![p("─".repeat(cols), Style::Dim)]);
    let warns: Vec<Line> = f
        .rows
        .iter()
        .filter_map(|r| warn_line(r, f, now_s, tr))
        .collect();
    if warns.is_empty() {
        lines.push(vec![p(tr.t("quota.fleet_no_warn", &[]), Style::Dim)]);
    } else {
        lines.extend(warns);
    }
    lines.push(vec![p(" ", Style::Plain)]);
    let compact = cols < qv::COMPACT_COLS;
    let mut tight = false;
    let mut first = true;
    for r in &f.rows {
        let mut e = entry(r, now_ms, tr);
        e.key = name_shown(r, name_cells(cols, compact));
        let facts = qv::facts_of(std::slice::from_ref(&e), now_s, tr);
        let fact = &facts[0];
        if compact {
            let mut line = qv::compact_line(fact, now_s as f64, cols, tr);
            // a reset window is named when it still fits
            if let Some(l) = reset_note(r, tr) {
                let mut with = line.clone();
                with.push(p(format!(" {l}"), Style::Idle));
                if qv::width(&with) <= cols {
                    line = with;
                }
            }
            if let Some(n) = r.source_note {
                let mut with = line.clone();
                with.push(p(format!(" · {}", n.text(tr)), Style::Dim));
                if qv::width(&with) <= cols {
                    line = with;
                }
            }
            // the operator's note before the email: it is what the schedule needs
            if let Some(l) = with_tail(&line, r, now_s, cols) {
                line = l;
            }
            // the email only when the line still fits with it; the numbers come first
            let mut with = line.clone();
            add_email(&mut with, r, " ");
            if qv::width(&with) <= cols {
                line = with;
            }
            lines.push(line);
            continue;
        }
        if !first {
            lines.push(vec![p(" ", Style::Plain)]);
        }
        first = false;
        lines.push(qv::header_line(
            fact,
            cols,
            tr,
            email_shown(r, EMAIL_CELLS).as_deref(),
        ));
        match r.state {
            State::Ok => {
                lines.extend(window_lines_of(r, &e, cols, now_s as f64, tr));
                tight |= e.m.windows.iter().any(qv::win_tight);
                if r.weekly
                    .as_ref()
                    .is_some_and(|w| !w.reset && w.projected.is_none())
                {
                    lines.push(vec![p(
                        format!("  {}", tr.t("quota.fleet_warmup", &[])),
                        Style::Idle,
                    )]);
                }
            }
            State::Stale => lines.push(vec![p(
                format!("  {}", tr.t("quota.fleet_reset_wait", &[])),
                Style::Idle,
            )]),
            State::Expired => lines.push(vec![p(
                format!("  {}", tr.t("quota.fleet_expired", &[])),
                Style::Idle,
            )]),
            State::NoSource => {
                let mut l = vec![p(
                    format!("  {}", tr.t("quota.fleet_no_source", &[])),
                    Style::Idle,
                )];
                if let Some(n) = r.source_note {
                    l.push(p(format!(" · {}", n.text(tr)), Style::Dim));
                }
                lines.push(l);
            }
        }
        // the operator's note ends the row: on its last line when it fits there, else its own line
        if r.note.is_some() || r.note_resets_at.is_some() {
            let last = lines.len() - 1; // the block has at least its header line
            match with_tail(&lines[last], r, now_s, cols) {
                Some(l) => lines[last] = l,
                None => {
                    if let Some(t) = tail_text(r, now_s, cols.saturating_sub(2)) {
                        lines.push(vec![p(format!("  {t}"), Style::Dim)]);
                    }
                }
            }
        }
    }
    let skipped: u64 = f.skipped.values().sum();
    if skipped > 0 {
        lines.push(vec![p(
            format!(
                "  {}",
                tr.t("quota.fleet_skipped", &[("n", &skipped.to_string())])
            ),
            Style::Dim,
        )]);
    }
    if tight {
        lines.push(vec![p(" ", Style::Plain)]);
        lines.push(vec![p(tr.t("quota.legend_tight", &[]), Style::Dim)]);
    }
    let mut out = String::new();
    for l in lines.iter().map(|l| qv::clip(l, cols)) {
        for s in &l {
            out.push_str(&qv::paint(s, color));
        }
        out.push('\n');
    }
    out
}
