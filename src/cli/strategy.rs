//! `codeforge strategy [name]` — Phase 3b CLI surface for Strategy Mode.
//!
//! No args → print current strategy + multiplier summary.
//! With name → validate, write `settings.pet_strategy`, confirm.
//!
//! The daemon owns `pet_snapshot`; the CLI never writes it. The daemon
//! calls `GameWorld::refresh_strategy_from_db` at tick step 3b, which
//! copies `settings.pet_strategy` into the ECS `PetStrategy` component and
//! serialises it into `pet_snapshot.strategy`. Readers prefer the setting
//! (see `read_current`) so the choice shows before the first tick.

use anyhow::{bail, Result};
use rusqlite::OptionalExtension;

use crate::daemon::strategy::{Strategy, DEFAULT_STRATEGY};
use crate::db;

pub fn run(ctx: &db::Context, name: Option<&str>) -> Result<()> {
    ctx.ensure_initialized()?;
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    match name {
        None => {
            let current = read_current(&conn)?;
            println!("{}", format_status(current));
        }
        Some(raw) => {
            let parsed = match Strategy::from_str(raw) {
                Some(s) => s,
                None => {
                    let options = Strategy::ALL
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    bail!("未知策略「{}」，可用：{}", raw, options);
                }
            };
            upsert_strategy(&conn, parsed)?;
            println!("✓ 策略切換為 {}", parsed.as_str());
            println!(
                "  ATK 倍率 {:.2}x  DEF 倍率 {:.2}x",
                parsed.atk_mult(),
                parsed.def_mult()
            );
        }
    }
    Ok(())
}

/// Read the chosen strategy: `settings.pet_strategy`, else
/// `pet_snapshot.strategy`, else `DEFAULT_STRATEGY`.
pub(crate) fn read_current(conn: &rusqlite::Connection) -> Result<Strategy> {
    if let Some(s) = crate::daemon::strategy::read_setting(conn)? {
        return Ok(s);
    }
    let stored: Option<String> = conn
        .query_row("SELECT strategy FROM pet_snapshot WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(stored
        .and_then(|s| Strategy::from_str(&s))
        .unwrap_or(DEFAULT_STRATEGY))
}

fn upsert_strategy(conn: &rusqlite::Connection, s: Strategy) -> Result<()> {
    crate::daemon::strategy::write_setting(conn, s)?;
    Ok(())
}

fn format_status(s: Strategy) -> String {
    format!(
        "當前策略：{}  ATK {:.2}x  DEF {:.2}x",
        s.as_str(),
        s.atk_mult(),
        s.def_mult()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use rusqlite::Connection;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn read_current_defaults_when_no_snapshot_row() {
        let conn = fresh();
        assert_eq!(read_current(&conn).unwrap(), DEFAULT_STRATEGY);
    }

    #[test]
    fn upsert_then_read_round_trips() {
        let conn = fresh();
        upsert_strategy(&conn, Strategy::Aggressive).unwrap();
        assert_eq!(read_current(&conn).unwrap(), Strategy::Aggressive);
    }

    #[test]
    fn strategy_set_does_not_touch_pet_snapshot() {
        let conn = fresh();
        upsert_strategy(&conn, Strategy::Aggressive).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "CLI must not create a pet_snapshot row");

        // Existing row stays byte-for-byte unchanged (incl. strategy).
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, strategy)
             VALUES (1, 'python', 7, 70, 80, 500, 1000, 25, 18, 15, 20, 'explorer')",
            [],
        )
        .unwrap();
        upsert_strategy(&conn, Strategy::Scholar).unwrap();
        let snap: String = conn
            .query_row("SELECT strategy FROM pet_snapshot WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(snap, "explorer");
    }

    #[test]
    fn status_prefers_setting_over_snapshot() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, strategy)
             VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10, 'explorer')",
            [],
        )
        .unwrap();
        upsert_strategy(&conn, Strategy::Defensive).unwrap();
        assert_eq!(read_current(&conn).unwrap(), Strategy::Defensive);
    }

    #[test]
    fn upsert_overwrites_same_key_only() {
        let conn = fresh();
        upsert_strategy(&conn, Strategy::Aggressive).unwrap();
        upsert_strategy(&conn, Strategy::Defensive).unwrap();
        assert_eq!(read_current(&conn).unwrap(), Strategy::Defensive);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'pet_strategy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn format_status_mentions_strategy_name() {
        assert!(format_status(Strategy::Scholar).contains("scholar"));
        assert!(format_status(Strategy::Aggressive).contains("aggressive"));
    }
}
