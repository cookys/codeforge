//! Active-effects runtime — Phase 3e.
//!
//! Consuming an item (via `craft::use_item`) inserts an `active_effects`
//! row with `expires_at` in unix seconds. Callers check "is this effect
//! live right now?" via `is_effect_active` or list all live effects in a
//! zone via `active_effects_for_zone`.
//!
//! Expired rows remain in the table for audit purposes; readers always
//! filter on `expires_at > now`. Daemon is free to GC but isn't required
//! to (table is small, growth is bounded by user crafts).

use anyhow::Result;
use rusqlite::Connection;

/// The set of runtime modifiers any `active_effects` row can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// `daemon::combat` multiplies the effective mob difficulty by
    /// `0.8` (−20%) when this kind is live in the target zone.
    /// (Daemon wiring lands in P3; storage shipped in P1.)
    ReduceDifficulty,
    /// `daemon::mob_scanner` skips Ghost spawns in this zone. If
    /// `zone_id` is None on the row, applies globally.
    SuppressGhostSpawn,
    /// `daemon::mob` skips Doppelganger split on death when live.
    /// (Daemon wiring deferred — storage only in 3e MVP.)
    SuppressDoppelgangerSplit,
}

impl EffectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReduceDifficulty => "reduce_difficulty",
            Self::SuppressGhostSpawn => "suppress_ghost_spawn",
            Self::SuppressDoppelgangerSplit => "suppress_doppelganger_split",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "reduce_difficulty" => Self::ReduceDifficulty,
            "suppress_ghost_spawn" => Self::SuppressGhostSpawn,
            "suppress_doppelganger_split" => Self::SuppressDoppelgangerSplit,
            _ => return None,
        })
    }
}

/// Effect metadata attached to a recipe's product. Duration measured in
/// whole days; multiplied by `24 * 60 * 60` before being added to the
/// unix-seconds `applied_at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub duration_days: i64,
}

/// Decoded `active_effects` row for callers that need more than a
/// yes/no check — e.g. `codeforge inventory` wants to render the
/// remaining TTL.
#[derive(Debug, Clone)]
pub struct ActiveEffect {
    pub kind: EffectKind,
    pub zone_id: Option<String>,
    pub expires_at: i64,
    pub source_item: String,
}

/// Quick "is this kind live for that zone right now?" check. Global
/// rows (zone_id IS NULL) count for any zone; zone-scoped rows count
/// only when `zone_id` matches.
pub fn is_effect_active(
    conn: &Connection,
    kind: EffectKind,
    zone_id: &str,
    now: i64,
) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM active_effects
             WHERE effect_kind = ?1
               AND (zone_id IS NULL OR zone_id = ?2)
               AND expires_at > ?3",
            rusqlite::params![kind.as_str(), zone_id, now],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(n > 0)
}

/// All live effects for a zone, ordered by soonest-to-expire. Global
/// rows (`zone_id IS NULL`) surface too.
pub fn active_effects_for_zone(
    conn: &Connection,
    zone_id: &str,
    now: i64,
) -> Result<Vec<ActiveEffect>> {
    let mut stmt = conn.prepare(
        "SELECT effect_kind, zone_id, expires_at, source_item
         FROM active_effects
         WHERE (zone_id IS NULL OR zone_id = ?1)
           AND expires_at > ?2
         ORDER BY expires_at",
    )?;
    let rows = stmt.query_map(rusqlite::params![zone_id, now], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (kind_s, zone, expires, source) = row?;
        if let Some(kind) = EffectKind::from_str(&kind_s) {
            out.push(ActiveEffect {
                kind,
                zone_id: zone,
                expires_at: expires,
                source_item: source,
            });
        }
        // Unknown kinds (future schema addition with older binary) are
        // silently skipped — forward compatibility with no crash.
    }
    Ok(out)
}

