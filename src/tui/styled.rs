//! Styled text primitives for the TUI render / paint pipeline.
//!
//! Introduced by the zone-color paint-layer project (B11): panels used
//! to emit `Vec<String>`, and paint `Print(&text)` coloured an entire
//! line at most. Tile-grid Local Map needs border-only colouring with
//! default-fg interior text, so the data model moves to per-range
//! [`StyledSpan`] segments grouped into a [`StyledLine`].
//!
//! Scope (this phase, P1): only foreground colour — no background,
//! bold, or italic. Extending `StyledSpan` later is additive.
//!
//! Width helpers are built in here (not delegated to `panels::*`) so
//! `styled` stays a foundation module that `panels` can depend on,
//! not the other way round.

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

/// One contiguous run of text with optional foreground colour.
///
/// `fg = None` means "default terminal foreground" — paint emits the
/// text with no escape sequences, matching pre-B11 behavior exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
}

impl StyledSpan {
    /// Default-fg span. Paint emits a bare `Print(text)` for this.
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), fg: None }
    }

    /// Coloured span. Paint brackets the text with `SetForegroundColor`
    /// / `ResetColor` so stateful colour does not leak into the next span.
    pub fn colored(text: impl Into<String>, fg: Color) -> Self {
        Self { text: text.into(), fg: Some(fg) }
    }

    /// Visible column width (CJK-safe via unicode-width).
    pub fn visible_width(&self) -> usize {
        UnicodeWidthStr::width(self.text.as_str())
    }
}

/// One row of the rendered frame: an ordered list of [`StyledSpan`]s.
///
/// An empty `spans` vec represents a no-op line — paint skips it and
/// does not even move the cursor. This preserves the observable
/// behavior of the old `Vec<String>` pipeline where an empty string
/// was effectively invisible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

impl StyledLine {
    /// A truly empty line: no spans, width 0, paint skips entirely.
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    /// Line with a single default-fg span. Empty input collapses to
    /// [`StyledLine::empty`] so that `spans.is_empty()` remains the one
    /// source of truth for "invisible line" in the paint loop.
    pub fn plain(text: impl Into<String>) -> Self {
        let s: String = text.into();
        if s.is_empty() {
            return Self::empty();
        }
        Self { spans: vec![StyledSpan::plain(s)] }
    }

    /// Build from a pre-computed span list (used by panels that mix
    /// coloured borders with default-fg interior).
    pub fn from_spans(spans: Vec<StyledSpan>) -> Self {
        Self { spans }
    }

    /// Sum of per-span visible widths. Avoids materialising the
    /// concatenated string for the common "how wide is this row" query.
    pub fn visible_width(&self) -> usize {
        self.spans.iter().map(StyledSpan::visible_width).sum()
    }

    /// Right-pad with a default-fg space span until the total visible
    /// width equals `w`. Returns the line unchanged when already >= `w`
    /// — this is intentionally asymmetric with `panels::pad_to_width`
    /// (the String helper clips overflows). The asymmetry exists
    /// because cross-span clipping must preserve per-span `fg`, which
    /// `pad` can't do correctly; callers that need truncation must
    /// run [`StyledLine::clip_to_width`] first. Passing an over-width
    /// line through here is a caller bug — the output will overflow
    /// into the next panel column.
    ///
    /// Note: padding `StyledLine::empty()` appends a default-fg space
    /// span and therefore turns the line into a non-empty "render me
    /// as blanks" line rather than the `spans.is_empty() == invisible`
    /// sentinel — this is intentional (padded blank rows should paint
    /// so the cursor moves and residual content is overwritten).
    /// Callers that need a truly invisible line must use
    /// [`StyledLine::empty`] directly.
    pub fn pad_to_width(&self, w: usize) -> Self {
        let current = self.visible_width();
        if current >= w {
            return self.clone();
        }
        let pad = w - current;
        let mut spans = self.spans.clone();
        spans.push(StyledSpan::plain(" ".repeat(pad)));
        Self { spans }
    }

