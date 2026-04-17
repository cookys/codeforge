//! `snapshot::render` — pure formatter. Takes a [`SnapshotData`] and
//! returns a box-drawing ASCII card. No I/O, no DB. Fully unit-testable
//! offline.
//!
//! Layout (CARD_WIDTH = 58 visible columns, 60 counting borders):
//!
//! ```text
//! ╔══════════════════════════════════════════════════════════╗
//! ║  Ferris の 冒險報告  ·  2026-04-18                       ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║  [Rust Forge-Ruins] ── [Python Scriptorium]              ║
//! ║       Veteran ✦✦         Forger ✦                        ║
//! ║  [Go Dockside] ── [TS Garrison]                          ║
//! ║       Traveler           ？？？（未解鎖）                    ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║  過去 30 天                                              ║
//! ║  → Boss 擊殺：12   Elite 擊殺：87   Zombie：341          ║
//! ║  → 最長連勝：rust 8 個 boss                              ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║  Ferris · Rust Forge-Ruins · Lv.7                        ║
//! ║  「所有權是責任，錯誤是公民」                             ║
//! ╚══════════════════════════════════════════════════════════╝
//! ```

use super::{SnapshotData, StreakRecord};
use crate::pet::village::VILLAGES;
use crate::world::ZoneStats;
use unicode_width::UnicodeWidthStr;

/// Inner width (between the `║` borders). Chosen so four zone-label
/// pairs fit on one row with CJK-safe padding; CJK village names up to
/// 4 chars wide still land inside the card.
pub const CARD_WIDTH: usize = 58;

/// The four mob kinds the card breaks out explicitly. Ordered Boss →
/// Ghost so the line reads dangerous-first, matching the spec sample.
/// Labels kept short — the full "X 擊殺：N" form overflows CARD_WIDTH
/// once four columns are joined. Header row states "擊殺" once, body
/// uses label + count.
const KIND_COLUMNS: &[(&str, &str)] = &[
    ("boss", "Boss"),
    ("elite", "Elite"),
    ("zombie", "Zombie"),
    ("ghost", "Ghost"),
];

pub fn render(data: &SnapshotData) -> String {
    let mut out = String::new();
    push_line(&mut out, &format!("╔{}╗", "═".repeat(CARD_WIDTH)));
    push_row(&mut out, &header_line(data));
    push_line(&mut out, &format!("╠{}╣", "═".repeat(CARD_WIDTH)));
    for line in zone_lines(&data.zones) {
        push_row(&mut out, &line);
    }
    push_line(&mut out, &format!("╠{}╣", "═".repeat(CARD_WIDTH)));
    push_row(&mut out, &format!("過去 {} 天", data.window_days));
    push_row(&mut out, &kill_breakdown_line(data));
    if let Some(streak) = streak_line(&data.top_streak) {
        push_row(&mut out, &streak);
    }
    push_line(&mut out, &format!("╠{}╣", "═".repeat(CARD_WIDTH)));
    for line in footer_lines(data) {
        push_row(&mut out, &line);
    }
    push_line(&mut out, &format!("╚{}╝", "═".repeat(CARD_WIDTH)));
    out
}

fn push_line(out: &mut String, s: &str) {
    out.push_str(s);
    out.push('\n');
}

/// Push a body row: pads `content` to `CARD_WIDTH` visible cols inside
/// `║ … ║`. Content is pre-prefixed with " " so the left border has a
/// breathing space; the caller passes the semantic text only.
fn push_row(out: &mut String, content: &str) {
    let body = format!("  {}", content);
    let padded = pad_visible(&body, CARD_WIDTH);
    out.push('║');
    out.push_str(&padded);
    out.push('║');
    out.push('\n');
}

fn header_line(data: &SnapshotData) -> String {
    let pet_name = data
        .pet
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "CodeForger".to_string());
    format!("{} の 冒險報告  ·  {}", pet_name, data.generated_date)
}

