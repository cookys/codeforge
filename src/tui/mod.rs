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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};

use crate::db::Context;
use crate::pet::session::{get_last_seen, update_last_seen, WelcomeBackSummary};
use crate::tui::layout::LayoutMode;
use crate::tui::panels::zoa::ZoaPanel;

/// Refresh interval — 1 Hz matches the pace of daemon-driven state
/// changes (daemon ticks every 60s, event_inbox overlay is cheap to
/// re-read). Also fast enough that quitting feels responsive.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Enter the TUI and run until the user presses q / Esc / Ctrl-C.
///
/// On any exit path — clean, error, or panic — the `TerminalGuard`
/// restores terminal state via its Drop impl, so the scrollback is
/// left pristine for the user.
///
/// Refuses to enter alt-screen mode when the terminal falls into the
/// `LayoutMode::Compact` breakpoint (<60 cols). Prints a one-line hint
/// to the user's scrollback instead and returns `Ok(())` — this is a
/// clean exit, not an error.
pub async fn run(ctx: &Context) -> Result<()> {
    ctx.ensure_initialized()?;

    // Size check BEFORE acquiring the terminal guard — on Compact we
    // want stderr/stdout to stay in the user's scrollback, not flash
    // an alt-screen they can't read.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((100, 40));
    if LayoutMode::from_size(cols, rows).should_abort() {
        eprintln!(
            "codeforge tui 需要至少 60 cols 的終端寬度（目前 {cols}×{rows}）。\n\
             請放大視窗後重試，或改用 `codeforge statusline` 取得精簡狀態。"
        );
        return Ok(());
    }

    let _guard = guard::TerminalGuard::new()?;

    let scan_root = scan_root_for_tui();
    let cwd = std::env::current_dir().ok();
    let mut zoa = ZoaPanel::new();

    // Phase 3d §3.1: compute welcome-back BEFORE alt-screen takeover so
    // the SQL work doesn't flash an empty frame. `update_last_seen` closes
    // the window immediately — if the user relaunches TUI seconds later,
    // no stale welcome-back will re-show.
    let welcome_lines = compute_welcome_lines(ctx).unwrap_or_default();
    // Hold the first-paint lines behind a Cell-like swap via take-on-use.
    // Avoids Arc<Mutex> for a single-threaded state transition.
    let mut welcome_once: Option<Vec<String>> = if welcome_lines.is_empty() {
        None
    } else {
        Some(welcome_lines)
    };

    // Buffered mpsc channel: capacity 1 means a quit-key pressed before
    // the main loop first polls rx.recv() is still delivered (unlike
    // Notify::notify_waiters which drops when no waiter is registered).
    let (tx, mut rx) = mpsc::channel::<()>(1);
    // Shared flag: keyboard task checks between polls so it returns
    // within ~100ms after the main loop signals exit, instead of
    // continuing to consume keystrokes from the user's shell post-TUI.
    let running = Arc::new(AtomicBool::new(true));
    let keyboard = events::spawn_keyboard_task(tx, running.clone());

    // Paint one frame immediately so the user doesn't stare at a blank
    // alt-screen for up to REFRESH_INTERVAL before the first tick.
    paint_once(
        ctx,
        scan_root.as_deref(),
        cwd.as_deref(),
        welcome_once.as_deref(),
        &mut zoa,
    )?;
    // Welcome-back lives for exactly one paint. Dropping it here means
    // subsequent ticker-driven paints show the normal combat log.
    welcome_once = None;

    let mut ticker = interval(REFRESH_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // First immediate tick already paid for by paint_once — skip it so
    // the next tick actually waits REFRESH_INTERVAL.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e) = paint_once(
                    ctx,
                    scan_root.as_deref(),
                    cwd.as_deref(),
                    welcome_once.as_deref(),
                    &mut zoa,
                ) {
                    // One bad frame shouldn't kill the TUI — log to stderr
                    // (goes to alt-screen too, will be cleared next frame).
                    eprintln!("render error: {e}");
                }
            }
            _ = rx.recv() => {
                break;
            }
        }
    }
    // Signal the keyboard task to exit at its next poll boundary so it
    // doesn't eat the user's next shell keystroke. Then await the task
    // with a bounded timeout as a belt-and-suspenders — `running` alone
    // is sufficient, but awaiting gives us a clean handoff.
    running.store(false, Ordering::Release);
    let _ = tokio::time::timeout(Duration::from_millis(200), keyboard).await;
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
    welcome_override: Option<&[String]>,
    zoa: &mut ZoaPanel,
) -> Result<()> {
    let conn = ctx.open_db()?;
    let (cols, rows) = crossterm::terminal::size().unwrap_or((100, 40));
    let root = scan_root.unwrap_or_else(|| std::path::Path::new("."));
    // Advance Zoa frame once per paint. At the current 1 Hz TUI cadence
    // the 250 ms FRAME_INTERVAL will always fire, so effectively the
    // animation runs at 1 fps — Phase 4 will add a faster repaint tier
    // for the Zoa region specifically.
    zoa.tick();
    let frame = render::build_frame(&conn, root, cwd, cols, rows, welcome_override, Some(zoa))?;
    let mut stdout = io::stdout();
    render::paint(&frame, &mut stdout)?;
    Ok(())
}

/// Compute the 2-3 line welcome-back greeting if the player's been away
/// long enough, and close the window via `update_last_seen`. Silent on
/// any DB error — a broken welcome-back must not block the TUI from
/// opening.
fn compute_welcome_lines(ctx: &Context) -> Result<Vec<String>> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let conn = ctx.open_db()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut lines: Vec<String> = Vec::new();
    if let Ok(Some(last)) = get_last_seen(&conn) {
        if let Ok(Some(summary)) = WelcomeBackSummary::build(&conn, last, now, None) {
            if summary.has_content() || summary.absence_seconds >= 3600 {
                lines = summary.render_lines();
            }
        }
    }
    let _ = update_last_seen(&conn, now);
    Ok(lines)
}