    /// Truncate to at most `w` visible columns, preserving per-span
    /// `fg`. A partial span at the boundary has its text cut CJK-safe
    /// (column-count based, never byte-indexed). Spans entirely past
    /// the boundary are dropped. Returns a clone when already narrower
    /// than `w`. No `…` ellipsis — callers append their own overlay
    /// (tile-grid overflow indicator does this).
    pub fn clip_to_width(&self, w: usize) -> Self {
        let current = self.visible_width();
        if current <= w {
            return self.clone();
        }
        let mut remaining = w;
        let mut out_spans: Vec<StyledSpan> = Vec::new();
        for span in &self.spans {
            if remaining == 0 {
                break;
            }
            let span_w = UnicodeWidthStr::width(span.text.as_str());
            if span_w <= remaining {
                out_spans.push(span.clone());
                remaining -= span_w;
            } else {
                // Partial span — CJK-safe char-by-char accumulation.
                let mut acc = 0usize;
                let mut buf = [0u8; 4];
                let mut truncated = String::new();
                for ch in span.text.chars() {
                    let cw = UnicodeWidthStr::width(ch.encode_utf8(&mut buf));
                    if acc + cw > remaining {
                        break;
                    }
                    acc += cw;
                    truncated.push(ch);
                }
                if !truncated.is_empty() {
                    out_spans.push(StyledSpan { text: truncated, fg: span.fg });
                }
                break;
            }
        }
        Self { spans: out_spans }
    }
}

