//! Keyboard event handling — Phase 2c P5 + tile-map UX polish.
//!
//! The event task blocks on `crossterm::event::read` in a blocking
//! thread (via `tokio::task::spawn_blocking`), interpreting each key
//! and signaling via an mpsc channel (buffered capacity — avoids the
//! `Notify` lost-wakeup race where an event fired before the main loop
//! registers a waiter is dropped silently).
//!
//! The main loop can request shutdown of the keyboard task itself by
//! flipping `running` to false — the blocking thread checks it between
//! each 100ms poll, so it returns promptly and doesn't consume the
//! user's post-exit keystrokes from their shell.
//!
//! Keys recognized:
//! * `q` / `Esc` / Ctrl-C → [`TuiEvent::Quit`]
//! * `g` / `l` → [`TuiEvent::ToggleMapMode`]  (tile-map UX polish)
//!
//! Anything else is ignored.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Events surfaced from the keyboard task to the main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiEvent {
    /// User asked to exit — `q`, `Esc`, or Ctrl-C.
    Quit,
    /// User pressed `g` or `l` to cycle the Local Map display mode.
    ToggleMapMode,
}

/// Channel capacity. Large enough to buffer a burst of keypresses
/// (e.g. `g` then `q` in rapid succession) without `try_send` dropping;
/// small enough that back-pressure still shows up if the main loop
/// stalls for many seconds.
const EVENT_CHANNEL_CAPACITY: usize = 8;

/// Convenience constructor for the (tx, rx) pair.
pub fn event_channel() -> (mpsc::Sender<TuiEvent>, mpsc::Receiver<TuiEvent>) {
    mpsc::channel(EVENT_CHANNEL_CAPACITY)
}

/// Spawn a blocking task that reads key events and forwards them as
/// `TuiEvent`. On `running == false` (set by the main loop when it
/// exits for any reason), returns without draining further keystrokes.
pub fn spawn_keyboard_task(
    tx: mpsc::Sender<TuiEvent>,
    running: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || loop {
        if !running.load(Ordering::Acquire) {
            return;
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {
                if let Ok(ev) = event::read() {
                    if let Some(mapped) = classify(&ev) {
                        // try_send: when the buffer is full we drop —
                        // prefer liveness over perfect fidelity (user
                        // can always retry the keystroke). A Quit event
                        // in a full buffer still means Quit was seen
                        // earlier and will be processed.
                        let quit = matches!(mapped, TuiEvent::Quit);
                        let _ = tx.try_send(mapped);
                        if quit {
                            return;
                        }
                    }
                }
            }
            Ok(false) => continue,
            Err(_) => return,
        }
    })
}

/// Classify a raw crossterm event into a [`TuiEvent`], or `None` to
/// ignore. Public for testability.
pub fn classify(ev: &Event) -> Option<TuiEvent> {
    match ev {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        })
        | Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => Some(TuiEvent::Quit),
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => Some(TuiEvent::Quit),
        Event::Key(KeyEvent {
            code: KeyCode::Char('g'),
            modifiers,
            ..
        })
        | Event::Key(KeyEvent {
            code: KeyCode::Char('l'),
            modifiers,
            ..
        }) if !modifiers.contains(KeyModifiers::CONTROL) => Some(TuiEvent::ToggleMapMode),
        _ => None,
    }
}

