//! Phase 3c — trigger detection (pure logic, no DB).
//!
//! Produces a list of `Trigger`s given this tick's state. The daemon's
//! per-tick pipeline passes in `TickContext`; the caller decides — based
//! on `budget::can_emit` + opt-in flag — whether to actually emit any.
//!
//! Spec reference: §4 trigger list, §3.9 global budget override.

use super::Kind;
use crate::daemon::combat::DefeatedMob;
use crate::daemon::mob::MobKind;

/// Session threshold per spec §4 low-freq list.
pub const SESSION_LONG_THRESHOLD_SECS: i64 = 4 * 3600;

/// Idle threshold. 30 min of no CLI interaction → pet wonders where you are.
pub const LONG_IDLE_THRESHOLD_SECS: i64 = 30 * 60;

/// Global rate-limit window per spec §3.9.
pub const RATE_LIMIT_WINDOW_SECS: i64 = 3600;

/// Spec §3.9: phrase cannot repeat inside this window.
pub const PHRASE_DEDUP_WINDOW_SECS: i64 = 30 * 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    BossKill {
        mob_name: String,
        mob_kind: MobKind,
    },
    LevelUp {
        prev_level: u32,
        new_level: u32,
    },
    SessionLong {
        duration_secs: i64,
    },
    LongIdle {
        absence_secs: i64,
    },
    /// Placeholder. Phase 3a fires this when a new Zone unlocks; Phase 3c
    /// detect always returns `None` for this variant — wired here so the
    /// generator/display paths are already variant-complete when 3a lands.
    ZoneUnlock {
        zone_id: String,
    },
}

impl Trigger {
    pub fn kind(&self) -> Kind {
        match self {
            Self::BossKill { .. } => Kind::BossKill,
            Self::LevelUp { .. } => Kind::LevelUp,
            Self::SessionLong { .. } => Kind::SessionLong,
            Self::LongIdle { .. } => Kind::LongIdle,
            Self::ZoneUnlock { .. } => Kind::ZoneUnlock,
        }
    }

    /// Priority ranking for when multiple triggers fire the same tick. The
    /// budget can only fund one — so we pick the most meaningful:
    ///   LevelUp > BossKill > ZoneUnlock > LongIdle > SessionLong.
    /// Reasoning: LevelUp is rarer and emotionally loaded (spec §3.8 framing);
    /// SessionLong is a nag, least valuable if the user just killed a boss.
    pub fn priority_rank(&self) -> u8 {
        match self.kind() {
            Kind::LevelUp => 0,
            Kind::BossKill => 1,
            Kind::ZoneUnlock => 2,
            Kind::LongIdle => 3,
            Kind::SessionLong => 4,
            Kind::ManualTest => 0, // test-only, always wins
        }
    }
}

/// Read-only inputs drawn from the current tick.
pub struct TickContext<'a> {
    pub now: i64,
    pub defeats: &'a [DefeatedMob],
    pub level_before: u32,
    pub level_after: u32,
    /// Unix seconds when the daemon started. Used for SessionLong.
    pub session_started_at: i64,
    /// Unix seconds of last CLI interaction (`last_player_seen_at` in settings).
    /// `None` on fresh install → LongIdle suppressed.
    pub last_seen_at: Option<i64>,
}

/// Detect all triggers that the current tick would warrant. The caller
/// applies rate-limit + opt-in filters on top — this function is pure.
pub fn detect(ctx: &TickContext<'_>) -> Vec<Trigger> {
    let mut out = Vec::new();

    // BossKill: spec §4 lists only Boss; Elite/Zombie/Ghost/etc are
    // too frequent to warrant commentary per spec §3.9 rate budget.
    // Take the first boss defeat — if multiple bosses fall the same
    // tick, one phrase is enough (pet isn't a sports commentator).
    if let Some(boss) = ctx.defeats.iter().find(|d| d.kind == MobKind::Boss) {
        out.push(Trigger::BossKill {
            mob_name: boss.name.clone(),
            mob_kind: boss.kind,
        });
    }

    // LevelUp: any positive delta. Cascades (double level-up) still emit
    // one trigger for the final level — avoids spamming "Lv2! Lv3!" when
    // the XP roll happened to span two thresholds.
    if ctx.level_after > ctx.level_before {
        out.push(Trigger::LevelUp {
            prev_level: ctx.level_before,
            new_level: ctx.level_after,
        });
    }

    // SessionLong: wall-clock uptime since daemon start. Rate-limit keeps
    // it from firing every tick — first tick past 4h trips it, next one
    // 1h later, etc.
    let duration = ctx.now.saturating_sub(ctx.session_started_at);
    if duration >= SESSION_LONG_THRESHOLD_SECS {
        out.push(Trigger::SessionLong {
            duration_secs: duration,
        });
    }

    // LongIdle: only meaningful once last_seen is populated.
    if let Some(last) = ctx.last_seen_at {
        let absence = ctx.now.saturating_sub(last);
        if absence >= LONG_IDLE_THRESHOLD_SECS {
            out.push(Trigger::LongIdle {
                absence_secs: absence,
            });
        }
    }

    out
}