fn zone_lines(zones: &[ZoneStats]) -> Vec<String> {
    // 2×2 grid over the first four zones, then a single line for the
    // fifth zone if present. Keeps the card compact while honoring the
    // stable 5-village layout from `world::load_all`.
    let mut out = Vec::new();
    for chunk in zones.chunks(2) {
        let left = zone_cell(&chunk[0]);
        let right = chunk.get(1).map(zone_cell).unwrap_or_default();
        let rank_left = rank_cell(&chunk[0]);
        let rank_right = chunk.get(1).map(rank_cell).unwrap_or_default();
        let pair_zone = if right.is_empty() {
            left
        } else {
            format!("{}  ──  {}", left, right)
        };
        let pair_rank = if rank_right.is_empty() {
            rank_left
        } else {
            // Indent the second rank so it sits roughly under its cell.
            // A half-card offset works for the 2×2 layout at CARD_WIDTH 58.
            format!("{}    {}", pad_visible(&rank_left, CARD_WIDTH / 2 - 2), rank_right)
        };
        out.push(pair_zone);
        out.push(pair_rank);
    }
    out
}

fn zone_cell(z: &ZoneStats) -> String {
    // The cell name itself is the same whether unlocked or not — the
    // rank line below is where the lock state surfaces visually. Kept
    // as a function instead of inlining so callers read naturally and
    // future spec changes (locked zone showing `???` name etc) land in
    // one place.
    format!("[{}]", village_short_name(&z.village_id))
}

fn rank_cell(z: &ZoneStats) -> String {
    if !z.unlocked {
        return "？？？（未解鎖）".to_string();
    }
    let glyph = z.rank.glyph();
    if glyph.is_empty() {
        z.rank.as_str().to_string()
    } else {
        format!("{} {}", z.rank.as_str(), glyph)
    }
}

/// Short-form village name combining language + a compact place name.
/// The raw `display_name` like "The Forge-Ruins" / "Border Garrison"
/// contains filler words that `first_word` would strip to the wrong
/// token; this mapping picks the flavourful noun per village so the
/// card reads like the spec sample ("Rust Forge-Ruins", "Python
/// Scriptorium", etc).
fn village_short_name(village_id: &str) -> String {
    let village = match VILLAGES.iter().find(|v| v.id == village_id) {
        Some(v) => v,
        None => return village_id.to_string(),
    };
    let place = match village.id {
        "rust" => "Forge-Ruins",
        "python" => "Scriptorium",
        "typescript" => "Garrison",
        "go" => "Dockside",
        "javascript" => "Strata",
        _ => village.display_name,
    };
    format!("{} {}", village.language, place)
}

fn kill_breakdown_line(data: &SnapshotData) -> String {
    let parts: Vec<String> = KIND_COLUMNS
        .iter()
        .map(|(key, label)| {
            let count = data.kills_by_kind.get(*key).copied().unwrap_or(0);
            format!("{} {}", label, count)
        })
        .collect();
    // " · " joiner keeps the line under CARD_WIDTH even at 4-digit kill
    // counts, while the leading "→ 擊殺：" labels the row semantically.
    format!("→ 擊殺：{}", parts.join(" · "))
}

fn streak_line(streak: &StreakRecord) -> Option<String> {
    if streak.count < 2 {
        return None;
    }
    Some(format!(
        "→ 最長連勝：{} 個 {} （{} Zone）",
        streak.count, streak.mob_kind, streak.zone_id,
    ))
}

fn footer_lines(data: &SnapshotData) -> Vec<String> {
    let (identity, tagline) = match &data.pet {
        Some(pet) => {
            let village = VILLAGES.iter().find(|v| v.id == pet.village_id);
            let zone_name = village_short_name(&pet.village_id);
            let identity = format!("{} · {} · Lv.{}", pet.name, zone_name, pet.level);
            let tagline = village.map(|v| v.tagline).unwrap_or("在程式的國度遊蕩");
            (identity, format!("「{}」", tagline))
        }
        None => (
            "（尚未收養寵物）".to_string(),
            "「執行 codeforge adopt 開始你的旅程」".to_string(),
        ),
    };
    vec![identity, tagline]
}

