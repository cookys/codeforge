# Phase 3a — World Map + Zone unlock

> L1 memory 語言分佈 → game_world concept_count + unlock state → `codeforge world` ASCII map；Explorer strategy 真正 cross-zone priority。

**Plan**: [../../plans/2026-04-18-phase3a-world-map.md](../../plans/2026-04-18-phase3a-world-map.md)
**Spec**: `doc/specs/codeforge-mud-engine.md` §1 + §3.3 + §2 (Explorer revisit)
**Branch**: `feature/phase3a-world-map`
**Started**: 2026-04-18
**Completed**: 2026-04-18

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 L1 analyzer + schema v8 | ✅ done | `962e816` |
| P2 Zone unlock + rank calculator | ✅ done | `338d68b` |
| P3 `codeforge world` CLI + ASCII map | ✅ done | `ee9c982` |
| P4 Explorer cross-zone priority | ✅ done | `37d8df1` |
| QG (466/466 tests, clippy clean) | ✅ done | `b2541ed` |
| Review r1 → fix (2 findings) | ✅ done | `d43b4ff` |
| Review r2 → clean | ✅ done | — |
| Merge + archive | ✅ done | `474e090` |

## Outcome

- `codeforge world [--refresh]` renders a 2×3 ASCII grid (5 villages + Nation placeholder). Home village always shows as 開放 regardless of DB flag.
- L1 language analyzer scans `concepts/*.md` + `qa/*.md` + `connections/*.md` with keyword heuristic (rust/python/typescript/go/javascript); each concept contributes at most +1 per village (soft metric).
- Zone unlock: `(is_home) OR (concept_count ≥ UNLOCK_THRESHOLD=3)`; SQL CASE ensures stickiness (once unlocked, never relocks).
- Zone Mastery rank: pure function of `kill_count` with spec §3.3 thresholds (Traveler 0-49 / Forger 50-199 / Iron Crafter 200-499 / Veteran 500+).
- Explorer strategy: `priority_order_ctx(kind, zone_id, zones)` uses `zone.kill_count` as sort key — lower wins. Other strategies still score zone-agnostic (via `priority_order(kind) as u32`).
- Combat `run_tick` only loads ZoneStats when `strategy == Explorer` — zero tick cost for Aggressive/Defensive/Scholar.
- Schema v7 → v8 via ALTER-guard `upgrade_game_world_concept_count`.

## Test Delta

427 baseline (post-3c merge) → 466. +39 tests:
- `memory::l1_stats` — 9 tests (keyword matching, CJK-safe, multi-subdir, dedup-per-concept)
- `world::tests` — 10 tests (rank boundaries, empty DB, home unlock, threshold, sticky unlock, idempotency, rank from kill_count, preserve kill_count)
- `db::mod` migration — 4 tests (ALTER-guard, idempotency, v7→v8, rows preserved)
- `cli::world::tests` — 10 tests (pad/clip visible, rank label, Cell tone, default-home, footer unlock count)
- `daemon::strategy::tests` — 5 ctx tests (Explorer cross-zone, non-Explorer parity, missing zone, kind-independence, single-zone Phase 2b-compat)
- Plus 1 incidental test caught by combat.rs refactor (counted in totals)

## Review Findings (r1)

1. **CRITICAL** `render_footer` undercounted unlocked villages — used raw `s.unlocked` instead of the home-village override from `Cell::from_stats`. Fixed by extracting `unlocked_count(stats, home_village)` helper + 2 regression tests.
2. **IMPORTANT** `combat::run_tick` unconditionally called `world::load_all` (5 queries/tick) even for strategies that never consult the result. Fixed by gating behind `strategy == Strategy::Explorer`.

Round 2: clean, no new Critical or Important findings.

## Deferred to Backlog

- Multi-zone MOB spawning (scanner still scans home zone only)
- Pet cross-zone movement (`pet_snapshot.current_zone` + `codeforge travel <zone>`)
- TUI world map panel integration
- L1 analyzer LLM classification upgrade (current is keyword heuristic)
- `???` Nation cell real rendering (waits on Phase 5 Nation P2P)
