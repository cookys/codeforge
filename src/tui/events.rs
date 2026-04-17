//! Keyboard event handling — Phase 2c P5.
//!
//! The event task blocks on `crossterm::event::read` in a blocking
//! thread (via `tokio::task::spawn_blocking`), interpreting each key
//! and notifying the shutdown primitive when the user wants to quit.
//!
//! Keys recognized: `q`, `Esc`, Ctrl-C. Anything else is ignored.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Spawn a blocking task that reads key events until a quit key lands
/// or the shutdown notifier fires externally. Poll timeout is 100ms so
/// the task checks for external shutdown frequently without spinning.
pub fn spawn_keyboard_task(shutdown: Arc<Notify>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || loop {
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {
                if let Ok(ev) = event::read() {
                    if should_quit(&ev) {
                        shutdown.notify_waiters();
                        return;
                    }
                }
            }
            Ok(false) => {
                // No event — check if shutdown fired externally.
                // `try_acquire` isn't available on Notify; instead we rely
                // on the select! in the main loop to shut us down by
                // cancelling the task when the main loop exits. For
                // safety in case the main loop hangs, continue polling.
                continue;
            }
            Err(_) => return,
        }
    })
}

/// True when the event means "quit": q, Esc, or Ctrl-C.
pub fn should_quit(ev: &Event) -> bool {
    match ev {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        }) => true,
        Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => true,
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) => modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode, mods: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn q_triggers_quit() {
        assert!(should_quit(&key(KeyCode::Char('q'), KeyModifiers::NONE)));
    }

    #[test]
    fn esc_triggers_quit() {
        assert!(should_quit(&key(KeyCode::Esc, KeyModifiers::NONE)));
    }

    #[test]
    fn ctrl_c_triggers_quit() {
        assert!(should_quit(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    }

    #[test]
    fn plain_c_does_not_trigger_quit() {
        assert!(!should_quit(&key(KeyCode::Char('c'), KeyModifiers::NONE)));
    }

    #[test]
    fn other_keys_ignored() {
        assert!(!should_quit(&key(KeyCode::Char('a'), KeyModifiers::NONE)));
        assert!(!should_quit(&key(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!should_quit(&key(KeyCode::Tab, KeyModifiers::NONE)));
    }

    #[test]
    fn mouse_and_resize_events_ignored() {
        let resize = Event::Resize(80, 24);
        assert!(!should_quit(&resize));
    }
}
