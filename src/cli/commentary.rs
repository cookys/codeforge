//! `codeforge commentary` — Phase 3c CLI surface for AI Commentary.
//!
//! Sub-commands:
//!   on          enable the opt-in flag (settings table)
//!   off         disable the opt-in flag
//!   list [--n]  print the latest N feed rows
//!   test [kind] manually trigger a commentary (bypasses rate limit; useful
//!               for verifying the pipeline end-to-end without a daemon)

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use crate::commentary::{
    budget::OPT_IN_ENV,
    generator::{self, GenContext},
    repo::{self, InsertFeed},
    Kind, Tone,
};
use crate::daemon::{mob::MobKind, strategy::DEFAULT_STRATEGY};
use crate::db;
use crate::pet::village::VILLAGES;

pub enum CommentaryCmd {
    On,
    Off,
    List { n: u32 },
    Test { kind: Option<String> },
}

pub fn run(ctx: &db::Context, cmd: CommentaryCmd) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    match cmd {
        CommentaryCmd::On => enable(&conn),
        CommentaryCmd::Off => disable(&conn),
        CommentaryCmd::List { n } => list(&conn, n),
        CommentaryCmd::Test { kind } => test(&conn, kind.as_deref()),
    }
}

fn enable(conn: &Connection) -> Result<()> {
    repo::set_opt_in_flag(conn, true)?;
    println!("✓ commentary 已開啟（settings.commentary_opt_in = 1）");
    println!("  env {OPT_IN_ENV}=1 也可同等啟用；兩者取 OR。");
    Ok(())
}

fn disable(conn: &Connection) -> Result<()> {
    repo::set_opt_in_flag(conn, false)?;
    println!("✓ commentary 已關閉（settings.commentary_opt_in = 0）");
    if std::env::var(OPT_IN_ENV).is_ok() {
        println!("  注意：env {OPT_IN_ENV} 仍有設；unset 才能完全關閉。");
    }
    Ok(())
}

fn list(conn: &Connection, n: u32) -> Result<()> {
    let rows = repo::recent_feed(conn, n)?;
    if rows.is_empty() {
        println!("（尚無 commentary — 啟用後等 daemon 觸發）");
        return Ok(());
    }
    for row in rows {
        let when = format_ts(row.created_at);
        println!(
            "[{when}] {kind:<13} ({source}) {phrase}",
            when = when,
            kind = row.kind.as_str(),
            source = row.source.as_str(),
            phrase = row.phrase,
        );
    }
    Ok(())
}

fn test(conn: &Connection, kind_arg: Option<&str>) -> Result<()> {
    let kind = match kind_arg {
        Some(s) => Kind::from_str(s)
            .ok_or_else(|| anyhow::anyhow!("未知 commentary kind：{}，可用：boss_kill/level_up/session_long/long_idle/zone_unlock", s))?,
        None => Kind::ManualTest,
    };

    // Build a synthetic GenContext from persisted state. We intentionally
    // do NOT go through dispatch::decide_and_reserve — this command is
    // the escape hatch for verifying the rule-based pipeline without
    // touching the budget. Haiku is not attempted here (kept sync + offline
    // for predictability in terminal test runs).
    let (village, level, hp, mood, strategy) = load_pet_context(conn)?;
    let pet_name = lookup_pet_name(conn, &village)?;
    let tone = Tone::new(village, strategy);
    let now = now_unix();

    let trigger = synth_trigger(kind, level);
    let gen_ctx = GenContext {
        pet_name: &pet_name,
        pet_level: level,
        pet_hp: hp,
        pet_mood: mood,
        tone: tone.clone(),
        trigger: &trigger,
    };
    let generated = generator::generate_rule_based(conn, &gen_ctx, now as u64, now)?;

    // Persist so the statusline/TUI surface it in the next refresh.
    repo::insert_feed(
        conn,
        InsertFeed {
            created_at: now,
            tick_count: 0,
            kind,
            tone: &tone,
            phrase: &generated.phrase,
            phrase_hash: &generated.phrase_hash,
            source: generated.source,
        },
    )?;
    repo::record_history(conn, &generated.phrase_hash, kind, now)?;

    println!("✓ 測試 commentary（{}, {}）：", kind.as_str(), generated.source.as_str());
    println!("  {}", generated.phrase);
    println!("  已寫入 commentary_feed，下次 statusline / TUI refresh 可見。");
    Ok(())
}

fn synth_trigger(kind: Kind, level: u32) -> crate::commentary::trigger::Trigger {
    use crate::commentary::trigger::Trigger;
    match kind {
        Kind::BossKill => Trigger::BossKill {
            mob_name: "測試 Boss".into(),
            mob_kind: MobKind::Boss,
        },
        Kind::LevelUp => Trigger::LevelUp {
            prev_level: level.saturating_sub(1),
            new_level: level,
        },
        Kind::SessionLong => Trigger::SessionLong { duration_secs: 14_400 },
        Kind::LongIdle => Trigger::LongIdle { absence_secs: 1800 },
        Kind::ZoneUnlock => Trigger::ZoneUnlock { zone_id: "test".into() },
        // For ManualTest, use a BossKill-shaped trigger so the rule pool
        // has something non-trivial to render.
        Kind::ManualTest => Trigger::BossKill {
            mob_name: "pipeline".into(),
            mob_kind: MobKind::Boss,
        },
    }
}

