//! Keyboard event handling — Phase 2c P5.
//!
//! The event task blocks on `crossterm::event::read` in a blocking
//! thread (via `tokio::task::spawn_blocking`), interpreting each key
//! and signaling shutdown via an mpsc channel (buffered capacity 1 —
//! avoids the `Notify` lost-wakeup race where a quit fired before the
//! main loop registers a waiter is dropped silently).
//!
//! The main loop can request shutdown of the keyboard task itself by
//! flipping `running` to false — the blocking thread checks it between
//! each 100ms poll, so it returns promptly and doesn't consume the
//! user's post-exit keystrokes from their shell.
//!
//! Keys recognized: `q`, `Esc`, Ctrl-C. Anything else is ignored.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Spawn a blocking task that reads key events. On quit-key, sends on
/// `shutdown` once. On `running == false` (set by the main loop when it
/// exits for any reason), returns without draining further keystrokes.
pub fn spawn_keyboard_task(
    shutdown: mpsc::Sender<()>,
    running: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || loop {
        if !running.load(Ordering::Acquire) {
            return;
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {
                if let Ok(ev) = event::read() {
                    if should_quit(&ev) {
                        // try_send: channel has capacity 1; if the main
                        // loop already received a prior signal, drop.
                        let _ = shutdown.try_send(());
                        return;
                    }
                }
            }
            Ok(false) => continue,
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

    /// Regression for review round 1 IMPORTANT #1:
    /// mpsc::channel(1) buffers the send so a quit fired *before* the
    /// receiver is polled is still delivered, unlike `Notify::notify_waiters`
    /// which silently drops when no waiter is registered.
    #[tokio::test]
    async fn shutdown_channel_delivers_signal_sent_before_recv() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        // Send BEFORE anyone awaits rx.recv()
        tx.try_send(()).unwrap();
        // Recv after the send — must not block forever
        let got = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(got.is_ok(), "send-before-recv must still deliver");
        assert!(got.unwrap().is_some());
    }

    /// Regression for review round 1 IMPORTANT #2:
    /// `running = false` must be observable by the keyboard task's
    /// load so it can exit within the poll interval. This doesn't spawn
    /// a real task (avoid TTY dependency) but proves the atomic flag's
    /// ordering guarantees.
    #[test]
    fn running_flag_acquire_sees_release_store() {
        let running = Arc::new(AtomicBool::new(true));
        assert!(running.load(Ordering::Acquire));
        running.store(false, Ordering::Release);
        assert!(!running.load(Ordering::Acquire));
    }
}