/// Test-only impl — `plain_text()` is used heavily by panel tests
/// (`lines[i].plain_text().contains(...)`) but production code walks
/// `spans` directly. Gating behind cfg(test) keeps dead-code analysis
/// clean without sprinkling `#[allow(dead_code)]`.
#[cfg(test)]
impl StyledLine {
    /// Concatenated text with style stripped.
    pub fn plain_text(&self) -> String {
        let cap = self.spans.iter().map(|s| s.text.len()).sum();
        let mut out = String::with_capacity(cap);
        for s in &self.spans {
            out.push_str(&s.text);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_plain_has_no_fg() {
        let s = StyledSpan::plain("hi");
        assert_eq!(s.text, "hi");
        assert_eq!(s.fg, None);
    }

    #[test]
    fn span_colored_carries_fg() {
        let s = StyledSpan::colored("hi", Color::Red);
        assert_eq!(s.fg, Some(Color::Red));
    }

    #[test]
    fn span_visible_width_cjk() {
        assert_eq!(StyledSpan::plain("前端").visible_width(), 4);
    }

    #[test]
    fn line_plain_of_empty_string_is_empty_spans() {
        let l = StyledLine::plain("");
        assert!(l.spans.is_empty(), "empty string must collapse to zero spans");
    }

    #[test]
    fn line_plain_of_text_is_single_default_span() {
        let l = StyledLine::plain("hello");
        assert_eq!(l.spans.len(), 1);
        assert_eq!(l.spans[0].text, "hello");
        assert_eq!(l.spans[0].fg, None);
    }

    #[test]
    fn plain_text_concats_all_spans_in_order() {
        let l = StyledLine::from_spans(vec![
            StyledSpan::plain("┌"),
            StyledSpan::colored("name", Color::Red),
            StyledSpan::plain("┐"),
        ]);
        assert_eq!(l.plain_text(), "┌name┐");
    }

    #[test]
    fn visible_width_sums_spans_cjk_safe() {
        let l = StyledLine::from_spans(vec![
            StyledSpan::plain("│"),     // 1 col
            StyledSpan::plain("前端"),   // 4 cols
            StyledSpan::plain("│"),     // 1 col
        ]);
        assert_eq!(l.visible_width(), 6);
    }

    #[test]
    fn pad_to_width_noop_when_already_wide_enough() {
        let l = StyledLine::plain("hi there");
        let padded = l.pad_to_width(3);
        assert_eq!(padded.plain_text(), "hi there");
        assert_eq!(padded.spans.len(), 1);
    }

    #[test]
    fn pad_to_width_appends_trailing_space_span() {
        let l = StyledLine::plain("hi");
        let padded = l.pad_to_width(5);
        assert_eq!(padded.visible_width(), 5);
        assert_eq!(padded.plain_text(), "hi   ");
    }

    #[test]
    fn pad_to_width_preserves_existing_colored_spans() {
        let l = StyledLine::from_spans(vec![
            StyledSpan::colored("red", Color::Red),
        ]);
        let padded = l.pad_to_width(8);
        assert_eq!(padded.visible_width(), 8);
        assert_eq!(padded.spans.len(), 2);
        assert_eq!(padded.spans[0].fg, Some(Color::Red));
        assert_eq!(padded.spans[1].fg, None);
        assert_eq!(padded.spans[1].text, "     ");
    }

    #[test]
    fn pad_to_width_cjk_input_counts_double_wide() {
        let l = StyledLine::plain("前端"); // 4 cols
        let padded = l.pad_to_width(8);
        assert_eq!(padded.visible_width(), 8);
        let last = padded.spans.last().expect("pad span must exist");
        assert_eq!(last.text, "    ", "4 trailing spaces for 8 - 4 = 4 cols");
    }

    #[test]
    fn empty_line_has_zero_width_and_no_plain_text() {
        let l = StyledLine::empty();
        assert_eq!(l.visible_width(), 0);
        assert_eq!(l.plain_text(), "");
        assert!(l.spans.is_empty());
    }

    #[test]
    fn clip_to_width_noop_when_already_narrow() {
        let l = StyledLine::plain("hi");
        let clipped = l.clip_to_width(5);
        assert_eq!(clipped.plain_text(), "hi");
    }

    #[test]
    fn clip_to_width_drops_spans_past_boundary() {
        let l = StyledLine::from_spans(vec![
            StyledSpan::colored("red", Color::Red),       // 3 cols
            StyledSpan::plain("middle"),                   // 6 cols
            StyledSpan::colored("tail", Color::Blue),      // 4 cols — dropped entirely when w < 9
        ]);
        let clipped = l.clip_to_width(9);
        assert_eq!(clipped.visible_width(), 9);
        assert_eq!(clipped.plain_text(), "redmiddle");
        assert_eq!(clipped.spans.len(), 2);
        // First two spans retained with original fg.
        assert_eq!(clipped.spans[0].fg, Some(Color::Red));
        assert_eq!(clipped.spans[1].fg, None);
    }

    #[test]
    fn clip_to_width_truncates_partial_span_preserving_fg() {
        let l = StyledLine::from_spans(vec![
            StyledSpan::colored("red", Color::Red),        // 3 cols
            StyledSpan::plain("xxxxx"),                    // 5 cols
        ]);
        let clipped = l.clip_to_width(5);
        assert_eq!(clipped.visible_width(), 5);
        assert_eq!(clipped.plain_text(), "redxx");
        assert_eq!(clipped.spans.len(), 2);
        assert_eq!(clipped.spans[1].fg, None);
        assert_eq!(clipped.spans[1].text, "xx", "partial-span text truncated to 2 cols");
    }

    #[test]
    fn clip_to_width_cjk_safe_at_partial_boundary() {
        // "前端" = 4 cols. Clipping to 3 cols must drop the 2nd char
        // whole (never mid-codepoint), leaving "前" (2 cols). No panic.
        let l = StyledLine::from_spans(vec![
            StyledSpan::plain("前端後端"), // 8 cols total
        ]);
        let clipped = l.clip_to_width(3);
        assert_eq!(clipped.visible_width(), 2, "can't split a 2-col char across 3-col budget");
        assert_eq!(clipped.plain_text(), "前");
    }

    #[test]
    fn clip_to_width_zero_yields_empty() {
        let l = StyledLine::plain("hello");
        let clipped = l.clip_to_width(0);
        assert!(clipped.spans.is_empty());
        assert_eq!(clipped.visible_width(), 0);
    }
}
