use crate::daemon::strategy::DEFAULT_STRATEGY;
use crate::db;
use crate::pet::live_state::LiveState;
use crate::pet::session::update_last_seen;
use crate::pet::village::VILLAGES;
/// Statusline — exactly one line, never the pet (the pet lives in its own tab; PM 01M7MEY1).
use anyhow::Result;
use owo_colors::OwoColorize;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

pub fn run(ctx: &db::Context) -> Result<()> {
    let (mut data, raw) = read_status_input();
    write_live_context(&raw);
    let raw_sid = raw["session_id"].as_str().unwrap_or("");
    // The statusline is always one line and never draws the pet: the pet lives in its own tab
    // (operator via PM 01M7MEY1, 2026-11-17). It used to draw a full RPG box unless a fresh
    // forge-stats `layout: band` heartbeat existed for this session (15be75a).
    {
        let (live_base, _source) = crate::live::resolve_live_base();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // G36: this account's quota snapshot out, the other accounts' back in.
        // A setup-token lane sets CODEFORGE_ACCOUNT_ALIAS and does not rewrite the login
        // file (PM 01M76F3F). That override is the whole identity: do not read the login.
        // A token lane without the override would be filed under the login file's account,
        // which is another org's (hangar 01M77KXH): such a session is pinned unattributed.
        // Only the token env's presence is checked; its value is never read.
        // Decision lives in usage::lane_pin (also used at SessionStart for headless -p).
        let lane = crate::usage::lane_pin::lane_pin_from_env();
        let pin = lane.is_some();
        let (config_dir, acct) = if let Some(lane) = &lane {
            let acct = lane
                .override_alias
                .as_ref()
                .map(|alias| crate::usage::alias::Identity {
                    alias: alias.clone(),
                    org: None,
                    org_id: None,
                    email: None,
                });
            (lane.config_dir.clone(), acct)
        } else {
            crate::usage::alias::current_claude_account(&live_base)
        };
        let quota_dir = crate::usage::quota::quota_dir(&live_base);
        let root_hash = crate::usage::alias::path_hash8(&config_dir);
        let session = crate::live::sanitize_session_id(raw_sid);
        let others = if pin {
            record_quota_with(
                &mut data,
                &quota_dir,
                acct.as_ref(),
                &root_hash,
                &session,
                now_ms,
                true,
            )
        } else {
            record_quota(
                &mut data,
                &quota_dir,
                acct.as_ref(),
                &root_hash,
                &session,
                now_ms,
            )
        };
        data.others = others;
        data.names = crate::usage::names::load_default();
        // G40: once `<live_dir>/quota/` exists the mirror is the only source on this row (design 7 rule 7).
        data.mirror = crate::usage::quota_mirror_read::load_for_statusline(now_ms / 1000);
        // G36: a background `usage collect` at most every 15 min (no SessionEnd hook needed).
        // Never from a test build: current_exe() would be the test harness.
        #[cfg(not(test))]
        crate::usage::autocollect::maybe_trigger(
            &crate::usage::summary::summary_dir(&live_base),
            std::env::var(crate::usage::autocollect::ENV_OFF)
                .ok()
                .as_deref(),
            now_ms,
            crate::usage::autocollect::spawn_collect,
        );
        data.band_facts = read_band_facts(&live_base, raw_sid, now_ms);
    }
    let conn = ctx.open_db()?;
    let has_pet = LiveState::exists(&conn).unwrap_or(false);

    let width = statusline_width(
        std::env::var("CODEFORGE_WIDTH").ok().as_deref(),
        std::env::var("COLUMNS").ok().as_deref(),
    );

    let stdout = io::stdout();
    let mut out = stdout.lock();

    // LiveState composes daemon-authored pet_snapshot (fallback: Phase 1 pet)
    // with an overlay of unseen event_inbox XP — so the pet number reacts
    // to hook events immediately, without waiting for the daemon tick.
    let loaded = if has_pet {
        LiveState::load(&conn).ok().flatten()
    } else {
        None
    };

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Window is closed — next statusline call sees a fresh baseline.
    let _ = update_last_seen(&conn, now_unix);

    let (brain, active_l1) = brain_health(ctx);

    // Colours follow the terminal theme (PM 01M88BD7K5): the line is drawn with the dark palette
    // and recoloured once; the layout never changes.
    let theme = detect_theme(|k| std::env::var(k).ok(), &raw);
    let mut line = Vec::new();
    render_line(&mut line, &data, width)?;
    out.write_all(apply_theme(&String::from_utf8_lossy(&line), theme).as_bytes())?;
    match loaded {
        Some(live) => {
            let village = VILLAGES
                .iter()
                .find(|v| v.id == live.state.village)
                .unwrap_or(&VILLAGES[2]);
            let strategy = live.strategy.unwrap_or(DEFAULT_STRATEGY);
            crate::pet::view::publish(
                Some((&live.state, village)),
                crate::pet::view::Extras {
                    mood: live.mood,
                    strategy: Some(strategy.as_str()),
                    message: live.last_message.as_deref(),
                },
            );
        }
        None => crate::pet::view::publish(None, Default::default()),
    }

    out.flush()?;

    let (memory_light, brain_light) = light_words(&brain);
    crate::pet::view::write_session_view(raw_sid, memory_light, active_l1, brain_light);

    // Task 8: fire-and-forget detached probe spawn (after render, non-blocking).
    // Conditions: opted_in && should_refresh && lock acquired → spawn child.
    crate::mnemos::health::maybe_spawn_probe();

    Ok(())
}

// ─── Brain health (Task 13): local + central lights ─────────────────────────

/// The local (L1 memory) and central (Mnemos) lights plus the active L1 count,
/// computed once per statusline run for the bottom border and the session view.
fn brain_health(ctx: &db::Context) -> (crate::mnemos::health::BrainHealth, usize) {
    use crate::memory::l1;
    use crate::mnemos::config::MnemosConfig;
    use crate::mnemos::health::{
        central_light, fresh_enough, local_light, queue_degraded, read_liveness, read_ship,
        BrainHealth,
    };

    let store_dir = ctx.project_dir.join("store");
    let active_l1 = l1::count_active(&store_dir);
    let has_store_history = store_dir.join("concepts").exists();
    let local = local_light(active_l1, has_store_history);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let lv = read_liveness();
    let lv_fresh = lv.as_ref().filter(|l| fresh_enough(l, now));
    let central = central_light(
        MnemosConfig::opted_in(),
        lv_fresh,
        read_ship().as_ref(),
        queue_degraded(),
        now,
    );
    (BrainHealth { local, central }, active_l1)
}

/// Session-view words for the lights (`codeforge.session-view/1`).
fn light_words(b: &crate::mnemos::health::BrainHealth) -> (&'static str, &'static str) {
    use crate::mnemos::health::{CentralLight, LocalLight};
    let memory = match b.local {
        LocalLight::Active => "active",
        LocalLight::Empty => "empty",
        LocalLight::Hidden => "hidden",
    };
    let brain = match b.central {
        CentralLight::Ok => "ok",
        CentralLight::Degraded => "degraded",
        CentralLight::Offline => "offline",
        CentralLight::Pending => "pending",
        CentralLight::Hidden => "hidden",
    };
    (memory, brain)
}

// ─── forge-stats band heartbeat (doc/plans/2026-10-31-statusline-band.md §2.5) ─

/// Autopilot band-facts older than this are treated as absent.
const BAND_FACTS_FRESH_MS: i64 = 30_000;
/// Heartbeats and band-facts are short status files; anything larger is rejected.
const LIVE_JSON_MAX_BYTES: u64 = 4096;

/// Read a size-capped per-session JSON file whose `session_id` equals `raw_sid`
/// and whose `written_at_ms` is within `0..fresh_ms` of `now_ms`.
fn read_live_session_json(
    dir: &std::path::Path,
    raw_sid: &str,
    now_ms: i64,
    fresh_ms: i64,
) -> Option<serde_json::Value> {
    if raw_sid.is_empty() {
        return None;
    }
    let path = dir.join(format!(
        "{}.json",
        crate::live::sanitize_session_id(raw_sid)
    ));
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > LIVE_JSON_MAX_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let age = v["written_at_ms"]
        .as_i64()
        .and_then(|w| now_ms.checked_sub(w))?;
    if v["session_id"].as_str() != Some(raw_sid) || !(0..fresh_ms).contains(&age) {
        return None;
    }
    Some(v)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoundMode {
    Floor,
    Round,
}

#[derive(Clone, Debug, PartialEq)]
struct Contract {
    quota_warn_pct: f64,
    quota_error_pct: f64,
    round: RoundMode,
    bar_on: String,
    bar_off: String,
    alert: String,
    #[allow(dead_code)] // v1 contract field reserved for age labels
    age: String,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            quota_warn_pct: 80.0,
            quota_error_pct: 100.0,
            round: RoundMode::Floor,
            bar_on: "▰".to_string(),
            bar_off: "▱".to_string(),
            alert: "⏶".to_string(),
            age: "◷".to_string(),
        }
    }
}

/// Strip control characters first, then take at most `max_chars` Unicode scalar values.
fn clean_user_str(s: &str, max_chars: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect()
}

