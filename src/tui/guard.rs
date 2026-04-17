//! RAII terminal guard — Phase 2c.
//!
//! `TerminalGuard::new()` enables raw mode + enters the alt-screen +
//! hides the cursor. `Drop` reverses all three, even on panic unwind.
//! This is the single source of truth for terminal state — any code path
//! that leaves the guard alive keeps the pristine-on-exit guarantee.
//!
//! **Design invariant**: after the guard drops, the user's scrollback is
//! untouched and their normal prompt is visible. No stray escape
//! sequences, no lingering raw mode.

use anyhow::Result;
use crossterm::{
    cursor,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    QueueableCommand,
};
use std::io::{self, Write};

/// Entering the TUI: acquires terminal state. Dropping exits the TUI.
///
/// Only one guard should be alive at a time per process; `new()` will
/// happily enter raw mode twice, but the nested `Drop`s race to restore.
/// The CLI guarantees single-instance by only constructing the guard at
/// the top of `codeforge tui`.
pub struct TerminalGuard {
    /// Sticky flag — guard is responsible for restore as long as this is
    /// true. Kept so tests that construct the guard without hitting a
    /// real terminal don't panic on Drop.
    active: bool,
}

impl TerminalGuard {
    /// Enter alt-screen + raw mode + hide cursor. On failure, partial
    /// state is rolled back before the error propagates.
    pub fn new() -> Result<Self> {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode()?;
        // If queueing any of these fails, disable raw mode before returning
        // so we don't leave the terminal in a mixed state.
        if let Err(e) = (|| -> io::Result<()> {
            stdout.queue(EnterAlternateScreen)?;
            stdout.queue(cursor::Hide)?;
            stdout.flush()
        })() {
            let _ = terminal::disable_raw_mode();
            return Err(e.into());
        }
        Ok(Self { active: true })
    }

    /// Relinquish ownership of terminal state without running the Drop
    /// restore. Intended for tests that constructed a guard in a non-tty
    /// environment and want to skip the restore entirely.
    #[cfg(test)]
    pub(crate) fn detach(mut self) {
        self.active = false;
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        // Best-effort restore. Ignore errors — if stdout is closed or the
        // terminal is already gone, there's nothing useful we can do, and
        // a panic during unwind would abort the process.
        let mut stdout = io::stdout();
        let _ = stdout.queue(cursor::Show);
        let _ = stdout.queue(LeaveAlternateScreen);
        let _ = stdout.flush();
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the `detach` path — guard without restore, no panics.
    /// Real terminal interaction is covered by the e2e smoke test in
    /// `cli::tui`; unit tests must not touch the real stdout.
    #[test]
    fn detach_skips_restore_without_panic() {
        // Construct a guard without going through `new()` so we don't
        // mutate the real terminal in CI / non-tty envs.
        let g = TerminalGuard { active: true };
        g.detach();
        // Dropped here — active=false path should be a no-op.
    }

    #[test]
    fn inactive_guard_drop_is_no_op() {
        let g = TerminalGuard { active: false };
        drop(g);
    }
}
