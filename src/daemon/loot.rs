//! Loot system — Phase 2b P4.
//!
//! When `combat::run_tick` marks a mob defeated, this module rolls the
//! loot per `doc/specs/codeforge-mud-engine.md` §2 Loot table:
//!
//! | MOB           | Primary                     | Secondary (30% roll) |
//! |---------------|-----------------------------|----------------------|
//! | boss          | Rare Item + 100 XP          | L1 Connection Tome   |
//! | elite         | 40 XP + 1 Skill Point       | Refactor Scroll      |
//! | zombie        | 10 XP                       | TODO Cleaner         |
//! | ghost         | Dead Code Crystal           | —                    |
//! | doppelganger  | Pattern Fragment            | Abstract Gem         |
//! | void          | 20 XP                       | —                    |
//!
//! XP goes straight to the pet (via the caller). Non-XP loot upserts into
//! `loot_inventory` keyed on (kind, name) so repeated drops aggregate
//! quantities instead of flooding the table.
//!
//! Every defeat also writes a row to `combat_log` for retro display.

use anyhow::Result;
use rusqlite::Connection;

use super::combat::{hash_seed, DefeatedMob, Rng};
use super::mob::MobKind;

const SECONDARY_DROP_CHANCE: f64 = 0.30;

/// A single drop from a defeated mob. XP is special (never goes into
/// inventory — applied to the pet directly by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LootDrop {
    Xp(u32),
    Item {
        kind: &'static str,
        name: &'static str,
        quantity: u32,
    },
}

/// Pure function — given a mob kind and an RNG, return the rolled drops.
/// Extracted from the DB layer for easy unit testing of the loot table.
pub fn roll_drops(kind: MobKind, rng: &mut Rng) -> Vec<LootDrop> {
    match kind {
        MobKind::Boss => {
            let mut d = vec![
                LootDrop::Item {
                    kind: "item",
                    name: "Rare Item",
                    quantity: 1,
                },
                LootDrop::Xp(100),
            ];
            if rng.next_f64() < SECONDARY_DROP_CHANCE {
                d.push(LootDrop::Item {
                    kind: "tome",
                    name: "L1 Connection Tome",
                    quantity: 1,
                });
            }
            d
        }
        MobKind::Elite => {
            let mut d = vec![
                LootDrop::Xp(40),
                LootDrop::Item {
                    kind: "skill_point",
                    name: "Skill Point",
                    quantity: 1,
                },
            ];
            if rng.next_f64() < SECONDARY_DROP_CHANCE {
                d.push(LootDrop::Item {
                    kind: "scroll",
                    name: "Refactor Scroll",
                    quantity: 1,
                });
            }
            d
        }
        MobKind::Zombie => {
            let mut d = vec![LootDrop::Xp(10)];
            if rng.next_f64() < SECONDARY_DROP_CHANCE {
                d.push(LootDrop::Item {
                    kind: "item",
                    name: "TODO Cleaner",
                    quantity: 1,
                });
            }
            d
        }
        MobKind::Ghost => vec![LootDrop::Item {
            kind: "crystal",
            name: "Dead Code Crystal",
            quantity: 1,
        }],
        MobKind::Doppelganger => {
            let mut d = vec![LootDrop::Item {
                kind: "fragment",
                name: "Pattern Fragment",
                quantity: 1,
            }];
            if rng.next_f64() < SECONDARY_DROP_CHANCE {
                d.push(LootDrop::Item {
                    kind: "gem",
                    name: "Abstract Gem",
                    quantity: 1,
                });
            }
            d
        }
        MobKind::Void => vec![LootDrop::Xp(20)],
    }
}

