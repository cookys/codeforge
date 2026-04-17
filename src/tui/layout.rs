//! Region layout arithmetic for the Phase 2c TUI.
//!
//! Given terminal size (cols × rows), slices the screen into four named
//! regions: top status header, left local-map column, right combat-log
//! column, and a 1-row border between sections. Pure function — no
//! terminal IO — so the renderer can verify layout in unit tests.
//!
//! Layout sketch (target = 100×40 terminal):
//!
//! ```text
//!   0            col_split       cols-1
//!   ┌────────────────────────────────────┐ 0
//!   │ PetStatus (3 rows)                 │
//!   │                                    │
//!   │                                    │
//!   ├──────────────┬─────────────────────┤ PET_ROWS
//!   │ LocalMap     │ CombatLog           │
//!   │              │                     │
//!   │              │                     │
//!   └──────────────┴─────────────────────┘ rows-1
//! ```
//!
//! Terminals smaller than MIN_COLS × MIN_ROWS get a clamped layout: every
//! region is still produced so the renderer never panics on tiny windows,
//! but regions shrink to whatever fits.

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
}

/// Named regions produced by [`compute`]. `map_col_split` in the source
/// determines the vertical split between LocalMap and CombatLog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub pet_status: Rect,
    pub local_map: Rect,
    pub combat_log: Rect,
}

/// Rows reserved for the top PetStatus panel.
pub const PET_ROWS: u16 = 3;
/// Minimum terminal cols before the split columns collapse to 0-width.
#[allow(dead_code)]
pub const MIN_COLS: u16 = 20;
/// Minimum rows before bottom panels collapse to 0-height (PET_ROWS + 2).
#[allow(dead_code)]
pub const MIN_ROWS: u16 = PET_ROWS + 2;

/// Fraction of horizontal space given to the LocalMap column. Combat log
/// takes the remainder. 0.4 keeps the map narrow enough to fit directory
/// names comfortably on a 100-col terminal.
const MAP_FRACTION: f32 = 0.4;

/// Compute region rectangles for a terminal of the given size.
pub fn compute(cols: u16, rows: u16) -> Layout {
    // Top panel: always PET_ROWS tall, truncated to available rows.
    let pet_h = PET_ROWS.min(rows);
    let pet_status = Rect {
        x: 0,
        y: 0,
        width: cols,
        height: pet_h,
    };

    // Bottom region starts below pet_status; 0 rows if the terminal is too short.
    let bottom_y = pet_h;
    let bottom_h = rows.saturating_sub(pet_h);

    // Column split: MAP_FRACTION of cols (rounded down), minimum 0.
    let map_w = ((cols as f32) * MAP_FRACTION).floor() as u16;
    let log_w = cols.saturating_sub(map_w);

    let local_map = Rect {
        x: 0,
        y: bottom_y,
        width: map_w,
        height: bottom_h,
    };
    let combat_log = Rect {
        x: map_w,
        y: bottom_y,
        width: log_w,
        height: bottom_h,
    };

    Layout {
        pet_status,
        local_map,
        combat_log,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_size_100x40() {
        let l = compute(100, 40);
        assert_eq!(l.pet_status, Rect { x: 0, y: 0, width: 100, height: 3 });
        assert_eq!(l.local_map.y, 3);
        assert_eq!(l.local_map.height, 37);
        assert_eq!(l.local_map.width, 40);
        assert_eq!(l.combat_log.x, 40);
        assert_eq!(l.combat_log.width, 60);
    }

    #[test]
    fn widths_always_sum_to_cols() {
        for cols in [20u16, 50, 80, 100, 120, 200] {
            let l = compute(cols, 40);
            assert_eq!(
                l.local_map.width + l.combat_log.width,
                cols,
                "cols={cols} split must cover full width exactly"
            );
        }
    }

    #[test]
    fn pet_status_spans_full_width() {
        let l = compute(100, 40);
        assert_eq!(l.pet_status.x, 0);
        assert_eq!(l.pet_status.width, 100);
    }

    #[test]
    fn tiny_terminal_produces_some_valid_regions() {
        // 10 × 3 — just enough for pet_status, nothing below. Should not
        // panic and bottom panels collapse to 0 height.
        let l = compute(10, 3);
        assert_eq!(l.pet_status.height, 3);
        assert_eq!(l.local_map.height, 0);
        assert_eq!(l.combat_log.height, 0);
        assert!(l.local_map.is_empty());
        assert!(l.combat_log.is_empty());
    }

    #[test]
    fn terminal_shorter_than_pet_panel_still_clamps() {
        // 50 × 1 — pet panel truncates to 1 row, bottom is empty.
        let l = compute(50, 1);
        assert_eq!(l.pet_status.height, 1);
        assert_eq!(l.local_map.height, 0);
    }

    #[test]
    fn zero_size_does_not_panic() {
        let l = compute(0, 0);
        assert!(l.pet_status.is_empty());
        assert!(l.local_map.is_empty());
        assert!(l.combat_log.is_empty());
    }

    #[test]
    fn very_narrow_terminal_map_width_zero_log_takes_all() {
        // cols=1 → map_w = floor(1 * 0.4) = 0, log_w = 1
        let l = compute(1, 10);
        assert_eq!(l.local_map.width, 0);
        assert_eq!(l.combat_log.width, 1);
    }
}
