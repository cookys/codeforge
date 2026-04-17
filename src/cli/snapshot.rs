//! `codeforge snapshot [--days N]` — Phase 3f CLI surface.
//!
//! Glue: open DB → `snapshot::collect` → `snapshot::render` → stdout.
//! Output is pure text so the user can pipe / copy-paste freely.

use anyhow::{bail, Result};

use crate::db;
use crate::snapshot::{collect, render::render, DEFAULT_WINDOW_DAYS};

pub fn run(ctx: &db::Context, days: Option<i64>) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    let window = days.unwrap_or(DEFAULT_WINDOW_DAYS);
    if window <= 0 {
        // Negative / zero windows would collapse the SQL filter into
        // "now >= now" and exclude every row — surface the error instead
        // of producing a silently empty card.
        bail!("--days 必須為正整數 (收到 {})", window);
    }

    let data = collect(&conn, window)?;
    let card = render(&data);
    print!("{}", card);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn scratch_ctx() -> (TempDir, db::Context) {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join(".codeforge");
        std::fs::create_dir_all(project_dir.join("store")).unwrap();
        let db_path = temp.path().join("state.db");
        let ctx = db::Context {
            project_dir,
            brain_dir: temp.path().join("brain"),
            db_path,
        };
        // Prime the DB with migrations so the first call in `run` finds
        // the tables it needs.
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        drop(conn);
        (temp, ctx)
    }

    fn open_and_seed(ctx: &db::Context) -> Connection {
        let conn = ctx.open_db().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn run_rejects_zero_days() {
        let (_t, ctx) = scratch_ctx();
        let err = run(&ctx, Some(0)).unwrap_err();
        assert!(err.to_string().contains("正整數"));
    }

    #[test]
    fn run_rejects_negative_days() {
        let (_t, ctx) = scratch_ctx();
        let err = run(&ctx, Some(-1)).unwrap_err();
        assert!(err.to_string().contains("正整數"));
    }

    #[test]
    fn collect_integration_produces_renderable_card_after_seed() {
        // End-to-end: pet row + combat_log + world → collect → render.
        // Runs `collect` directly (not `run`) so stdout stays clean.
        let (_t, ctx) = scratch_ctx();
        let conn = open_and_seed(&ctx);

        conn.execute(
            "INSERT INTO pet (id, village, name, level, xp, xp_to_next,
                              atk, hp, def, sup, ver)
             VALUES (1, 'rust', 'Ferris', 11, 0, 100, 12, 80, 10, 10, 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_name, mob_kind, xp_gained, occurred_at)
             VALUES ('rust', 'refactor-boss', 'boss', 50, datetime('now', '-2 days'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO combat_log (zone_id, mob_name, mob_kind, xp_gained, occurred_at)
             VALUES ('rust', 'todo-elite', 'elite', 30, datetime('now', '-1 days'))",
            [],
        )
        .unwrap();

        let data = collect(&conn, 30).unwrap();
        let card = render(&data);
        assert!(card.contains("Ferris"));
        assert!(card.contains("Boss 1"));
        assert!(card.contains("Elite 1"));
        assert!(card.contains("擊殺"));
        assert!(card.contains("Lv.11"));
    }

    /// Guards the `ctx.project_dir` contract: the CLI calls
    /// `ensure_initialized` which must point at an existing directory.
    /// Regression test for the silent-failure path where the project
    /// dir was swapped out from under the helper.
    #[test]
    fn ensure_initialized_required_before_run() {
        let temp = TempDir::new().unwrap();
        let ctx = db::Context {
            project_dir: PathBuf::from(temp.path()).join("does-not-exist"),
            brain_dir: temp.path().join("brain"),
            db_path: temp.path().join("state.db"),
        };
        let err = run(&ctx, Some(30)).unwrap_err();
        assert!(err.to_string().contains(".codeforge"));
    }
}
