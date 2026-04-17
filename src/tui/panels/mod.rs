//! Panel renderers — Phase 2c P4.
//!
//! Each submodule exposes a pure `render(data, width) -> Vec<String>`
//! function. Lines are bare strings (no ANSI escapes yet) so they
//! golden-test cleanly; colorization happens at the paint layer in P5.
//!
//! Lines are already padded/truncated to `width` columns using
//! unicode-width so CJK double-wide chars land correctly.

pub mod combat_log;
pub mod local_map;
pub mod pet;

use unicode_width::UnicodeWidthStr;

/// Visible-column width (respects CJK double-wide).
pub(super) fn vis_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate `s` so its visible width is at most `max_cols`, appending `…`
/// if we had to cut. CJK-safe per HARD RULE: iterates chars, not bytes.
pub(super) fn clip_to_width(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if vis_width(s) <= max_cols {
        return s.to_string();
    }
    // Greedily consume chars until adding the next would exceed max_cols-1
    // (saving 1 col for the `…`).
    let mut cols = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]));
        if cols + w > max_cols.saturating_sub(1) {
            break;
        }
        cols += w;
        out.push(ch);
    }
    out.push('…');
    out
}

/// Pad `s` on the right with spaces so its visible width equals exactly
/// `width`. If `s` already overflows, it's clipped first.
pub(super) fn pad_to_width(s: &str, width: usize) -> String {
    let clipped = clip_to_width(s, width);
    let w = vis_width(&clipped);
    if w >= width {
        clipped
    } else {
        format!("{}{}", clipped, " ".repeat(width - w))
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn clip_ascii_unchanged_when_fits() {
        assert_eq!(clip_to_width("hello", 10), "hello");
    }

    #[test]
    fn clip_ascii_truncates_with_ellipsis() {
        assert_eq!(clip_to_width("hello world", 5), "hell…");
    }

    #[test]
    fn clip_cjk_respects_double_width() {
        // Each CJK char is 2 cols — budget 5 fits 2 chars (4 cols) + …
        let out = clip_to_width("代號七七七", 5);
        assert_eq!(vis_width(&out), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn clip_zero_width_returns_empty() {
        assert_eq!(clip_to_width("anything", 0), "");
    }

    #[test]
    fn pad_extends_short_string() {
        assert_eq!(pad_to_width("hi", 5), "hi   ");
    }

    #[test]
    fn pad_clips_long_string() {
        assert_eq!(vis_width(&pad_to_width("hello world", 5)), 5);
    }

    #[test]
    fn pad_cjk_string_to_exact_width() {
        // "七七七" = 6 cols; pad to 10 adds 4 spaces
        let out = pad_to_width("七七七", 10);
        assert_eq!(vis_width(&out), 10);
    }
}
