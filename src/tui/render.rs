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

use super::layout::{compute, Layout};
use super::local_map::compute as compute_local_map;
use super::panels::{combat_log, local_map as map_panel, pet as pet_panel};
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
pub fn build_frame(
    conn: &Connection,
    scan_root: &Path,
    cwd: Option<&Path>,
    cols: u16,
    rows: u16,
) -> Result<Frame> {
    let layout = compute(cols, rows);

    let pet_lines = match LiveState::load(conn)? {
        Some(live) => pet_panel::render(&live.state, layout.pet_status.width as usize),
        None => placeholder_lines(
            "尚未 adopt 任何寵物 — 執行 `codeforge adopt`",
            layout.pet_status.width as usize,
            layout.pet_status.height as usize,
        ),
    };

    let rooms = compute_local_map(conn, scan_root, cwd)?;
    let map_lines = map_panel::render(
        &rooms,
        layout.local_map.width as usize,
        layout.local_map.height as usize,
    );

    let log_rows = load_combat_log(conn, layout.combat_log.height as usize)?;
    let log_lines = combat_log::render(
        &log_rows,
        layout.combat_log.width as usize,
        layout.combat_log.height as usize,
    );

    Ok(Frame {
        lines: compose(&layout, &pet_lines, &map_lines, &log_lines),
    })
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
    map_lines: &[String],
    log_lines: &[String],
) -> Vec<PositionedLine> {
    let mut out = Vec::with_capacity(pet_lines.len() + map_lines.len() + log_lines.len());
    for (i, text) in pet_lines.iter().enumerate() {
        out.push(PositionedLine {
            col: layout.pet_status.x,
            row: layout.pet_status.y + i as u16,
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
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20).unwrap();
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
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20).unwrap();
        let has_ferris = frame.lines.iter().any(|l| l.text.contains("Ferris"));
        assert!(has_ferris, "pet name must appear in frame");
    }

    #[test]
    fn build_frame_includes_local_map_header() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20).unwrap();
        assert!(frame.lines.iter().any(|l| l.text.contains("Local Map")));
    }

    #[test]
    fn build_frame_includes_combat_log_header() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20).unwrap();
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
        let frame = build_frame(&conn, Path::new("/repo"), None, 80, 20).unwrap();
        // Most-recent-first: "mob-2" should be in frame somewhere
        assert!(frame.lines.iter().any(|l| l.text.contains("mob-2")));
    }

    #[test]
    fn build_frame_respects_layout_positions() {
        let conn = fresh();
        let frame = build_frame(&conn, Path::new("/repo"), None, 100, 40).unwrap();
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
        let frame = build_frame(&conn, Path::new("/repo"), None, 60, 15).unwrap();
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
        let _ = build_frame(&conn, Path::new("/repo"), None, 100, 40).unwrap();
        let mut sink: Vec<u8> = Vec::new();

        const N: u32 = 50;
        let start = std::time::Instant::now();
        for _ in 0..N {
            let frame = build_frame(&conn, Path::new("/repo"), None, 100, 40).unwrap();
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
        let frame = build_frame(&conn, Path::new("/repo"), None, 10, 3).unwrap();
        // Frame should contain pet rows; bottom panels may be 0-height so
        // their lines are all empty strings (we skip empty at paint time).
        assert!(frame.lines.iter().any(|l| l.row < 3));
    }
}