/// Insert a new `active_effects` row. Used by `craft::use_item`; exposed
/// here so tests can seed effects directly.
pub fn apply_effect(
    conn: &Connection,
    kind: EffectKind,
    zone_id: Option<&str>,
    applied_at: i64,
    duration_days: i64,
    source_item: &str,
) -> Result<i64> {
    let expires_at = applied_at + duration_days * 24 * 60 * 60;
    conn.execute(
        "INSERT INTO active_effects
             (effect_kind, zone_id, applied_at, expires_at, source_item)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![kind.as_str(), zone_id, applied_at, expires_at, source_item],
    )?;
    Ok(expires_at)
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
    fn effect_kind_roundtrip() {
        for kind in [
            EffectKind::ReduceDifficulty,
            EffectKind::SuppressGhostSpawn,
            EffectKind::SuppressDoppelgangerSplit,
        ] {
            assert_eq!(EffectKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn effect_kind_from_unknown_returns_none() {
        assert_eq!(EffectKind::from_str("nope"), None);
    }

    #[test]
    fn is_effect_active_returns_false_when_empty() {
        let conn = fresh();
        assert!(
            !is_effect_active(&conn, EffectKind::SuppressGhostSpawn, "rust", 1_000).unwrap()
        );
    }

    #[test]
    fn is_effect_active_detects_live_zone_match() {
        let conn = fresh();
        apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("rust"),
            1_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();
        assert!(is_effect_active(
            &conn,
            EffectKind::SuppressGhostSpawn,
            "rust",
            1_500
        )
        .unwrap());
    }

    #[test]
    fn is_effect_active_skips_other_zone() {
        let conn = fresh();
        apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("rust"),
            1_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();
        assert!(!is_effect_active(
            &conn,
            EffectKind::SuppressGhostSpawn,
            "python",
            1_500
        )
        .unwrap());
    }

    #[test]
    fn global_effect_applies_to_every_zone() {
        let conn = fresh();
        apply_effect(
            &conn,
            EffectKind::ReduceDifficulty,
            None, // global
            1_000,
            7,
            "Refactor Blueprint",
        )
        .unwrap();
        assert!(is_effect_active(
            &conn,
            EffectKind::ReduceDifficulty,
            "rust",
            1_500
        )
        .unwrap());
        assert!(is_effect_active(
            &conn,
            EffectKind::ReduceDifficulty,
            "python",
            1_500
        )
        .unwrap());
    }

    #[test]
    fn expired_effect_no_longer_active() {
        let conn = fresh();
        apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("rust"),
            1_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();
        // 3 days later = 1000 + 3*86400 = 260_200; strictly >, so 260_200
        // is NOT active but 260_199 is.
        let edge = 1_000 + 3 * 86_400;
        assert!(!is_effect_active(&conn, EffectKind::SuppressGhostSpawn, "rust", edge).unwrap());
        assert!(is_effect_active(&conn, EffectKind::SuppressGhostSpawn, "rust", edge - 1).unwrap());
    }

    #[test]
    fn active_effects_for_zone_returns_zone_and_global() {
        let conn = fresh();
        apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("rust"),
            1_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();
        apply_effect(
            &conn,
            EffectKind::ReduceDifficulty,
            None,
            1_000,
            7,
            "Refactor Blueprint",
        )
        .unwrap();
        // Different-zone effect should NOT surface
        apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("python"),
            1_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();

        let live = active_effects_for_zone(&conn, "rust", 1_500).unwrap();
        assert_eq!(live.len(), 2);
        let kinds: Vec<EffectKind> = live.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EffectKind::SuppressGhostSpawn));
        assert!(kinds.contains(&EffectKind::ReduceDifficulty));
    }

    #[test]
    fn apply_effect_returns_correct_expires_at() {
        let conn = fresh();
        let expires = apply_effect(
            &conn,
            EffectKind::SuppressGhostSpawn,
            Some("rust"),
            10_000,
            3,
            "Ghost Repellent",
        )
        .unwrap();
        assert_eq!(expires, 10_000 + 3 * 86_400);
    }
}
