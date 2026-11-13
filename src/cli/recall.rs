//! `codeforge memory context` — local recall: rank active L1 → lean budgeted
//! index → stdout (markdown) or SessionStart hook JSON (`--hook`).
//!
//! The no-mnemos READ path, symmetric to `mnemos-cli context`. Pure ranking /
//! rendering lives in `crate::memory::recall`; this is the IO glue.

use crate::db;
use crate::memory::{l1, projection, recall};
use anyhow::Result;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(ctx: &db::Context, max_tokens: usize, hook: bool) -> Result<()> {
    let store_dir = ctx.project_dir.join("store");
    // Hook mode must never break the SessionStart hook → swallow scan errors to a
    // clean no-op. Interactive mode propagates so a broken store isn't silently
    // misreported as "no knowledge".
    let entries = if hook {
        l1::scan_all(&store_dir).unwrap_or_default()
    } else {
        l1::scan_all(&store_dir)?
    };
    let ranked = recall::rank(entries);

    if hook {
        // Hook mode: Claude Code pipes the SessionStart event JSON on stdin —
        // drain it (hooks must consume stdin), then emit our additionalContext.
        let mut buf = String::new();
        let _ = std::io::stdin().read_to_string(&mut buf);
        // Token-lane pin at SessionStart: headless `claude -p` never runs the
        // statusline, so pin here (same decision + write as statusline). Must not
        // touch stdout/stderr — additionalContext stays byte-identical.
        {
            let lane = crate::usage::lane_pin::lane_pin_from_env();
            let (live_base, _) = crate::live::resolve_live_base();
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            crate::usage::lane_pin::pin_from_session_start(lane.as_ref(), &live_base, &buf, now_ms);
        }
        // Nothing to surface (no store / no active L1) → inject nothing.
        if ranked.is_empty() {
            return Ok(());
        }
        let md = recall::render_index(&ranked, "", max_tokens);
        println!("{}", recall::wrap_session_start_json(&md));
    } else {
        // Interactive: always show something (the empty note is informative).
        let md = recall::render_index(&ranked, "", max_tokens);
        println!("{md}");
    }
    Ok(())
}

/// `codeforge memory project` (B7) — materialize the ranked L1 index to
/// `.codeforge/projections/AGENTS.md` for non-Claude agents to read.
pub fn run_project(ctx: &db::Context) -> Result<()> {
    ctx.ensure_initialized()?;
    let now = chrono::Utc::now().to_rfc3339();
    let (path, count) = projection::regenerate(&ctx.project_dir, &now)?;
    println!(
        "✓ {} → {} ({count} active L1)",
        rust_i18n::t!("memory.projected"),
        path.display()
    );
    Ok(())
}
