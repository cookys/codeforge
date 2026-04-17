//! PetStatus panel — Phase 2c P4.
//!
//! Renders a 3-line block summarizing the pet: name/level, HP/XP bars,
//! five-stat row. Pure function of `PetState` + width; no terminal IO.

use super::pad_to_width;
use crate::pet::state::PetState;

/// Render the 3-row PetStatus panel as bare strings, each padded to
/// exactly `width` visible columns. The panel never exceeds 3 lines —
/// callers that ask for taller regions pad with blanks themselves.
pub fn render(pet: &PetState, width: usize) -> Vec<String> {
    let header = format!("{}  Lv.{}", pet.name, pet.level);
    let bars = format!(
        "HP {}  XP {} {}/{}",
        bar(pet.hp, 100, 6),
        bar(pet.xp, pet.xp_to_next.max(1), 6),
        pet.xp,
        pet.xp_to_next,
    );
    let stats = format!(
        "ATK:{:3}  DEF:{:3}  SUP:{:3}  VER:{:3}",
        pet.atk, pet.def, pet.sup, pet.ver
    );

    vec![
        pad_to_width(&header, width),
        pad_to_width(&bars, width),
        pad_to_width(&stats, width),
    ]
}

/// 6-cell progress bar in plain ASCII chars: `█` filled, `░` empty.
/// Cheap to golden-test because there's no color / ANSI escape.
fn bar(current: u32, max: u32, cells: usize) -> String {
    if max == 0 || cells == 0 {
        return "░".repeat(cells);
    }
    let ratio = (current as f64 / max as f64).clamp(0.0, 1.0);
    let filled = (ratio * cells as f64).round() as usize;
    let filled = filled.min(cells);
    format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vis_width;

    fn sample() -> PetState {
        PetState {
            village: "rust".to_string(),
            name: "Ferris".to_string(),
            level: 5,
            xp: 420,
            xp_to_next: 1000,
            atk: 18,
            hp: 82,
            def: 12,
            sup: 10,
            ver: 11,
        }
    }

    #[test]
    fn renders_exactly_three_lines() {
        let lines = render(&sample(), 50);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn every_line_matches_requested_width() {
        let lines = render(&sample(), 60);
        for l in &lines {
            assert_eq!(vis_width(l), 60, "line must be padded/clipped to 60");
        }
    }

    #[test]
    fn header_contains_name_and_level() {
        let lines = render(&sample(), 50);
        assert!(lines[0].contains("Ferris"));
        assert!(lines[0].contains("Lv.5"));
    }

    #[test]
    fn bars_reflect_values() {
        let lines = render(&sample(), 60);
        // HP=82/100 → ceil(0.82*6) = 5 filled
        assert!(lines[1].contains("█████░"), "got: {}", lines[1]);
    }

    #[test]
    fn zero_xp_to_next_does_not_panic() {
        // Degenerate state (no level-up config yet) — render must not divide by 0
        let mut p = sample();
        p.xp_to_next = 0;
        p.xp = 0;
        let lines = render(&p, 60);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn overflow_hp_is_clamped_in_bar() {
        let mut p = sample();
        p.hp = 500; // > max
        let lines = render(&p, 60);
        // Bar should be fully filled (6 █, 0 ░)
        assert!(lines[1].contains("██████"), "got: {}", lines[1]);
    }

    #[test]
    fn narrow_width_clips_with_ellipsis() {
        let lines = render(&sample(), 10);
        for l in &lines {
            assert_eq!(vis_width(l), 10);
        }
        // At width 10 the stats line ("ATK: 18  DEF: 12 ..." is >10) must end with …
        assert!(lines[2].contains('…'));
    }

    #[test]
    fn cjk_name_does_not_break_width() {
        let mut p = sample();
        p.name = "代號七七七".to_string();
        let lines = render(&p, 50);
        assert_eq!(vis_width(&lines[0]), 50);
    }
}