/// Pick the single trigger to emit this tick. Returns `None` if empty.
/// When multiple fire, `priority_rank` (lower = higher priority) decides;
/// ties broken by input order.
pub fn pick_best(triggers: Vec<Trigger>) -> Option<Trigger> {
    triggers.into_iter().min_by_key(|t| t.priority_rank())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mob(kind: MobKind, name: &str) -> DefeatedMob {
        DefeatedMob {
            id: 1,
            kind,
            name: name.into(),
            hp_max: 100,
        }
    }

    fn base_ctx<'a>(defeats: &'a [DefeatedMob]) -> TickContext<'a> {
        TickContext {
            now: 10_000,
            defeats,
            level_before: 1,
            level_after: 1,
            session_started_at: 10_000,
            last_seen_at: None,
        }
    }

    #[test]
    fn boss_kill_fires_on_boss_defeat() {
        let defeats = vec![mob(MobKind::Boss, "src/auth.rs Boss")];
        let ctx = base_ctx(&defeats);
        let triggers = detect(&ctx);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].kind(), Kind::BossKill);
    }

    #[test]
    fn non_boss_defeats_do_not_fire_boss_kill() {
        let defeats = vec![
            mob(MobKind::Zombie, "TODO cluster"),
            mob(MobKind::Elite, "complex fn"),
            mob(MobKind::Ghost, "dead import"),
        ];
        let ctx = base_ctx(&defeats);
        let triggers = detect(&ctx);
        assert!(!triggers.iter().any(|t| t.kind() == Kind::BossKill));
    }

    #[test]
    fn multiple_boss_kills_emit_one_trigger() {
        let defeats = vec![
            mob(MobKind::Boss, "boss A"),
            mob(MobKind::Boss, "boss B"),
        ];
        let ctx = base_ctx(&defeats);
        let triggers = detect(&ctx);
        let boss_count = triggers.iter().filter(|t| t.kind() == Kind::BossKill).count();
        assert_eq!(boss_count, 1);
    }

    #[test]
    fn level_up_fires_only_on_positive_delta() {
        let defeats: Vec<DefeatedMob> = vec![];
        let mut ctx = base_ctx(&defeats);
        ctx.level_before = 5;
        ctx.level_after = 6;
        let triggers = detect(&ctx);
        assert!(triggers.iter().any(|t| t.kind() == Kind::LevelUp));

        ctx.level_before = 5;
        ctx.level_after = 5;
        let triggers = detect(&ctx);
        assert!(!triggers.iter().any(|t| t.kind() == Kind::LevelUp));
    }

    #[test]
    fn level_up_cascade_emits_single_trigger() {
        let defeats: Vec<DefeatedMob> = vec![];
        let mut ctx = base_ctx(&defeats);
        ctx.level_before = 5;
        ctx.level_after = 7; // double level-up
        let triggers = detect(&ctx);
        let ups: Vec<_> = triggers.iter().filter(|t| t.kind() == Kind::LevelUp).collect();
        assert_eq!(ups.len(), 1);
        if let Trigger::LevelUp { prev_level, new_level } = ups[0] {
            assert_eq!(*prev_level, 5);
            assert_eq!(*new_level, 7);
        } else {
            panic!("expected LevelUp");
        }
    }

    #[test]
    fn session_long_fires_exactly_at_4h() {
        let defeats: Vec<DefeatedMob> = vec![];
        let mut ctx = base_ctx(&defeats);
        ctx.session_started_at = 0;

        ctx.now = SESSION_LONG_THRESHOLD_SECS - 1;
        assert!(!detect(&ctx).iter().any(|t| t.kind() == Kind::SessionLong));

        ctx.now = SESSION_LONG_THRESHOLD_SECS;
        assert!(detect(&ctx).iter().any(|t| t.kind() == Kind::SessionLong));

        ctx.now = SESSION_LONG_THRESHOLD_SECS + 10_000;
        assert!(detect(&ctx).iter().any(|t| t.kind() == Kind::SessionLong));
    }

    #[test]
    fn long_idle_requires_last_seen() {
        let defeats: Vec<DefeatedMob> = vec![];
        let mut ctx = base_ctx(&defeats);
        ctx.last_seen_at = None;
        assert!(!detect(&ctx).iter().any(|t| t.kind() == Kind::LongIdle));

        ctx.last_seen_at = Some(ctx.now - LONG_IDLE_THRESHOLD_SECS);
        assert!(detect(&ctx).iter().any(|t| t.kind() == Kind::LongIdle));

        ctx.last_seen_at = Some(ctx.now - (LONG_IDLE_THRESHOLD_SECS - 1));
        assert!(!detect(&ctx).iter().any(|t| t.kind() == Kind::LongIdle));
    }

    #[test]
    fn pick_best_prefers_level_up_over_session_long() {
        let triggers = vec![
            Trigger::SessionLong { duration_secs: 14400 },
            Trigger::LevelUp { prev_level: 1, new_level: 2 },
        ];
        let best = pick_best(triggers).unwrap();
        assert_eq!(best.kind(), Kind::LevelUp);
    }

    #[test]
    fn pick_best_prefers_boss_over_long_idle() {
        let triggers = vec![
            Trigger::LongIdle { absence_secs: 3600 },
            Trigger::BossKill {
                mob_name: "x".into(),
                mob_kind: MobKind::Boss,
            },
        ];
        let best = pick_best(triggers).unwrap();
        assert_eq!(best.kind(), Kind::BossKill);
    }

    #[test]
    fn pick_best_on_empty_returns_none() {
        assert!(pick_best(vec![]).is_none());
    }

    #[test]
    fn detect_no_triggers_on_idle_tick() {
        let defeats: Vec<DefeatedMob> = vec![];
        let ctx = base_ctx(&defeats);
        assert!(detect(&ctx).is_empty());
    }
}