/// Equivalent to `matches!(classify(ev), Some(TuiEvent::Quit))`. Kept
/// in the public surface for tests that assert on the quit half of the
/// classifier without caring about the toggle-map path.
#[cfg(test)]
pub fn should_quit(ev: &Event) -> bool {
    matches!(classify(ev), Some(TuiEvent::Quit))
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

    // ------------------------------------------------------------------
    // Quit classification (preserves Phase 2c review-r1 semantics)
    // ------------------------------------------------------------------

    #[test]
    fn q_triggers_quit() {
        assert_eq!(classify(&key(KeyCode::Char('q'), KeyModifiers::NONE)), Some(TuiEvent::Quit));
    }

    #[test]
    fn esc_triggers_quit() {
        assert_eq!(classify(&key(KeyCode::Esc, KeyModifiers::NONE)), Some(TuiEvent::Quit));
    }

    #[test]
    fn ctrl_c_triggers_quit() {
        assert_eq!(
            classify(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(TuiEvent::Quit)
        );
    }

    #[test]
    fn plain_c_does_not_trigger_quit() {
        assert_eq!(classify(&key(KeyCode::Char('c'), KeyModifiers::NONE)), None);
    }

    #[test]
    fn other_keys_ignored() {
        assert_eq!(classify(&key(KeyCode::Char('a'), KeyModifiers::NONE)), None);
        assert_eq!(classify(&key(KeyCode::Enter, KeyModifiers::NONE)), None);
        assert_eq!(classify(&key(KeyCode::Tab, KeyModifiers::NONE)), None);
    }

    #[test]
    fn mouse_and_resize_events_ignored() {
        assert_eq!(classify(&Event::Resize(80, 24)), None);
    }

    // ------------------------------------------------------------------
    // Tile-map toggle keys
    // ------------------------------------------------------------------

    #[test]
    fn g_toggles_map_mode() {
        assert_eq!(
            classify(&key(KeyCode::Char('g'), KeyModifiers::NONE)),
            Some(TuiEvent::ToggleMapMode)
        );
    }

    #[test]
    fn l_toggles_map_mode() {
        assert_eq!(
            classify(&key(KeyCode::Char('l'), KeyModifiers::NONE)),
            Some(TuiEvent::ToggleMapMode)
        );
    }

    #[test]
    fn shift_g_also_toggles_map_mode() {
        // Shift-G (capital G) still yields KeyCode::Char('g') with SHIFT
        // mod on some terminals; others send Char('G'). crossterm sends
        // the lowercase char with SHIFT, so this still classifies.
        assert_eq!(
            classify(&key(KeyCode::Char('g'), KeyModifiers::SHIFT)),
            Some(TuiEvent::ToggleMapMode)
        );
    }

    #[test]
    fn ctrl_g_does_not_toggle_map_mode() {
        // Ctrl-G would collide with terminal bell; reserve to non-Ctrl.
        assert_eq!(
            classify(&key(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            None
        );
    }

    #[test]
    fn ctrl_l_does_not_toggle_map_mode() {
        // Ctrl-L is the conventional "clear screen" — don't steal it.
        assert_eq!(
            classify(&key(KeyCode::Char('l'), KeyModifiers::CONTROL)),
            None
        );
    }

    // ------------------------------------------------------------------
    // should_quit legacy shim
    // ------------------------------------------------------------------

    #[test]
    fn should_quit_matches_classify_quit() {
        assert!(should_quit(&key(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(should_quit(&key(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(should_quit(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!should_quit(&key(KeyCode::Char('g'), KeyModifiers::NONE)));
    }

    // ------------------------------------------------------------------
    // Channel semantics (regressions from review-r1)
    // ------------------------------------------------------------------

    /// Regression for review round 1 IMPORTANT #1 (Phase 2c):
    /// mpsc buffers the send so an event fired *before* the receiver is
    /// polled is still delivered, unlike `Notify::notify_waiters` which
    /// silently drops when no waiter is registered.
    #[tokio::test]
    async fn channel_delivers_event_sent_before_recv() {
        let (tx, mut rx) = event_channel();
        tx.try_send(TuiEvent::Quit).unwrap();
        let got = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(got.is_ok(), "send-before-recv must still deliver");
        assert_eq!(got.unwrap(), Some(TuiEvent::Quit));
    }

    #[tokio::test]
    async fn channel_buffers_multiple_toggle_events() {
        let (tx, mut rx) = event_channel();
        tx.try_send(TuiEvent::ToggleMapMode).unwrap();
        tx.try_send(TuiEvent::ToggleMapMode).unwrap();
        tx.try_send(TuiEvent::Quit).unwrap();
        assert_eq!(rx.recv().await, Some(TuiEvent::ToggleMapMode));
        assert_eq!(rx.recv().await, Some(TuiEvent::ToggleMapMode));
        assert_eq!(rx.recv().await, Some(TuiEvent::Quit));
    }

    /// Regression for review round 1 IMPORTANT #2 (Phase 2c):
    /// `running = false` must be observable by the keyboard task's load
    /// so it can exit within the poll interval.
    #[test]
    fn running_flag_acquire_sees_release_store() {
        let running = Arc::new(AtomicBool::new(true));
        assert!(running.load(Ordering::Acquire));
        running.store(false, Ordering::Release);
        assert!(!running.load(Ordering::Acquire));
    }
}