fn clean_glyph(v: Option<&serde_json::Value>, default: &str) -> String {
    v.and_then(|x| x.as_str())
        .map(|s| clean_user_str(s, 2))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

impl Contract {
    fn from_json(v: &serde_json::Value) -> Self {
        let def = Self::default();
        let Some(obj) = v.as_object() else {
            return def;
        };
        let quota_warn_pct = obj
            .get("quota_warn_pct")
            .and_then(|x| x.as_f64())
            .filter(|x| x.is_finite() && *x >= 0.0)
            .unwrap_or(def.quota_warn_pct);
        let quota_error_pct = obj
            .get("quota_error_pct")
            .and_then(|x| x.as_f64())
            .filter(|x| x.is_finite() && *x >= 0.0)
            .unwrap_or(def.quota_error_pct);
        let round = match obj.get("round").and_then(|x| x.as_str()) {
            Some("round") => RoundMode::Round,
            _ => def.round,
        };
        Self {
            quota_warn_pct,
            quota_error_pct,
            round,
            bar_on: clean_glyph(obj.get("bar_on"), &def.bar_on),
            bar_off: clean_glyph(obj.get("bar_off"), &def.bar_off),
            alert: clean_glyph(obj.get("alert"), &def.alert),
            age: clean_glyph(obj.get("age"), &def.age),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct BandFacts {
    contract: Contract,
    live_dispatches: u64,
    position: Option<String>,
    unit: Option<String>,
    review_round: Option<u64>,
    decisions_open: u64,
    usd: Option<f64>,
    cap_usd: Option<f64>,
}

/// Read `<live_base>/autopilot/band-facts/<sid>.json` (v1, untrusted).
/// Valid only if it parses, `v == 1`, `session_id` equals `raw_sid`, size `<= 4096`, and age `< 30 s`.
fn read_band_facts(base: &std::path::Path, raw_sid: &str, now_ms: i64) -> Option<BandFacts> {
    let dir = base.join("autopilot").join("band-facts");
    let v = read_live_session_json(&dir, raw_sid, now_ms, BAND_FACTS_FRESH_MS)?;
    if v["v"].as_u64() != Some(1) {
        return None;
    }
    let p = &v["progress"];
    let s = &v["spend"];
    Some(BandFacts {
        contract: Contract::from_json(&v["contract"]),
        live_dispatches: p["live_dispatches"].as_u64().unwrap_or(0),
        position: p["position"]
            .as_str()
            .map(|raw| clean_user_str(raw, 16))
            .filter(|clean| !clean.is_empty()),
        unit: p["unit"]
            .as_str()
            .map(|raw| clean_user_str(raw, 12))
            .filter(|clean| !clean.is_empty()),
        review_round: p["review_round"].as_u64(),
        decisions_open: p["decisions_open"].as_u64().unwrap_or(0),
        usd: s["usd"].as_f64().filter(|x| x.is_finite() && *x >= 0.0),
        cap_usd: s["cap_usd"].as_f64().filter(|x| x.is_finite() && *x > 0.0),
    })
}

/// The width to draw at: `CODEFORGE_WIDTH` when set, else the terminal's `COLUMNS` (set by
/// Claude Code when it starts the statusline, following a resize) minus 4 (Claude Code indents
/// the statusline 2 columns and keeps 2 free on the right: measured at 120 and 160 columns),
/// else 100.
fn statusline_width(codeforge_width: Option<&str>, columns: Option<&str>) -> usize {
    let parse = |v: Option<&str>| v.and_then(|v| v.trim().parse::<usize>().ok());
    parse(codeforge_width)
        .or_else(|| parse(columns).map(|c| c.saturating_sub(4)))
        .unwrap_or(100)
}

// ─── Input ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct StatusInput {
    model: Option<String>,
    cwd: Option<String>,
    version: Option<String>, // running session version (from /proc tree)
    latest_version: Option<String>, // latest installed version (symlink target)
    update_available: bool,  // latest > session version
    context_pct: Option<f64>, // 0.0–1.0
    five_hour_pct: Option<f64>,
    five_hour_resets_at: Option<i64>,
    seven_day_pct: Option<f64>,
    seven_day_resets_at: Option<i64>,
    /// G36: the other accounts' fresh quota snapshots, most constrained first.
    others: Vec<crate::usage::quota::Snapshot>,
    /// The operator's account names (`codeforge usage alias set`); see `usage::names`.
    names: crate::usage::names::Names,
    /// G40: the quota mirror's merged entries; `None` while `<live_dir>/quota/` does not exist (then `others` is drawn, as before).
    mirror: Option<Vec<crate::usage::quota_mirror_read::QEntry>>,
    /// Autopilot band-facts for the compact top row and unified quota styling.
    band_facts: Option<BandFacts>,
}

// ─── G36 per-account quota (doc/plans/2026-11-03-usage-collector.md §2–3) ─────

/// Show this account's on-disk snapshot in place of the stdin `rate_limits`. A window the
/// file does not have (or a missing file) is cleared, so a pre-switch reading is not drawn
/// as the new account's. A read problem leaves the bars empty rather than failing the render.
fn apply_disk_quota(data: &mut StatusInput, dir: &std::path::Path, alias: &str, now_ms: i64) {
    use crate::usage::quota::{read_own, Window};
    let snap = read_own(dir, alias);
    let live = |w: Option<Window>| w.filter(|w| w.resets_at * 1000 > now_ms);
    let (five, seven) = match snap {
        Some(s) => (live(s.five_hour), live(s.seven_day)),
        None => (None, None),
    };
    data.five_hour_pct = five.as_ref().map(|w| w.pct);
    data.five_hour_resets_at = five.as_ref().map(|w| w.resets_at);
    data.seven_day_pct = seven.as_ref().map(|w| w.pct);
    data.seven_day_resets_at = seven.as_ref().map(|w| w.resets_at);
}

/// Merge this account's quota into its snapshot when Claude Code reported `rate_limits`
/// (throttled, 0600; a window's value only grows, see `quota::merge_snapshot`) and show
/// the merged value on this session's own bars, so an idle session no longer shows the
/// lower value of its last API call. Then read the other accounts' snapshots back.
/// Never fails the statusline: a write or read error only means fewer accounts shown.
///
/// A session that already filed this exact `rate_limits` fingerprint under another alias
/// is still showing the pre-switch response. That reading is not merged; the bars show
/// the current account's snapshot on disk instead. Any new read or write error skips the
/// gate and merges as before, with nothing on stderr.
fn record_quota(
    data: &mut StatusInput,
    dir: &std::path::Path,
    acct: Option<&crate::usage::alias::Identity>,
    root: &str,
    session: &str,
    now_ms: i64,
) -> Vec<crate::usage::quota::Snapshot> {
    record_quota_with(data, dir, acct, root, session, now_ms, false)
}

/// `pin`: this process's account is `CODEFORGE_ACCOUNT_ALIAS`, not the login file; with
/// `acct` `None` it is a token lane without that override, pinned unattributed.
/// The snapshot is filed under that alias with `root: None` (so `root_alias` does not
/// adopt it), the login timeline is not observed, and the session is pinned. The session
/// gate still runs with the override alias. Org and email are dropped.
fn record_quota_with(
    data: &mut StatusInput,
    dir: &std::path::Path,
    acct: Option<&crate::usage::alias::Identity>,
    root: &str,
    session: &str,
    now_ms: i64,
    pin: bool,
) -> Vec<crate::usage::quota::Snapshot> {
    use crate::usage::alias::Identity;
    use crate::usage::logins::{self, SessionGate};
    use crate::usage::quota::{merge_snapshot, read_others, Seen, Snapshot, Window};
    let window = |pct: Option<f64>, at: Option<i64>| match (pct, at) {
        // an overrun past the cap is filed as the cap, so the file always reads back
        (Some(pct), Some(resets_at)) if pct.is_finite() && pct >= 0.0 => Some(Window {
            pct: pct.min(crate::usage::quota::PCT_CAP),
            resets_at,
            seen: Some(Seen {
                at_ms: now_ms,
                session: session.to_string(),
            }),
        }),
        _ => None,
    };
    // an override never carries the login file's org or email
    let filed = acct.map(|a| {
        if pin {
            Identity {
                alias: a.alias.clone(),
                org: None,
                org_id: None,
                email: None,
            }
        } else {
            a.clone()
        }
    });
    // no login: unattributed, nothing written (PM 01M7052H79); the others are still read.
    // A token lane without an override alias (`pin` and no account) pins its session
    // unattributed, so collect does not file its requests under the login's account.
    let Some(acct) = filed.as_ref() else {
        if pin && crate::usage::quota::is_hash8(root) {
            if let Some(parent) = dir.parent() {
                let _ =
                    logins::pin_session(parent, root, session, logins::PIN_UNATTRIBUTED, now_ms);
            }
        }
        return read_others(dir, "", now_ms);
    };
    // a pinned lane must not append itself to the root's login timeline
    if let Some(parent) = dir.parent() {
        if crate::usage::quota::is_hash8(root) && crate::usage::alias::valid_alias(&acct.alias) {
            if pin {
                let _ = logins::pin_session(parent, root, session, &acct.alias, now_ms);
            } else {
                let _ = logins::observe(parent, root, &acct.alias, now_ms);
            }
        }
    }
    let snap = Snapshot {
        alias: acct.alias.clone(),
        org: acct.org.clone(),
        org_id: acct.org_id.clone(),
        email: acct.email.clone(),
        root: if pin {
            None
        } else {
            crate::usage::quota::is_hash8(root).then(|| root.to_string())
        },
        written_at_ms: now_ms,
        // the session's own login state is not reported in the row: the writer does not know
        auth_state: None,
        five_hour: window(data.five_hour_pct, data.five_hour_resets_at),
        seven_day: window(data.seven_day_pct, data.seven_day_resets_at),
    };
    let has_limits = snap.five_hour.is_some() || snap.seven_day.is_some();
    // fingerprint the stdin values before a merge or a disk render replaces them
    let carried = has_limits
        && dir.parent().is_some_and(|parent| {
            let fp = logins::rate_fingerprint(
                data.five_hour_pct,
                data.five_hour_resets_at,
                data.seven_day_pct,
                data.seven_day_resets_at,
            );
            match logins::session_gate(parent, root, session, &acct.alias, &fp, now_ms) {
                Ok(SessionGate::Carry) => true,
                // a gate error, or Merge: file the reading as before
                _ => false,
            }
        });
    if carried {
        apply_disk_quota(data, dir, &acct.alias, now_ms);
    } else if has_limits {
        if let Ok((merged, _)) = merge_snapshot(dir, &snap) {
            // merge keeps an older file's root; an override must not become this root's alias
            let merged = if pin && merged.root.is_some() {
                let mut cleared = merged;
                cleared.root = None;
                let _ = crate::live::write_live_json(
                    dir,
                    &format!("{}.json", cleared.alias),
                    &cleared.to_json(),
                );
                cleared
            } else {
                merged
            };
            // a window past its reset is unknown (quota.rs), on this account's own row too
            let live = |w: Option<Window>| w.filter(|w| w.resets_at * 1000 > now_ms);
            if let Some(w) = live(merged.five_hour) {
                data.five_hour_pct = Some(w.pct);
                data.five_hour_resets_at = Some(w.resets_at);
            }
            if let Some(w) = live(merged.seven_day) {
                data.seven_day_pct = Some(w.pct);
                data.seven_day_resets_at = Some(w.resets_at);
            }
        }
    }
    read_others(dir, &acct.alias, now_ms)
}

/// The quota part of the usage row: the mirror's chips when the mirror exists (G40), else the G36
/// snapshot chips exactly as before. The statusline never draws both.
fn quota_segment(
    data: &StatusInput,
    now_s: i64,
    avail: usize,
    yellow_taken: bool,
    contract: &Contract,
) -> String {
    match &data.mirror {
        None => others_segment(
            &data.others,
            &data.names,
            now_s,
            avail,
            yellow_taken,
            contract,
        ),
        Some(entries) => mirror_segment(entries, now_s, avail),
    }
}

/// G40 chips: ` · ` then the widest form of the quota group that fits `avail` (design 6.1 drop order);
/// nothing when no form fits or there is nothing to draw (never an empty ` · `).
fn mirror_segment(
    entries: &[crate::usage::quota_mirror_read::QEntry],
    now_s: i64,
    avail: usize,
) -> String {
    use crate::usage::quota_view as qv;
    let tr = qv::Tr::current();
    let band = qv::quota_band(entries, now_s, &tr);
    let sep_w = 3;
    for form in qv::band_forms(&band) {
        if sep_w + qv::width(&form) > avail {
            continue;
        }
        let mut out = tc(" · ", DELIM);
        for part in &form {
            out.push_str(&match part.style {
                qv::Style::Warn => tc(&part.text, BAR_MID),
                qv::Style::Idle => tc(&part.text, REMAIN),
                qv::Style::Dim => tc(&part.text, DELIM),
                qv::Style::Bold => tc_bold(&part.text, BAR_LBL),
                qv::Style::Plain => tc(&part.text, BAR_LBL),
            });
        }
        return out;
    }
    String::new()
}

/// The other accounts on the usage row, only those with a weekly warning: ` · ⏶ name 7d 97% 3d1h
/// 後重置` when the week will run short (or is at 95 %+), ` · name 7d 12% 20h 後重置 會浪費` when
/// it will be wasted; nothing else. The name comes from `usage::names` (the set name, else the
/// org short form): the `acct-` key is never drawn, and an account with no name (an identity
/// from before the login-derived alias) is skipped. Most constrained first, as many as fit in
/// `avail` cells (an account that does not fit with its countdown is tried without it; the first
/// that does not fit ends the list). Red at 95 %, else yellow for the first short account only
/// (one yellow per row), the rest bold. `yellow_taken`: the row already shows a yellow (this
/// session's own bars, see [`own_bars_yellow`]).
fn others_segment(
    others: &[crate::usage::quota::Snapshot],
    names: &crate::usage::names::Names,
    now_s: i64,
    avail: usize,
    yellow_taken: bool,
    contract: &Contract,
) -> String {
    use crate::usage::fleet_quota as fq;
    use crate::usage::names::{assign, Who};
    let who: Vec<Who> = others
        .iter()
        .map(|s| Who {
            key: &s.alias,
            org: s.org.as_deref(),
            email: s.email.as_deref(),
        })
        .collect();
    let tr = crate::usage::quota_view::Tr::current();
    let mut out = String::new();
    let mut used = 0usize;
    let mut yellow_used = yellow_taken;
    for (s, name) in others.iter().zip(assign(&who, names)) {
        // no name: an identity from before the login-derived alias, never drawn
        let Some(name) = name else {
            continue;
        };
        let Some(w) = s.seven_day.as_ref().filter(|w| w.resets_at > now_s) else {
            continue;
        };
        let wk = fq::weekly(
            fq::Win {
                pct: w.pct,
                resets_at: w.resets_at,
            },
            now_s,
            fq::DEFAULT_USE_TARGET,
        );
        let short = w.pct >= 0.95 || wk.warn == Some(fq::Warn::Short);
        let waste = !short && wk.warn == Some(fq::Warn::Waste);
        if !short && !waste {
            continue;
        }
        let pct = fmt_pct(w.pct, contract);
        let wk_lbl = tr.t("quota.sl_weekly", &[]);
        let head = if short {
            format!("{} {name} {wk_lbl} {pct}", contract.alert)
        } else {
            format!("{name} {wk_lbl} {pct}")
        };
        let mut tail = tr.t(
            "quota.sl_resets_in",
            &[("t", &fmt_remaining_at(w.resets_at, now_s))],
        );
        if waste {
            tail = format!("{tail} {}", tr.t("quota.fleet_waste", &[]));
        }
        let color = |t: &str| {
            if !short {
                tc(t, REMAIN)
            } else if w.pct >= 0.95 {
                tc(t, BAR_HIGH)
            } else if !yellow_used {
                tc(t, BAR_MID)
            } else {
                tc_bold(t, BAR_LBL)
            }
        };
        let sep = tc(" · ", DELIM);
        let head_w = 3 + vis(&head);
        let full_w = head_w + 1 + vis(&tail);
        let (piece, w_piece) = if used + full_w <= avail {
            (
                format!("{sep}{} {}", color(&head), tc(&tail, REMAIN)),
                full_w,
            )
        } else if used + head_w <= avail {
            (format!("{sep}{}", color(&head)), head_w)
        } else {
            break;
        };
        if short && w.pct < 0.95 {
            yellow_used = true;
        }
        out.push_str(&piece);
        used += w_piece;
    }
    out
}

/// Returns the rendered `StatusInput` plus the raw parsed JSON (or
/// `Value::Null` when stdin was empty/unparseable) — the live-context
/// writer builds its record from the raw value, per plan §2.5, rather than
/// from the display struct.
fn read_status_input() -> (StatusInput, serde_json::Value) {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() && !line.trim().is_empty() {
        if std::env::var("CODEFORGE_DEBUG").is_ok() {
            let _ = std::fs::write(std::env::temp_dir().join("codeforge-sl.json"), &line);
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let model = v["model"]["display_name"]
                .as_str()
                .or_else(|| v["model"]["id"].as_str())
                .or_else(|| v["model"].as_str())
                .map(|s| s.replace("claude-", "").replace("-20", " 20"));

            let context_pct = v["context_window"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let five_hour_pct = v["rate_limits"]["five_hour"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let five_hour_resets_at = v["rate_limits"]["five_hour"]["resets_at"].as_i64();
            let seven_day_pct = v["rate_limits"]["seven_day"]["used_percentage"]
                .as_f64()
                .map(|p| p / 100.0);
            let seven_day_resets_at = v["rate_limits"]["seven_day"]["resets_at"].as_i64();

            // Compute version info once — avoids triple subprocess spawn.
            // session_v: running CC version from /proc tree; latest_v: from `claude --version`
            let session_v = claude_session_version();
            let latest_v = claude_latest_version();
            let update_available = matches!((&session_v, &latest_v),
                (Some(s), Some(l)) if s != l);

            let input = StatusInput {
                model,
                cwd: v["cwd"]
                    .as_str()
                    .or_else(|| v["workspace"]["current_dir"].as_str())
                    .map(|s| s.to_string()),
                version: session_v.or_else(|| latest_v.clone()),
                latest_version: latest_v,
                update_available,
                context_pct,
                five_hour_pct,
                five_hour_resets_at,
                seven_day_pct,
                seven_day_resets_at,
                others: Vec::new(),
                names: Default::default(),
                mirror: None,
                band_facts: None,
            };
            return (input, v);
        }
    }
    (StatusInput::default(), serde_json::Value::Null)
}

/// Build the main live-context record from the raw stdin JSON and write it
/// to `<live-base>/context/<sid>.json`. Never touches rendering or exit
/// code — write failures are logged to stderr once per process (see
/// `crate::live::warn_once`-style behaviour inside `write_live_json`) and
/// otherwise swallowed here.
fn write_live_context(raw: &serde_json::Value) {
    if raw.is_null() {
        return;
    }
    let (base, _source) = crate::live::resolve_live_base();
    let context_dir = base.join("context");
    write_live_context_to(raw, &context_dir);
}

/// Build the main record from `raw` (built from the raw `serde_json::Value`,
/// not the display struct — plan §2.5) and write it to
/// `<context_dir>/<sid>.json`. Split out from `write_live_context` so tests
/// can point at an isolated directory instead of the process-cached
/// `resolve_live_base()` base.
fn write_live_context_to(raw: &serde_json::Value, context_dir: &std::path::Path) {
    let session_id = raw["session_id"].as_str().unwrap_or("");
    let sid = crate::live::sanitize_session_id(session_id);
    let record = build_main_record(raw, session_id);
    if let Err(e) = crate::live::write_live_json(context_dir, &format!("{sid}.json"), &record) {
        eprintln!("codeforge: failed to write live context file: {e}");
    }
}

/// Build the main live-context record schema (`schema_version: 1`) from the
/// raw stdin JSON. Fields are copied only when present in `raw` — never
/// invented.
fn build_main_record(raw: &serde_json::Value, session_id: &str) -> serde_json::Value {
    let mut record = serde_json::json!({
        "schema_version": 1,
        "session_id": session_id,
        "written_at": chrono::Utc::now().to_rfc3339(),
    });
    let obj = record.as_object_mut().expect("object literal");
    if let Some(v) = raw.get("version") {
        obj.insert("cc_version".to_string(), v.clone());
    }
    if let Some(model) = raw.get("model") {
        let mut m = serde_json::Map::new();
        if let Some(id) = model.get("id") {
            m.insert("id".to_string(), id.clone());
        }
        if let Some(dn) = model.get("display_name") {
            m.insert("display_name".to_string(), dn.clone());
        }
        obj.insert("model".to_string(), serde_json::Value::Object(m));
    }
    if let Some(cw) = raw.get("context_window") {
        let mut c = serde_json::Map::new();
        for key in [
            "context_window_size",
            "used_percentage",
            "total_input_tokens",
        ] {
            if let Some(v) = cw.get(key) {
                c.insert(key.to_string(), v.clone());
            }
        }
        if let Some(v) = cw.get("current_usage") {
            c.insert("current_usage".to_string(), v.clone());
        }
        obj.insert("context_window".to_string(), serde_json::Value::Object(c));
    }
    record
}

// ─── UX Pro color palette (ANSI256 → truecolor) ───────────────────────────────

type Rgb = (u8, u8, u8);

// Dark palette (on black): normal text >= 4.5:1, dim / secondary >= 3:1 (WCAG). The light
// palette is LIGHT below; every role and its bar is listed in THEME_ROLES.
const DELIM: Rgb = (0x6C, 0x6C, 0x6C); // ( ) separators, 4.0:1 (was 0x44, 2.2:1)
const DELIM_DIM: Rgb = (0x5C, 0x5C, 0x5C); // dimmed separator (ctx>80%), 3.1:1 (was 0x30, 1.6:1)
const AHEAD_BEHIND: Rgb = (0x87, 0xAF, 0xD7); // 110 — git ⇡⇣ chip (soft blue)
const MODEL_C: Rgb = (0xFF, 0xD7, 0x87); // 222 — model name
const CWD_C: Rgb = (0x5F, 0xAF, 0xAF); // 73  — cwd path
const BRANCH_C: Rgb = (0x87, 0xD7, 0x87); // 114 — git branch
const BAR_LOW: Rgb = (0x87, 0xD7, 0x87); // 114 — bar <warn (default 80%)
const BAR_MID: Rgb = (0xD7, 0xAF, 0x5F); // 179 — bar warn..error (default 80-100%)
const BAR_HIGH: Rgb = (0xD7, 0x5F, 0x5F); // 167 — bar >=error (default 100%)
const BAR_EMPTY: Rgb = (0x5C, 0x5C, 0x5C); // bar ▱, 3.1:1 (was 0x30, 1.6:1)
const BAR_LBL: Rgb = (0x8A, 0x8A, 0x8A); // 245 — "5h" "7d" "ctx"
const REMAIN: Rgb = (0x88, 0x88, 0x88); // "4h2m", 5.9:1 on black, 4.7:1 on the pane's #1e1e1e (was 0x62, 2.7:1 there)
const STAT_VAL: Rgb = (0x94, 0x94, 0x94); // 246 — stat numbers
const UPDATE_C: Rgb = (0xFF, 0xAF, 0x00); // 214 — amber update banner

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    Dark,
    Light,
}

/// Contrast a role needs against its background.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Bar {
    Text, // 4.5:1
    Dim,  // 3:1
}

/// Every colour the line draws: (role, dark, light, bar). Same-hue families: model amber, branch
/// and low bar green, cwd teal, ahead/behind blue, bar green/amber/red, update amber.
const THEME_ROLES: [(&str, Rgb, Rgb, Bar); 14] = [
    ("DELIM", DELIM, (0x8A, 0x8A, 0x8A), Bar::Dim),
    ("DELIM_DIM", DELIM_DIM, (0x94, 0x94, 0x94), Bar::Dim),
    ("AHEAD_BEHIND", AHEAD_BEHIND, (0x1F, 0x5F, 0xA8), Bar::Text),
    ("MODEL_C", MODEL_C, (0x8A, 0x5A, 0x00), Bar::Text),
    ("CWD_C", CWD_C, (0x00, 0x6B, 0x6B), Bar::Text),
    ("BRANCH_C", BRANCH_C, (0x1E, 0x7A, 0x2E), Bar::Text),
    ("BAR_LOW", BAR_LOW, (0x1E, 0x7A, 0x2E), Bar::Text),
    ("BAR_MID", BAR_MID, (0x9A, 0x5B, 0x00), Bar::Text),
    ("BAR_HIGH", BAR_HIGH, (0xB3, 0x1D, 0x1D), Bar::Text),
    ("BAR_EMPTY", BAR_EMPTY, (0x94, 0x94, 0x94), Bar::Dim),
    ("BAR_LBL", BAR_LBL, (0x76, 0x76, 0x76), Bar::Dim),
    ("REMAIN", REMAIN, (0x76, 0x76, 0x76), Bar::Text),
    ("STAT_VAL", STAT_VAL, (0x4A, 0x4A, 0x4A), Bar::Text),
    ("UPDATE_C", UPDATE_C, (0x9A, 0x5B, 0x00), Bar::Text),
];

/// Theme, first match wins (PM 01M88BD7K5): `CODEFORGE_THEME` = light|dark (the settings switch,
/// set in Claude Code settings.json `env`), then `COLORFGBG` (last field = background index:
/// 7 or 9-15 light, 0-6 or 8 dark), then a theme field in the statusline stdin (`theme`,
/// `theme_mode`, `output_style.theme`; Claude Code sends none today), then dark.
fn detect_theme(env: impl Fn(&str) -> Option<String>, stdin: &serde_json::Value) -> Theme {
    match env("CODEFORGE_THEME")
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("light") => return Theme::Light,
        Some("dark") => return Theme::Dark,
        _ => {}
    }
    if let Some(bg) = env("COLORFGBG").and_then(|v| {
        v.rsplit(';')
            .next()
            .and_then(|b| b.trim().parse::<u8>().ok())
    }) {
        match bg {
            7 | 9..=15 => return Theme::Light,
            0..=6 | 8 => return Theme::Dark,
            _ => {}
        }
    }
    for v in [
        &stdin["theme"],
        &stdin["theme_mode"],
        &stdin["output_style"]["theme"],
    ] {
        if let Some(t) = v.as_str().map(str::to_ascii_lowercase) {
            if t.contains("light") {
                return Theme::Light;
            }
            if t.contains("dark") {
                return Theme::Dark;
            }
        }
    }
    Theme::Dark
}

fn light_of(c: Rgb) -> Rgb {
    THEME_ROLES.iter().find(|r| r.1 == c).map_or(c, |r| r.2)
}

/// Recolour every `38;2;r;g;b` foreground of a dark-palette line for `theme`, in one pass.
fn apply_theme(line: &str, theme: Theme) -> String {
    if theme == Theme::Dark {
        return line.to_string();
    }
    const TAG: &str = "38;2;";
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find(TAG) {
        out.push_str(&rest[..i + TAG.len()]);
        rest = &rest[i + TAG.len()..];
        let end = rest
            .find(|ch: char| ch != ';' && !ch.is_ascii_digit())
            .unwrap_or(rest.len());
        let nums: Vec<&str> = rest[..end].splitn(4, ';').collect();
        let rgb: Option<Rgb> = match nums.as_slice() {
            [r, g, b, ..] => r
                .parse()
                .ok()
                .zip(g.parse().ok())
                .zip(b.parse().ok())
                .map(|((r, g), b)| (r, g, b)),
            _ => None,
        };
        if let Some(c) = rgb {
            let (r, g, b) = light_of(c);
            let used = nums[0].len() + nums[1].len() + nums[2].len() + 2;
            out.push_str(&format!("{r};{g};{b}"));
            rest = &rest[used..];
        }
    }
    out.push_str(rest);
    out
}
// Brain indicator palette (Task 13)

// ─── Color helpers ────────────────────────────────────────────────────────────
// The statusline's stdout is consumed by Claude Code's renderer (which handles
// ANSI escape codes), NOT a raw terminal — CC ALWAYS pipes it. So we colorize by
// default and gate ONLY on the standard NO_COLOR env var. We must NOT use
// tty/if_supports_color detection: a pipe is not a tty, so that strips all color
// in the real statusline (the 2026-06-24 regression this replaces). NO_COLOR
// (non-empty, per https://no-color.org) is the one switch that turns color off.
fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

fn tc(s: &str, (r, g, b): Rgb) -> String {
    if color_enabled() {
        format!("{}", s.truecolor(r, g, b))
    } else {
        s.to_string()
    }
}

fn tc_bold(s: &str, (r, g, b): Rgb) -> String {
    if color_enabled() {
        format!("{}", s.truecolor(r, g, b).bold())
    } else {
        s.to_string()
    }
}

fn tcs(s: String, (r, g, b): Rgb) -> String {
    if color_enabled() {
        format!("{}", s.truecolor(r, g, b))
    } else {
        s
    }
}

fn fmt_pct(pct: f64, contract: &Contract) -> String {
    let p = pct.max(0.0) * 100.0;
    let n = match contract.round {
        RoundMode::Floor => (p + 1e-9).floor() as i64,
        RoundMode::Round => p.round() as i64,
    };
    format!("{n}%")
}

fn bar_rgb(pct: f64, contract: &Contract) -> Rgb {
    let p = pct * 100.0;
    if p < contract.quota_warn_pct {
        BAR_LOW
    } else if p < contract.quota_error_pct {
        BAR_MID
    } else {
        BAR_HIGH
    }
}

/// Colored progress bar (filled=semantic, empty=dim 236)
fn colored_bar(pct: f64, width: usize, contract: &Contract) -> String {
    let filled = ((pct.clamp(0.0, 1.0) * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);
    format!(
        "{}{}",
        tcs(contract.bar_on.repeat(filled), bar_rgb(pct, contract)),
        tcs(contract.bar_off.repeat(empty), BAR_EMPTY)
    )
}

/// Visible column width (handles wide chars: CJK=2col, ASCII/▰▱=1col)
/// Use this for calculating layout, NOT str.len() or chars().count()
fn vis(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Shorten path keeping tail, using column width (~/projects/very/long → …/very/long)
fn shorten_path(path: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return "…".to_string();
    }
    if vis(path) <= max_cols {
        return path.to_string();
    }
    // Walk from end, collect chars until we've used max_cols-1 columns
    let mut cols = 0usize;
    let tail: String = path
        .chars()
        .rev()
        .take_while(|c| {
            let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
            if cols + w > max_cols - 1 {
                return false;
            }
            cols += w;
            true
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{}", tail)
}

/// Shorten string keeping head, using column width
fn shorten_str(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return "…".to_string();
    }
    if vis(s) <= max_cols {
        return s.to_string();
    }
    let mut cols = 0usize;
    let head: String = s
        .chars()
        .take_while(|c| {
            let w = UnicodeWidthStr::width(c.encode_utf8(&mut [0u8; 4]));
            if cols + w > max_cols - 1 {
                return false;
            }
            cols += w;
            true
        })
        .collect();
    format!("{}…", head)
}

fn to_home_rel(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(h.as_ref()) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

/// Strip ANSI/VT escape sequences from a string for visible-width measurement.
/// Handles CSI sequences (ESC [ ... <final>) where final byte is 0x40–0x7E,
/// covering SGR (colors/bold), cursor movement, erase, etc.
/// Note: OSC sequences (ESC ] ... BEL/ST) are not produced by owo_colors and
/// are not handled here — callers must only pass owo_colors output.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
                          // Consume until ANSI CSI final byte (0x40–0x7E = '@'–'~')
            for c2 in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&c2) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Visible column width of an ANSI string (excludes escape codes)
fn ansi_vis(s: &str) -> usize {
    vis(&strip_ansi(s))
}

/// True when this session's own usage bars already draw the yellow (`BAR_MID`),
/// so the other accounts must not add a second one.
fn own_bars_yellow(data: &StatusInput, contract: &Contract) -> bool {
    [data.five_hour_pct, data.seven_day_pct, data.context_pct]
        .into_iter()
        .flatten()
        .any(|p| bar_rgb(p, contract) == BAR_MID)
}

fn fmt_remaining(resets_at: i64) -> String {
    fmt_remaining_at(resets_at, chrono::Utc::now().timestamp())
}

fn fmt_remaining_at(resets_at: i64, now_s: i64) -> String {
    let secs = (resets_at - now_s).max(0) as u64;
    if secs >= 86400 {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    } else if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}m", secs / 60)
    }
}

/// Extract Claude version from a binary path like:
///   /home/user/.local/share/claude/versions/2.1.107
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only the Linux /proc walk calls it
fn extract_claude_ver(path: &str) -> Option<String> {
    let after = path.split("/versions/").nth(1)?;
    let ver = after.split('/').next()?;
    if ver.starts_with(|c: char| c.is_ascii_digit()) {
        Some(ver.to_string())
    } else {
        None
    }
}

/// Walk up the /proc process tree to find the Claude Code binary version.
/// Claude Code uses a symlink model: ~/.local/share/claude/versions/X.Y.Z
/// The running session's ancestor process has that path as its exe.
#[cfg(target_os = "linux")]
fn claude_session_version() -> Option<String> {
    fn ppid(pid: u32) -> Option<u32> {
        let s = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        s.lines()
            .find(|l| l.starts_with("PPid:\t"))
            .and_then(|l| l.split('\t').nth(1)?.trim().parse().ok())
    }
    let mut pid = std::process::id();
    for _ in 0..10 {
        pid = ppid(pid)?;
        if pid <= 1 {
            break;
        }
        if let Ok(exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
            if let Some(ver) = extract_claude_ver(&exe.to_string_lossy()) {
                return Some(ver);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn claude_session_version() -> Option<String> {
    None
}

/// Version of the Claude binary on disk — follows symlink to latest installed.
/// Output format: "2.1.107 (Claude Code)" → strip → "2.1.107"
fn claude_latest_version() -> Option<String> {
    std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            let t = s.trim();
            // strip_prefix/strip_suffix: one-shot match, unlike trim_*_matches which repeats
            let t = t.strip_prefix("claude ").unwrap_or(t);
            let t = t.strip_suffix(" (Claude Code)").unwrap_or(t);
            t.to_string()
        })
        .filter(|s| !s.is_empty() && s.starts_with(|c: char| c.is_ascii_digit()))
}

fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Local ahead / behind counts vs upstream. Returns (ahead, behind).
/// `None` means no upstream is set, no git repo, or the command failed —
/// caller renders nothing in that case. Powerlevel10k-style indicator.
fn git_ahead_behind() -> Option<(u32, u32)> {
    let out = std::process::Command::new("git")
        .args(["rev-list", "--count", "--left-right", "@{u}...HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.split_whitespace();
    let behind: u32 = parts.next()?.parse().ok()?;
    let ahead: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// Powerlevel10k-style `⇡N⇣M` chip. Empty string when in sync (or unknown).
fn fmt_ahead_behind(ab: Option<(u32, u32)>) -> String {
    match ab {
        Some((0, 0)) | None => String::new(),
        Some((a, 0)) => format!("⇡{}", a),
        Some((0, b)) => format!("⇣{}", b),
        Some((a, b)) => format!("⇡{}⇣{}", a, b),
    }
}

fn current_dir_short() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let home = dirs::home_dir()?;
    let path = if let Ok(rel) = cwd.strip_prefix(&home) {
        format!("~/{}", rel.display())
    } else {
        cwd.display().to_string()
    };
    Some(shorten_path(&path, 25))
}

// ─── Usage segment builder ────────────────────────────────────────────────────

/// Usage segment WITHOUT ▏▕ wrappers — for box layout row 1
fn usage_bare(label: &str, pct: f64, remain: Option<String>, contract: &Contract) -> String {
    let bar = colored_bar(pct, 6, contract);
    let pct_num = fmt_pct(pct, contract);
    let (r, g, b) = bar_rgb(pct, contract);
    let remain_part = match &remain {
        Some(s) => format!(" {}", tc(s, REMAIN)),
        None => String::new(),
    };
    format!(
        "{} {} {}{}",
        tc(label, BAR_LBL),
        bar,
        tc(&pct_num, (r, g, b)),
        remain_part,
    )
}

// ─── Position-based renderer ──────────────────────────────────────────────────

fn band_facts_pieces(facts: &BandFacts) -> Vec<(String, usize)> {
    let mut pieces = Vec::new();
    if facts.live_dispatches > 0 {
        let s = format!("◉ {}", facts.live_dispatches);
        pieces.push((tc(&s, AHEAD_BEHIND), vis(&s)));
    }
    if let Some(pos) = &facts.position {
        pieces.push((tc(pos, STAT_VAL), vis(pos)));
    }
    if let Some(unit) = &facts.unit {
        pieces.push((tc(unit, BAR_LBL), vis(unit)));
    }
    if let Some(round) = facts.review_round {
        let s = format!("↺ R{round}");
        pieces.push((tc(&s, BAR_LBL), vis(&s)));
    }
    if facts.decisions_open > 0 {
        let s = format!("⬥ {}", facts.decisions_open);
        pieces.push((tc_bold(&s, BAR_LBL), vis(&s)));
    }
    if let Some(usd) = facts.usd {
        let dollars = format!("${}", (usd + 1e-9).floor() as u64);
        if let Some(cap) = facts.cap_usd.filter(|c| *c > 0.0) {
            let pct = usd / cap;
            let rgb = bar_rgb(pct, &facts.contract);
            let bar = colored_bar(pct, 5, &facts.contract);
            let styled = format!("{} {}", tc(&dollars, rgb), bar);
            let w = ansi_vis(&styled);
            pieces.push((styled, w));
        } else {
            pieces.push((tc(&dollars, BAR_LBL), vis(&dollars)));
        }
    }
    pieces
}

fn fit_band_facts(pieces: &[(String, usize)], avail: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used = 0usize;
    for (styled, w) in pieces {
        let need = 2 + *w;
        if used + need > avail {
            break;
        }
        out.push_str("  ");
        out.push_str(styled);
        used += need;
    }
    (out, used)
}

/// The statusline: exactly one line, never the pet (operator via PM 01M7MEY1).
/// Identity left (model ── cwd ⇡⇣ branch [band-facts]), usage then version right. When it
/// does not fit, band-facts shrink first, then the version goes, then the line is cut at
/// `width` columns; it never wraps into a second row.
fn render_line<W: Write>(out: &mut W, data: &StatusInput, width: usize) -> Result<()> {
    let default_contract = Contract::default();
    let contract = data
        .band_facts
        .as_ref()
        .map_or(&default_contract, |f| &f.contract);
    let facts_pieces = data
        .band_facts
        .as_ref()
        .map(band_facts_pieces)
        .unwrap_or_default();

    // One line: identity left (model ── cwd ⇡⇣ branch [band-facts]), usage bars, quota chips and
    // the version right. Width budget, in the order things give way: version first, then the
    // quota chips (they take the narrower form their drop order defines), then cwd and branch
    // shrink so the usage bars keep their room; the line is cut only as the last resort.
    let dimmed = data.context_pct.is_some_and(|p| p > 0.8);
    let delim_c = if dimmed { DELIM_DIM } else { DELIM };

    let (model_text, model_vis) = render_model(&data.model, MODEL_C);

    let cwd_raw = data
        .cwd
        .clone()
        .or_else(current_dir_short)
        .unwrap_or_else(|| "~".to_string());
    let cwd_home = to_home_rel(&cwd_raw);

    let branch_full = git_branch().unwrap_or_else(|| "—".to_string());
    let ab_str = fmt_ahead_behind(git_ahead_behind());
    // ab_part carries ONLY its leading space.
    // The space BEFORE branch is always written explicitly so cwd and branch
    // can't glue together when ab_part is empty (no-upstream case).
    let (ab_part, ab_vis) = if ab_str.is_empty() {
        (String::new(), 0)
    } else {
        (format!(" {}", tc(&ab_str, AHEAD_BEHIND)), 1 + vis(&ab_str))
    };

    let (ver_str, ver_vis) = render_version(data, delim_c);

    // Usage bars (5h / 7d / ctx), built first: they reserve their room before cwd and branch.
    let mut usage_parts: Vec<String> = Vec::new();
    if let Some(pct) = data.five_hour_pct {
        usage_parts.push(usage_bare(
            "5h",
            pct,
            data.five_hour_resets_at.map(fmt_remaining),
            contract,
        ));
    }
    if let Some(pct) = data.seven_day_pct {
        usage_parts.push(usage_bare(
            "7d",
            pct,
            data.seven_day_resets_at.map(fmt_remaining),
            contract,
        ));
    }
    if let Some(pct) = data.context_pct {
        usage_parts.push(usage_bare("ctx", pct, None, contract));
    }
    let usage_bars = if usage_parts.is_empty() {
        tc("─", delim_c)
    } else {
        usage_parts.join("   ")
    };
    let usage_bars_vis = ansi_vis(&usage_bars);

    // Left block: " " model " ── " cwd <ab_part> " " branch. The version is not reserved here
    // (it is dropped first); the bars and a 2-col gap are. cwd >= 10 cols, branch >= 4 cols.
    let fixed = 1 + model_vis + 4 + ab_vis + 1;
    let share = width.saturating_sub(fixed + usage_bars_vis + 2);
    let cwd_budget = share.saturating_sub(4).max(10);
    let cwd_display = shorten_path(&cwd_home, cwd_budget);
    let cwd_vis = vis(&cwd_display);
    let branch_budget = share.saturating_sub(cwd_vis).max(4);
    let branch_display = shorten_str(&branch_full, branch_budget);
    let branch_vis = vis(&branch_display);
    let left = format!(
        " {}{}{}{} {}",
        model_text,
        tc(" ── ", delim_c),
        tc(&cwd_display, CWD_C),
        ab_part,
        tc(&branch_display, BRANCH_C),
    );
    let left_vis = 1 + model_vis + 4 + cwd_vis + ab_vis + 1 + branch_vis;

    // Quota chips get what the identity block and the bars leave (the segment counts its own
    // ` · `), so quota_segment picks a form that fits instead of being cut mid-chip.
    let usage_left = {
        let room = width.saturating_sub(left_vis + 2 + usage_bars_vis);
        let now_s = chrono::Utc::now().timestamp();
        format!(
            "{usage_bars}{}",
            quota_segment(data, now_s, room, own_bars_yellow(data, contract), contract)
        )
    };
    let usage_left_vis = ansi_vis(&usage_left);
    // Version rightmost; dropped before anything is cut.
    let with_ver = left_vis + 2 + usage_left_vis + 3 + ver_vis <= width && ver_vis > 0;
    let (right, right_vis) = if with_ver {
        (
            format!("{usage_left}   {ver_str}"),
            usage_left_vis + 3 + ver_vis,
        )
    } else {
        (usage_left, usage_left_vis)
    };
    let facts_avail = width.saturating_sub(left_vis + 2 + right_vis);
    let (facts, facts_vis) = fit_band_facts(&facts_pieces, facts_avail);
    let used = left_vis + facts_vis + right_vis;
    let pad = width.saturating_sub(used).max(2);
    let line = format!("{left}{facts}{}{right}", " ".repeat(pad));
    writeln!(out, "{}", cut_to_width(&line, width))?;
    Ok(())
}

/// Cut an ANSI string to at most `width` visible columns, keeping escape codes whole and
/// resetting the colour when something was cut. Never splits a wide character.
fn cut_to_width(s: &str, width: usize) -> String {
    if ansi_vis(s) <= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut cols = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            out.push(c);
            out.push(chars.next().unwrap_or('['));
            for c2 in chars.by_ref() {
                out.push(c2);
                if ('\x40'..='\x7e').contains(&c2) {
                    break;
                }
            }
            continue;
        }
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if cols + w > width {
            break;
        }
        cols += w;
        out.push(c);
    }
    if color_enabled() {
        out.push_str("\x1b[0m");
    }
    out
}

/// Format model name + optional context-window size. Shared between both modes.
/// Returns (rendered ansi string, visible width).
fn render_model(model: &Option<String>, color: Rgb) -> (String, usize) {
    match model {
        Some(m) => {
            let m_s = shorten_str(m, 20);
            (tc_bold(&m_s, color), vis(&m_s))
        }
        None => (tc("—", STAT_VAL), 1),
    }
}

/// Format CC version chip. Returns (rendered ansi, visible width). Empty when
/// neither version nor latest_version resolved (e.g. running outside Claude).
fn render_version(data: &StatusInput, dim_c: Rgb) -> (String, usize) {
    if data.version.is_none() && data.latest_version.is_none() {
        return (String::new(), 0);
    }
    if data.update_available {
        let latest = data
            .latest_version
            .as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let s = format!("{} {}", tc_bold("⬆", UPDATE_C), tc(&latest, UPDATE_C));
        (s, 2 + vis(&latest))
    } else {
        let cur = data
            .version
            .as_deref()
            .map(|v| format!("v{}", v))
            .unwrap_or_else(|| "—".to_string());
        let s = tc(&cur, dim_c);
        (s, vis(&cur))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serialize env mutations across tests to avoid race conditions.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── shared fixtures ──────────────────────────────────────────────────

    const HB_SID: &str = "93196c52-25cb-47ca-821c-cec391832eed";
    const HB_NOW: i64 = 1_793_400_000_000;

    #[test]
    fn width_is_codeforge_width_else_columns_minus_4_else_100() {
        assert_eq!(statusline_width(Some("90"), Some("160")), 90);
        assert_eq!(statusline_width(None, Some("160")), 156);
        assert_eq!(statusline_width(None, Some("120")), 116);
        assert_eq!(statusline_width(Some("x"), Some("130")), 126);
        assert_eq!(statusline_width(None, Some("4")), 0);
        assert_eq!(statusline_width(None, Some("30")), 26);
        assert_eq!(statusline_width(None, Some("wide")), 100);
        assert_eq!(statusline_width(None, None), 100);
    }

    fn compact_lines(data: &StatusInput, width: usize) -> Vec<String> {
        let mut buf = Vec::new();
        render_line(&mut buf, data, width).unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn compact_form_is_one_line_exactly_when_everything_fits() {
        let data = StatusInput {
            model: Some("Opus 5.5".into()),
            cwd: Some("/tmp/範例專案".into()),
            version: Some("2.1.318".into()),
            context_pct: Some(0.31),
            five_hour_pct: Some(0.08),
            five_hour_resets_at: Some(chrono::Utc::now().timestamp() + 1800),
            seven_day_pct: Some(0.46),
            seven_day_resets_at: Some(chrono::Utc::now().timestamp() + 5 * 86400),
            ..Default::default()
        };
        // wide: one line, exactly `width` columns, identity left and usage right
        let wide = compact_lines(&data, 300);
        assert_eq!(wide.len(), 1, "{wide:?}");
        let line = &wide[0];
        assert_eq!(vis(line), 300, "{line:?}");
        for part in ["Opus 5.5", "範例專案", "5h", "7d", "ctx"] {
            assert!(line.contains(part), "{part} missing: {line:?}");
        }
        assert!(line.find("範例專案") < line.find("5h"));
        // the version is the last field, flush right; the two-line form's first row too
        assert!(line.ends_with("v2.1.318"), "{line:?}");
        assert!(line.find("ctx") < line.find("v2.1.318"));
        // the threshold is the content: identity + 2 spaces + version and usage
        let pad = {
            let mut best = 0;
            let mut run = 0;
            for c in line.chars() {
                run = if c == ' ' { run + 1 } else { 0 };
                best = best.max(run);
            }
            best
        };
        let need = 300 - pad + 2;
        let at = compact_lines(&data, need);
        assert_eq!(at.len(), 1, "at the threshold {need}: {at:?}");
        assert!(vis(&at[0]) <= need);
        // one column short: still one line; the version goes first, the usage stays
        let below = compact_lines(&data, need - 1);
        assert_eq!(below.len(), 1, "one column short of {need}: {below:?}");
        assert!(vis(&below[0]) < need, "{below:?}");
        assert!(!below[0].contains("v2.1.318"), "{below:?}");
        assert!(below[0].contains("ctx"), "{below:?}");
        // narrow and tiny terminals: still one line, never wider than the width
        for w in [80, 40, 10, 0] {
            let l = compact_lines(&data, w);
            assert_eq!(l.len(), 1, "width {w}: {l:?}");
            assert!(vis(&l[0]) <= w, "width {w}: {l:?}");
        }
    }

    /// (g) statusline fixture from p0/statusline.json ⇒ main file matches
    /// schema, context_window_size == 1000000.
    #[test]
    fn live_context_from_p0_fixture_matches_schema() {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/statusline-p0.json"
        );
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture_path).unwrap()).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let context_dir = dir.path().join("context");
        write_live_context_to(&raw, &context_dir);

        let sid = crate::live::sanitize_session_id(raw["session_id"].as_str().unwrap());
        let written: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(context_dir.join(format!("{sid}.json"))).unwrap(),
        )
        .unwrap();

        assert_eq!(written["schema_version"], 1);
        assert_eq!(
            written["session_id"],
            "93196c52-25cb-47ca-821c-cec391832eed"
        );
        assert!(written["written_at"].as_str().unwrap().contains('T'));
        assert_eq!(written["cc_version"], "2.1.260");
        assert_eq!(written["model"]["id"], "claude-fable-5-1");
        assert_eq!(written["model"]["display_name"], "Fable 5.1");
        assert_eq!(written["context_window"]["context_window_size"], 1_000_000);
        assert_eq!(written["context_window"]["used_percentage"], 26);
        assert_eq!(written["context_window"]["total_input_tokens"], 262_707);
        assert_eq!(
            written["context_window"]["current_usage"]["input_tokens"],
            32
        );
    }

    /// NO_COLOR env var causes tc/tc_bold/tcs to emit plain text (no ANSI).
    #[test]
    fn no_color_strips_ansi() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("NO_COLOR", "1");
        let s = tc("hi", (0xFF, 0x00, 0x00));
        assert!(
            !s.contains('\x1b'),
            "NO_COLOR 下不該有 ANSI escape，got: {:?}",
            s
        );
        let sb = tc_bold("bold", (0x00, 0xFF, 0x00));
        assert!(
            !sb.contains('\x1b'),
            "tc_bold NO_COLOR 下不該有 ANSI escape，got: {:?}",
            sb
        );
        std::env::remove_var("NO_COLOR");
    }

    /// Regression (2026-06-24): CC pipes the statusline stdout (not a tty), so
    /// color must be ON by default — NOT gated on tty detection. Without NO_COLOR,
    /// tc() must emit ANSI even when stdout is not a terminal (as in `cargo test`).
    #[test]
    fn color_on_by_default_when_not_a_tty() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("NO_COLOR");
        let s = tc("x", (0xFF, 0x00, 0x00));
        assert!(
            s.contains('\x1b'),
            "color 必須預設開（CC 吃 ANSI；不可靠 tty 偵測），got: {:?}",
            s
        );
    }

    #[test]
    fn ansi_vis_strips_escapes() {
        // Even with colors on, ansi_vis should return visible width only.
        let colored = tc("hello", (0xFF, 0xFF, 0xFF));
        // visible width = 5 regardless of ANSI wrapping
        assert_eq!(ansi_vis(&colored), 5);
    }

    // ── Task 13: bottom_border tests ─────────────────────────────────────────

    // ── G36 per-account quota ───────────────────────────────────────────────

    /// A snapshot whose org is the alias without `acct-` (so its default name is that).
    fn quota_snap(alias: &str, pct: f64, resets_at: i64) -> crate::usage::quota::Snapshot {
        crate::usage::quota::Snapshot {
            alias: alias.into(),
            org: Some(alias.trim_start_matches("acct-").to_string()),
            org_id: None,
            email: None,
            root: None,
            auth_state: None,
            written_at_ms: 0,
            five_hour: None,
            seven_day: Some(crate::usage::quota::Window {
                pct,
                resets_at,
                seen: None,
            }),
        }
    }

    const Q_NOW: i64 = 1_793_600_000;

    #[test]
    fn others_segment_shows_only_warned_weeks_by_name_and_fits_its_room() {
        let _g = ENV_LOCK.lock().unwrap();
        let mut old = quota_snap("acct-ae88ee5d", 0.98, Q_NOW + 3 * 86400 + 3600);
        old.org = None; // an identity from before the login-derived alias
        let others = [
            // 97 % with 3d1h left: short
            quota_snap("acct-Stranity", 0.97, Q_NOW + 3 * 86400 + 3600),
            old,
            // 10 % with 20h left: waste
            quota_snap("acct-work", 0.10, Q_NOW + 20 * 3600),
            // 30 % mid-week: no warning, not shown
            quota_snap("acct-calm", 0.30, Q_NOW + 4 * 86400),
        ];
        let tr = crate::usage::quota_view::Tr::current();
        let wk = tr.t("quota.sl_weekly", &[]);
        let r1 = tr.t("quota.sl_resets_in", &[("t", "3d1h")]);
        let r2 = tr.t("quota.sl_resets_in", &[("t", "20h0m")]);
        let waste = tr.t("quota.fleet_waste", &[]);
        let c = Contract::default();
        let full = strip_ansi(&others_segment(
            &others,
            &Default::default(),
            Q_NOW,
            200,
            false,
            &c,
        ));
        assert_eq!(
            full,
            format!(" · ⏶ stranity {wk} 97% {r1} · work {wk} 10% {r2} {waste}")
        );
        assert!(!full.contains("acct-") && !full.contains("calm"), "{full}");
        // every room: never wider than given
        for room in 0..80 {
            let got = strip_ansi(&others_segment(
                &others,
                &Default::default(),
                Q_NOW,
                room,
                false,
                &c,
            ));
            assert!(vis(&got) <= room, "room {room}: {got:?}");
        }
        // no countdown when only the head fits; nothing when not even that
        let head = format!(" · ⏶ stranity {wk} 97%");
        assert_eq!(
            strip_ansi(&others_segment(
                &others,
                &Default::default(),
                Q_NOW,
                vis(&head),
                false,
                &c,
            )),
            head
        );
        assert_eq!(
            others_segment(
                &others,
                &Default::default(),
                Q_NOW,
                vis(&head) - 1,
                false,
                &c,
            ),
            ""
        );
        // a name the operator set wins over the org
        let names = crate::usage::names::Names(std::collections::BTreeMap::from([(
            "acct-Stranity".to_string(),
            "公司".to_string(),
        )]));
        let got = strip_ansi(&others_segment(&others, &names, Q_NOW, 200, false, &c));
        assert!(got.starts_with(&format!(" · ⏶ 公司 {wk} 97%")), "{got}");
    }

    fn mirror_entries(now: i64) -> Vec<crate::usage::quota_mirror_read::QEntry> {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("quota/claude");
        std::fs::create_dir_all(&d).unwrap();
        let rec = serde_json::json!({"schema": "fleet.quota/1", "provider": "claude", "alias": "work",
            "visibility": "L3", "host_role": "agent", "refusal": null, "token_state": "unknown",
            "token_state_at": null, "token_issued_at": null, "token_expires_at": null,
            "windows": [{"kind": "rolling_5h", "utilization": 0.62, "used": null, "limit": null, "unit": null,
                "resets_at": now + 7500, "source": "passive", "observed_at": now - 60}]});
        let row = serde_json::json!({"writer_courier": "local", "host_role": "agent",
            "observed_at_hub": null, "skewed": false, "record": rec});
        let view = serde_json::json!({"schema": "fleet.quota-view/1", "namespace": null,
            "hub_offset": 0, "rows": [row]});
        std::fs::write(d.join("work.json"), view.to_string()).unwrap();
        crate::usage::quota_mirror_read::load_from(t.path(), now).expect("quota dir exists")
    }

    /// G40 transition (design 7 rule 7): with no mirror the quota segment IS the G36 segment, byte for byte.
    #[test]
    fn quota_segment_without_mirror_is_byte_identical_to_the_g36_segment() {
        // both sides read NO_COLOR, which other tests toggle under ENV_LOCK
        let _g = ENV_LOCK.lock().unwrap();
        let others = vec![
            quota_snap("acct-b", 0.81, Q_NOW + 3 * 86400 + 7200),
            quota_snap("acct-c", 0.12, Q_NOW + 4 * 3600),
        ];
        let data = StatusInput {
            others: others.clone(),
            mirror: None,
            ..Default::default()
        };
        let c = Contract::default();
        for avail in [0usize, 12, 14, 31, 40, 80, 200] {
            for yellow in [false, true] {
                assert_eq!(
                    quota_segment(&data, Q_NOW, avail, yellow, &c),
                    others_segment(&others, &Default::default(), Q_NOW, avail, yellow, &c),
                    "avail {avail}"
                );
            }
        }
        // and nothing under a missing quota dir ever turns into `Some`
        let t = tempfile::tempdir().unwrap();
        assert!(crate::usage::quota_mirror_read::load_from(t.path(), Q_NOW).is_none());
    }

    #[test]
    fn quota_segment_with_mirror_draws_mirror_chips_instead_of_snapshots() {
        let data = StatusInput {
            others: vec![quota_snap("acct-b", 0.81, Q_NOW + 7200)],
            mirror: Some(mirror_entries(Q_NOW)),
            ..Default::default()
        };
        let c = Contract::default();
        let seg = strip_ansi(&quota_segment(&data, Q_NOW, 200, false, &c));
        assert!(seg.starts_with(" · "), "{seg:?}");
        assert!(
            seg.contains("claude:work") && seg.contains("62%") && seg.contains("◷ 2h5m"),
            "{seg:?}"
        );
        assert!(!seg.contains("acct-b"), "{seg:?}");
        // fits its room at every width, never panics, never a bare separator
        for avail in 0..120 {
            let s = strip_ansi(&quota_segment(&data, Q_NOW, avail, false, &c));
            assert!(s.is_empty() || vis(&s) <= avail, "avail {avail}: {s:?}");
            assert_ne!(s.trim(), "·", "avail {avail}");
        }
        // narrowest form: alias and value only
        let tight = strip_ansi(&quota_segment(&data, Q_NOW, 3 + vis("work 62%"), false, &c));
        assert_eq!(tight, " · work 62%");
    }

    #[test]
    fn quota_segment_with_an_empty_mirror_draws_nothing() {
        let data = StatusInput {
            others: vec![quota_snap("acct-b", 0.81, Q_NOW + 7200)],
            mirror: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(
            quota_segment(&data, Q_NOW, 200, false, &Contract::default()),
            ""
        );
    }

    #[test]
    fn others_segment_one_yellow_per_row_red_at_95() {
        // other tests set NO_COLOR under ENV_LOCK; colour assertions must not race them
        let _g = ENV_LOCK.lock().unwrap();
        if !color_enabled() {
            return; // NO_COLOR set for this test run: nothing coloured to check
        }
        let esc = |(r, g, b): Rgb| format!("\x1b[38;2;{r};{g};{b}m");
        // 2 days into the week: 88 % and 84 % both project past 100 % (short)
        let left = 5 * 86400;
        let others = [
            quota_snap("acct-r", 0.97, Q_NOW + left),
            quota_snap("acct-y", 0.88, Q_NOW + left),
            quota_snap("acct-z", 0.84, Q_NOW + left),
        ];
        let seg = others_segment(
            &others,
            &Default::default(),
            Q_NOW,
            200,
            false,
            &Contract::default(),
        );
        assert_eq!(seg.matches(&esc(BAR_MID)).count(), 1, "{seg:?}");
        assert_eq!(seg.matches(&esc(BAR_HIGH)).count(), 1, "{seg:?}");
        assert!(
            seg.contains("\x1b[1m"),
            "the second short account is bold: {seg:?}"
        );
    }

    fn ident(alias: &str, email: Option<&str>) -> crate::usage::alias::Identity {
        crate::usage::alias::Identity {
            alias: alias.into(),
            org: None,
            org_id: None,
            email: email.map(str::to_string),
        }
    }

    #[test]
    fn an_overrun_past_the_cap_is_filed_as_the_cap() {
        // fleet 01M7PDG6: used_percentage 180 must not write a pct the reader refuses
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let mut data = StatusInput {
            five_hour_pct: Some(1.8),
            five_hour_resets_at: Some(Q_NOW + 3600),
            ..Default::default()
        };
        record_quota(
            &mut data,
            &dir,
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-1",
            Q_NOW * 1000,
        );
        let got = crate::usage::quota::read_own(&dir, "acct-a").unwrap();
        assert_eq!(
            got.five_hour.map(|w| w.pct),
            Some(crate::usage::quota::PCT_CAP)
        );
    }

    #[test]
    fn own_row_never_shows_a_window_past_its_reset() {
        // the file holds a 5h window that reset 30h ago; CC now sends only 7d
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let mut old = quota_snap("acct-a", 0.7, Q_NOW + 86400);
        old.five_hour = Some(crate::usage::quota::Window {
            pct: 0.99,
            resets_at: Q_NOW - 30 * 3600,
            seen: Some(crate::usage::quota::Seen {
                at_ms: now_ms - 31 * 3600 * 1000,
                session: "sess-0".into(),
            }),
        });
        crate::live::write_live_json(&dir, "acct-a.json", &old.to_json()).unwrap();
        let mut data = StatusInput {
            seven_day_pct: Some(0.77),
            seven_day_resets_at: Some(Q_NOW + 86400),
            ..Default::default()
        };
        record_quota(
            &mut data,
            &dir,
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-1",
            now_ms,
        );
        assert_eq!(data.five_hour_pct, None);
        assert_eq!(data.five_hour_resets_at, None);
        assert_eq!(data.seven_day_pct, Some(0.77));
    }

    #[test]
    fn record_quota_writes_this_account_and_reads_the_others() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let other = quota_snap("acct-b", 0.4, Q_NOW + 600);
        let other = crate::usage::quota::Snapshot {
            written_at_ms: now_ms - 1000,
            ..other
        };
        crate::live::write_live_json(&dir, "acct-b.json", &other.to_json()).unwrap();
        // no rate_limits: nothing written, the others still read
        let mut none = StatusInput::default();
        let got = record_quota(
            &mut none,
            &dir,
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-1",
            now_ms,
        );
        assert_eq!(
            got.iter().map(|s| s.alias.as_str()).collect::<Vec<_>>(),
            ["acct-b"]
        );
        assert!(!dir.join("acct-a.json").exists());
        // rate_limits: this account's snapshot written, not shown among the others
        let mut data = StatusInput {
            seven_day_pct: Some(0.75),
            seven_day_resets_at: Some(Q_NOW + 86400),
            ..Default::default()
        };
        let got = record_quota(
            &mut data,
            &dir,
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-1",
            now_ms,
        );
        assert_eq!(got.len(), 1);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("acct-a.json")).unwrap())
                .unwrap();
        assert_eq!(v["seven_day"]["pct"], 0.75);
        assert_eq!(v["seven_day"]["seen_by"], "sess-1");
        assert_eq!(v["seven_day"]["seen_at_ms"], now_ms);
        assert_eq!(v["five_hour"], serde_json::Value::Null);
        // an idle session of the same account, still holding a lower value of the same
        // window: the file keeps 0.75 and this session's own bar shows 0.75 too
        let mut idle = StatusInput {
            seven_day_pct: Some(0.48),
            seven_day_resets_at: Some(Q_NOW + 86400),
            ..Default::default()
        };
        record_quota(
            &mut idle,
            &dir,
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-2",
            now_ms + 1000,
        );
        assert_eq!(idle.seven_day_pct, Some(0.75));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("acct-a.json")).unwrap())
                .unwrap();
        assert_eq!(v["seven_day"]["pct"], 0.75);
        assert_eq!(v["seven_day"]["seen_by"], "sess-1");
        // the login email travels in the snapshot (0600 file), and a newer one replaces it
        let mut mail = StatusInput {
            seven_day_pct: Some(0.8),
            seven_day_resets_at: Some(Q_NOW + 86400),
            ..Default::default()
        };
        record_quota(
            &mut mail,
            &dir,
            Some(&ident("acct-a", Some("me@example.com"))),
            "1a2b3c4d",
            "sess-1",
            now_ms + 120_000,
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("acct-a.json")).unwrap())
                .unwrap();
        assert_eq!(v["email"], "me@example.com");
        // no login (no account): rate_limits are not written under any key, not even a hash
        // of the config dir, and the others are still read
        let mut anon = StatusInput {
            seven_day_pct: Some(0.9),
            seven_day_resets_at: Some(Q_NOW + 86400),
            ..Default::default()
        };
        let before: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let got = record_quota(
            &mut anon,
            &dir,
            None,
            "ae88ee5d",
            "sess-3",
            now_ms + 130_000,
        );
        let after: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(before.len(), after.len());
        assert!(!dir.join("acct-ae88ee5d.json").exists());
        assert_eq!(anon.seven_day_pct, Some(0.9));
        assert!(got.iter().any(|s| s.alias == "acct-b"));
        // a missing dir is no error
        assert!(record_quota(
            &mut none,
            &base.path().join("nope"),
            Some(&ident("acct-a", None)),
            "1a2b3c4d",
            "sess-1",
            now_ms
        )
        .is_empty());
    }

    /// Setup-token lane (PM 01M76F3F): the override alias is filed with `root: null`,
    /// the login timeline is not touched, and the session is pinned. No written file contains `@`.
    #[test]
    fn an_override_files_under_that_alias_with_no_root_and_pins_the_session() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let root = "1a2b3c4d";
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        let usage = dir.parent().unwrap();
        crate::usage::logins::observe(usage, root, "acct-aaaaaaaa", now_ms - 10_000).unwrap();
        let logins_path = usage.join(format!("logins-{root}.json"));
        let before = std::fs::read(&logins_path).unwrap();
        let mut data = StatusInput {
            five_hour_pct: Some(0.4),
            five_hour_resets_at: Some(Q_NOW + 3600),
            seven_day_pct: Some(0.2),
            seven_day_resets_at: Some(Q_NOW + 86_400),
            ..Default::default()
        };
        // an email on the identity must not survive: the override carries neither org nor email
        record_quota_with(
            &mut data,
            &dir,
            Some(&ident("acct-bbbbbbbb", Some("me@example.com"))),
            root,
            "sess-token",
            now_ms,
            true,
        );
        let snap: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("acct-bbbbbbbb.json")).unwrap())
                .unwrap();
        assert_eq!(snap["alias"], "acct-bbbbbbbb");
        assert_eq!(snap["root"], serde_json::Value::Null);
        assert_eq!(snap["org"], serde_json::Value::Null);
        assert_eq!(snap["email"], serde_json::Value::Null);
        assert_eq!(
            std::fs::read(&logins_path).unwrap(),
            before,
            "login timeline changed"
        );
        let pins: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(usage.join(format!("pins-{root}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(pins["sess-token"]["alias"], "acct-bbbbbbbb");
        assert_eq!(pins["sess-token"]["at_ms"], now_ms);
        let sessions =
            std::fs::read_to_string(usage.join(format!("sessions-{root}.json"))).unwrap();
        assert!(sessions.contains("sess-token"), "{sessions}");
        assert!(sessions.contains("acct-bbbbbbbb"), "{sessions}");
        assert_tree_has_no_at(usage);
    }

    /// A token lane without an override alias (hangar 01M77KXH): nothing is filed under
    /// any account, the login timeline is untouched, and the session is pinned unattributed.
    #[test]
    fn a_token_lane_without_an_override_pins_the_session_unattributed() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let root = "1a2b3c4d";
        std::fs::create_dir_all(&dir).unwrap();
        let usage = dir.parent().unwrap();
        crate::usage::logins::observe(usage, root, "acct-aaaaaaaa", now_ms - 10_000).unwrap();
        let logins_path = usage.join(format!("logins-{root}.json"));
        let before = std::fs::read(&logins_path).unwrap();
        let mut data = StatusInput {
            five_hour_pct: Some(0.4),
            five_hour_resets_at: Some(Q_NOW + 3600),
            ..Default::default()
        };
        record_quota_with(&mut data, &dir, None, root, "sess-tok", now_ms, true);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a snapshot was filed"
        );
        assert_eq!(std::fs::read(&logins_path).unwrap(), before);
        let pins: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(usage.join(format!("pins-{root}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            pins["sess-tok"]["alias"],
            crate::usage::logins::PIN_UNATTRIBUTED
        );
    }

    fn assert_tree_has_no_at(dir: &std::path::Path) {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for ent in std::fs::read_dir(&d).unwrap() {
                let p = ent.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let body = std::fs::read(&p).unwrap();
                    assert!(!body.contains(&b'@'), "{} contains @", p.display());
                }
            }
        }
    }

    /// cuda, re-login at 17:12: session S was on A and still holds A's window (96%, reset
    /// tomorrow). Filing that window under B showed B at 96% while every other host had B at 6%.
    #[test]
    fn a_session_from_before_a_relogin_never_files_its_window_under_the_new_account() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let tomorrow = Q_NOW + 14 * 3600;
        let week = Q_NOW + 7 * 86400;
        let root = "1a2b3c4d";
        let limits = |seven: f64, seven_at: i64| StatusInput {
            five_hour_pct: Some(0.5),
            five_hour_resets_at: Some(Q_NOW + 5 * 3600),
            seven_day_pct: Some(seven),
            seven_day_resets_at: Some(seven_at),
            ..Default::default()
        };
        // S writes A's window under A.
        record_quota(
            &mut limits(0.96, tomorrow),
            &dir,
            Some(&ident("acct-aaaaaaaa", None)),
            root,
            "sess-s",
            now_ms,
        );
        let acct_a = std::fs::read(dir.join("acct-aaaaaaaa.json")).unwrap();
        let a_json: serde_json::Value = serde_json::from_slice(&acct_a).unwrap();
        assert_eq!(a_json["seven_day"]["pct"], 0.96);
        // login switches to B; S renders again with the SAME rate_limits. B's file must not appear.
        let mut carried = limits(0.96, tomorrow);
        record_quota(
            &mut carried,
            &dir,
            Some(&ident("acct-bbbbbbbb", None)),
            root,
            "sess-s",
            now_ms + 1_000,
        );
        assert!(
            !dir.join("acct-bbbbbbbb.json").exists(),
            "a pre-switch window was filed under the new account"
        );
        assert_eq!(
            std::fs::read(dir.join("acct-aaaaaaaa.json")).unwrap(),
            acct_a
        );
        // B already has its real 6% window (what every other host shows). The same render must
        // leave that file untouched and draw it, not A's 96%.
        let mut on_disk = quota_snap("acct-bbbbbbbb", 0.06, week);
        on_disk.written_at_ms = now_ms - 5_000;
        on_disk.root = Some(root.into());
        crate::live::write_live_json(&dir, "acct-bbbbbbbb.json", &on_disk.to_json()).unwrap();
        let before_b = std::fs::read(dir.join("acct-bbbbbbbb.json")).unwrap();
        let mut still = limits(0.96, tomorrow);
        record_quota(
            &mut still,
            &dir,
            Some(&ident("acct-bbbbbbbb", None)),
            root,
            "sess-s",
            now_ms + 2_000,
        );
        assert_eq!(
            std::fs::read(dir.join("acct-bbbbbbbb.json")).unwrap(),
            before_b
        );
        assert_eq!(still.seven_day_pct, Some(0.06));
        assert_eq!(still.five_hour_pct, None);
        // S makes a new API call after the switch: that reading is B's and is filed under B.
        record_quota(
            &mut limits(0.07, week),
            &dir,
            Some(&ident("acct-bbbbbbbb", None)),
            root,
            "sess-s",
            now_ms + 60_000,
        );
        let b_json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("acct-bbbbbbbb.json")).unwrap())
                .unwrap();
        assert_eq!(b_json["alias"], "acct-bbbbbbbb");
        assert_eq!(b_json["seven_day"]["pct"], 0.07);
        assert_eq!(b_json["seven_day"]["seen_by"], "sess-s");
        // session T, first seen after the switch, files under B.
        record_quota(
            &mut limits(0.08, week),
            &dir,
            Some(&ident("acct-bbbbbbbb", None)),
            root,
            "sess-t",
            now_ms + 120_000,
        );
        let b_json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("acct-bbbbbbbb.json")).unwrap())
                .unwrap();
        assert_eq!(b_json["seven_day"]["pct"], 0.08);
        assert_eq!(b_json["seven_day"]["seen_by"], "sess-t");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let usage = dir.parent().unwrap();
        for name in ["logins-1a2b3c4d.json", "sessions-1a2b3c4d.json"] {
            let body = std::fs::read_to_string(usage.join(name)).unwrap();
            assert!(!body.contains('@'), "{name}: {body}");
            assert!(
                !body.contains("email") && !body.contains("org"),
                "{name}: {body}"
            );
        }
    }

    #[test]
    fn an_empty_session_is_not_gated_on_relogin() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("codeforge/usage/quota");
        let now_ms = Q_NOW * 1000;
        let tomorrow = Q_NOW + 14 * 3600;
        let mut data = StatusInput {
            seven_day_pct: Some(0.96),
            seven_day_resets_at: Some(tomorrow),
            ..Default::default()
        };
        record_quota(
            &mut data,
            &dir,
            Some(&ident("acct-aaaaaaaa", None)),
            "1a2b3c4d",
            "sess-s",
            now_ms,
        );
        let mut switched = StatusInput {
            seven_day_pct: Some(0.96),
            seven_day_resets_at: Some(tomorrow),
            ..Default::default()
        };
        record_quota(
            &mut switched,
            &dir,
            Some(&ident("acct-bbbbbbbb", None)),
            "1a2b3c4d",
            "",
            now_ms + 1_000,
        );
        let b: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("acct-bbbbbbbb.json")).unwrap())
                .unwrap();
        assert_eq!(b["seven_day"]["pct"], 0.96);
    }

    /// Width budget: identity (model, long cwd, branch) must not push the 7d/ctx bars out, and
    /// the quota chips must take the narrower form their drop order defines instead of being cut.
    #[test]
    fn long_identity_keeps_usage_bars_and_quota_chips_whole() {
        let _g = ENV_LOCK.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let entries = mirror_entries(now);
        let forms: Vec<String> = {
            use crate::usage::quota_view as qv;
            let band = qv::quota_band(&entries, now, &qv::Tr::current());
            qv::band_forms(&band)
                .iter()
                .map(|f| f.iter().map(|p| p.text.as_str()).collect::<String>())
                .collect()
        };
        let data = StatusInput {
            model: Some("Opus 5.5 (1M context)".into()),
            cwd: Some("/srv/work/projects/codeforge/.claude/worktrees/review-1118x".into()),
            five_hour_pct: Some(0.08),
            five_hour_resets_at: Some(now + 1800),
            seven_day_pct: Some(0.46),
            seven_day_resets_at: Some(now + 5 * 86400),
            context_pct: Some(0.31),
            mirror: Some(entries),
            ..Default::default()
        };
        for width in [100usize, 108, 116, 124, 140] {
            let lines = compact_lines(&data, width);
            assert_eq!(lines.len(), 1, "{lines:?}");
            let line = &lines[0];
            assert!(vis(line) <= width, "width {width}: {line:?}");
            assert!(
                line.contains("7d") && line.contains("ctx"),
                "width {width}: bars cut: {line:?}"
            );
            if let Some(at) = line.find(" · ") {
                let chips = line[at + 3..].trim_end();
                assert!(
                    forms.iter().any(|f| f == chips),
                    "width {width}: chip cut mid-way: {chips:?} not in {forms:?}"
                );
            }
        }
    }

    #[test]
    fn one_yellow_per_row_counts_this_sessions_own_bars() {
        // other tests set NO_COLOR under ENV_LOCK; colour assertions must not race them
        let _g = ENV_LOCK.lock().unwrap();
        if !color_enabled() {
            return;
        }
        let esc = format!("\x1b[38;2;{};{};{}m", BAR_MID.0, BAR_MID.1, BAR_MID.2);
        let now = chrono::Utc::now().timestamp();
        let row1 = |own: f64| -> String {
            let data = StatusInput {
                model: Some("Opus 5.5".into()),
                seven_day_pct: Some(own),
                seven_day_resets_at: Some(now + 86400),
                // 2 days in at 85 %: short, below red
                others: vec![quota_snap("acct-b", 0.85, now + 5 * 86400)],
                ..Default::default()
            };
            let mut buf = Vec::new();
            render_line(&mut buf, &data, 100).unwrap();
            let text = String::from_utf8(buf).unwrap();
            text.lines()
                .find(|l| l.contains("⏶ b"))
                .unwrap_or_default()
                .to_string()
        };
        // from the separator before the other account (its colour escape precedes the ⏶)
        let other_part = |r: &str| {
            let at = r.find("⏶ b").unwrap();
            r[r[..at].rfind('·').unwrap()..].to_string()
        };
        // own 7d in the yellow band (80-100 %): the 85 % account is bold, not a second yellow
        let r = row1(0.85);
        assert!(
            r[..r.find('·').unwrap()].contains(&esc),
            "own bar is yellow: {r:?}"
        );
        assert!(!other_part(&r).contains(&esc), "{r:?}");
        assert!(other_part(&r).contains("\x1b[1m"), "{r:?}");
        // own bars green: the other account takes the yellow
        let r = row1(0.2);
        assert!(other_part(&r).contains(&esc), "{r:?}");
        let def = Contract::default();
        assert!(own_bars_yellow(
            &StatusInput {
                context_pct: Some(0.85),
                ..Default::default()
            },
            &def,
        ));
        assert!(!own_bars_yellow(
            &StatusInput {
                context_pct: Some(0.70),
                ..Default::default()
            },
            &def,
        ));
        assert!(!own_bars_yellow(
            &StatusInput {
                context_pct: Some(1.0),
                ..Default::default()
            },
            &def,
        ));
    }

    // ── autopilot band-facts + unified quota styling ─────────────────────────

    fn write_band_facts(base: &std::path::Path, file_sid: &str, body: &str) {
        let dir = base.join("autopilot").join("band-facts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{file_sid}.json")), body).unwrap();
    }

    fn sample_band_facts_json(sid: &str, written_at_ms: i64) -> String {
        serde_json::json!({
            "v": 1,
            "written_at_ms": written_at_ms,
            "session_id": sid,
            "contract": {
                "quota_warn_pct": 80,
                "quota_error_pct": 100,
                "round": "floor",
                "bar_on": "▰",
                "bar_off": "▱",
                "alert": "⏶",
                "age": "◷"
            },
            "progress": {
                "live_dispatches": 2,
                "position": "M·finish",
                "unit": "3/3",
                "review_round": 2,
                "decisions_open": 1
            },
            "spend": {
                "usd": 271.4,
                "cap_usd": 300
            }
        })
        .to_string()
    }

    #[test]
    fn band_facts_reader_validates_freshness_size_sid_nulls_cjk_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // missing file -> None
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // valid file -> Some
        write_band_facts(base, HB_SID, &sample_band_facts_json(HB_SID, HB_NOW - 1000));
        let facts = read_band_facts(base, HB_SID, HB_NOW).expect("valid band-facts");
        assert_eq!(facts.contract, Contract::default());
        assert_eq!(facts.live_dispatches, 2);
        assert_eq!(facts.position.as_deref(), Some("M·finish"));
        assert_eq!(facts.unit.as_deref(), Some("3/3"));
        assert_eq!(facts.review_round, Some(2));
        assert_eq!(facts.decisions_open, 1);
        assert_eq!(facts.usd, Some(271.4));
        assert_eq!(facts.cap_usd, Some(300.0));

        // sanitized sid maps raw "a/b:c d" to "a_b_c_d.json"
        let raw_sid = "a/b:c d";
        write_band_facts(
            base,
            "a_b_c_d",
            &sample_band_facts_json(raw_sid, HB_NOW - 500),
        );
        assert!(read_band_facts(base, raw_sid, HB_NOW).is_some());

        // stale (>= 30_000 ms, e.g. 31 s) -> None
        write_band_facts(
            base,
            HB_SID,
            &sample_band_facts_json(HB_SID, HB_NOW - 31_000),
        );
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);
        write_band_facts(
            base,
            HB_SID,
            &sample_band_facts_json(HB_SID, HB_NOW - 30_000),
        );
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // future timestamp -> None
        write_band_facts(base, HB_SID, &sample_band_facts_json(HB_SID, HB_NOW + 1));
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // wrong session_id -> None
        write_band_facts(
            base,
            HB_SID,
            &sample_band_facts_json("other-session", HB_NOW - 100),
        );
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // v=2 -> None
        let mut v2: serde_json::Value =
            serde_json::from_str(&sample_band_facts_json(HB_SID, HB_NOW - 100)).unwrap();
        v2["v"] = serde_json::json!(2);
        write_band_facts(base, HB_SID, &v2.to_string());
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // > 4096 bytes -> None
        let mut huge: serde_json::Value =
            serde_json::from_str(&sample_band_facts_json(HB_SID, HB_NOW - 100)).unwrap();
        huge["progress"]["position"] = serde_json::json!("x".repeat(4200));
        write_band_facts(base, HB_SID, &huge.to_string());
        assert_eq!(read_band_facts(base, HB_SID, HB_NOW), None);

        // nulls + zeros -> omitted
        let nulls = serde_json::json!({
            "v": 1,
            "written_at_ms": HB_NOW - 100,
            "session_id": HB_SID,
            "progress": {
                "live_dispatches": 0,
                "position": null,
                "unit": null,
                "review_round": null,
                "decisions_open": 0
            },
            "spend": {
                "usd": null,
                "cap_usd": null
            }
        });
        write_band_facts(base, HB_SID, &nulls.to_string());
        let f_nulls = read_band_facts(base, HB_SID, HB_NOW).expect("null fields still parse");
        assert_eq!(f_nulls.contract, Contract::default());
        assert_eq!(f_nulls.live_dispatches, 0);
        assert_eq!(f_nulls.position, None);
        assert_eq!(f_nulls.unit, None);
        assert_eq!(f_nulls.review_round, None);
        assert_eq!(f_nulls.decisions_open, 0);
        assert_eq!(f_nulls.usd, None);
        assert_eq!(f_nulls.cap_usd, None);

        // bad contract key falls back for that key only; valid sibling keys preserved
        let partial = serde_json::json!({
            "v": 1,
            "written_at_ms": HB_NOW - 100,
            "session_id": HB_SID,
            "contract": {
                "quota_warn_pct": "not-a-number",
                "quota_error_pct": 95,
                "round": "round",
                "bar_on": "",
                "bar_off": "-",
                "alert": 999
            },
            "progress": {
                "position": "\x1b[31m一二三四五六七八九十甲乙丙丁戊己庚辛壬癸\n",
                "unit": "\x07abcdefghijklmno"
            }
        });
        write_band_facts(base, HB_SID, &partial.to_string());
        let f_partial = read_band_facts(base, HB_SID, HB_NOW).expect("partial contract");
        assert_eq!(f_partial.contract.quota_warn_pct, 80.0);
        assert_eq!(f_partial.contract.quota_error_pct, 95.0);
        assert_eq!(f_partial.contract.round, RoundMode::Round);
        assert_eq!(f_partial.contract.bar_on, "▰");
        assert_eq!(f_partial.contract.bar_off, "-");
        assert_eq!(f_partial.contract.alert, "⏶");
        assert_eq!(f_partial.contract.age, "◷");
        // Control chars stripped and 20 CJK chars cut to 16 chars without panic
        assert_eq!(
            f_partial.position.as_deref(),
            Some("[31m一二三四五六七八九十甲乙")
        );
        assert_eq!(f_partial.position.as_ref().unwrap().chars().count(), 16);
        assert_eq!(f_partial.unit.as_deref(), Some("abcdefghijkl"));

        // Pure 20 CJK chars in position -> 16 chars cleanly
        let cjk20 = serde_json::json!({
            "v": 1,
            "written_at_ms": HB_NOW - 100,
            "session_id": HB_SID,
            "progress": {
                "position": "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸"
            }
        });
        write_band_facts(base, HB_SID, &cjk20.to_string());
        let f_cjk = read_band_facts(base, HB_SID, HB_NOW).unwrap();
        assert_eq!(
            f_cjk.position.as_deref(),
            Some("一二三四五六七八九十甲乙丙丁戊己")
        );
        assert_eq!(f_cjk.position.as_ref().unwrap().chars().count(), 16);
    }

    #[test]
    fn quota_styling_uses_80_100_thresholds_floor_rounding_and_unified_glyphs() {
        let def = Contract::default();
        assert_eq!(bar_rgb(0.799, &def), BAR_LOW);
        assert_eq!(bar_rgb(0.85, &def), BAR_MID);
        assert_eq!(bar_rgb(0.999, &def), BAR_MID);
        assert_eq!(bar_rgb(1.0, &def), BAR_HIGH);

        let bare_floor = strip_ansi(&usage_bare("5h", 0.799, None, &def));
        assert_eq!(bare_floor, "5h ▰▰▰▰▱▱ 79%");

        let round_c = Contract {
            round: RoundMode::Round,
            ..Contract::default()
        };
        let bare_round = strip_ansi(&usage_bare("5h", 0.799, None, &round_c));
        assert_eq!(bare_round, "5h ▰▰▰▰▱▱ 80%");

        let bar_only = strip_ansi(&colored_bar(0.5, 6, &def));
        assert_eq!(bar_only, "▰▰▰▱▱▱");
        assert!(!bar_only.contains('▮') && !bar_only.contains('▯'));

        let left = 5 * 86400;
        let others = [quota_snap("acct-b", 0.85, Q_NOW + left)];
        let other_seg = strip_ansi(&others_segment(
            &others,
            &Default::default(),
            Q_NOW,
            200,
            false,
            &def,
        ));
        assert!(other_seg.contains("⏶ b 7d 85%"), "{other_seg:?}");
        assert!(!other_seg.contains('⚠'), "{other_seg:?}");

        let _g = ENV_LOCK.lock().unwrap();
        if color_enabled() {
            let mid_esc = format!("\x1b[38;2;{};{};{}m", BAR_MID.0, BAR_MID.1, BAR_MID.2);
            let bare_85 = usage_bare("5h", 0.85, None, &def);
            assert!(
                bare_85.contains(&mid_esc),
                "85% must use BAR_MID: {bare_85:?}"
            );
        }
    }

    #[test]
    fn compact_row_with_band_facts_shows_progress_and_spend_and_drops_tail_first() {
        let facts = BandFacts {
            contract: Contract::default(),
            live_dispatches: 2,
            position: Some("M·finish".into()),
            unit: Some("3/3".into()),
            review_round: Some(2),
            decisions_open: 1,
            usd: Some(271.4),
            cap_usd: Some(300.0),
        };
        let with_facts = StatusInput {
            model: Some("Opus 5.5".into()),
            cwd: Some("/tmp/範例專案".into()),
            version: Some("2.1.318".into()),
            context_pct: Some(0.31),
            five_hour_pct: Some(0.08),
            seven_day_pct: Some(0.46),
            band_facts: Some(facts),
            ..Default::default()
        };

        // Wide enough for one line: all progress + spend pieces present
        let one_line = compact_lines(&with_facts, 240);
        assert_eq!(one_line.len(), 1, "{one_line:?}");
        for expected in ["◉ 2", "M·finish", "3/3", "↺ R2", "⬥ 1", "$271 ▰▰▰▰▱"] {
            assert!(
                one_line[0].contains(expected),
                "missing {expected} in {:?}",
                one_line[0]
            );
        }

        // Narrower: still one line; band-facts shrink to fit
        let mid = compact_lines(&with_facts, 130);
        assert_eq!(mid.len(), 1, "{mid:?}");
        assert!(vis(&mid[0]) <= 130, "{:?}", mid[0]);
        assert!(mid[0].contains("◉ 2"), "{:?}", mid[0]);

        // Narrow width: spend & open decisions drop in reverse order before model/path
        let narrow = compact_lines(&with_facts, 50);
        assert!(narrow[0].contains("Opus 5.5"), "{:?}", narrow[0]);
        assert!(narrow[0].contains("範例專案"), "{:?}", narrow[0]);
        assert!(
            !narrow[0].contains("$271"),
            "spend should drop first: {:?}",
            narrow[0]
        );
        assert!(
            !narrow[0].contains("⬥ 1"),
            "decisions should drop before model/path: {:?}",
            narrow[0]
        );

        // Absent band-facts: no progress/spend pieces emitted
        let without_facts = StatusInput {
            band_facts: None,
            ..with_facts
        };
        let baseline = compact_lines(&without_facts, 240);
        assert!(!baseline[0].contains("$271"), "{baseline:?}");
        assert!(!baseline[0].contains("M·finish"), "{baseline:?}");
        assert!(!baseline[0].contains('▮') && !baseline[0].contains('▯'));
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    fn lum((r, g, b): Rgb) -> f64 {
        let f = |c: u8| {
            let x = c as f64 / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
    }

    /// WCAG contrast ratio (L1 + 0.05) / (L2 + 0.05).
    pub(super) fn contrast(a: Rgb, b: Rgb) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    const BLACK: Rgb = (0, 0, 0);
    const WHITE: Rgb = (255, 255, 255);

    fn need(bar: Bar) -> f64 {
        match bar {
            Bar::Text => 4.5,
            Bar::Dim => 3.0,
        }
    }

    /// Foregrounds of a line that fail `bar` against `bg`.
    fn failing(line: &str, bg: Rgb, bar: f64) -> Vec<Rgb> {
        let mut bad = Vec::new();
        for part in line.split("38;2;").skip(1) {
            let n: Vec<u8> = part
                .split([';', 'm'])
                .take(3)
                .filter_map(|x| x.parse().ok())
                .collect();
            if let [r, g, b] = n[..] {
                if contrast((r, g, b), bg) < bar {
                    bad.push((r, g, b));
                }
            }
        }
        bad
    }

    #[test]
    fn contrast_helper_matches_wcag() {
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 1e-9);
        assert!((contrast((0x77, 0x77, 0x77), WHITE) - 4.48).abs() < 0.01);
    }

    #[test]
    fn every_role_passes_its_bar_in_both_palettes() {
        for (name, dark, light, bar) in THEME_ROLES {
            let (d, l) = (contrast(dark, BLACK), contrast(light, WHITE));
            assert!(
                d >= need(bar),
                "{name} dark {dark:?} {d:.2} < {}",
                need(bar)
            );
            assert!(
                l >= need(bar),
                "{name} light {light:?} {l:.2} < {}",
                need(bar)
            );
        }
    }

    #[test]
    fn roles_that_share_a_dark_colour_share_the_light_one() {
        for a in THEME_ROLES {
            for b in THEME_ROLES {
                if a.1 == b.1 {
                    assert_eq!(a.2, b.2, "{} and {} share dark {:?}", a.0, b.0, a.1);
                }
            }
        }
    }

    /// The live pane's dark background is #1e1e1e, not black; every text role must still clear
    /// 4.5:1 against it (autopilot 01M8DV39 measured the old REMAIN 0x62 at 2.73 there).
    #[test]
    fn text_roles_clear_4_5_on_pane_grey() {
        let pane = (0x1E, 0x1E, 0x1E);
        for (name, dark, _light, bar) in THEME_ROLES {
            if bar == Bar::Text {
                let c = contrast(dark, pane);
                assert!(c >= 4.5, "{name} dark {dark:?} on #1e1e1e {c:.2} < 4.5");
            }
        }
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string())
    }

    #[test]
    fn theme_detection_order_and_bad_values() {
        let none = serde_json::Value::Null;
        let light_in = serde_json::json!({"theme": "light-daltonized"});
        assert_eq!(detect_theme(env(&[]), &none), Theme::Dark);
        assert_eq!(
            detect_theme(env(&[("CODEFORGE_THEME", "light")]), &none),
            Theme::Light
        );
        assert_eq!(
            detect_theme(env(&[("CODEFORGE_THEME", "Dark")]), &light_in),
            Theme::Dark
        );
        // the settings switch beats COLORFGBG, which beats stdin
        assert_eq!(
            detect_theme(
                env(&[("CODEFORGE_THEME", "dark"), ("COLORFGBG", "0;15")]),
                &none
            ),
            Theme::Dark
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "0;15")]), &none),
            Theme::Light
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "15;default;0")]), &light_in),
            Theme::Dark
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "0;7")]), &none),
            Theme::Light
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "15;8")]), &none),
            Theme::Dark
        );
        assert_eq!(detect_theme(env(&[]), &light_in), Theme::Light);
        assert_eq!(
            detect_theme(
                env(&[]),
                &serde_json::json!({"output_style": {"theme": "light"}})
            ),
            Theme::Light
        );
        // ignored: an unknown switch value, an unparseable or out-of-range COLORFGBG
        assert_eq!(
            detect_theme(env(&[("CODEFORGE_THEME", "sepia")]), &light_in),
            Theme::Light
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "x;y")]), &none),
            Theme::Dark
        );
        assert_eq!(
            detect_theme(env(&[("COLORFGBG", "0;200")]), &light_in),
            Theme::Light
        );
    }

    #[test]
    fn apply_theme_recolours_plain_and_bold_codes_and_keeps_text() {
        let line = format!("{}{}", tc("a", MODEL_C), tc_bold("b", DELIM));
        let light = apply_theme(&line, Theme::Light);
        assert!(light.contains("38;2;138;90;0m"), "{light:?}");
        assert!(light.contains("38;2;138;138;138m"), "{light:?}");
        assert_eq!(strip(&light), "ab");
        assert_eq!(apply_theme(&line, Theme::Dark), line);
    }

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for c in s.chars() {
            match (esc, c) {
                (false, '\x1b') => esc = true,
                (true, 'm') => esc = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out
    }

    fn sample_line() -> String {
        THEME_ROLES
            .iter()
            .map(|r| tc(r.0, r.1))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn light_line_passes_on_white_and_dark_line_on_black() {
        assert_eq!(
            failing(&apply_theme(&sample_line(), Theme::Light), WHITE, 3.0),
            vec![]
        );
        assert_eq!(failing(&sample_line(), BLACK, 3.0), vec![]);
    }

    /// Reverse control (PM 01M88BD7K5): with theme detection off, a light terminal gets the dark
    /// palette, and the light contrast check must report failing colours.
    #[test]
    fn light_check_fails_when_detection_is_off() {
        let detection_off = Theme::Dark;
        let bad = failing(&apply_theme(&sample_line(), detection_off), WHITE, 3.0);
        assert!(!bad.is_empty(), "the light check found nothing to fail");
        assert!(bad.contains(&MODEL_C), "{bad:?}");
    }
}