/// Process all defeats: insert combat_log rows, upsert inventory, sum XP.
///
/// Returns total XP to award to the pet (caller feeds into `systems::apply_xp`).
/// Loot RNG is seeded from `(rng_salt XOR constant, mob_id)` — distinct from
/// the combat seed (extra XOR) so hit/loot rolls aren't correlated.
/// `rng_salt` should be the same monotonic value combat used (`tick_count`)
/// so tests can reproduce outcomes; `tick_at` is unix seconds, used only
/// for DB timestamp fields.
pub fn apply_for_defeats(
    conn: &Connection,
    zone_id: &str,
    defeats: &[DefeatedMob],
    rng_salt: u64,
    tick_at: i64,
) -> Result<u32> {
    if defeats.is_empty() {
        return Ok(0);
    }
    let mut total_xp: u32 = 0;
    for def in defeats {
        let mut rng = Rng::from_seed(hash_seed(rng_salt ^ 0x0A11_1007_BADC_0FFE, def.id as u64));
        let drops = roll_drops(def.kind, &mut rng);

        let mut xp_this_drop = 0u32;
        let mut names: Vec<String> = Vec::new();
        for drop in &drops {
            match drop {
                LootDrop::Xp(n) => {
                    xp_this_drop = xp_this_drop.saturating_add(*n);
                    names.push(format!("{n} XP"));
                }
                LootDrop::Item {
                    kind,
                    name,
                    quantity,
                } => {
                    upsert_inventory(conn, kind, name, *quantity, tick_at)?;
                    names.push(if *quantity == 1 {
                        (*name).to_string()
                    } else {
                        format!("{name} × {quantity}")
                    });
                }
            }
        }
        total_xp = total_xp.saturating_add(xp_this_drop);
        log_defeat(conn, zone_id, def, xp_this_drop, &names.join(", "), tick_at)?;
    }
    Ok(total_xp)
}

fn upsert_inventory(
    conn: &Connection,
    kind: &str,
    name: &str,
    quantity: u32,
    at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO loot_inventory
             (kind, name, quantity, first_acquired_at, last_acquired_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(kind, name) DO UPDATE SET
             quantity = loot_inventory.quantity + excluded.quantity,
             last_acquired_at = excluded.last_acquired_at",
        rusqlite::params![kind, name, quantity as i64, at],
    )?;
    Ok(())
}

