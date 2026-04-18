//! Region layout arithmetic for the TUI.
//!
//! Given terminal size (cols × rows), first classifies into a
//! [`LayoutMode`] via column-width breakpoints, then slices the screen
//! into up to five named regions (PetStatus header, Zoa animation,
//! LocalMap, CombatLog; plus the implicit divider row). Pure function —
//! no terminal IO — so the renderer can verify every breakpoint in unit
//! tests.
//!
//! ## Breakpoints
//!
//! | Mode | Cols | Renders |
//! |------|------|---------|
//! | Compact | <60 | refuses to enter alt-screen; caller prints fallback |
//! | Narrow | 60–79 | PetStatus + CombatLog (single column) |
//! | Standard | 80–119 | PetStatus + LocalMap(40%) + CombatLog(60%) |
//! | Wide | 120+ | PetStatus + Zoa(24) + LocalMap + CombatLog |
//!
//! The breakpoint numbers come from a survey of developer screen sizes
//! (Steam HW 2025-11: 1080p 53% / 1440p 21%) cross-referenced with
//! typical tmux companion-pane widths (`-p 30` → ~54 cols on 1080p,
//! ~66 on 1440p). See `doc/plans/2026-04-18-tui-foundation.md`.

/// One rectangular screen region. All fields are column/row counts, not
/// pixels. `(x, y)` is the top-left corner; drawable extent is
/// `[x, x+width)` × `[y, y+height)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    /// True if the rectangle has zero area. Renderers should no-op on these.
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// A zero-sized rect at the origin. Used as a placeholder for regions
    /// that a given [`LayoutMode`] omits (e.g. Zoa in Standard mode).
    pub const EMPTY: Rect = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };
}

/// Breakpoint bucket chosen for a given terminal width. Renderers use
/// this to short-circuit work (Compact → fallback; Narrow → skip map).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Below the minimum usable width — caller should abort TUI startup
    /// and print a fallback hint instead of entering alt-screen.
    Compact,
    /// Single-column layout: PetStatus over CombatLog. LocalMap and Zoa
    /// collapse to empty rects.
    Narrow,
    /// The historical layout shipped in Phase 2c: PetStatus +
    /// LocalMap(40%) + CombatLog(60%). Zoa empty.
    Standard,
    /// Adds a fixed 24-col Zoa column on the left for ASCII animation.
    Wide,
}

impl LayoutMode {
    /// Classify a terminal by its column count. Row count doesn't affect
    /// the mode — even a 200×3 terminal stays Wide (vertical clamping
    /// happens inside `compute`).
    pub fn from_size(cols: u16, _rows: u16) -> LayoutMode {
        match cols {
            0..=59 => LayoutMode::Compact,
            60..=79 => LayoutMode::Narrow,
            80..=119 => LayoutMode::Standard,
            _ => LayoutMode::Wide,
        }
    }

    /// True when the TUI should refuse to open. Caller prints a hint
    /// and exits cleanly; no alt-screen takeover happens.
    pub fn should_abort(self) -> bool {
        matches!(self, LayoutMode::Compact)
    }
}

/// Named regions produced by [`compute`]. Omitted regions carry
/// [`Rect::EMPTY`] so renderers can call `.is_empty()` instead of
/// branching on mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub mode: LayoutMode,
    pub pet_status: Rect,
    pub zoa: Rect,
    pub local_map: Rect,
    pub combat_log: Rect,
}

/// Rows reserved for the top PetStatus panel.
pub const PET_ROWS: u16 = 3;
/// Column threshold at which `LayoutMode::Narrow` activates — below this
/// we refuse TUI startup (Compact).
pub const MIN_USABLE_COLS: u16 = 60;
/// Rows minimum before bottom panels collapse to 0-height (PET_ROWS + 2).
#[allow(dead_code)]
pub const MIN_ROWS: u16 = PET_ROWS + 2;
/// Fixed width reserved for the Zoa column in Wide mode. Chosen to fit
/// a 24-col ASCII pet sprite without scaling.
pub const ZOA_WIDTH: u16 = 24;
/// Fraction of the map+log strip given to LocalMap. CombatLog takes the
/// remainder. 0.4 keeps directory names readable on ~80-col terminals.
const MAP_FRACTION: f32 = 0.4;

