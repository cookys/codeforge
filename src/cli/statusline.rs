use crate::commentary::display::statusline_override;
use crate::daemon::strategy::{Strategy, DEFAULT_STRATEGY};
use crate::db;
use crate::pet::ability::next_unlock;
use crate::pet::live_state::LiveState;
use crate::pet::session::{get_last_seen, update_last_seen, WelcomeBackSummary};
use crate::pet::state::PetState;
use crate::pet::village::VILLAGES;
/// Statusline — 兩欄渲染：info（左）+ art（右）
/// info 欄 pad 到 panel_w，後接 2 空格，再寫 art（已 pad 到 ART_W）
/// UX Pro palette: 三層亮度 (Tier1=222-231 身份, Tier2=71-179 狀態, Tier3=236-246 背景)
use anyhow::Result;
use owo_colors::OwoColorize;
use rust_i18n::t;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

pub fn run(ctx: &db::Context) -> Result<()> {
    let (mut data, raw) = read_status_input();
    write_live_context(&raw);
    let raw_sid = raw["session_id"].as_str().unwrap_or("");
    // forge-stats band showing the pet for THIS session → compact form (plan §2.5).
    let compact = {
        let (live_base, _source) = crate::live::resolve_live_base();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // G36: this account's quota snapshot out, the other accounts' back in.
        let (config_dir, alias) = crate::usage::alias::current_claude_account();
        let others = record_quota(
            &mut data,
            &crate::usage::quota::quota_dir(&live_base),
            &alias,
            &crate::usage::alias::path_hash8(&config_dir),
            &crate::live::sanitize_session_id(raw_sid),
            now_ms,
        );
        data.others = others;
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
        read_mod_heartbeat(&live_base, raw_sid, now_ms)
    };
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

    // Phase 3d §3.1: Welcome Back Report. Compute BEFORE updating
    // last_seen — otherwise the first post-gap call overwrites its own
    // baseline. Only rendered when the session gap ≥ ABSENCE_THRESHOLD,
    // so intra-session statusline refreshes (~every 5s) stay silent.
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let hp_snapshot = loaded
        .as_ref()
        .and_then(|l| l.mood.map(|_| (l.state.hp, l.state.hp.max(1))));
    // hp_max isn't exposed on LiveState yet — PetState lacks it too since
    // Phase 1 only had hp. The snapshot does, but adding a field is beyond
    // P5 scope. Pass None so render_lines skips the HP line rather than
    // fabricating a misleading percentage. P3d follow-up in backlog.
    let _ = hp_snapshot;
    if let Ok(Some(last)) = get_last_seen(&conn) {
        if let Ok(Some(summary)) = WelcomeBackSummary::build(&conn, last, now_unix, None) {
            if summary.has_content() || summary.absence_seconds >= 3600 {
                // Print each line plainly — avoids fighting the panel's
                // ANSI framing. Prefix with a soft indicator so the user
                // spots it amid normal output.
                for line in summary.render_lines() {
                    writeln!(out, "{}", line)?;
                }
            }
        }
    }
    // Window is closed — next statusline call sees a fresh baseline.
    let _ = update_last_seen(&conn, now_unix);

    let (brain, active_l1) = brain_health(ctx);

    match loaded {
        Some(live) => {
            // Phase 3c P5: commentary bubble. Priority order when picking
            // the speech-bubble text:
            //   1. stdin "message" (explicit caller intent — never stomped)
            //   2. Commentary feed (5% roll, ≤1h old, opt-in gate)
            //   3. daemon-authored last_message (combat narration)
            if data.message.is_none() {
                if let Ok(Some(phrase)) = statusline_override(&conn, now_unix) {
                    data.message = Some(phrase);
                } else {
                    data.message = live.last_message.clone();
                }
            }
            let village = VILLAGES
                .iter()
                .find(|v| v.id == live.state.village)
                .unwrap_or(&VILLAGES[2]);
            // Phase 3b: current strategy from snapshot (falls back to
            // DEFAULT_STRATEGY when the snapshot predates v6 — wait-for-tick
            // window is one daemon cycle).
            let strategy = live.strategy.unwrap_or(DEFAULT_STRATEGY);
            if compact {
                render_compact(&mut out, &data, width)?;
            } else {
                render_full(
                    &mut out,
                    &data,
                    &live.state,
                    village,
                    strategy,
                    width,
                    &brain,
                )?;
            }
            crate::pet::view::publish(
                Some((&live.state, village)),
                crate::pet::view::Extras {
                    mood: live.mood,
                    strategy: Some(strategy.as_str()),
                    message: live.last_message.as_deref(),
                },
            );
        }
        None => {
            if compact {
                render_compact(&mut out, &data, width)?;
            } else {
                render_no_pet(&mut out, &data, width)?;
            }
            crate::pet::view::publish(None, Default::default());
        }
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

/// Schema of the heartbeat forge-stats writes to `<live_base>/codeforge/mod-alive/<sid>.json`.
const MOD_ALIVE_SCHEMA: &str = "codeforge.mod-alive/1";
/// A heartbeat older than this no longer counts (the mod rewrites it every 2 s).
const MOD_ALIVE_FRESH_MS: i64 = 10_000;
/// Heartbeats are one short line; anything larger is not one.
const MOD_ALIVE_MAX_BYTES: u64 = 4096;

/// True only when this session's forge-stats mod runs in its band layout: the
/// heartbeat for `raw_sid` exists, parses, has the right schema and session id, is
/// fresh (`0 <= now - written_at < 10 s`) and says `layout: band`. The pet then lives
/// in the band (autopilot's forge tab), so the statusline keeps only the slow data
/// (owner, 2026-10-31). Anything else → false, and the statusline draws its full form.
fn read_mod_heartbeat(base: &std::path::Path, raw_sid: &str, now_ms: i64) -> bool {
    if raw_sid.is_empty() {
        return false;
    }
    let path = base
        .join(crate::pet::view::DIR)
        .join("mod-alive")
        .join(format!(
            "{}.json",
            crate::live::sanitize_session_id(raw_sid)
        ));
    let Ok(meta) = std::fs::metadata(&path) else {
        return false;
    };
    if meta.len() > MOD_ALIVE_MAX_BYTES {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let age = v["written_at_ms"]
        .as_i64()
        .and_then(|w| now_ms.checked_sub(w));
    v["schema"] == MOD_ALIVE_SCHEMA
        && v["session_id"] == raw_sid
        && age.is_some_and(|a| (0..MOD_ALIVE_FRESH_MS).contains(&a))
        && v["layout"] == "band"
        // B34: a forge-stats whose band was not drawn lately (a plugin above it drew its own band) says
        // `band_seen: false`; an older one never writes the field and keeps compacting as before.
        && v["band_seen"] != false
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

/// The compact form drawn while the forge-stats band layout runs: the no-pet
/// identity + usage strip (2 lines, no frame, no pet rows), without the adopt hint.
/// When the identity, the version and the whole usage part fit on one line with 2
/// columns between them, it is one line: identity left, usage then version right (the
/// version rightmost on either form).
fn render_compact<W: Write>(out: &mut W, data: &StatusInput, width: usize) -> Result<()> {
    render_strip(out, data, width, false)
}

// ─── Input ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct StatusInput {
    model: Option<String>,
    cwd: Option<String>,
    version: Option<String>, // running session version (from /proc tree)
    latest_version: Option<String>, // latest installed version (symlink target)
    update_available: bool,  // latest > session version
    message: Option<String>, // pet speech bubble (from JSON "message" field)
    context_pct: Option<f64>, // 0.0–1.0
    context_window_size: Option<u64>,
    five_hour_pct: Option<f64>,
    five_hour_resets_at: Option<i64>,
    seven_day_pct: Option<f64>,
    seven_day_resets_at: Option<i64>,
    /// G36: the other accounts' fresh quota snapshots, most constrained first.
    others: Vec<crate::usage::quota::Snapshot>,
    /// G40: the quota mirror's merged entries; `None` while `<live_dir>/quota/` does not exist (then `others` is drawn, as before).
    mirror: Option<Vec<crate::usage::quota_mirror_read::QEntry>>,
}

// ─── G36 per-account quota (doc/plans/2026-11-03-usage-collector.md §2–3) ─────

/// Merge this account's quota into its snapshot when Claude Code reported `rate_limits`
/// (throttled, 0600; a window's value only grows, see `quota::merge_snapshot`) and show
/// the merged value on this session's own bars, so an idle session no longer shows the
/// lower value of its last API call. Then read the other accounts' snapshots back.
/// Never fails the statusline: a write or read error only means fewer accounts shown.
fn record_quota(
    data: &mut StatusInput,
    dir: &std::path::Path,
    alias: &str,
    root: &str,
    session: &str,
    now_ms: i64,
) -> Vec<crate::usage::quota::Snapshot> {
    use crate::usage::quota::{merge_snapshot, read_others, Seen, Snapshot, Window};
    let window = |pct: Option<f64>, at: Option<i64>| match (pct, at) {
        (Some(pct), Some(resets_at)) => Some(Window {
            pct,
            resets_at,
            seen: Some(Seen {
                at_ms: now_ms,
                session: session.to_string(),
            }),
        }),
        _ => None,
    };
    let snap = Snapshot {
        alias: alias.to_string(),
        root: crate::usage::quota::is_hash8(root).then(|| root.to_string()),
        written_at_ms: now_ms,
        five_hour: window(data.five_hour_pct, data.five_hour_resets_at),
        seven_day: window(data.seven_day_pct, data.seven_day_resets_at),
    };
    if snap.five_hour.is_some() || snap.seven_day.is_some() {
        if let Ok((merged, _)) = merge_snapshot(dir, &snap) {
            if let Some(w) = merged.five_hour {
                data.five_hour_pct = Some(w.pct);
                data.five_hour_resets_at = Some(w.resets_at);
            }
            if let Some(w) = merged.seven_day {
                data.seven_day_pct = Some(w.pct);
                data.seven_day_resets_at = Some(w.resets_at);
            }
        }
    }
    read_others(dir, alias, now_ms)
}

/// The quota part of the usage row: the mirror's chips when the mirror exists (G40), else the G36
/// snapshot chips exactly as before. The statusline never draws both.
fn quota_segment(data: &StatusInput, now_s: i64, avail: usize, yellow_taken: bool) -> String {
    match &data.mirror {
        None => others_segment(&data.others, now_s, avail, yellow_taken),
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

/// The other accounts on the usage row: ` · alias NN% countdown` per account, most
/// constrained first, as many as fit in `avail` visible cells (an account that does
/// not fit with its countdown is tried without it; the first that does not fit ends
/// the list). Red at 95 %, yellow at 80 % for the first such account only (TUI
/// guideline: one yellow per row), the rest bold. `yellow_taken`: the row already
/// shows a yellow (this session's own bars, see [`own_bars_yellow`]).
fn others_segment(
    others: &[crate::usage::quota::Snapshot],
    now_s: i64,
    avail: usize,
    yellow_taken: bool,
) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    let mut yellow_used = yellow_taken;
    for s in others {
        let Some((_label, w)) = s.constrained(now_s) else {
            continue;
        };
        let pct = format!("{:.0}%", w.pct * 100.0);
        let remain = fmt_remaining_at(w.resets_at, now_s);
        let pct_c = if w.pct >= 0.95 {
            tc(&pct, BAR_HIGH)
        } else if w.pct >= 0.80 && !yellow_used {
            yellow_used = true;
            tc(&pct, BAR_MID)
        } else if w.pct >= 0.80 {
            tc_bold(&pct, BAR_LBL)
        } else {
            tc(&pct, BAR_LBL)
        };
        let sep = tc(" · ", DELIM);
        let head_w = 3 + vis(&s.alias) + 1 + vis(&pct);
        let full_w = head_w + 1 + vis(&remain);
        let (piece, w_piece) = if used + full_w <= avail {
            (
                format!(
                    "{sep}{} {pct_c} {}",
                    tc(&s.alias, BAR_LBL),
                    tc(&remain, REMAIN)
                ),
                full_w,
            )
        } else if used + head_w <= avail {
            (format!("{sep}{} {pct_c}", tc(&s.alias, BAR_LBL)), head_w)
        } else {
            break;
        };
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
            let context_window_size = v["context_window"]["context_window_size"].as_u64();
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
                message: v["message"].as_str().map(|s| s.to_string()),
                context_pct,
                context_window_size,
                five_hour_pct,
                five_hour_resets_at,
                seven_day_pct,
                seven_day_resets_at,
                others: Vec::new(),
                mirror: None,
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

const DELIM: Rgb = (0x44, 0x44, 0x44); // 238 — ( )
const DELIM_DIM: Rgb = (0x30, 0x30, 0x30); // 236 — dimmed separator (ctx>80%)
const AHEAD_BEHIND: Rgb = (0x87, 0xAF, 0xD7); // 110 — git ⇡⇣ chip (soft blue)
const HINT_C: Rgb = (0x5F, 0x5F, 0x5F); // 240 — onboarding hint (slightly above DELIM)
const MODEL_C: Rgb = (0xFF, 0xD7, 0x87); // 222 — model name
const CTX_SZ: Rgb = (0xD7, 0xAF, 0x5F); // 179 — ctx window size
const CWD_C: Rgb = (0x5F, 0xAF, 0xAF); // 73  — cwd path
const BRANCH_C: Rgb = (0x87, 0xD7, 0x87); // 114 — git branch
const BAR_LOW: Rgb = (0x87, 0xD7, 0x87); // 114 — bar <50%
const BAR_MID: Rgb = (0xD7, 0xAF, 0x5F); // 179 — bar 50-80%
const BAR_HIGH: Rgb = (0xD7, 0x5F, 0x5F); // 167 — bar >80%
const BAR_EMPTY: Rgb = (0x30, 0x30, 0x30); // 236 — bar ▯
const BAR_LBL: Rgb = (0x8A, 0x8A, 0x8A); // 245 — "5h" "7d" "ctx"
const REMAIN: Rgb = (0x58, 0x58, 0x58); // 240 — "4h2m"
const PET_NAME: Rgb = (0xEE, 0xEE, 0xEE); // 255 — pet name
const PET_LV: Rgb = (0xFF, 0xD7, 0x87); // 222 — "Lv.N"
const STAT_LBL: Rgb = (0x58, 0x58, 0x58); // 240 — "ATK:"
const STAT_VAL: Rgb = (0x94, 0x94, 0x94); // 246 — stat numbers
const UPDATE_C: Rgb = (0xFF, 0xAF, 0x00); // 214 — amber update banner
                                          // Brain indicator palette (Task 13)
const BRAIN_GREEN: Rgb = (0x5F, 0xAF, 0x5F); // 71  — ● active / ok
const BRAIN_YELLOW: Rgb = (0xD7, 0xAF, 0x5F); // 179 — ◐ degraded
const BRAIN_GRAY: Rgb = (0x5F, 0x5F, 0x5F); // 240 — ○ offline / ◌ pending / empty
const BRAIN_LBL: Rgb = (0x5F, 0x5F, 0x5F); // 240 — "memory"/"mnemos" labels
const BRAIN_HINT: Rgb = (0x5F, 0x5F, 0x5F); // 240 — "→ doctor" hint

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

/// HSV(S=V=1)的 hue(度,0..360)→ RGB。給彩虹漸層用(純函式,可測)。
fn hue_rgb(h: f64) -> Rgb {
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = 1.0 - (hp % 2.0 - 1.0).abs();
    let (r, g, b) = match hp as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// 連上央腦(CentralLight::Ok)時,把 "mnemos" label 渲染成隨時間流動的彩虹漸層:
/// 每字一個 hue stop、相位由系統時鐘推進 → statusline 每次重繪 hue 位移,字看起來在流動
/// (Claude Code ultra-think 那種閃爍感)。bold 讓它跳出來。
/// 只走有色路徑(no-color 用 `label:status` 純文字變體);gate 在 color_enabled。
fn rainbow(text: &str) -> String {
    if !color_enabled() {
        return text.to_string();
    }
    // 相位:系統時鐘毫秒。CC 約每數秒重繪,每次重繪相位前進 → 彩虹沿字流動。
    let phase = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let phase_deg = (phase / 90) as f64 % 360.0;
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        let (r, g, b) = hue_rgb(phase_deg + i as f64 * 45.0); // 45°/字 散開成漸層
        out.push_str(&format!("{}", ch.truecolor(r, g, b).bold()));
    }
    out
}

/// Whether to render the plain (no-ANSI) statusline form. Mirrors `color_enabled()`:
/// the standard NO_COLOR env var (non-empty) selects plain. Pipe-vs-tty is
/// irrelevant — CC consumes ANSI, so color is never stripped just because stdout
/// isn't a terminal.
fn stdout_is_plain() -> bool {
    !color_enabled()
}

fn bar_rgb(pct: f64) -> Rgb {
    if pct < 0.5 {
        BAR_LOW
    } else if pct < 0.8 {
        BAR_MID
    } else {
        BAR_HIGH
    }
}

/// Colored progress bar (filled=semantic, empty=dim 236)
fn colored_bar(pct: f64, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 1.0)) * width as f64) as usize;
    let empty = width - filled;
    format!(
        "{}{}",
        tcs("▮".repeat(filled), bar_rgb(pct)),
        tcs("▯".repeat(empty), BAR_EMPTY)
    )
}

/// HP bar color: green >60%, amber 30-60%, red <30%
fn hp_rgb(hp: u32) -> Rgb {
    if hp > 60 {
        BAR_LOW
    } else if hp > 30 {
        BAR_MID
    } else {
        BAR_HIGH
    }
}

/// Visible column width (handles wide chars: CJK=2col, ASCII/▮▯=1col)
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

fn fmt_ctx_window(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{}", n)
    }
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

/// Pad an ANSI string to at least `width` visible columns with trailing spaces
fn pad_to_vis(s: &str, width: usize) -> String {
    let w = ansi_vis(s);
    if w < width {
        format!("{}{}", s, " ".repeat(width - w))
    } else {
        s.to_string()
    }
}

/// True when this session's own usage bars already draw the yellow (`BAR_MID`,
/// 50–80 % in `bar_rgb`), so the other accounts must not add a second one.
fn own_bars_yellow(data: &StatusInput) -> bool {
    [data.five_hour_pct, data.seven_day_pct, data.context_pct]
        .into_iter()
        .flatten()
        .any(|p| bar_rgb(p) == BAR_MID)
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
fn usage_bare(label: &str, pct: f64, remain: Option<String>) -> String {
    let bar = colored_bar(pct, 6);
    let pct_num = format!("{:.0}%", pct * 100.0);
    let (r, g, b) = bar_rgb(pct);
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

/// 一行的兩欄資料
struct Row {
    /// art 欄（None = 不寫 art，讓 col 0..ART_W 保持空白）
    art: Option<String>,
    /// info 欄（從 col INFO_COL 開始寫）
    info: String,
}

/// 核心渲染引擎：info 欄（左）pad 到 info_w 寬，後接 2 空格，再寫 art（右）
///
/// art 已在 art() closure 中 pad 到 ART_W，讓右側欄對齊。
/// 不需要 MoveToColumn — 純 write! + writeln!，相容所有環境。
/// info_w=0（render_no_pet）時 None art 行直接輸出 info，不加尾端空格。
fn render_rows<W: Write>(out: &mut W, rows: &[Row], info_w: usize) -> Result<()> {
    for row in rows {
        match &row.art {
            Some(art) => writeln!(out, "{}  {}", pad_to_vis(&row.info, info_w), art)?,
            None => writeln!(out, "{}", row.info)?,
        }
    }
    Ok(())
}

fn render_no_pet<W: Write>(out: &mut W, data: &StatusInput, width: usize) -> Result<()> {
    render_strip(out, data, width, true)
}

/// The 2-line identity + usage strip shared by the no-pet form (with the adopt
/// hint) and the compact form (without it).
fn render_strip<W: Write>(
    out: &mut W,
    data: &StatusInput,
    width: usize,
    adopt_hint: bool,
) -> Result<()> {
    // Row 0 — identity strip (no box, no chip wrappers).
    // Composition: model ── cwd ⇡⇣ branch ─────── version
    // Cross-mode anchor: same model/cwd/branch/version order as render_full's
    // top border, just without the ╭─...─╮ frame. Adopting a pet is a "bloom"
    // (extra rows appear), not a re-layout.
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
    // ab_part carries ONLY its leading space (matches render_full's pattern).
    // The space BEFORE branch is always written explicitly so cwd and branch
    // can't glue together when ab_part is empty (no-upstream case).
    let (ab_part, ab_vis) = if ab_str.is_empty() {
        (String::new(), 0)
    } else {
        (format!(" {}", tc(&ab_str, AHEAD_BEHIND)), 1 + vis(&ab_str))
    };

    let (ver_str, ver_vis) = render_version(data, delim_c);

    // Layout: " " model " ── " cwd <ab_part> " " branch " " fill(N) " " version
    //   fixed widths: 1 + model + 4 + ab_vis + 1 + 1 + 1 + ver_vis
    let fixed = 1 + model_vis + 4 + ab_vis + 1 + 1 + 1 + ver_vis;
    // Allocate cwd ≥ 10 cols, branch ≥ 4 cols.
    let cwd_budget = width.saturating_sub(fixed + 4).max(10);
    let cwd_display = shorten_path(&cwd_home, cwd_budget);
    let cwd_vis = vis(&cwd_display);
    let branch_budget = width.saturating_sub(fixed + cwd_vis).max(4);
    let branch_display = shorten_str(&branch_full, branch_budget);
    let branch_vis = vis(&branch_display);
    let fill = width.saturating_sub(fixed + cwd_vis + branch_vis).max(2);

    let row0 = format!(
        " {}{}{}{} {} {} {}",
        model_text,
        tc(" ── ", delim_c),
        tc(&cwd_display, CWD_C),
        ab_part,
        tc(&branch_display, BRANCH_C),
        tcs("─".repeat(fill), delim_c),
        ver_str,
    );

    // Row 1 — usage bars on LEFT, onboarding hint right-aligned on the
    // SAME row. Hint stays visible regardless of usage data because
    // "no pet adopted" is orthogonal to "Claude is reporting usage" —
    // the user can be deep in a CC session and still need the nudge.
    // Degradation: if the row is too narrow to fit both, hint shrinks
    // to "→ adopt"; if still no room, hint is dropped entirely.
    let mut usage_parts: Vec<String> = Vec::new();
    if let Some(pct) = data.five_hour_pct {
        usage_parts.push(usage_bare(
            "5h",
            pct,
            data.five_hour_resets_at.map(fmt_remaining),
        ));
    }
    if let Some(pct) = data.seven_day_pct {
        usage_parts.push(usage_bare(
            "7d",
            pct,
            data.seven_day_resets_at.map(fmt_remaining),
        ));
    }
    if let Some(pct) = data.context_pct {
        usage_parts.push(usage_bare("ctx", pct, None));
    }
    let usage_left = if usage_parts.is_empty() {
        tc("─", delim_c)
    } else {
        usage_parts.join("   ")
    };
    let usage_left = {
        let room = width.saturating_sub(1 + ansi_vis(&usage_left) + 1);
        let now_s = chrono::Utc::now().timestamp();
        format!(
            "{usage_left}{}",
            quota_segment(data, now_s, room, own_bars_yellow(data))
        )
    };
    let usage_left_vis = ansi_vis(&usage_left);
    // Two hint forms, picked by available width
    let hint_long = "→ codeforge adopt";
    let hint_short = "→ adopt";
    let min_gap = 3; // breathing room between usage and hint
    let avail_for_hint = width.saturating_sub(1 + usage_left_vis + min_gap + 1); // leading space + content + gap + trailing
    let hint_render = if !adopt_hint {
        None
    } else if avail_for_hint >= vis(hint_long) {
        Some((hint_long, vis(hint_long)))
    } else if avail_for_hint >= vis(hint_short) {
        Some((hint_short, vis(hint_short)))
    } else {
        None
    };
    // One line (compact form only): " " identity, at least 2 spaces, usage "   " version
    // (the version stays rightmost, as on the two-line form's first row).
    // The usage part is the one the two-line form would draw, so nothing is dropped.
    if !adopt_hint {
        let left = format!(
            " {}{}{}{} {}",
            model_text,
            tc(" ── ", delim_c),
            tc(&cwd_display, CWD_C),
            ab_part,
            tc(&branch_display, BRANCH_C),
        );
        let left_vis = 1 + model_vis + 4 + cwd_vis + ab_vis + 1 + branch_vis;
        let sep = if ver_vis == 0 { "" } else { "   " };
        let right = format!("{usage_left}{sep}{ver_str}");
        let right_vis = usage_left_vis + sep.len() + ver_vis;
        if left_vis + 2 + right_vis <= width {
            let pad = width - left_vis - right_vis;
            let rows = [Row {
                art: None,
                info: format!("{left}{}{right}", " ".repeat(pad)),
            }];
            return render_rows(out, &rows, 0);
        }
    }

    let row1 = match hint_render {
        Some((h, hv)) => {
            let pad = width.saturating_sub(1 + usage_left_vis + hv + 1);
            format!(" {}{}{}", usage_left, " ".repeat(pad), tc(h, HINT_C))
        }
        None => format!(" {}", usage_left),
    };

    let rows = vec![
        Row {
            art: None,
            info: row0,
        },
        Row {
            art: None,
            info: row1,
        },
    ];
    render_rows(out, &rows, 0)
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

fn render_full<W: Write>(
    out: &mut W,
    data: &StatusInput,
    pet: &PetState,
    village: &crate::pet::village::Village,
    strategy: Strategy,
    width: usize,
    brain_health: &crate::mnemos::health::BrainHealth,
) -> Result<()> {
    const ART_W: usize = 10;
    const ART_THRESHOLD: usize = 90; // drop art column below this width

    // V2: context-pressure dimming. When ctx > 80%, all Tier-3 elements
    // (separators, box-drawing, dim labels) shift one stop dimmer. The chat
    // gets visual priority back; the statusline becomes peripheral signal.
    let dimmed = data.context_pct.is_some_and(|p| p > 0.8);
    let delim_c = if dimmed { DELIM_DIM } else { DELIM };

    let art_visible = width >= ART_THRESHOLD;
    let panel_w = if art_visible {
        width.saturating_sub(ART_W + 2)
    } else {
        width
    };
    let vrgb = village.rgb();

    let cwd_raw = data
        .cwd
        .clone()
        .or_else(current_dir_short)
        .unwrap_or_else(|| "~".to_string());
    let cwd_home = to_home_rel(&cwd_raw);
    let branch_full = git_branch().unwrap_or_else(|| "—".to_string());
    let ab_str = fmt_ahead_behind(git_ahead_behind());

    let box_mid = |content: &str, left: &str, right: &str| -> String {
        let pad = panel_w.saturating_sub(vis(left) + ansi_vis(content) + vis(right));
        format!(
            "{}{}{}{}",
            tc(left, delim_c),
            content,
            " ".repeat(pad),
            tc(right, delim_c)
        )
    };

    let art_lines: Vec<&str> = village.ascii_small.lines().collect();

    // ── Row 0: top border ╭─ model (ctx) ─── cwd ⇡N main ──── Village ──╮
    //
    // V2: village name lives in the top border (saves 1 row vs. the old
    // dedicated divider). Identity chips share the same model→cwd→branch
    // order as render_no_pet — cross-mode anchor.

    let (model_inner, model_inner_vis) = match (&data.model, data.context_window_size) {
        (Some(m), Some(cw)) => {
            let m_s = shorten_str(m, 18);
            let cw_s = fmt_ctx_window(cw);
            let v = vis(&m_s) + 3 + vis(&cw_s); // "m (cw)"
            (
                format!(
                    "{} {}{}{}",
                    tc_bold(&m_s, MODEL_C),
                    tc("(", delim_c),
                    tc(&cw_s, CTX_SZ),
                    tc(")", delim_c),
                ),
                v,
            )
        }
        (Some(m), None) => {
            let m_s = shorten_str(m, 18);
            (tc_bold(&m_s, MODEL_C), vis(&m_s))
        }
        _ => (tc("—", STAT_VAL), 1),
    };

    let vkey = format!("village.{}.name", village.id);
    let vname = t!(&vkey).to_string();
    let vname_short = shorten_str(&vname, 22);
    let vname_vis = vis(&vname_short);

    // Fixed overhead on row 0:
    //   ╭─ (3) + model + " ── " (4) + cwd + ⇡N (ab_vis) + " " (1) + branch
    //   + " ──── " (6) + village + " ──╮" (4) + fill(N)
    let ab_part = if ab_str.is_empty() {
        String::new()
    } else {
        format!(" {}", tc(&ab_str, AHEAD_BEHIND))
    };
    let ab_vis = if ab_str.is_empty() {
        0
    } else {
        1 + vis(&ab_str)
    };
    let fixed0 = 3 + model_inner_vis + 4 + ab_vis + 1 + 6 + vname_vis + 4;
    // cwd budget: leave ≥ 4 for branch
    let cwd_budget = panel_w.saturating_sub(fixed0 + 4).max(8);
    let cwd_display = shorten_path(&cwd_home, cwd_budget);
    let cwd_vis_w = vis(&cwd_display);
    let branch_avail = panel_w.saturating_sub(fixed0 + cwd_vis_w).max(4);
    let branch_display = shorten_str(&branch_full, branch_avail);
    let branch_vis_w = vis(&branch_display);
    let r0_fill = panel_w.saturating_sub(fixed0 + cwd_vis_w + branch_vis_w);

    let row0_info = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        tc("╭─ ", delim_c),
        model_inner,
        tc(" ── ", delim_c),
        tc(&cwd_display, CWD_C),
        ab_part,
        tc(&format!(" {}", branch_display), BRANCH_C),
        tc(" ──── ", delim_c),
        tc_bold(&vname_short, vrgb),
        tcs(format!(" {}", "─".repeat(r0_fill)), delim_c),
        tc("──╮", delim_c),
    );

    // ── Row 1: usage bars inside box │ 5h ▮▯ 5%  7d ▮▮ 39%  ctx ▮▮▮ 54%   │

    let row1_info = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(pct) = data.five_hour_pct {
            parts.push(usage_bare(
                "5h",
                pct,
                data.five_hour_resets_at.map(fmt_remaining),
            ));
        }
        if let Some(pct) = data.seven_day_pct {
            parts.push(usage_bare(
                "7d",
                pct,
                data.seven_day_resets_at.map(fmt_remaining),
            ));
        }
        if let Some(pct) = data.context_pct {
            parts.push(usage_bare("ctx", pct, None));
        }
        let mut content = parts.join("  ");
        let room = panel_w.saturating_sub(vis("│ ") + ansi_vis(&content) + vis("│") + 1);
        let now_s = chrono::Utc::now().timestamp();
        content.push_str(&quota_segment(data, now_s, room, own_bars_yellow(data)));
        box_mid(&content, "│ ", "│")
    };

    // ── Version chip (used by bottom border) ─────────────────────────────────

    let (ver_str, ver_vis) = render_version(data, delim_c);

    // ── Brain health (Task 13): local + central lights for bottom border ──────

    // Probe tc() to decide plain vs. color path — see stdout_is_plain() for rationale.
    let no_color = stdout_is_plain();

    // ── Art helper: pad art line to ART_W, colored with village rgb ──────────
    // When art_visible is false, every helper returns None so render_rows
    // omits the trailing "  art" entirely (info_w = panel_w).

    let art = |i: usize| -> Option<String> {
        if !art_visible {
            return None;
        }
        art_lines.get(i).map(|s| {
            let w = vis(s);
            let pad = if w < ART_W {
                " ".repeat(ART_W - w)
            } else {
                String::new()
            };
            format!("{}{}", tcs(s.to_string(), vrgb), pad)
        })
    };
    let art_s = |i: usize| -> String {
        art(i).unwrap_or_else(|| {
            if art_visible {
                " ".repeat(ART_W)
            } else {
                String::new()
            }
        })
    };

    // ── Rows 2-4: pet stats or speech bubble (V2: was rows 3-5) ──────────────
    //
    // Normal:
    //   │ Ferris Lv.1  ♥ █░░ ...                                    │  art[2]
    //   │ ATK 18  DEF 20  ...  ▸ agg                                 │  art[3]
    //   ╰─ memory ● active ──────────────────────────────── v2.1.105 ╯  art[4]
    //
    // Bubble path renumbered (now rows 2,3,4 instead of 3,4,5):

    if let Some(msg) = data.message.as_deref() {
        render_rows(
            out,
            &[
                Row {
                    art: art(0),
                    info: row0_info,
                },
                Row {
                    art: art(1),
                    info: row1_info,
                },
            ],
            panel_w,
        )?;

        let max_msg = panel_w.saturating_sub(7).max(1);
        let msg_display = shorten_str(msg, max_msg);
        let msg_vis_w = vis(&msg_display);
        let bw = (msg_vis_w + 4).min(panel_w.saturating_sub(3)).max(6);
        let inner_pad = bw.saturating_sub(4 + msg_vis_w);
        let left_fill = panel_w.saturating_sub(2 + bw);

        // Row 2 (was 3): bubble top
        let r2 = format!(
            "{}{}{}{}{}",
            tc("│ ", delim_c),
            " ".repeat(left_fill),
            tc("╭", vrgb),
            tcs("─".repeat(bw.saturating_sub(2)), vrgb),
            tc("╮", vrgb),
        );
        if art_visible {
            writeln!(
                out,
                "{}{} {}",
                pad_to_vis(&r2, panel_w),
                tc("\\", vrgb),
                art_s(2)
            )?;
        } else {
            writeln!(out, "{}", r2)?;
        }

        // Row 3 (was 4): bubble content
        let r3 = format!(
            "{}{}{}{}{}{}{}",
            tc("│ ", delim_c),
            " ".repeat(left_fill),
            tc("│ ", vrgb),
            tc(&msg_display, PET_NAME),
            " ".repeat(inner_pad),
            tc(" ", vrgb),
            tc("│", vrgb),
        );
        if art_visible {
            writeln!(
                out,
                "{} {}{}",
                pad_to_vis(&r3, panel_w),
                tc(">", vrgb),
                art_s(3)
            )?;
        } else {
            writeln!(out, "{}", r3)?;
        }

        // Row 4 (was 5): bottom border with memory status + version
        let r4 = bottom_border(
            panel_w,
            delim_c,
            ver_str.clone(),
            ver_vis,
            brain_health,
            no_color,
        );
        if art_visible {
            writeln!(
                out,
                "{}{} {}",
                pad_to_vis(&r4, panel_w),
                tc("/", vrgb),
                art_s(4)
            )?;
        } else {
            writeln!(out, "{}", r4)?;
        }
    } else {
        // ── Row 2 (was 3): pet HP / XP ──────────────────────────────────────
        // V2: ♥/✦ symbols replace stat.hp / stat.xp text labels.

        let hp_pct = (pet.hp as f64 / 100.0).clamp(0.0, 1.0);
        let hp_filled = (hp_pct * 6.0) as usize;
        let hp_bar = format!(
            "{}{}",
            tcs("█".repeat(hp_filled), hp_rgb(pet.hp)),
            tcs("░".repeat(6 - hp_filled), BAR_EMPTY),
        );
        let xp_filled = ((pet.xp as f64 / pet.xp_to_next as f64).clamp(0.0, 1.0) * 6.0) as usize;
        let xp_bar = format!(
            "{}{}",
            tcs("█".repeat(xp_filled), vrgb),
            tcs("░".repeat(6 - xp_filled), BAR_EMPTY),
        );
        let unlock_suffix = next_unlock(pet.level)
            .map(|u| {
                let name = shorten_str(u.name, 16);
                let lv_label = t!("stat.lv");
                format!(
                    "  {} {}{} {}",
                    tc("→", delim_c),
                    tc(&lv_label, STAT_LBL),
                    tc(&u.required_level.to_string(), PET_LV),
                    tc(&name, STAT_VAL),
                )
            })
            .unwrap_or_default();

        let r2_content = format!(
            "{} {}  {} {} {}  {} {} {}/{}{}",
            tc_bold(&pet.name, PET_NAME),
            tc(&format!("{}{}", t!("stat.lv"), pet.level), PET_LV),
            tc("♥", hp_rgb(pet.hp)),
            hp_bar,
            tc(&pet.hp.to_string(), hp_rgb(pet.hp)),
            tc("✦", vrgb),
            xp_bar,
            tc(&pet.xp.to_string(), vrgb),
            tc(&pet.xp_to_next.to_string(), STAT_VAL),
            unlock_suffix,
        );
        let row2_info = box_mid(&r2_content, "│ ", "│");

        // ── Row 3 (was 4): stats ────────────────────────────────────────────
        // V2: no colons on labels; strategy as "▸ agg" (mode indicator, not k:v).

        let r3_content = format!(
            "{} {}  {} {}  {} {}  {} {}   {} {}",
            tc(&t!("stat.atk"), STAT_LBL),
            tc(&format!("{:2}", pet.atk), STAT_VAL),
            tc(&t!("stat.def"), STAT_LBL),
            tc(&format!("{:2}", pet.def), STAT_VAL),
            tc(&t!("stat.sup"), STAT_LBL),
            tc(&format!("{:2}", pet.sup), STAT_VAL),
            tc(&t!("stat.ver"), STAT_LBL),
            tc(&format!("{:2}", pet.ver), STAT_VAL),
            tc("▸", delim_c),
            tc(strategy.short_tag(), STAT_VAL),
        );
        let row3_info = box_mid(&r3_content, "│ ", "│");

        // ── Row 4 (was 5): bottom border with memory ● active + version ─────

        let row4_info = bottom_border(panel_w, delim_c, ver_str, ver_vis, brain_health, no_color);

        let rows = vec![
            Row {
                art: art(0),
                info: row0_info,
            },
            Row {
                art: art(1),
                info: row1_info,
            },
            Row {
                art: art(2),
                info: row2_info,
            },
            Row {
                art: art(3),
                info: row3_info,
            },
            Row {
                art: art(4),
                info: row4_info,
            },
        ];
        render_rows(out, &rows, panel_w)?;
    }

    Ok(())
}

/// Render the bottom border with dual brain lights + 6-level degradation ladder.
///
/// Format (colored): `╰─ memory ● active   mnemos ● ok ─── v0.0.5 ──╯`
/// Format (no-color): `╰─ memory:active   mnemos:ok ─── v0.0.5 ──╯`
///
/// Degradation ladder (wide → narrow):
///   1. Full: both segments + hint (only in yellow/gray states) + version
///   2. Drop `→ doctor` hint
///   3. central word → short code (skipped when no_color=true)
///   4. Drop version chip (keep `⬆` update banner when present)
///   5. Central pure glyph, no word
///   6. Only local segment
///
/// Each level ensures `ansi_vis(output) <= panel_w`.
fn bottom_border(
    panel_w: usize,
    delim_c: Rgb,
    ver_str: String,
    ver_vis: usize,
    brain: &crate::mnemos::health::BrainHealth,
    no_color: bool,
) -> String {
    use crate::mnemos::health::{CentralLight, LocalLight};

    // ─── Segment builders ────────────────────────────────────────────────
    // local segment: "memory ● active" (colored) or "memory:active" (no-color)
    let local_label = t!("ui.brain.local_label").to_string();
    let local_status = match brain.local {
        LocalLight::Active => t!("ui.brain.status.active").to_string(),
        LocalLight::Empty => t!("ui.brain.status.empty").to_string(), // gray: store exists but active=0
        LocalLight::Hidden => String::new(),                          // not rendered
    };
    let local_glyph = match brain.local {
        LocalLight::Active => "●",
        LocalLight::Empty => "◌",
        LocalLight::Hidden => "",
    };
    let local_glyph_color = match brain.local {
        LocalLight::Active => BRAIN_GREEN,
        _ => BRAIN_GRAY,
    };

    let central_label = t!("ui.brain.central_label").to_string();
    let central_status_full = match brain.central {
        CentralLight::Ok => t!("ui.brain.status.ok").to_string(),
        CentralLight::Degraded => t!("ui.brain.status.degraded").to_string(),
        CentralLight::Offline => t!("ui.brain.status.offline").to_string(),
        CentralLight::Pending => t!("ui.brain.status.pending").to_string(),
        CentralLight::Hidden => String::new(),
    };
    let central_status_short = match brain.central {
        CentralLight::Ok => "ok",
        CentralLight::Degraded => "deg",
        CentralLight::Offline => "off",
        CentralLight::Pending => "pend",
        CentralLight::Hidden => "",
    };
    let central_glyph = match brain.central {
        CentralLight::Ok => "●",
        CentralLight::Degraded => "◐",
        CentralLight::Offline => "○",
        CentralLight::Pending => "◌",
        CentralLight::Hidden => "",
    };
    let central_glyph_color = match brain.central {
        CentralLight::Ok => BRAIN_GREEN,
        CentralLight::Degraded => BRAIN_YELLOW,
        _ => BRAIN_GRAY,
    };
    // 連上央腦(Ok)→ "mnemos" label 跑彩虹流光(ultra-think vibe);其餘狀態維持灰。
    let central_label_render = if matches!(brain.central, CentralLight::Ok) {
        rainbow(&central_label)
    } else {
        tc(&central_label, BRAIN_LBL)
    };

    // hint only appears for yellow/gray central states
    let show_hint = matches!(
        brain.central,
        CentralLight::Degraded | CentralLight::Offline | CentralLight::Pending
    );
    let hint_text = "→ doctor";

    // Whether the version chip includes an update banner (preserve `⬆` across lvl 4)
    let has_update_banner = ver_str.contains('⬆');

    // ─── Segment rendering helpers ──────────────────────────────────────
    // Build colored/no-color segment strings and measure their visible widths.

    let make_local_seg_colored = |_glyph_only: bool| -> (String, usize) {
        if matches!(brain.local, LocalLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!(
            "{} {} {}",
            tc(&local_label, BRAIN_LBL),
            tc(local_glyph, local_glyph_color),
            tc(&local_status, STAT_VAL),
        );
        let w = vis(&local_label) + 1 + 1 + 1 + vis(&local_status);
        (s, w)
    };

    let make_local_seg_glyph_only_colored = || -> (String, usize) {
        if matches!(brain.local, LocalLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!(
            "{} {}",
            tc(&local_label, BRAIN_LBL),
            tc(local_glyph, local_glyph_color),
        );
        let w = vis(&local_label) + 1 + 1; // label + space + glyph(1col)
        (s, w)
    };

    let make_local_seg_nocolor = || -> (String, usize) {
        if matches!(brain.local, LocalLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!("{}:{}", local_label, local_status);
        let w = vis(&local_label) + 1 + vis(&local_status);
        (s, w)
    };

    let make_central_seg_colored = |word: &str| -> (String, usize) {
        if matches!(brain.central, CentralLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!(
            "{} {} {}",
            central_label_render,
            tc(central_glyph, central_glyph_color),
            tc(word, STAT_VAL),
        );
        let w = vis(&central_label) + 1 + 1 + 1 + vis(word);
        (s, w)
    };

    let make_central_seg_glyph_only_colored = || -> (String, usize) {
        if matches!(brain.central, CentralLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!(
            "{} {}",
            central_label_render,
            tc(central_glyph, central_glyph_color),
        );
        let w = vis(&central_label) + 1 + 1;
        (s, w)
    };

    let make_central_seg_nocolor = |word: &str| -> (String, usize) {
        if matches!(brain.central, CentralLight::Hidden) {
            return (String::new(), 0);
        }
        let s = format!("{}:{}", central_label, word);
        let w = vis(&central_label) + 1 + vis(word);
        (s, w)
    };

    // ─── 未來 indicator 擴充點(hangar-bridge / autopilot / …) ─────────────
    // 新 indicator 是「左側 content」的另一段,擺在 central(mnemos)之後、右側版號之前
    // (中間的 fill 會自動吸收/縮短)。接法(對齊 central segment 的既有 pattern):
    //   1. 加狀態來源(如 health.rs 的 CentralLight 那樣的 enum)+ glyph/顏色 match。
    //   2. 寫 make_<name>_seg_{colored,glyph_only_colored,nocolor} 三個變體(回 (String, vis))。
    //   3. 在下面 degradation ladder 的每個 level:`total += seg_vis + gap`、
    //      `content.push_str("   "); content.push_str(&seg)`(接在 central 之後)。
    //      width 自動正確 —— ladder 用 vis 量、assemble 的 fill = panel_w - overhead
    //      - left_vis - right_vis,indicator 算進 left_vis 即可。
    //   4. 窄寬度時於 ladder 較低 level 優先丟掉此 indicator(如 hint 的處理方式)。
    // 設計細節 + 觸發語義見 BACKLOG B27。

    // ─── Assemble final string from chosen components ───────────────────
    // The caller passes `no_color` so we can vary layout.
    // The border itself is: ╰─ <content> ──╯
    // ╰─ (3) + content + ──╯ (4) = 7 overhead
    let overhead = vis("╰─ ") + vis(" ──╯"); // 3 + 4 = 7

    // Narrower than the frame itself: no level fits (the no-color local-only level and the
    // ultimate fallback would both emit the 7-cell frame), so draw a plain rule of exactly
    // panel_w dashes and never overflow.
    if panel_w < overhead {
        return tcs("─".repeat(panel_w), delim_c);
    }

    // Helper: assemble the full border from measured components.
    // Layout: ╰─ <left content> <fill dashes> <right> ──╯
    // `right` (version chip) is right-aligned; the middle fill is reserved空間 —
    // 未來 indicator(hangar-bridge / autopilot 之類)就 slot 在這段 fill。
    let assemble = |left: String, left_vis: usize, right: String, right_vis: usize| -> String {
        let fill = panel_w.saturating_sub(overhead + left_vis + right_vis);
        format!(
            "{}{}{}{}{}",
            tc("╰─ ", delim_c),
            left,
            tcs("─".repeat(fill), delim_c),
            right,
            tc(" ──╯", delim_c),
        )
    };

    // 版號做成右對齊段:前置一個空格與 fill dashes 分隔。`v`==0(無版號/被丟)→ 空。
    let ver_right = |s: &str, v: usize| -> (String, usize) {
        if v > 0 {
            (format!(" {s}"), 1 + v)
        } else {
            (String::new(), 0)
        }
    };

    // ─── Degradation ladder ─────────────────────────────────────────────
    // Each level assembles a candidate and checks if it fits.
    // Returns early with the first level that fits.

    if no_color {
        // No-color path: simpler candidates (no glyphs, label:word form)
        let (local_seg, local_vis) = make_local_seg_nocolor();
        let (central_seg, central_vis) = make_central_seg_nocolor(&central_status_full);
        let has_central = !matches!(brain.central, CentralLight::Hidden) && !central_seg.is_empty();

        // Level 1/2 (merged for no-color — no hint, no short code)
        // both segments + version
        {
            let seg_gap = if has_central { 3 } else { 0 }; // "   " gap
            let ver_part_vis = if ver_vis > 0 { 1 + ver_vis } else { 0 }; // " v..."
            let total_content_vis = local_vis + seg_gap + central_vis + ver_part_vis;
            if overhead + total_content_vis <= panel_w {
                let mut content = local_seg.clone();
                if has_central {
                    content.push_str("   ");
                    content.push_str(&central_seg);
                }
                let (vr, vr_vis) = ver_right(&ver_str, ver_vis);
                let result = assemble(content, total_content_vis - vr_vis, vr, vr_vis);
                if ansi_vis(&result) <= panel_w {
                    return result;
                }
            }
        }

        // Level 4: drop version chip (keep ⬆ if present)
        {
            let (eff_ver_str, eff_ver_vis) = if has_update_banner {
                (ver_str.clone(), ver_vis)
            } else {
                (String::new(), 0)
            };
            let ver_part_vis = if eff_ver_vis > 0 { 1 + eff_ver_vis } else { 0 };
            let seg_gap = if has_central { 3 } else { 0 };
            let total_content_vis = local_vis + seg_gap + central_vis + ver_part_vis;
            if overhead + total_content_vis <= panel_w {
                let mut content = local_seg.clone();
                if has_central {
                    content.push_str("   ");
                    content.push_str(&central_seg);
                }
                let (vr, vr_vis) = ver_right(&eff_ver_str, eff_ver_vis);
                let result = assemble(content, total_content_vis - vr_vis, vr, vr_vis);
                if ansi_vis(&result) <= panel_w {
                    return result;
                }
            }
        }

        // Level 6: only local
        {
            let total_content_vis = local_vis;
            let mut content = local_seg.clone();
            // If even this overflows, truncate label
            if overhead + total_content_vis > panel_w {
                let avail = panel_w.saturating_sub(overhead);
                content = content.chars().take(avail).collect::<String>();
            }
            return assemble(
                content,
                total_content_vis.min(panel_w.saturating_sub(overhead)),
                String::new(),
                0,
            );
        }
    }

    // Colored path:
    let (local_seg_full, local_vis_full) = make_local_seg_colored(false);
    let (central_seg_full, central_vis_full) = make_central_seg_colored(&central_status_full);
    let (central_seg_short, central_vis_short) = make_central_seg_colored(central_status_short);
    let (central_seg_glyph, central_vis_glyph) = make_central_seg_glyph_only_colored();
    let (local_seg_glyph, local_vis_glyph) = make_local_seg_glyph_only_colored();
    let has_central =
        !matches!(brain.central, CentralLight::Hidden) && !central_seg_full.is_empty();
    let hint_colored = tc(hint_text, BRAIN_HINT);
    let hint_vis = vis(hint_text);

    // Level 1: full (both segments + hint + version)
    {
        let seg_gap = if has_central { 3 } else { 0 };
        let hint_part_vis = if show_hint { 3 + hint_vis } else { 0 }; // "   → doctor"
        let ver_part_vis = if ver_vis > 0 { 1 + ver_vis } else { 0 };
        let total = local_vis_full + seg_gap + central_vis_full + hint_part_vis + ver_part_vis;
        if overhead + total <= panel_w {
            let mut content = local_seg_full.clone();
            if has_central {
                content.push_str("   ");
                content.push_str(&central_seg_full);
            }
            if show_hint {
                content.push_str("   ");
                content.push_str(&hint_colored);
            }
            let (vr, vr_vis) = ver_right(&ver_str, ver_vis);
            let result = assemble(content, total - vr_vis, vr, vr_vis);
            if ansi_vis(&result) <= panel_w {
                return result;
            }
        }
    }

    // Level 2: drop hint
    {
        let seg_gap = if has_central { 3 } else { 0 };
        let ver_part_vis = if ver_vis > 0 { 1 + ver_vis } else { 0 };
        let total = local_vis_full + seg_gap + central_vis_full + ver_part_vis;
        if overhead + total <= panel_w {
            let mut content = local_seg_full.clone();
            if has_central {
                content.push_str("   ");
                content.push_str(&central_seg_full);
            }
            let (vr, vr_vis) = ver_right(&ver_str, ver_vis);
            let result = assemble(content, total - vr_vis, vr, vr_vis);
            if ansi_vis(&result) <= panel_w {
                return result;
            }
        }
    }

    // Level 3: central word → short code (colored only)
    {
        let seg_gap = if has_central { 3 } else { 0 };
        let ver_part_vis = if ver_vis > 0 { 1 + ver_vis } else { 0 };
        let total = local_vis_full + seg_gap + central_vis_short + ver_part_vis;
        if overhead + total <= panel_w {
            let mut content = local_seg_full.clone();
            if has_central {
                content.push_str("   ");
                content.push_str(&central_seg_short);
            }
            let (vr, vr_vis) = ver_right(&ver_str, ver_vis);
            let result = assemble(content, total - vr_vis, vr, vr_vis);
            if ansi_vis(&result) <= panel_w {
                return result;
            }
        }
    }

    // Level 4: drop version chip (keep ⬆ if present)
    {
        let (eff_ver_str, eff_ver_vis) = if has_update_banner {
            (ver_str.clone(), ver_vis)
        } else {
            (String::new(), 0)
        };
        let seg_gap = if has_central { 3 } else { 0 };
        let ver_part_vis = if eff_ver_vis > 0 { 1 + eff_ver_vis } else { 0 };
        let total = local_vis_full + seg_gap + central_vis_short + ver_part_vis;
        if overhead + total <= panel_w {
            let mut content = local_seg_full.clone();
            if has_central {
                content.push_str("   ");
                content.push_str(&central_seg_short);
            }
            let (vr, vr_vis) = ver_right(&eff_ver_str, eff_ver_vis);
            let result = assemble(content, total - vr_vis, vr, vr_vis);
            if ansi_vis(&result) <= panel_w {
                return result;
            }
        }
    }

    // Level 5: central pure glyph, no word
    {
        let seg_gap = if has_central { 3 } else { 0 };
        let (eff_ver_str, eff_ver_vis) = if has_update_banner {
            (ver_str.clone(), ver_vis)
        } else {
            (String::new(), 0)
        };
        let ver_part_vis = if eff_ver_vis > 0 { 1 + eff_ver_vis } else { 0 };
        let total = local_vis_full + seg_gap + central_vis_glyph + ver_part_vis;
        if overhead + total <= panel_w {
            let mut content = local_seg_full.clone();
            if has_central {
                content.push_str("   ");
                content.push_str(&central_seg_glyph);
            }
            let (vr, vr_vis) = ver_right(&eff_ver_str, eff_ver_vis);
            let result = assemble(content, total - vr_vis, vr, vr_vis);
            if ansi_vis(&result) <= panel_w {
                return result;
            }
        }
    }

    // Level 6: local segment only (glyph-only variant if still too wide)
    {
        let total = local_vis_full;
        if overhead + total <= panel_w {
            return assemble(local_seg_full, total, String::new(), 0);
        }
    }
    {
        let total = local_vis_glyph;
        if overhead + total <= panel_w {
            return assemble(local_seg_glyph, total, String::new(), 0);
        }
    }

    // Ultimate fallback: just the border frame
    assemble(String::new(), 0, String::new(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serialize env mutations across tests to avoid race conditions.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── forge-stats heartbeat → compact form ─────────────────────────────

    const HB_SID: &str = "93196c52-25cb-47ca-821c-cec391832eed";
    const HB_NOW: i64 = 1_793_400_000_000;

    fn write_heartbeat(base: &std::path::Path, file_sid: &str, body: &str) {
        let dir = base.join("codeforge").join("mod-alive");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{file_sid}.json")), body).unwrap();
    }

    fn heartbeat(sid: &str, written_at_ms: i64, band: bool) -> String {
        serde_json::json!({
            "schema": "codeforge.mod-alive/1",
            "session_id": sid,
            "written_at_ms": written_at_ms,
            "layout": if band { "band" } else { "pane" },
        })
        .to_string()
    }

    #[test]
    fn heartbeat_fresh_band_compacts_even_with_the_forge_tab_closed() {
        let dir = tempfile::tempdir().unwrap();
        write_heartbeat(dir.path(), HB_SID, &heartbeat(HB_SID, HB_NOW - 1_500, true));
        assert!(read_mod_heartbeat(dir.path(), HB_SID, HB_NOW));
    }

    #[test]
    fn heartbeat_band_not_seen_keeps_the_full_statusline() {
        let dir = tempfile::tempdir().unwrap();
        let with = |seen: serde_json::Value| {
            let mut v: serde_json::Value =
                serde_json::from_str(&heartbeat(HB_SID, HB_NOW - 1_500, true)).unwrap();
            v["band_seen"] = seen;
            v.to_string()
        };
        // B34: the band layout chosen, but the band not drawn lately (autopilot's O4 shell above it)
        write_heartbeat(dir.path(), HB_SID, &with(serde_json::json!(false)));
        assert!(!read_mod_heartbeat(dir.path(), HB_SID, HB_NOW));
        // drawn lately
        write_heartbeat(dir.path(), HB_SID, &with(serde_json::json!(true)));
        assert!(read_mod_heartbeat(dir.path(), HB_SID, HB_NOW));
        // an older forge-stats writes no band_seen: it compacts as before (the fresh-band test above)
    }

    #[test]
    fn heartbeat_rejects_every_other_case() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        // no file
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // stale (exactly 10 s old)
        write_heartbeat(base, HB_SID, &heartbeat(HB_SID, HB_NOW - 10_000, true));
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // from the future
        write_heartbeat(base, HB_SID, &heartbeat(HB_SID, HB_NOW + 1, true));
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // pane layout
        write_heartbeat(base, HB_SID, &heartbeat(HB_SID, HB_NOW - 100, false));
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // another session's id inside this session's file
        write_heartbeat(base, HB_SID, &heartbeat("someone-else", HB_NOW - 100, true));
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // wrong schema
        let other = heartbeat(HB_SID, HB_NOW - 100, true).replace("mod-alive/1", "mod-alive/9");
        write_heartbeat(base, HB_SID, &other);
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // half-written / not JSON
        write_heartbeat(base, HB_SID, "{\"schema\":\"codeforge.mod");
        assert!(!read_mod_heartbeat(base, HB_SID, HB_NOW));
        // empty stdin session id never matches
        assert!(!read_mod_heartbeat(base, "", HB_NOW));
    }

    #[test]
    fn heartbeat_extreme_timestamps_never_compact_or_panic() {
        let dir = tempfile::tempdir().unwrap();
        for w in [i64::MIN, i64::MAX] {
            write_heartbeat(dir.path(), HB_SID, &heartbeat(HB_SID, w, true));
            assert!(
                !read_mod_heartbeat(dir.path(), HB_SID, HB_NOW),
                "written_at_ms {w}"
            );
        }
    }

    #[test]
    fn heartbeat_found_under_sanitized_name() {
        let dir = tempfile::tempdir().unwrap();
        let raw = "a/b:c d";
        write_heartbeat(dir.path(), "a_b_c_d", &heartbeat(raw, HB_NOW - 100, true));
        assert!(read_mod_heartbeat(dir.path(), raw, HB_NOW));
    }

    #[test]
    fn compact_form_is_two_lines_without_frame_or_hint() {
        let data = StatusInput {
            model: Some("Opus 5.5".into()),
            context_pct: Some(0.45),
            five_hour_pct: Some(0.32),
            ..Default::default()
        };
        let mut buf = Vec::new();
        render_compact(&mut buf, &data, 100).unwrap();
        let text = strip_ansi(&String::from_utf8(buf).unwrap());
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines.len() <= 2,
            "compact form must be at most 2 lines: {lines:?}"
        );
        assert!(
            !text.contains('╭') && !text.contains('│'),
            "no box frame: {text}"
        );
        // the hint itself, not the word: the line also shows the git branch, which may contain "adopt"
        assert!(
            !text.contains("→ adopt") && !text.contains("→ codeforge adopt"),
            "no adopt hint: {text}"
        );
        assert!(text.contains("ctx"), "usage row kept: {text}");
    }

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
        render_compact(&mut buf, data, width).unwrap();
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
        assert!(compact_lines(&data, 80)[0].ends_with("v2.1.318"));
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
        let below = compact_lines(&data, need - 1);
        assert_eq!(below.len(), 2, "one column short of {need}: {below:?}");
        for l in &below {
            assert!(vis(l) < need, "{l:?}");
        }
        // narrow: the two-line form as before; a tiny terminal still draws
        assert_eq!(compact_lines(&data, 80).len(), 2);
        assert_eq!(compact_lines(&data, 0).len(), 2);
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

    fn bh(
        l: crate::mnemos::health::LocalLight,
        c: crate::mnemos::health::CentralLight,
    ) -> crate::mnemos::health::BrainHealth {
        crate::mnemos::health::BrainHealth {
            local: l,
            central: c,
        }
    }

    #[test]
    fn bottom_border_wide_shows_both() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        // Use no_color=true so output is predictable regardless of terminal
        let s = bottom_border(
            80,
            DELIM,
            String::new(),
            0,
            &bh(LocalLight::Active, CentralLight::Ok),
            true,
        );
        assert!(
            s.contains("memory"),
            "should contain 'memory', got: {:?}",
            s
        );
        assert!(
            s.contains("mnemos"),
            "should contain 'mnemos', got: {:?}",
            s
        );
        assert!(
            ansi_vis(&s) <= 80,
            "不可溢出 panel_w=80，actual={}",
            ansi_vis(&s)
        );
    }

    #[test]
    fn bottom_border_hidden_central_omits_mnemos() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        let s = bottom_border(
            80,
            DELIM,
            String::new(),
            0,
            &bh(LocalLight::Active, CentralLight::Hidden),
            true,
        );
        assert!(s.contains("memory"), "should contain 'memory'");
        assert!(
            !s.contains("mnemos"),
            "Hidden central should not show 'mnemos', got: {:?}",
            s
        );
    }

    #[test]
    fn bottom_border_narrow_no_overflow() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        for w in [20usize, 30, 40, 60] {
            let s = bottom_border(
                w,
                DELIM,
                "v0.0.5".into(),
                6,
                &bh(LocalLight::Active, CentralLight::Degraded),
                true,
            );
            let actual = ansi_vis(&s);
            assert!(actual <= w, "w={w} 溢出：actual={actual}, content={:?}", s);
        }
    }

    /// Every width from 0 to past the widest level, for every light combination, with and
    /// without colour and version chip — so each ladder breakpoint is hit exactly and at ±1.
    #[test]
    fn bottom_border_never_overflows_at_any_width() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        let locals = [LocalLight::Hidden, LocalLight::Active, LocalLight::Empty];
        let centrals = [
            CentralLight::Hidden,
            CentralLight::Ok,
            CentralLight::Degraded,
            CentralLight::Offline,
            CentralLight::Pending,
        ];
        for no_color in [true, false] {
            for (ver, ver_vis) in [("v0.0.5", 6usize), ("", 0)] {
                for &l in &locals {
                    for &c in &centrals {
                        for w in 0..=120usize {
                            let s =
                                bottom_border(w, DELIM, ver.into(), ver_vis, &bh(l, c), no_color);
                            let actual = ansi_vis(&s);
                            assert!(
                                actual <= w,
                                "w={w} no_color={no_color} ver={ver:?} local={l:?} central={c:?} \
                                 溢出：actual={actual}, content={s:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bottom_border_colored_wide_shows_both() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        // Test with colors forced on
        let _g = ENV_LOCK.lock().unwrap();
        owo_colors::set_override(true);
        let s = bottom_border(
            80,
            DELIM,
            String::new(),
            0,
            &bh(LocalLight::Active, CentralLight::Ok),
            false, // colored path
        );
        // strip ANSI to check content
        let plain = strip_ansi(&s);
        assert!(
            plain.contains("memory"),
            "colored path should contain 'memory', plain={:?}",
            plain
        );
        assert!(
            plain.contains("mnemos"),
            "colored path should contain 'mnemos', plain={:?}",
            plain
        );
        assert!(ansi_vis(&s) <= 80, "不可溢出 panel_w=80");
        owo_colors::unset_override();
    }

    #[test]
    fn bottom_border_local_empty_word() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        // LocalLight::Empty means store exists but active=0 — must show "empty", NOT "offline"
        let s = bottom_border(
            80,
            DELIM,
            String::new(),
            0,
            &bh(LocalLight::Empty, CentralLight::Hidden),
            true, // no_color=true for deterministic plain output
        );
        assert!(
            s.contains("empty"),
            "LocalLight::Empty 應輸出 'empty'，got: {:?}",
            s
        );
        assert!(
            !s.contains("offline"),
            "LocalLight::Empty 不應輸出 'offline'，got: {:?}",
            s
        );
    }

    /// hue_rgb 純函式:三原色錨點 + 越界折回(rem_euclid)。
    #[test]
    fn hue_rgb_primary_anchors() {
        assert_eq!(hue_rgb(0.0), (255, 0, 0)); // 紅
        assert_eq!(hue_rgb(120.0), (0, 255, 0)); // 綠
        assert_eq!(hue_rgb(240.0), (0, 0, 255)); // 藍
        assert_eq!(hue_rgb(360.0), (255, 0, 0)); // 折回紅
        assert_eq!(hue_rgb(-120.0), (0, 0, 255)); // 負值折回藍
    }

    /// rainbow:NO_COLOR 下回純文字;有色時含 ANSI 但 vis() 寬度不變(只改色不改字)。
    #[test]
    fn rainbow_respects_no_color_and_preserves_width() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("NO_COLOR", "1");
        assert_eq!(rainbow("mnemos"), "mnemos");
        std::env::remove_var("NO_COLOR");
        let colored = rainbow("mnemos");
        assert!(
            colored.contains('\u{1b}'),
            "有色時應含 ANSI escape: {colored:?}"
        );
        // 剝掉 ESC...m 序列後應還原成原字 → 彩虹只改色、不動字元(layout 不變)。
        let mut stripped = String::new();
        let mut chars = colored.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for d in chars.by_ref() {
                    if d == 'm' {
                        break;
                    }
                }
            } else {
                stripped.push(c);
            }
        }
        assert_eq!(stripped, "mnemos", "剝色後應還原原字");
    }

    /// 版號右對齊:寬版有版號時,strip 後是 `…mnemos… ─fill─ v0.0.5 ──╯`(版號在 fill 之後、
    /// 靠右),而非黏在 mnemos 左側。防「版號回到左邊」的 layout regression。
    #[test]
    fn bottom_border_version_right_aligned() {
        use crate::mnemos::health::{CentralLight, LocalLight};
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("NO_COLOR", "1"); // 純文字、好斷言位置
        let s = bottom_border(
            80,
            DELIM,
            "v0.0.5".to_string(),
            6,
            &bh(LocalLight::Active, CentralLight::Ok),
            true,
        );
        std::env::remove_var("NO_COLOR");
        assert!(
            s.trim_end().ends_with("v0.0.5 ──╯"),
            "版號應靠右收尾,got: {s:?}"
        );
        let mpos = s.find("mnemos").expect("應含 mnemos");
        let vpos = s.find("v0.0.5").expect("應含版號");
        assert!(vpos > mpos, "版號應在 mnemos 之後,got: {s:?}");
        assert!(
            s[mpos..vpos].contains('─'),
            "mnemos 與版號之間應有 fill dash(證明版號在 fill 右側),got: {s:?}"
        );
    }

    // ── G36 per-account quota ───────────────────────────────────────────────

    fn quota_snap(alias: &str, pct: f64, resets_at: i64) -> crate::usage::quota::Snapshot {
        crate::usage::quota::Snapshot {
            alias: alias.into(),
            root: None,
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
    fn others_segment_fits_its_room_and_drops_from_the_end() {
        let others = [
            quota_snap("acct-b", 0.81, Q_NOW + 3 * 86400 + 7200),
            quota_snap("acct-c", 0.12, Q_NOW + 4 * 3600),
        ];
        let full = strip_ansi(&others_segment(&others, Q_NOW, 200, false));
        assert_eq!(full, " · acct-b 81% 3d2h · acct-c 12% 4h0m");
        // every room: never wider than given
        for room in 0..60 {
            let got = strip_ansi(&others_segment(&others, Q_NOW, room, false));
            assert!(vis(&got) <= room, "room {room}: {got:?}");
        }
        // room for the first with its countdown and the second without
        assert_eq!(
            strip_ansi(&others_segment(&others, Q_NOW, 31, false)),
            " · acct-b 81% 3d2h · acct-c 12%"
        );
        // the first without its countdown only
        assert_eq!(
            strip_ansi(&others_segment(&others, Q_NOW, 14, false)),
            " · acct-b 81%"
        );
        // no room at all
        assert_eq!(others_segment(&others, Q_NOW, 12, false), "");
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
        for avail in [0usize, 12, 14, 31, 40, 80, 200] {
            for yellow in [false, true] {
                assert_eq!(
                    quota_segment(&data, Q_NOW, avail, yellow),
                    others_segment(&others, Q_NOW, avail, yellow),
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
        let seg = strip_ansi(&quota_segment(&data, Q_NOW, 200, false));
        assert!(seg.starts_with(" · "), "{seg:?}");
        assert!(
            seg.contains("claude:work") && seg.contains("62%") && seg.contains("◷ 2h5m"),
            "{seg:?}"
        );
        assert!(!seg.contains("acct-b"), "{seg:?}");
        // fits its room at every width, never panics, never a bare separator
        for avail in 0..120 {
            let s = strip_ansi(&quota_segment(&data, Q_NOW, avail, false));
            assert!(s.is_empty() || vis(&s) <= avail, "avail {avail}: {s:?}");
            assert_ne!(s.trim(), "·", "avail {avail}");
        }
        // narrowest form: alias and value only
        let tight = strip_ansi(&quota_segment(&data, Q_NOW, 3 + vis("work 62%"), false));
        assert_eq!(tight, " · work 62%");
    }

    #[test]
    fn quota_segment_with_an_empty_mirror_draws_nothing() {
        let data = StatusInput {
            others: vec![quota_snap("acct-b", 0.81, Q_NOW + 7200)],
            mirror: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(quota_segment(&data, Q_NOW, 200, false), "");
    }

    #[test]
    fn others_segment_one_yellow_per_row_red_at_95() {
        // other tests set NO_COLOR under ENV_LOCK; colour assertions must not race them
        let _g = ENV_LOCK.lock().unwrap();
        if !color_enabled() {
            return; // NO_COLOR set for this test run: nothing coloured to check
        }
        let esc = |(r, g, b): Rgb| format!("\x1b[38;2;{r};{g};{b}m");
        let others = [
            quota_snap("acct-r", 0.97, Q_NOW + 60),
            quota_snap("acct-y", 0.88, Q_NOW + 60),
            quota_snap("acct-z", 0.84, Q_NOW + 60),
        ];
        let seg = others_segment(&others, Q_NOW, 200, false);
        assert_eq!(seg.matches(&esc(BAR_MID)).count(), 1, "{seg:?}");
        assert_eq!(seg.matches(&esc(BAR_HIGH)).count(), 1, "{seg:?}");
        assert!(
            seg.contains("\x1b[1m"),
            "the second 80 % account is bold: {seg:?}"
        );
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
        let got = record_quota(&mut none, &dir, "acct-a", "1a2b3c4d", "sess-1", now_ms);
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
        let got = record_quota(&mut data, &dir, "acct-a", "1a2b3c4d", "sess-1", now_ms);
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
            "acct-a",
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
        // a missing dir is no error
        assert!(record_quota(
            &mut none,
            &base.path().join("nope"),
            "acct-a",
            "1a2b3c4d",
            "sess-1",
            now_ms
        )
        .is_empty());
    }

    #[test]
    fn both_layouts_show_the_other_accounts() {
        let data = StatusInput {
            model: Some("Opus 5.5".into()),
            seven_day_pct: Some(0.75),
            seven_day_resets_at: Some(chrono::Utc::now().timestamp() + 86400),
            others: vec![quota_snap(
                "acct-b",
                0.81,
                chrono::Utc::now().timestamp() + 7200,
            )],
            ..Default::default()
        };
        let mut buf = Vec::new();
        render_compact(&mut buf, &data, 100).unwrap();
        let text = strip_ansi(&String::from_utf8(buf).unwrap());
        assert!(text.contains("· acct-b 81%"), "{text}");
        for line in text.lines() {
            assert!(vis(line) <= 100, "{line:?}");
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
                others: vec![quota_snap("acct-b", 0.85, now + 7200)],
                ..Default::default()
            };
            let mut buf = Vec::new();
            render_compact(&mut buf, &data, 100).unwrap();
            let text = String::from_utf8(buf).unwrap();
            text.lines()
                .find(|l| l.contains("acct-b"))
                .unwrap_or_default()
                .to_string()
        };
        let other_part = |r: &str| r[r.find("acct-b").unwrap()..].to_string();
        // own 7d in the yellow band (50-80 %): the 85 % account is bold, not a second yellow
        let r = row1(0.6);
        assert!(
            r[..r.find("acct-b").unwrap()].contains(&esc),
            "own bar is yellow: {r:?}"
        );
        assert!(!other_part(&r).contains(&esc), "{r:?}");
        assert!(other_part(&r).contains("\x1b[1m"), "{r:?}");
        // own bars green: the other account takes the yellow
        let r = row1(0.2);
        assert!(other_part(&r).contains(&esc), "{r:?}");
        assert!(own_bars_yellow(&StatusInput {
            context_pct: Some(0.7),
            ..Default::default()
        }));
        assert!(!own_bars_yellow(&StatusInput {
            context_pct: Some(0.9),
            ..Default::default()
        }));
    }
}
