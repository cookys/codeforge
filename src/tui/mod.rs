//! TUI renderer — Phase 2c.
//!
//! Entry point: [`run`] constructs a terminal guard, spawns a keyboard
//! task, and drives a 1 Hz refresh loop that rebuilds + paints a frame
//! each tick. All DB access is read-only (Two-writer rule); the daemon
//! remains the sole writer to derived state.

pub mod events;
pub mod guard;
pub mod layout;
pub mod local_map;
pub mod panels;
pub mod render;

use anyhow::Result;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::{interval, MissedTickBehavior};

use crate::db::Context;

/// Refresh interval — 1 Hz matches the pace of daemon-driven state
/// changes (daemon ticks every 60s, event_inbox overlay is cheap to
/// re-read). Also fast enough that quitting feels responsive.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Enter the TUI and run until the user presses q / Esc / Ctrl-C.
///
/// On any exit path — clean, error, or panic — the `TerminalGuard`
/// restores terminal state via its Drop impl, so the scrollback is
/// left pristine for the user.
pub async fn run(ctx: &Context) -> Result<()> {
    ctx.ensure_initialized()?;
    let _guard = guard::TerminalGuard::new()?;

    let scan_root = scan_root_for_tui();
    let cwd = std::env::current_dir().ok();

    let shutdown = Arc::new(Notify::new());
    let keyboard = events::spawn_keyboard_task(shutdown.clone());

    // Paint one frame immediately so the user doesn't stare at a blank
    // alt-screen for up to REFRESH_INTERVAL before the first tick.
    paint_once(ctx, scan_root.as_deref(), cwd.as_deref())?;

    let mut ticker = interval(REFRESH_INTERVAL);
    // First immediate tick already paid for by paint_once — skip it so
    // the next tick actually waits REFRESH_INTERVAL.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e) = paint_once(ctx, scan_root.as_deref(), cwd.as_deref()) {
                    // One bad frame shouldn't kill the TUI — log to stderr
                    // (goes to alt-screen too, will be cleared next frame).
                    eprintln!("render error: {e}");
                }
            }
            _ = shutdown.notified() => {
                keyboard.abort();
                break;
            }
        }
    }
    Ok(())
}

/// Resolve the scan root for the TUI session. Prefer `CODEFORGE_SCAN_DIR`
/// (same env var the daemon scanner honors) so the TUI shows MOBs for
/// the directory the daemon is scanning; fall back to `$PWD` so running
/// `codeforge tui` without the env var still does something sensible.
fn scan_root_for_tui() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("CODEFORGE_SCAN_DIR") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    std::env::current_dir().ok()
}

fn paint_once(
    ctx: &Context,
    scan_root: Option<&std::path::Path>,
    cwd: Option<&std::path::Path>,
) -> Result<()> {
    let conn = ctx.open_db()?;
    let (cols, rows) = crossterm::terminal::size().unwrap_or((100, 40));
    let root = scan_root.unwrap_or_else(|| std::path::Path::new("."));
    let frame = render::build_frame(&conn, root, cwd, cols, rows)?;
    let mut stdout = io::stdout();
    render::paint(&frame, &mut stdout)?;
    Ok(())
}