/// Compute region rectangles for a terminal of the given size.
///
/// Always returns a valid [`Layout`]; tiny terminals get zero-area
/// regions rather than panicking. Callers should check `.mode` first —
/// if it's `Compact`, skip painting and surface a hint to the user.
pub fn compute(cols: u16, rows: u16) -> Layout {
    let mode = LayoutMode::from_size(cols, rows);

    // Top panel: always PET_ROWS tall, truncated to available rows.
    let pet_h = PET_ROWS.min(rows);
    let pet_status = Rect {
        x: 0,
        y: 0,
        width: cols,
        height: pet_h,
    };

    let bottom_y = pet_h;
    let bottom_h = rows.saturating_sub(pet_h);

    let (zoa, local_map, combat_log) = match mode {
        LayoutMode::Compact => {
            // Nothing to render beyond the header — caller will bail.
            (Rect::EMPTY, Rect::EMPTY, Rect::EMPTY)
        }
        LayoutMode::Narrow => {
            // Single-column: CombatLog takes the full bottom strip,
            // LocalMap + Zoa sit out this breakpoint.
            let log = Rect {
                x: 0,
                y: bottom_y,
                width: cols,
                height: bottom_h,
            };
            (Rect::EMPTY, Rect::EMPTY, log)
        }
        LayoutMode::Standard => {
            // Phase 2c layout: MAP_FRACTION of bottom goes to LocalMap.
            let map_w = ((cols as f32) * MAP_FRACTION).floor() as u16;
            let log_w = cols.saturating_sub(map_w);
            let map = Rect {
                x: 0,
                y: bottom_y,
                width: map_w,
                height: bottom_h,
            };
            let log = Rect {
                x: map_w,
                y: bottom_y,
                width: log_w,
                height: bottom_h,
            };
            (Rect::EMPTY, map, log)
        }
        LayoutMode::Wide => {
            // Zoa takes a fixed 24-col slab on the far left. Remaining
            // horizontal space splits 40/60 between map and log, same
            // ratio Standard uses — keeps the map/log look consistent
            // across the two modes.
            let zoa_w = ZOA_WIDTH.min(cols);
            let remaining = cols.saturating_sub(zoa_w);
            let map_w = ((remaining as f32) * MAP_FRACTION).floor() as u16;
            let log_w = remaining.saturating_sub(map_w);
            let zoa = Rect {
                x: 0,
                y: bottom_y,
                width: zoa_w,
                height: bottom_h,
            };
            let map = Rect {
                x: zoa_w,
                y: bottom_y,
                width: map_w,
                height: bottom_h,
            };
            let log = Rect {
                x: zoa_w + map_w,
                y: bottom_y,
                width: log_w,
                height: bottom_h,
            };
            (zoa, map, log)
        }
    };

    Layout {
        mode,
        pet_status,
        zoa,
        local_map,
        combat_log,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- LayoutMode breakpoints ----

    #[test]
    fn mode_boundary_59_is_compact() {
        assert_eq!(LayoutMode::from_size(59, 40), LayoutMode::Compact);
    }

    #[test]
    fn mode_boundary_60_is_narrow() {
        assert_eq!(LayoutMode::from_size(60, 40), LayoutMode::Narrow);
    }

    #[test]
    fn mode_boundary_79_is_narrow() {
        assert_eq!(LayoutMode::from_size(79, 40), LayoutMode::Narrow);
    }

    #[test]
    fn mode_boundary_80_is_standard() {
        assert_eq!(LayoutMode::from_size(80, 40), LayoutMode::Standard);
    }

    #[test]
    fn mode_boundary_119_is_standard() {
        assert_eq!(LayoutMode::from_size(119, 40), LayoutMode::Standard);
    }

    #[test]
    fn mode_boundary_120_is_wide() {
        assert_eq!(LayoutMode::from_size(120, 40), LayoutMode::Wide);
    }

    #[test]
    fn mode_huge_terminal_is_wide() {
        assert_eq!(LayoutMode::from_size(340, 60), LayoutMode::Wide);
    }

    #[test]
    fn mode_zero_is_compact() {
        assert_eq!(LayoutMode::from_size(0, 0), LayoutMode::Compact);
    }

    #[test]
    fn compact_mode_signals_abort() {
        assert!(LayoutMode::Compact.should_abort());
        assert!(!LayoutMode::Narrow.should_abort());
        assert!(!LayoutMode::Standard.should_abort());
        assert!(!LayoutMode::Wide.should_abort());
    }

    // ---- compute() regions per mode ----

    #[test]
    fn compact_returns_all_empty_bottom_regions() {
        let l = compute(50, 30);
        assert_eq!(l.mode, LayoutMode::Compact);
        assert!(l.zoa.is_empty());
        assert!(l.local_map.is_empty());
        assert!(l.combat_log.is_empty());
        // Pet header still filled so a caller that forgets to check
        // mode and paints anyway won't crash.
        assert_eq!(l.pet_status.width, 50);
    }

    #[test]
    fn narrow_hides_map_and_zoa() {
        let l = compute(72, 40);
        assert_eq!(l.mode, LayoutMode::Narrow);
        assert!(l.zoa.is_empty());
        assert!(l.local_map.is_empty());
        // CombatLog takes the full bottom strip.
        assert_eq!(l.combat_log.x, 0);
        assert_eq!(l.combat_log.width, 72);
        assert_eq!(l.combat_log.height, 40 - PET_ROWS);
    }

    #[test]
    fn standard_preserves_40_60_split() {
        let l = compute(100, 40);
        assert_eq!(l.mode, LayoutMode::Standard);
        assert!(l.zoa.is_empty());
        assert_eq!(l.local_map.width, 40);
        assert_eq!(l.combat_log.x, 40);
        assert_eq!(l.combat_log.width, 60);
    }

    #[test]
    fn wide_allocates_24_to_zoa() {
        let l = compute(140, 50);
        assert_eq!(l.mode, LayoutMode::Wide);
        assert_eq!(l.zoa.x, 0);
        assert_eq!(l.zoa.width, ZOA_WIDTH);
        // Remaining 116 cols split 40/60 between map and log.
        let expected_map = ((140 - 24) as f32 * 0.4).floor() as u16;
        assert_eq!(l.local_map.x, ZOA_WIDTH);
        assert_eq!(l.local_map.width, expected_map);
        assert_eq!(l.combat_log.x, ZOA_WIDTH + expected_map);
        assert_eq!(l.combat_log.width, 140 - ZOA_WIDTH - expected_map);
    }

    #[test]
    fn widths_sum_to_cols_in_every_mode() {
        // Excludes Compact (all-empty bottom) — that's covered separately.
        for cols in [60u16, 72, 80, 100, 119, 120, 140, 200, 340] {
            let l = compute(cols, 40);
            let total = l.zoa.width + l.local_map.width + l.combat_log.width;
            assert_eq!(
                total, cols,
                "cols={cols} mode={:?}: widths must cover full row",
                l.mode
            );
        }
    }

    #[test]
    fn pet_status_always_spans_full_width() {
        for cols in [30u16, 60, 100, 140, 200] {
            let l = compute(cols, 40);
            assert_eq!(l.pet_status.x, 0);
            assert_eq!(l.pet_status.width, cols);
        }
    }

    #[test]
    fn tiny_terminal_still_produces_valid_layout() {
        // 10 × 3 — tiny, sizes to Compact, pet clips to 3 rows, rest zero.
        let l = compute(10, 3);
        assert_eq!(l.mode, LayoutMode::Compact);
        assert_eq!(l.pet_status.height, 3);
        assert!(l.local_map.is_empty());
        assert!(l.combat_log.is_empty());
        assert!(l.zoa.is_empty());
    }

    #[test]
    fn terminal_shorter_than_pet_panel_clamps_pet() {
        // Row count < PET_ROWS shouldn't panic; pet just truncates.
        let l = compute(100, 1);
        assert_eq!(l.pet_status.height, 1);
        assert_eq!(l.local_map.height, 0);
        assert_eq!(l.combat_log.height, 0);
    }

    #[test]
    fn zero_size_does_not_panic() {
        let l = compute(0, 0);
        assert_eq!(l.mode, LayoutMode::Compact);
        assert!(l.pet_status.is_empty());
    }

    #[test]
    fn wide_map_takes_40_pct_of_non_zoa_strip() {
        // 200-col terminal → Zoa 24, remaining 176 → map 70 log 106
        let l = compute(200, 50);
        assert_eq!(l.mode, LayoutMode::Wide);
        assert_eq!(l.zoa.width, ZOA_WIDTH);
        assert_eq!(l.local_map.width, 70);
        assert_eq!(l.combat_log.width, 106);
    }

    #[test]
    fn narrow_then_standard_same_cols_differ_mode() {
        // Boundary sanity: 79 → Narrow (no map), 80 → Standard (has map).
        let a = compute(79, 40);
        let b = compute(80, 40);
        assert_eq!(a.mode, LayoutMode::Narrow);
        assert_eq!(b.mode, LayoutMode::Standard);
        assert!(a.local_map.is_empty());
        assert!(!b.local_map.is_empty());
    }
}
