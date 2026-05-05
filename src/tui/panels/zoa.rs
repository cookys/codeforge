//! Zoa panel — ASCII pet animation (Phase 4 prelude).
//!
//! This module lands the rendering pipeline for the Zoa pet sprite in
//! the TUI's Wide breakpoint. Only the `Idle` emotion ships with real
//! frames here — Happy / Tired / Hunting are enum stubs that return
//! Idle frames + a TODO marker, wired in Phase 4.
//!
//! ## Design
//!
//! A frame is `&'static [&'static str]`: 8 rows × 24 visible columns of
//! pure ASCII. Width is constant so `compute_regions` can hand us exactly
//! [`crate::tui::layout::ZOA_WIDTH`] columns; height auto-adapts via
//! blank-line padding / truncation to fit the panel.
//!
//! The cycling cadence is 250 ms/frame (4 Hz) — faster than the TUI's
//! 1 Hz repaint loop, so `ZoaPanel::tick` advances the frame index even
//! when state is otherwise idle. In Wide mode the main loop should
//! request a repaint at that rate; until Phase 4 wires that, the 1 Hz
//! repaint will just look like a slower animation.

use std::time::{Duration, Instant};

use super::super::styled::StyledLine;
use super::pad_to_width;

/// Fixed visible-column width of every Zoa frame. Matches
/// `crate::tui::layout::ZOA_WIDTH` so the panel fits the allocated
/// region exactly. Asserted in tests.
pub const FRAME_WIDTH: usize = 24;
/// Fixed row count of every Zoa frame. Shorter panels truncate the tail.
pub const FRAME_HEIGHT: usize = 8;
/// Wall-clock gap between frame advances. 250 ms = 4 Hz = smooth breath
/// loop without burning tick budget.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(250);

/// High-level pet mood buckets. Phase 4 will wire `Mood` → `Emotion`
/// mapping based on daemon state (XP streak, recent combat, idle time,
/// etc.). For now all variants render the Idle frame set.
///
/// Happy/Tired/Hunting are pattern-matched in `frames_for` but never
/// CONSTRUCTED in production code (only in tests, which the `--bin`
/// clippy target ignores), so the enum-level `allow(dead_code)` stays
/// until Phase 4 wires the consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Emotion {
    Idle,
    /// TODO(phase4): streak of wins / fresh loot.
    Happy,
    /// TODO(phase4): prolonged idle / no daemon ticks.
    Tired,
    /// TODO(phase4): active combat in the last tick.
    Hunting,
}

/// Return the frame strip for the given emotion. Each frame is exactly
/// [`FRAME_WIDTH`] × [`FRAME_HEIGHT`] ASCII characters (panels pad or
/// clip as needed).
///
/// Phase 2 only ships Idle frames; other variants delegate to keep the
/// rendering pipeline exercised before Phase 4 fills in real sprites.
pub fn frames_for(emotion: Emotion) -> &'static [Frame] {
    match emotion {
        Emotion::Idle | Emotion::Happy | Emotion::Tired | Emotion::Hunting => IDLE_FRAMES,
    }
}

