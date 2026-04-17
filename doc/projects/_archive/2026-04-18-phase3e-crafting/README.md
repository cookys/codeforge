# Phase 3e — Loot Crafting + Active Item

> Recipes + `codeforge craft/inventory/use` + daemon Ghost Repellent consumer. Spec §3.5 + §3.7.

**Plan**: [../../../plans/2026-04-18-phase3e-crafting.md](../../../plans/2026-04-18-phase3e-crafting.md)
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.5 + §3.7
**Branch**: `feature/phase3e-crafting`
**Started**: 2026-04-18
**Completed**: 2026-04-18

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 Schema v9 + recipe module | ✅ done | `1865ffa` |
| P2 inventory + craft CLI | ✅ done | `1bb4ca4` |
| P3 use CLI + daemon ghost-repellent | ✅ done | `1bb4ca4` |
| Chore: dedupe use_item via apply_effect | ✅ done | `c91951a` |
| Review r1 → fix 4 findings | ✅ done | `1e22e65` |
| Review r2 → clean | ✅ done | — |
| Merge + archive | ✅ done | `(merge commit)` |

## Outcome

- 3 recipes (spec §3.5): Refactor Blueprint / Ghost Repellent / Doppelganger Ward. `recipes()` static list, `find_recipe` case-insensitive.
- Schema v9 `active_effects` (id / effect_kind / zone_id / applied_at / expires_at / source_item). Global effects have zone_id NULL. Readers filter on `expires_at > now`.
- `codeforge inventory` — items + live effects + available recipes in 3 sections.
- `codeforge craft [<name>]` — single-transaction debit materials → credit product. Insufficient-materials bail rolls back.
- `codeforge use <name>` — single-transaction inventory debit → `active_effects` INSERT via `apply_effect`. Guards against double-use while same (effect_kind, zone_id) already live (spec has no stacking mechanic).
- `daemon::mob_scanner::rate_limited_scan` filters Ghost MobSpecs when SuppressGhostSpawn is live; existing alive ghosts untouched (matches spec "不再生成").
- Deferred runtime consumers (storage shipped): ReduceDifficulty → `combat::run_tick` damage math; SuppressDoppelgangerSplit → `mob.rs` split path.

## Test Delta

495 baseline (post-3f merge) → 537 total. +42 tests:
- `craft::recipes::tests` — 6 recipe table invariants
- `craft::effects::tests` — 9 effect runtime (kinds round-trip, zone match, global, expiry, expires_at math)
- `craft::tests` — 12 flow (craft happy / partial / insufficient / use / unknown / not-owned / rollback / stacking / cross-zone / reuse-after-expiry / mid-tx rollback)
- `cli::inventory::tests` — 2 (stable order, empty)
- `cli::craft::tests` — 4 (list, reject unknown, end-to-end, shortage surfaced)
- `cli::use_item::tests` — 5 (unknown, case-insensitive, home-zone scope, no-pet global, not-owned)
- `daemon::mob_scanner::tests` — 2 (repellent drops ghosts, expired admits)
- `db::tests` — 3 Phase 3e migration (version seeded, table present, upgrade from v8)

## Review Findings (r1)

1. **CRITICAL** — `use_item` allowed double-use while prior effect still live; no stacking in spec. Fixed with pre-insert `SELECT 1` guard + bail + 3 regression tests.
2. **IMPORTANT** — Missing v9 migration tests. Added 3 tests mirroring prior phase patterns.
3. **IMPORTANT** — Scanner tests mutated `CODEFORGE_SCAN_DIR` under parallel `cargo test`. Extracted `rate_limited_scan_with_dir(conn, zone, tick, Option<&Path>)` so tests inject the path directly.
4. **IMPORTANT** — `use_item_rollback_preserves_inventory_on_error` only exercised the early-bail path. Added `use_item_rolls_back_inventory_when_effect_insert_fails` that drops `active_effects` mid-setup so the mid-tx rollback path is actually exercised.

Round 2: all findings addressed, no new Critical or Important findings.

## Deferred to Backlog

- `combat::run_tick` apply ReduceDifficulty multiplier on mob.difficulty
- `mob.rs` Doppelganger split — check SuppressDoppelgangerSplit before splitting
- `codeforge use refactor-scroll` (next-tick double damage) — spec §3.7 mentions this as separate from Refactor Blueprint; Refactor Scroll is existing Elite loot drop, needs its own `use` effect
- Active-effect cleanup GC (table grows on every use; size-bounded by user crafts so not urgent)