/// Pad `s` with spaces so its visible width equals `cols` (clips with `…`
/// if `s` overflows). CJK-safe via unicode-width; mirrors the helpers in
/// `cli/world.rs` rather than importing them — `snapshot/` stays free of
/// CLI dependencies.
fn pad_visible(s: &str, cols: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w == cols {
        return s.to_string();
    }
    if w > cols {
        return clip_visible(s, cols);
    }
    format!("{}{}", s, " ".repeat(cols - w))
}

fn clip_visible(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    // Exact-fit fast path. Without this the loop below consumes all chars,
    // exits naturally (not via `break`), and still pushes `…` — producing
    // an output `max_cols + 1` wide. `pad_visible` guards the overflow
    // path with a strict `>`, so this never trips in production today,
    // but the function's standalone contract should hold for any caller.
    if UnicodeWidthStr::width(s) <= max_cols {
        return s.to_string();
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{PetHeader, SnapshotData};
    use crate::world::{ZoneRank, ZoneStats};
    use std::collections::HashMap;

    fn zone(id: &str, unlocked: bool, kills: u32, rank: ZoneRank) -> ZoneStats {
        ZoneStats {
            village_id: id.to_string(),
            unlocked,
            kill_count: kills,
            concept_count: 0,
            rank,
        }
    }

    fn sample_data() -> SnapshotData {
        let mut kills: HashMap<String, u32> = HashMap::new();
        kills.insert("boss".to_string(), 12);
        kills.insert("elite".to_string(), 87);
        kills.insert("zombie".to_string(), 341);
        SnapshotData {
            pet: Some(PetHeader {
                name: "Ferris".to_string(),
                village_id: "rust".to_string(),
                level: 23,
            }),
            generated_date: "2026-04-18".to_string(),
            window_days: 30,
            kills_by_kind: kills,
            top_streak: StreakRecord {
                zone_id: "rust".to_string(),
                mob_kind: "boss".to_string(),
                count: 8,
            },
            zones: vec![
                zone("rust", true, 500, ZoneRank::Veteran),
                zone("python", true, 60, ZoneRank::Forger),
                zone("typescript", false, 0, ZoneRank::Traveler),
                zone("go", true, 10, ZoneRank::Traveler),
                zone("javascript", false, 0, ZoneRank::Traveler),
            ],
        }
    }

    #[test]
    fn render_contains_pet_name_and_date() {
        let out = render(&sample_data());
        assert!(out.contains("Ferris"), "pet name must appear in header");
        assert!(out.contains("2026-04-18"), "generated_date must appear");
        assert!(out.contains("過去 30 天"), "window label must appear");
    }

    #[test]
    fn render_contains_known_kill_counts() {
        let out = render(&sample_data());
        assert!(out.contains("Boss 12"));
        assert!(out.contains("Elite 87"));
        assert!(out.contains("Zombie 341"));
        assert!(out.contains("擊殺"));
    }

    #[test]
    fn render_prints_streak_when_at_least_two() {
        let out = render(&sample_data());
        assert!(out.contains("最長連勝"));
        assert!(out.contains("8"));
        assert!(out.contains("rust"));
    }

    #[test]
    fn render_suppresses_streak_when_zero() {
        let mut data = sample_data();
        data.top_streak = StreakRecord::default();
        let out = render(&data);
        assert!(!out.contains("最長連勝"), "streak section must be hidden");
    }

    #[test]
    fn render_every_body_line_fits_card_width() {
        let out = render(&sample_data());
        for (i, line) in out.lines().enumerate() {
            // Skip top/separator/bottom lines — they're pure border.
            // Body rows start with ║.
            if !line.starts_with('║') {
                continue;
            }
            // visible width between the two ║ chars must equal CARD_WIDTH.
            let inner: String = line.chars().skip(1).take(line.chars().count() - 2).collect();
            let w = UnicodeWidthStr::width(inner.as_str());
            assert_eq!(
                w, CARD_WIDTH,
                "line {i} has width {w}, expected {CARD_WIDTH}: {line:?}"
            );
        }
    }

    #[test]
    fn render_handles_missing_pet() {
        let mut data = sample_data();
        data.pet = None;
        let out = render(&data);
        assert!(out.contains("CodeForger"), "header falls back without pet");
        assert!(out.contains("（尚未收養寵物）"));
        assert!(out.contains("codeforge adopt"));
    }

    #[test]
    fn render_locked_zone_shows_question_marks() {
        let out = render(&sample_data());
        assert!(out.contains("？？？"), "locked zone must show ??? placeholder");
        assert!(out.contains("（未解鎖）"));
    }

    #[test]
    fn render_veteran_rank_gets_glyph() {
        let out = render(&sample_data());
        assert!(out.contains("Veteran ✦✦"));
    }

    #[test]
    fn render_missing_kills_show_as_zero() {
        let mut data = sample_data();
        data.kills_by_kind.remove("elite");
        let out = render(&data);
        assert!(
            out.contains("Elite 0"),
            "absent kind must render as 0 for consistent layout"
        );
    }

    #[test]
    fn render_respects_custom_window_days() {
        let mut data = sample_data();
        data.window_days = 7;
        let out = render(&data);
        assert!(out.contains("過去 7 天"));
    }

    #[test]
    fn pad_visible_handles_cjk() {
        let padded = pad_visible("本月", 10);
        assert_eq!(UnicodeWidthStr::width(padded.as_str()), 10);
    }

    #[test]
    fn clip_visible_trims_and_appends_ellipsis() {
        let out = clip_visible("abcdefghij", 5);
        assert!(out.ends_with('…'));
        assert!(UnicodeWidthStr::width(out.as_str()) <= 5);
    }

    #[test]
    fn clip_visible_fits_exactly_returns_input_unchanged() {
        // Regression (review r1 finding #1): clip_visible used to always
        // append `…`, so a string that already fit the budget would come
        // back as `max_cols + 1` wide. Exact-width fast path fixes it.
        let out = clip_visible("abcde", 5);
        assert_eq!(out, "abcde");
        assert!(!out.ends_with('…'));
    }

    #[test]
    fn clip_visible_under_width_returns_input_unchanged() {
        let out = clip_visible("abc", 5);
        assert_eq!(out, "abc");
    }

    #[test]
    fn village_short_name_uses_compact_place_nouns() {
        // Per-village mapping — the raw display_name prefix "The" /
        // "Border" / "Dockside" / "Strata" isn't useful, so we pick
        // the flavourful noun per id.
        assert_eq!(village_short_name("rust"), "Rust Forge-Ruins");
        assert_eq!(village_short_name("python"), "Python Scriptorium");
        assert_eq!(village_short_name("typescript"), "TypeScript Garrison");
        assert_eq!(village_short_name("go"), "Go Dockside");
        assert_eq!(village_short_name("javascript"), "JavaScript Strata");
        // Unknown id falls through to raw id so we never blow up on a
        // stale DB row.
        assert_eq!(village_short_name("nonexistent"), "nonexistent");
    }

    #[test]
    fn kill_breakdown_fits_card_width_at_4_digit_counts() {
        let mut data = sample_data();
        data.kills_by_kind.insert("zombie".to_string(), 9999);
        data.kills_by_kind.insert("ghost".to_string(), 9999);
        let out = render(&data);
        for line in out.lines() {
            if !line.starts_with('║') {
                continue;
            }
            let inner: String = line.chars().skip(1).take(line.chars().count() - 2).collect();
            assert_eq!(
                UnicodeWidthStr::width(inner.as_str()),
                CARD_WIDTH,
                "line overflowed at 4-digit kill counts: {line:?}"
            );
        }
    }
}