/// Returns (village, level, hp, mood, strategy_str).
fn load_pet_context(conn: &Connection) -> Result<(String, u32, u32, u8, String)> {
    let row: Option<(String, i64, i64, i64, String)> = conn
        .query_row(
            "SELECT village, level, hp, mood, strategy
             FROM pet_snapshot WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    match row {
        Some((v, l, h, m, s)) => {
            let level = l.clamp(1, i64::from(u32::MAX)) as u32;
            let hp = h.clamp(0, i64::from(u32::MAX)) as u32;
            let mood = m.clamp(0, 100) as u8;
            Ok((v, level, hp, mood, s))
        }
        None => {
            // Fresh install: no snapshot row yet. Fall back to a safe Rust-
            // village default matching pet::state defaults. This lets
            // `commentary test` succeed before the first daemon tick.
            Ok((
                "rust".into(),
                1,
                10,
                60,
                DEFAULT_STRATEGY.as_str().to_string(),
            ))
        }
    }
}

fn lookup_pet_name(conn: &Connection, village_id: &str) -> Result<String> {
    let from_row: Option<String> = conn
        .query_row("SELECT name FROM pet WHERE id = 1", [], |r| r.get(0))
        .optional()?;
    if let Some(name) = from_row {
        return Ok(name);
    }
    Ok(VILLAGES
        .iter()
        .find(|v| v.id == village_id)
        .map(|v| v.pet_name.to_string())
        .unwrap_or_else(|| "Pet".to_string()))
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_ts(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn enable_sets_flag() {
        let conn = fresh();
        assert!(!repo::opt_in_flag(&conn).unwrap());
        repo::set_opt_in_flag(&conn, false).unwrap();
        // Test runs the pure repo write — the CLI enable() prints to stdout
        // so we don't invoke it directly in unit tests.
        repo::set_opt_in_flag(&conn, true).unwrap();
        assert!(repo::opt_in_flag(&conn).unwrap());
    }

    #[test]
    fn disable_clears_flag() {
        let conn = fresh();
        repo::set_opt_in_flag(&conn, true).unwrap();
        repo::set_opt_in_flag(&conn, false).unwrap();
        assert!(!repo::opt_in_flag(&conn).unwrap());
    }

    #[test]
    fn load_pet_context_uses_defaults_when_no_snapshot() {
        let conn = fresh();
        let (village, level, hp, mood, strategy) = load_pet_context(&conn).unwrap();
        assert_eq!(village, "rust");
        assert_eq!(level, 1);
        assert_eq!(hp, 10);
        assert_eq!(mood, 60);
        assert_eq!(strategy, DEFAULT_STRATEGY.as_str());
    }

    #[test]
    fn load_pet_context_reads_snapshot_row() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, mood, strategy)
             VALUES (1, 'python', 5, 45, 60, 100, 500,
                     18, 14, 12, 16, 80, 'scholar')",
            [],
        )
        .unwrap();
        let (village, level, hp, mood, strategy) = load_pet_context(&conn).unwrap();
        assert_eq!(village, "python");
        assert_eq!(level, 5);
        assert_eq!(hp, 45);
        assert_eq!(mood, 80);
        assert_eq!(strategy, "scholar");
    }

    #[test]
    fn test_command_inserts_feed_row() {
        let conn = fresh();
        // Silences stdout by calling test() then checking DB state
        test(&conn, Some("manual_test")).unwrap();
        let rows = crate::commentary::display::recent_commentary(&conn, 5).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].phrase.is_empty());
        // History recorded
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM commentary_history",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn test_command_default_kind_is_manual_test() {
        let conn = fresh();
        test(&conn, None).unwrap();
        let kind: String = conn
            .query_row("SELECT kind FROM commentary_feed ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind, "manual_test");
    }

    #[test]
    fn test_command_bypasses_rate_limit() {
        // Even if the budget has a recent reservation, test() should still
        // emit — ManualTest bypasses can_emit per spec.
        let conn = fresh();
        repo::budget_reserve(&conn, 9_999_999_999).unwrap();
        test(&conn, Some("manual_test")).unwrap();
        let rows = repo::recent_feed(&conn, 5).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_command_all_kinds_produce_phrase() {
        for kind in [
            "boss_kill",
            "level_up",
            "session_long",
            "long_idle",
            "zone_unlock",
            "manual_test",
        ] {
            let conn = fresh();
            test(&conn, Some(kind)).unwrap();
            let rows = repo::recent_feed(&conn, 1).unwrap();
            assert_eq!(rows.len(), 1, "kind {kind} produced nothing");
            assert!(!rows[0].phrase.is_empty(), "kind {kind} produced empty phrase");
        }
    }

    #[test]
    fn test_command_rejects_unknown_kind() {
        let conn = fresh();
        let err = test(&conn, Some("nonsense")).unwrap_err();
        assert!(err.to_string().contains("未知 commentary kind"));
    }

    #[test]
    fn format_ts_handles_epoch() {
        let s = format_ts(0);
        // "01-01 00:00" for unix epoch in UTC
        assert_eq!(s, "01-01 00:00");
    }
}
