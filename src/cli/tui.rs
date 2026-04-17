//! `codeforge tui` — Phase 2c entry point.
//!
//! Builds a current-thread tokio runtime (same pattern as `daemon start`)
//! and delegates to `crate::tui::run`. The terminal guard lives inside
//! the async fn so its Drop runs when the future completes — clean or
//! otherwise — restoring the user's terminal.

use anyhow::{Context, Result};

use crate::db::Context as DbContext;

pub fn run(ctx: &DbContext) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("建立 tokio runtime 失敗")?;
    rt.block_on(crate::tui::run(ctx))
}
