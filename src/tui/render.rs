//! Frame composition + paint — Phase 2c P5.
//!
//! `build_frame` assembles the three panels into absolute-position
//! string chunks keyed by (col, row). `paint` writes them to stdout
//! using crossterm's absolute positioning. Separating composition from
//! paint means tests can verify layout without touching a real terminal.

use anyhow::Result;
use crossterm::{cursor, queue, style::Print, terminal};
use rusqlite::Connection;
use std::io::Write;
use std::path::Path;

use super::layout::{compute, Layout, LayoutMode};
use super::local_map::compute as compute_local_map;
use super::panels::local_map::LocalMapPanel;
use super::panels::{combat_log, local_map as map_panel, pet as pet_panel, zoa::ZoaPanel};
use crate::pet::live_state::LiveState;

/// A fully-composed frame ready for paint. Each line carries an
/// absolute (col, row) position — no relative cursor math at paint time.
#[derive(Debug)]
pub struct Frame {
    pub lines: Vec<PositionedLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedLine {
    pub col: u16,
    pub row: u16,
    pub text: String,
}

/// Load all data from DB and compose the full frame for the given
/// terminal size. Does not touch stdout; callable from tests. When the
/// pet snapshot / combat log are missing (fresh install) we render a
/// graceful placeholder instead of returning an error.
///
/// `welcome_override`: when `Some`, the Welcome Back Report lines (Phase
/// 3d §3.1) take over the combat-log region for that frame. Callers are
/// expected to pass `Some` on the very first paint and `None` thereafter.
///
/// `zoa`: when `Some` AND the layout is Wide, the Zoa sprite renders into
/// its reserved left column. Tests that don't exercise Wide mode can pass
/// `None` to skip the panel entirely.
///
/// `local_map`: when `Some`, routes through the display-mode dispatcher
/// (List / Grid) honouring the panel's current `display_mode`. `None`
/// preserves the Phase 2c behaviour (default List mode, no toggle
/// support) so legacy tests keep passing without alteration.
#[allow(clippy::too_many_arguments)] // composes four independent panels + two
// overrides; splitting into a builder would hide the test call-site intent
pub fn build_frame(
    conn: &Connection,
    scan_root: &Path,
    cwd: Option<&Path>,
    cols: u16,
    rows: u16,
    welcome_override: Option<&[String]>,
    zoa: Option<&ZoaPanel>,
    local_map: Option<&LocalMapPanel>,
) -> Result<Frame> {
    let layout = compute(cols, rows);

    let pet_lines = match LiveState::load(conn)? {
        Some(live) => {
            // Phase 3b: strategy falls back to the baseline Explorer when
            // the snapshot predates v6 (wait-for-tick window is <60s).
            let strategy = live.strategy.unwrap_or(
                crate::daemon::strategy::DEFAULT_STRATEGY,
            );
            pet_panel::render(&live.state, strategy, layout.pet_status.width as usize)
        }
        None => placeholder_lines(
            "尚未 adopt 任何寵物 — 執行 `codeforge adopt`",
            layout.pet_status.width as usize,
            layout.pet_status.height as usize,
        ),
    };

    // Skip the DB scan + render work entirely when the mode hides the
    // map (Narrow/Compact) — saves both a disk walk and a SQL query.
    let map_lines: Vec<String> = if layout.local_map.is_empty() {
        Vec::new()
    } else {
        let rooms = compute_local_map(conn, scan_root, cwd)?;
        let default_panel = LocalMapPanel::new();
        let panel = local_map.unwrap_or(&default_panel);
        map_panel::render(
            panel,
            &rooms,
            layout.local_map.width as usize,
            layout.local_map.height as usize,
        )
    };

    // Compact mode has no bottom strip at all — skip both the log DB
    // query and the welcome override. Narrow keeps the log as its sole
    // bottom widget.
    let log_lines: Vec<String> = if layout.combat_log.is_empty() {
        Vec::new()
    } else {
        match welcome_override {
            Some(welcome) => render_welcome_lines(
                welcome,
                layout.combat_log.width as usize,
                layout.combat_log.height as usize,
            ),
            None => {
                let log_rows = load_combat_log(conn, layout.combat_log.height as usize)?;
                // Phase 3c P5: pull commentary feed into the same panel.
                // -1 so either stream on its own could fill the panel up to
                // its header+body limit — merge truncates to max_rows-1 anyway.
                let commentary_rows = crate::commentary::display::recent_commentary(
                    conn,
                    layout.combat_log.height as usize,
                )
                .unwrap_or_default();
                combat_log::render_mixed(
                    &log_rows,
                    &commentary_rows,
                    layout.combat_log.width as usize,
                    layout.combat_log.height as usize,
                )
            }
        }
    };

    // Zoa renders only in Wide mode (the only mode where layout.zoa is
    // non-empty). Callers may still pass `Some(panel)` in other modes —
    // we just skip rendering then.
    let zoa_lines: Vec<String> = match (zoa, layout.mode) {
        (Some(panel), LayoutMode::Wide) => panel.render(
            layout.zoa.width as usize,
            layout.zoa.height as usize,
        ),
        _ => Vec::new(),
    };

    Ok(Frame {
        lines: compose(&layout, &pet_lines, &zoa_lines, &map_lines, &log_lines),
    })
}

/// Pad a welcome-back greeting to the combat-log panel's dimensions. The
/// greeting arrives from `pet::session::WelcomeBackSummary::render_lines`
/// as plain strings; here we add a panel title, cap to `max_rows`, and
/// pad short lines to `width`.
fn render_welcome_lines(welcome: &[String], width: usize, max_rows: usize) -> Vec<String> {
    use super::panels::pad_to_width;
    let mut out = Vec::with_capacity(max_rows);
    out.push(pad_to_width("💤 歸來摘要", width));
    for line in welcome.iter().take(max_rows.saturating_sub(1)) {
        out.push(pad_to_width(line, width));
    }
    while out.len() < max_rows {
        out.push(pad_to_width("", width));
    }
    out
}

/// Paint a pre-built frame to stdout. Clears the alt screen once before
/// writing so stale content from a smaller previous frame doesn't leak.
pub fn paint(frame: &Frame, out: &mut impl Write) -> Result<()> {
    queue!(out, terminal::Clear(terminal::ClearType::All))?;
    for line in &frame.lines {
        if line.text.is_empty() {
            continue;
        }
        queue!(
            out,
            cursor::MoveTo(line.col, line.row),
            Print(&line.text),
        )?;
    }
    out.flush()?;
    Ok(())
}

fn compose(
    layout: &Layout,
    pet_lines: &[String],
    zoa_lines: &[String],
    map_lines: &[String],
    log_lines: &[String],
) -> Vec<PositionedLine> {
    let cap = pet_lines.len() + zoa_lines.len() + map_lines.len() + log_lines.len();
    let mut out = Vec::with_capacity(cap);
    for (i, text) in pet_lines.iter().enumerate() {
        out.push(PositionedLine {
            col: layout.pet_status.x,
            row: layout.pet_status.y + i as u16,
            text: text.clone(),
        });
    }
    for (i, text) in zoa_lines.iter().enumerate() {
        out.push(PositionedLine {
            col: layout.zoa.x,
            row: layout.zoa.y + i as u16,
            text: text.clone(),
        });
    }
    for (i, text) in map_lines.iter().enumerate() {
        out.push(PositionedLine {
            col: layout.local_map.x,
            row: layout.local_map.y + i as u16,
            text: text.clone(),
        });
    }
    for (i, text) in log_lines.iter().enumerate() {
        out.push(PositionedLine {
            col: layout.combat_log.x,
            row: layout.combat_log.y + i as u16,
            text: text.clone(),
        });
    }
    out
}

fn placeholder_lines(msg: &str, width: usize, height: usize) -> Vec<String> {
    use super::panels::pad_to_width;
    let mut out = Vec::with_capacity(height);
    if height > 0 {
        out.push(pad_to_width(msg, width));
    }
    while out.len() < height {
        out.push(pad_to_width("", width));
    }
    out
}

fn load_combat_log(conn: &Connection, max: usize) -> Result<Vec<combat_log::CombatLogRow>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT mob_kind, mob_name, xp_gained, loot, occurred_at
         FROM combat_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![max as i64], |r| {
            Ok(combat_log::CombatLogRow {
                mob_kind: r.get(0)?,
                mob_name: r.get(1)?,
                xp_gained: r.get(2)?,
                loot: r.get(3)?,
                occurred_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn build_frame_on_empty_db_shows_placeholder() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20, None, None, None).unwrap();
        // Top row should be the no-pet placeholder
        let top = frame
            .lines
            .iter()
            .find(|l| l.row == 0)
            .expect("top line exists");
        assert!(top.text.contains("adopt"));
    }

    #[test]
    fn build_frame_with_snapshot_renders_pet_name() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO pet_snapshot
                (id, village, level, hp, hp_max, xp, xp_to_next,
                 atk, def, sup, ver, last_message, updated_at)
             VALUES (1, 'rust', 4, 80, 100, 200, 500,
                     15, 12, 10, 11, NULL, datetime('now'))",
            [],
        )
        .unwrap();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20, None, None, None).unwrap();
        let has_ferris = frame.lines.iter().any(|l| l.text.contains("Ferris"));
        assert!(has_ferris, "pet name must appear in frame");
    }

    #[test]
    fn build_frame_includes_local_map_header() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20, None, None, None).unwrap();
        assert!(frame.lines.iter().any(|l| l.text.contains("Local Map")));
    }

    #[test]
    fn build_frame_includes_combat_log_header() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20, None, None, None).unwrap();
        assert!(frame.lines.iter().any(|l| l.text.contains("Combat Log")));
    }

    #[test]
    fn build_frame_combat_log_pulls_recent_rows() {
        let conn = fresh();
        for i in 0..3 {
            conn.execute(
                "INSERT INTO combat_log
                    (zone_id, mob_kind, mob_name, xp_gained, occurred_at)
                 VALUES ('rust', 'ghost', ?1, ?2, '2026-04-17 14:00:00')",
                rusqlite::params![format!("mob-{i}"), (i + 1) * 5],
            )
            .unwrap();
        }
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20, None, None, None).unwrap();
        // Most-recent-first: "mob-2" should be in frame somewhere
        assert!(frame.lines.iter().any(|l| l.text.contains("mob-2")));
    }

    #[test]
    fn build_frame_respects_layout_positions() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 100, 40, None, None, None).unwrap();
        // Pet status lines should be at rows 0..3
        let pet_rows: Vec<u16> = frame
            .lines
            .iter()
            .filter(|l| l.col == 0 && l.row < 3)
            .map(|l| l.row)
            .collect();
        assert!(!pet_rows.is_empty());
        // Combat log should be at col 40 (40% of 100)
        assert!(frame
            .lines
            .iter()
            .any(|l| l.col == 40 && l.row >= 3));
    }

    #[test]
    fn paint_does_not_panic_on_in_memory_sink() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 60, 15, None, None, None).unwrap();
        let mut sink: Vec<u8> = Vec::new();
        paint(&frame, &mut sink).unwrap();
        assert!(!sink.is_empty(), "paint must write something");
    }

    /// Perf KR: a full `build_frame` + `paint` to an in-memory sink
    /// should average well under 16ms (60 FPS budget) on typical
    /// workloads. Prime one frame to warm caches; measure the next 50.
    #[test]
    fn render_budget_under_16ms_average() {
        let conn = fresh();
        // Seed realistic state: 1 pet_snapshot + 20 combat_log + 10 mobs
        conn.execute(
            "INSERT INTO pet_snapshot
                (id, village, level, hp, hp_max, xp, xp_to_next,
                 atk, def, sup, ver, last_message, updated_at)
             VALUES (1, 'rust', 5, 80, 100, 420, 1000,
                     18, 12, 10, 11, NULL, datetime('now'))",
            [],
        )
        .unwrap();
        for i in 0..20 {
            conn.execute(
                "INSERT INTO combat_log
                    (zone_id, mob_kind, mob_name, xp_gained, occurred_at)
                 VALUES ('rust', 'ghost', ?1, ?2, '2026-04-17 14:00:00')",
                rusqlite::params![format!("mob-{i}"), i * 5],
            )
            .unwrap();
        }
        for i in 0..10 {
            conn.execute(
                "INSERT INTO mobs
                    (zone_id, kind, name, hp, hp_max, atk, def, spawned_at, origin_path)
                 VALUES ('rust', 'zombie', ?1, 10, 10, 1, 1, 100, ?2)",
                rusqlite::params![format!("m-{i}"), format!("src/m{i}.rs")],
            )
            .unwrap();
        }

        // Prime
        let _ = build_frame(&conn, Path::new("/repo"), None, 100, 40, None, None, None).unwrap();
        let mut sink: Vec<u8> = Vec::new();

        const N: u32 = 50;
        let start = std::time::Instant::now();
        for _ in 0..N {
            let frame = build_frame(&conn, Path::new("/repo"), None, 100, 40, None, None, None).unwrap();
            sink.clear();
            paint(&frame, &mut sink).unwrap();
        }
        let avg_us = start.elapsed().as_micros() / N as u128;
        assert!(
            avg_us < 16_000,
            "render avg {avg_us}µs exceeds 16ms budget (60 FPS target)"
        );
    }

    #[test]
    fn build_frame_tiny_terminal_does_not_panic() {
        let conn = fresh();
        // 10x3 — only pet row, no bottom panels
        let frame = build_frame(&conn, Path::new("/repo"), None, 10, 3, None, None, None).unwrap();
        // Frame should contain pet rows; bottom panels may be 0-height so
        // their lines are all empty strings (we skip empty at paint time).
        assert!(frame.lines.iter().any(|l| l.row < 3));
    }

    #[test]
    fn build_frame_with_welcome_override_replaces_combat_log_header() {
        let conn = fresh();
        // Seed a real combat_log row that would normally appear.
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_kind, mob_name, xp_gained, occurred_at)
             VALUES ('rust', 'ghost', 'x', 5, '2026-04-17 14:00:00')",
            [],
        )
        .unwrap();

        let welcome = vec![
            "你不在的 8 小時：".to_string(),
            "  → 擊殺 ghost ×5".to_string(),
        ];
        let frame =
            build_frame(&conn, Path::new("/repo"), None, 100, 30, Some(&welcome), None, None).unwrap();

        // The welcome-back title must appear (it replaces the normal
        // "⚔ Combat Log" header).
        assert!(
            frame.lines.iter().any(|l| l.text.contains("歸來摘要")),
            "welcome-back title must appear in frame"
        );
        // The real combat_log row should NOT appear in the override frame.
        assert!(
            !frame.lines.iter().any(|l| l.text.contains("ghost — x")),
            "override frame must not show real combat_log content"
        );
    }

    // ─── Phase tui-foundation P4: adaptive-layout integration ──────

    #[test]
    fn build_frame_narrow_mode_omits_map_panel() {
        // 72 cols is inside the Narrow breakpoint — local_map should
        // collapse to zero rows (no "📍 Local Map" header).
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 72, 20, None, None, None).unwrap();
        assert!(
            !frame.lines.iter().any(|l| l.text.contains("Local Map")),
            "Narrow mode must not render the Local Map panel"
        );
        // CombatLog panel still there — it's the sole bottom widget.
        assert!(frame.lines.iter().any(|l| l.text.contains("Combat Log")));
    }

    #[test]
    fn build_frame_compact_mode_produces_only_pet_header() {
        // 50 cols → Compact. Layout returns empty rects for bottom panels
        // even when called (caller is responsible for not entering alt-screen).
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 50, 20, None, None, None).unwrap();
        // No Combat Log / Local Map at all.
        assert!(!frame.lines.iter().any(|l| l.text.contains("Combat Log")));
        assert!(!frame.lines.iter().any(|l| l.text.contains("Local Map")));
        // Pet placeholder must still appear so the caller can at least
        // display something before bailing.
        assert!(frame.lines.iter().any(|l| l.text.contains("adopt")));
    }

    #[test]
    fn build_frame_wide_mode_renders_zoa_at_column_zero() {
        // 140 cols → Wide. Passing Some(ZoaPanel) should render it at x=0.
        let conn = fresh();
        let zoa = super::ZoaPanel::new();
        let frame = build_frame(&conn, Path::new("/repo"), None, 140, 30, None, Some(&zoa), None).unwrap();
        // Zoa occupies col 0 at rows >= 3 (below the pet header). Look for
        // the top frame row which contains "_______".
        let zoa_top_row = frame.lines.iter().find(|l| l.col == 0 && l.row >= 3 && l.text.contains("_______"));
        assert!(
            zoa_top_row.is_some(),
            "Wide mode must render Zoa's top skull-cap row at col 0"
        );
    }

    #[test]
    fn build_frame_standard_mode_does_not_render_zoa_even_if_provided() {
        // 100 cols → Standard. Passing Some(ZoaPanel) must NOT render Zoa
        // (there's no reserved column for it).
        let conn = fresh();
        let zoa = super::ZoaPanel::new();
        let frame = build_frame(&conn, Path::new("/repo"), None, 100, 30, None, Some(&zoa), None).unwrap();
        assert!(
            !frame.lines.iter().any(|l| l.text.contains("_______")),
            "Standard mode must not render Zoa frames"
        );
    }

    #[test]
    fn build_frame_wide_mode_zoa_sits_left_of_map() {
        // Wide at 140 cols: zoa x=0, map x=24, log x=24+map_w.
        let conn = fresh();
        let zoa = super::ZoaPanel::new();
        let frame = build_frame(&conn, Path::new("/repo"), None, 140, 30, None, Some(&zoa), None).unwrap();
        // Local Map header must appear at col 24 (ZOA_WIDTH).
        let map_header = frame
            .lines
            .iter()
            .find(|l| l.text.contains("Local Map"))
            .expect("map header must exist in Wide mode");
        assert_eq!(
            map_header.col, 24,
            "Local Map must sit immediately after the 24-col Zoa slab"
        );
    }

    #[test]
    fn build_frame_without_welcome_override_shows_real_combat_log() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_kind, mob_name, xp_gained, occurred_at)
             VALUES ('rust', 'ghost', 'unique-marker', 5, '2026-04-17 14:00:00')",
            [],
        )
        .unwrap();
        let frame = build_frame(&conn, Path::new("/repo"), None, 100, 30, None, None, None).unwrap();
        assert!(
            frame.lines.iter().any(|l| l.text.contains("unique-marker")),
            "normal frame must surface real combat_log rows"
        );
        assert!(
            !frame.lines.iter().any(|l| l.text.contains("歸來摘要")),
            "no welcome-back title without override"
        );
    }
}