fn log_defeat(
    conn: &Connection,
    zone_id: &str,
    def: &DefeatedMob,
    xp: u32,
    loot: &str,
    at: i64,
) -> Result<()> {
    let occurred = chrono::DateTime::<chrono::Utc>::from_timestamp(at, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
    conn.execute(
        "INSERT INTO combat_log
             (zone_id, mob_name, mob_kind, xp_gained, loot, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            zone_id,
            def.name,
            def.kind.as_str(),
            xp as i64,
            loot,
            occurred
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn dm(id: i64, kind: MobKind, name: &str) -> DefeatedMob {
        DefeatedMob {
            id,
            kind,
            name: name.to_string(),
            hp_max: 10,
        }
    }

    // ─── Loot table determinism ────────────────────────────────

    #[test]
    fn boss_always_drops_rare_item_and_100_xp() {
        // Primary drops are unconditional — test across many seeds
        for seed in 0..50 {
            let mut rng = Rng::from_seed(seed);
            let drops = roll_drops(MobKind::Boss, &mut rng);
            let xp: u32 = drops
                .iter()
                .filter_map(|d| match d {
                    LootDrop::Xp(n) => Some(*n),
                    _ => None,
                })
                .sum();
            assert_eq!(xp, 100);
            assert!(
                drops.iter().any(|d| matches!(
                    d,
                    LootDrop::Item {
                        name: "Rare Item",
                        ..
                    }
                )),
                "Boss primary 'Rare Item' missing at seed {seed}"
            );
        }
    }

    #[test]
    fn ghost_drops_dead_code_crystal_only() {
        for seed in 0..20 {
            let mut rng = Rng::from_seed(seed);
            let drops = roll_drops(MobKind::Ghost, &mut rng);
            assert_eq!(drops.len(), 1);
            assert!(matches!(
                &drops[0],
                LootDrop::Item {
                    name: "Dead Code Crystal",
                    ..
                }
            ));
        }
    }

    #[test]
    fn zombie_gives_10_xp_always() {
        for seed in 0..20 {
            let mut rng = Rng::from_seed(seed);
            let drops = roll_drops(MobKind::Zombie, &mut rng);
            let xp: u32 = drops
                .iter()
                .filter_map(|d| match d {
                    LootDrop::Xp(n) => Some(*n),
                    _ => None,
                })
                .sum();
            assert_eq!(xp, 10);
        }
    }

    #[test]
    fn secondary_drops_roughly_30_percent() {
        // Loose stats check: over 1000 boss rolls, count tome drops
        let mut tome_count = 0;
        for seed in 0..1000u64 {
            let mut rng = Rng::from_seed(seed);
            let drops = roll_drops(MobKind::Boss, &mut rng);
            if drops.iter().any(|d| {
                matches!(
                    d,
                    LootDrop::Item {
                        name: "L1 Connection Tome",
                        ..
                    }
                )
            }) {
                tome_count += 1;
            }
        }
        assert!(
            (200..=400).contains(&tome_count),
            "secondary drop rate should be ~30%, got {tome_count}/1000"
        );
    }

    // ─── End-to-end apply_for_defeats ──────────────────────────

    #[test]
    fn apply_zero_defeats_is_noop() {
        let conn = fresh_conn();
        let total = apply_for_defeats(&conn, "rust", &[], 1, 1000).unwrap();
        assert_eq!(total, 0);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM combat_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn apply_one_zombie_adds_xp_and_logs() {
        let conn = fresh_conn();
        let defeats = vec![dm(1, MobKind::Zombie, "TODOs × 8 @ x.rs")];
        let total = apply_for_defeats(&conn, "rust", &defeats, 1, 1000).unwrap();
        assert_eq!(total, 10);

        let log_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM combat_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(log_count, 1);
    }

    #[test]
    fn apply_boss_adds_rare_item_to_inventory() {
        let conn = fresh_conn();
        let defeats = vec![dm(42, MobKind::Boss, "src/huge.rs")];
        apply_for_defeats(&conn, "rust", &defeats, 2, 2000).unwrap();

        let qty: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Rare Item'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(qty, 1);
    }

    #[test]
    fn repeat_defeats_aggregate_inventory_quantity() {
        let conn = fresh_conn();
        // Defeat two ghosts → two Dead Code Crystals should aggregate, not duplicate
        let defeats = vec![
            dm(1, MobKind::Ghost, "a.rs"),
            dm(2, MobKind::Ghost, "b.rs"),
            dm(3, MobKind::Ghost, "c.rs"),
        ];
        apply_for_defeats(&conn, "rust", &defeats, 3, 3000).unwrap();

        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM loot_inventory WHERE name = 'Dead Code Crystal'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1, "must aggregate, not duplicate");
        let qty: i64 = conn
            .query_row(
                "SELECT quantity FROM loot_inventory WHERE name = 'Dead Code Crystal'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(qty, 3);
    }

    #[test]
    fn apply_combat_log_includes_zone_mob_and_xp() {
        let conn = fresh_conn();
        let defeats = vec![dm(7, MobKind::Boss, "boss.rs")];
        apply_for_defeats(&conn, "python", &defeats, 5, 5000).unwrap();

        let (zone, mob_name, mob_kind, xp): (String, String, String, i64) = conn
            .query_row(
                "SELECT zone_id, mob_name, mob_kind, xp_gained FROM combat_log",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(zone, "python");
        assert_eq!(mob_name, "boss.rs");
        assert_eq!(mob_kind, "boss");
        assert_eq!(xp, 100);
    }

    #[test]
    fn first_acquired_at_stays_on_aggregate() {
        let conn = fresh_conn();
        apply_for_defeats(&conn, "rust", &[dm(1, MobKind::Ghost, "a.rs")], 1, 1000).unwrap();
        apply_for_defeats(&conn, "rust", &[dm(2, MobKind::Ghost, "b.rs")], 2, 2000).unwrap();

        let (first, last): (i64, i64) = conn
            .query_row(
                "SELECT first_acquired_at, last_acquired_at FROM loot_inventory
                 WHERE name = 'Dead Code Crystal'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(first, 1000);
        assert_eq!(last, 2000);
    }
}
