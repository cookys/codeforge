//! `codeforge memory context` — local recall: rank active L1 → lean budgeted
//! index → stdout (markdown) or SessionStart hook JSON (`--hook`).
//!
//! The no-mnemos READ path, symmetric to `mnemos-cli context`. Pure ranking /
//! rendering lives in `crate::memory::recall`; this is the IO glue.

use crate::db;
use crate::memory::{l1, recall};
use anyhow::Result;
use std::io::Read;

pub fn run(ctx: &db::Context, max_tokens: usize, hook: bool) -> Result<()> {
    let store_dir = ctx.project_dir.join("store");
    let ranked = recall::rank(l1::scan_all(&store_dir).unwrap_or_default());

    if hook {
        // Hook mode: Claude Code pipes the SessionStart event JSON on stdin —
        // drain it (hooks must consume stdin), then emit our additionalContext.
        let mut buf = String::new();
        let _ = std::io::stdin().read_to_string(&mut buf);
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