/// One animation frame: an owned slice of rows, each row a `&'static str`
/// with exactly [`FRAME_WIDTH`] visible columns.
pub type Frame = [&'static str; FRAME_HEIGHT];

// Idle breathe-loop: open-eyes → blink → open-eyes → gentle smile.
// Every row is 24 ASCII chars; verified by test `idle_frames_are_24_wide`.
//                                1234567890123456789012 34
const IDLE_FRAMES: &[Frame] = &[
    // Frame 0 — open eyes, neutral mouth.
    [
        "       _______          ",
        "      /       \\         ",
        "     /  o   o  \\        ",
        "    |    ___    |       ",
        "     \\  \\___/  /        ",
        "      \\_______/         ",
        "         | |            ",
        "       (     )          ",
    ],
    // Frame 1 — blink (eyes become dashes).
    [
        "       _______          ",
        "      /       \\         ",
        "     /  -   -  \\        ",
        "    |    ___    |       ",
        "     \\  \\___/  /        ",
        "      \\_______/         ",
        "         | |            ",
        "       (     )          ",
    ],
    // Frame 2 — open eyes again (bounce back).
    [
        "       _______          ",
        "      /       \\         ",
        "     /  o   o  \\        ",
        "    |    ___    |       ",
        "     \\  \\___/  /        ",
        "      \\_______/         ",
        "         | |            ",
        "       (     )          ",
    ],
    // Frame 3 — small smile (mouth widens).
    [
        "       _______          ",
        "      /       \\         ",
        "     /  o   o  \\        ",
        "    |   \\___/   |       ",
        "     \\         /        ",
        "      \\_______/         ",
        "         | |            ",
        "       (     )          ",
    ],
];

/// Runtime state for the Zoa panel: which emotion is active, which
/// frame of that emotion is on screen, when the last advance happened.
#[derive(Debug, Clone)]
pub struct ZoaPanel {
    emotion: Emotion,
    frame_idx: usize,
    last_tick: Option<Instant>,
}

impl ZoaPanel {
    /// Fresh panel starting at Idle, frame 0. `Instant::now()` is not
    /// captured eagerly so tests can control timing via `tick_at`.
    pub fn new() -> Self {
        ZoaPanel {
            emotion: Emotion::Idle,
            frame_idx: 0,
            last_tick: None,
        }
    }

    /// Switch emotion. Resets the frame index so the new animation
    /// starts on frame 0 instead of resuming the prior offset into a
    /// different frame set (which could land mid-blink). Phase 4 will
    /// call this from the main loop based on mood mapping.
    #[allow(dead_code)] // Phase 4 consumer
    pub fn set_emotion(&mut self, emotion: Emotion) {
        if self.emotion != emotion {
            self.emotion = emotion;
            self.frame_idx = 0;
        }
    }

    /// Advance the frame index if `FRAME_INTERVAL` has elapsed since
    /// the last advance. Call every TUI paint; no-ops when it's too
    /// early. Returns whether the frame changed, so callers can skip
    /// a repaint when nothing moved.
    pub fn tick(&mut self) -> bool {
        self.tick_at(Instant::now())
    }

    /// Test seam for [`tick`] — lets unit tests drive timing without
    /// sleeping. Uses `checked_duration_since` so tests passing
    /// non-monotonic `Instant` values (or startup edge cases) don't
    /// panic on backward deltas; a backward clock reads as elapsed=0
    /// which keeps the animation paused until time moves forward.
    pub fn tick_at(&mut self, now: Instant) -> bool {
        let should_advance = match self.last_tick {
            None => true,
            Some(last) => now
                .checked_duration_since(last)
                .map(|d| d >= FRAME_INTERVAL)
                .unwrap_or(false),
        };
        if should_advance {
            let frames = frames_for(self.emotion);
            if !frames.is_empty() {
                self.frame_idx = (self.frame_idx + 1) % frames.len();
            }
            self.last_tick = Some(now);
        }
        should_advance
    }

    /// True when the allocated panel width is wide enough to render the
    /// sprite intact. Narrower than this we skip rendering entirely
    /// (callers fall back to blanks or omit the panel).
    pub fn should_render(&self, width: usize) -> bool {
        width >= FRAME_WIDTH
    }

    /// Render the current frame as `height` rows of `width` columns.
    /// Short panels truncate the frame's tail; tall panels pad with
    /// blank rows. Returns an empty Vec if the panel is too narrow.
    ///
    /// All spans are default-fg (Zoa's colour pipeline is Phase 4
    /// territory); B11 just wraps the existing string rows in
    /// StyledLine::plain so the render signature aligns with the new
    /// paint pipeline.
    pub fn render(&self, width: usize, height: usize) -> Vec<StyledLine> {
        if !self.should_render(width) || height == 0 {
            return Vec::new();
        }
        let frames = frames_for(self.emotion);
        let frame = &frames[self.frame_idx % frames.len()];
        let mut out: Vec<StyledLine> = Vec::with_capacity(height);
        for (i, _) in (0..height).enumerate() {
            let row = if i < FRAME_HEIGHT { frame[i] } else { "" };
            out.push(StyledLine::plain(pad_to_width(row, width)));
        }
        out
    }

    /// Current emotion — exposed for tests + future mood-driven code.
    #[allow(dead_code)] // read by Phase 4 main-loop mood diffing
    pub fn emotion(&self) -> Emotion {
        self.emotion
    }
}

impl Default for ZoaPanel {
    fn default() -> Self {
        ZoaPanel::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::vis_width;
    use super::*;

    #[test]
    fn idle_frames_are_24_wide() {
        for (i, frame) in IDLE_FRAMES.iter().enumerate() {
            assert_eq!(frame.len(), FRAME_HEIGHT, "frame {i} wrong row count");
            for (r, row) in frame.iter().enumerate() {
                assert_eq!(
                    vis_width(row),
                    FRAME_WIDTH,
                    "frame {i} row {r}: width != {FRAME_WIDTH}: {row:?}"
                );
            }
        }
    }

    #[test]
    fn render_produces_exactly_height_rows() {
        let z = ZoaPanel::new();
        let rows = z.render(24, 10);
        assert_eq!(rows.len(), 10);
    }

    #[test]
    fn render_rows_are_exactly_width_cols() {
        let z = ZoaPanel::new();
        for row in z.render(30, 8) {
            assert_eq!(row.visible_width(), 30);
        }
    }

    #[test]
    fn render_below_frame_width_returns_empty() {
        let z = ZoaPanel::new();
        assert!(z.render(23, 8).is_empty());
        assert!(z.render(0, 8).is_empty());
    }

    #[test]
    fn render_zero_height_returns_empty() {
        let z = ZoaPanel::new();
        assert!(z.render(24, 0).is_empty());
    }

    #[test]
    fn render_short_panel_truncates_frame_tail() {
        let z = ZoaPanel::new();
        // Height 3 — only the first 3 rows of the 8-row frame should surface.
        let rows = z.render(24, 3);
        assert_eq!(rows.len(), 3);
        let top = rows[0].plain_text();
        assert!(
            top.contains("_______"),
            "top row should be the skull cap: {top}"
        );
    }

    #[test]
    fn render_tall_panel_pads_blanks() {
        let z = ZoaPanel::new();
        let rows = z.render(24, 12);
        assert_eq!(rows.len(), 12);
        // Rows past FRAME_HEIGHT must be all spaces.
        for row in &rows[FRAME_HEIGHT..] {
            assert_eq!(row.visible_width(), 24);
            let text = row.plain_text();
            assert!(
                text.chars().all(|c| c == ' '),
                "padded row not blank: {text:?}"
            );
        }
    }

    #[test]
    fn tick_advances_frame_after_interval() {
        let mut z = ZoaPanel::new();
        let start = Instant::now();
        let advanced = z.tick_at(start);
        assert!(advanced);
        let idx0 = z.frame_idx;

        // Too early — should not advance.
        assert!(!z.tick_at(start + Duration::from_millis(100)));
        assert_eq!(z.frame_idx, idx0);

        // Past interval — advances.
        assert!(z.tick_at(start + FRAME_INTERVAL));
        assert_ne!(z.frame_idx, idx0);
    }

    #[test]
    fn tick_wraps_back_to_zero() {
        let mut z = ZoaPanel::new();
        let mut now = Instant::now();
        let n_frames = frames_for(Emotion::Idle).len();
        // Drive enough ticks to wrap once.
        for _ in 0..(n_frames + 1) {
            z.tick_at(now);
            now += FRAME_INTERVAL;
        }
        assert!(z.frame_idx < n_frames);
    }

    #[test]
    fn set_emotion_resets_frame_idx() {
        let mut z = ZoaPanel::new();
        z.frame_idx = 2;
        z.set_emotion(Emotion::Happy);
        assert_eq!(z.frame_idx, 0);
        assert_eq!(z.emotion(), Emotion::Happy);
    }

    #[test]
    fn set_same_emotion_preserves_frame_idx() {
        let mut z = ZoaPanel::new();
        z.frame_idx = 2;
        z.set_emotion(Emotion::Idle); // same as default
        assert_eq!(
            z.frame_idx, 2,
            "no-op emotion change must not restart animation"
        );
    }

    #[test]
    fn placeholder_emotions_return_idle_frames() {
        // Until Phase 4 lands real sprites, every emotion yields the same
        // frames as Idle. `const` slice literals may deduplicate to
        // different addresses at each use site, so we compare by value.
        assert_eq!(frames_for(Emotion::Happy), frames_for(Emotion::Idle));
        assert_eq!(frames_for(Emotion::Tired), frames_for(Emotion::Idle));
        assert_eq!(frames_for(Emotion::Hunting), frames_for(Emotion::Idle));
    }

    #[test]
    fn should_render_respects_width() {
        let z = ZoaPanel::new();
        assert!(!z.should_render(23));
        assert!(z.should_render(24));
        assert!(z.should_render(40));
    }

    #[test]
    fn blink_frame_differs_from_open_frame() {
        // Sanity: frame 1 (blink) must differ from frame 0 (open).
        assert_ne!(IDLE_FRAMES[0], IDLE_FRAMES[1]);
        // Frame 2 is intentionally equal to frame 0 (bounce-back pattern).
        assert_eq!(IDLE_FRAMES[0], IDLE_FRAMES[2]);
    }
}
