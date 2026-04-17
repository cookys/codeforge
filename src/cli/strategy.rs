//! `codeforge strategy [name]` — Phase 3b CLI surface for Strategy Mode.
//!
//! No args → print current strategy + multiplier summary.
//! With name → validate, upsert into `pet_snapshot.strategy`, confirm.
//!
//! Writing directly to `pet_snapshot` is safe because the race with the
//! daemon's tick-end `serialize_to_db` is mitigated: the daemon calls
//! `GameWorld::refresh_strategy_from_db` at tick step 3b (inside the tick
//! transaction, before combat + serialize), so any CLI-written value is
//! picked up into the ECS `PetStrategy` component and written back
//! unchanged. The spec keeps strategy user-toggled (not daemon-derived),
//! and this mitigation preserves that ownership without routing through
//! `event_inbox`.

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
            println!("  ATK 倍率 {:.2}x  DEF 倍率 {:.2}x", parsed.atk_mult(), parsed.def_mult());
        }
    }
    Ok(())
}

/// Read the persisted strategy. Falls back to `DEFAULT_STRATEGY` when:
///   - `pet_snapshot` is empty (daemon has never ticked, Phase 1 adopt only)
///   - the stored value fails to parse (corrupt — should not happen)
pub(crate) fn read_current(conn: &rusqlite::Connection) -> Result<Strategy> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT strategy FROM pet_snapshot WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored
        .and_then(|s| Strategy::from_str(&s))
        .unwrap_or(DEFAULT_STRATEGY))
}

/// UPSERT so `codeforge strategy <name>` works even when no daemon tick
/// has ever written a snapshot row (fresh adopt → strategy flip before
/// first tick). For a greenfield row we seed reasonable defaults that
/// the daemon will overwrite on first tick; for an existing row we touch
/// only the strategy column. `updated_at` is set explicitly (rather than
/// leaning on the column DEFAULT) so both branches write the same value
/// and there's no risk of a CHECK/NOT NULL regression if the schema
/// default is ever relaxed.
fn upsert_strategy(conn: &rusqlite::Connection, s: Strategy) -> Result<()> {
    conn.execute(
        "INSERT INTO pet_snapshot
             (id, village, level, hp, hp_max, xp, xp_to_next,
              atk, def, sup, ver, strategy, updated_at)
         VALUES (1, 'rust', 1, 10, 10, 0, 100, 10, 10, 10, 10, ?1,
                 datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
             strategy = excluded.strategy,
             updated_at = excluded.updated_at",
        rusqlite::params![s.as_str()],
    )?;
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
    fn upsert_preserves_existing_row_fields() {
        let conn = fresh();
        // Pre-seed a snapshot with real pet state (simulating daemon tick)
        conn.execute(
            "INSERT INTO pet_snapshot
                 (id, village, level, hp, hp_max, xp, xp_to_next,
                  atk, def, sup, ver, last_message, mood, mood_tick_stamp,
                  strategy)
             VALUES (1, 'python', 7, 70, 80, 500, 1000,
                     25, 18, 15, 20, 'hi', 80, 12, 'explorer')",
            [],
        )
        .unwrap();

        upsert_strategy(&conn, Strategy::Scholar).unwrap();

        // Strategy changed; other fields intact.
        let (village, level, hp, mood, strategy): (String, i64, i64, i64, String) = conn
            .query_row(
                "SELECT village, level, hp, mood, strategy FROM pet_snapshot WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(village, "python");
        assert_eq!(level, 7);
        assert_eq!(hp, 70);
        assert_eq!(mood, 80);
        assert_eq!(strategy, "scholar");
    }

    #[test]
    fn upsert_overwrites_same_column_only() {
        let conn = fresh();
        upsert_strategy(&conn, Strategy::Aggressive).unwrap();
        upsert_strategy(&conn, Strategy::Defensive).unwrap();
        assert_eq!(read_current(&conn).unwrap(), Strategy::Defensive);
        // Exactly one row must exist — single-row pet_snapshot PK.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM pet_snapshot", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn format_status_mentions_strategy_name() {
        assert!(format_status(Strategy::Scholar).contains("scholar"));
        assert!(format_status(Strategy::Aggressive).contains("aggressive"));
    }
}
