# Phase 3d — 黏著度機制（Stickiness）

**Status**: ✅ Completed · 2026-04-17
**Plan**: [doc/plans/2026-04-17-phase3d-stickiness.md](../../../plans/2026-04-17-phase3d-stickiness.md)
**Branch**: `feature/phase3d-stickiness` (merged + deleted)
**Baseline**: `0c19e49` → **Merge**: `311dd45`

## Project Goal

> **Final goal**: 四個黏著度機制上線 — 讓 Ferris 從狀態機升級成「讓人想回來陪」的寵物
> **Success criteria**:
>
> | KR | 驗證方法 | 閾值 |
> |----|---------|-----|
> | KR1 | Welcome Back Report | 缺席 >10min 顯示 2-3 行摘要，<10min 隱藏 |
> | KR2 | Mood Decay 0-100 | 4 signal 各有 unit test |
> | KR3 | Next Unlock Anchor | statusline + TUI 都顯示，Lv 1/3/7/12/25/35/50 查找正確 |
> | KR4 | first_events idempotent | 重複觸發 return false；重啟不漏 |
> | KR5 | 品質 | `cargo check` / `cargo clippy` / `cargo test` 全綠；233 + ≥ 15 新測；新檔 0 warning |
>
> **Scope boundary**:
> - **IN**：3.1 Welcome Back、3.2 Mood Decay、3.4 Next Unlock、3.8 First-Time Moments
> - **OUT**：3.3 Zone Mastery（→ 3a）、3.5/3.7/3.6 Crafting/Item/Snapshot（→ 3e/3f）、Mood 對 commentary 語氣（→ 3c）、Ability 效果本體（未排）

## Progress

| Phase | 內容 | Commit | Status |
|-------|------|--------|--------|
| P1 | Schema v5 + Mood ECS + ability table | a9e4c2b | ✅ |
| P2 | Mood Decay system + live_state | 9ebfe78 | ✅ |
| P3 | Next Unlock Anchor | 7ec56a2 | ✅ |
| P4 | First-Time Events | 28a96de | ✅ |
| P5 | Welcome Back Report | 8abb2ad | ✅ |
| QG | cargo check/clippy/test | — | ✅ 302 tests PASS |
| Review R1 | code-reviewer agent | — | ✅ 0 CRITICAL/IMPORTANT |
| Fix R1 | — | — | N/A (no findings) |
| Review R2 | — | — | N/A (no fixes) |
| Merge | → main | 311dd45 | ✅ |
| Archive | post-merge + INDEX | — | ✅ |

## Final KR Check

| KR | Target | Actual |
|----|--------|--------|
| KR1 | Welcome Back shown ≥10 min absence, hidden <10 min | ✅ statusline + TUI override (13 session tests + 2 render tests) |
| KR2 | Mood 0-100 with 4 signal unit tests | ✅ 11 rule + 5 orchestrator tests, integrated in daemon tick |
| KR3 | Next unlock anchor in statusline + TUI | ✅ 3 TUI tests + 6 ability lookup tests cover Lv 1/3/7/12/25/35/50 |
| KR4 | first_events idempotent; 5 events | ✅ 3 active + 2 scaffolded; 13 tests |
| KR5 | 0 test regression, +15 tests, 0 new clippy | ✅ 233 → 302 (+69), clippy baseline unchanged at 32 |

## Design Notes

見 plan。重點：

- **Mood TTL** 防釘死（feedback `ecs-component-ttl`）：`Mood { value, tick_stamp }` 超過 TTL 重回預設 60
- **Mood write coalescing**：值沒變就不 serialize（避免每 tick 寫 DB）
- **first_events idempotent**：PK(event_id) + INSERT OR IGNORE
- **last_player_seen_at**：用 settings kv，不建新表
- **常數表 ABILITY_UNLOCKS**：const slice，不進 DB（Phase 3d 只做 lookup，效果本體未排）

## Decisions Log

- 2026-04-17：Mood default 60（落在「正常」區間 50-79 中央）
- 2026-04-17：Welcome Back 缺席下限 10 min（<10 min 視為同 session）
- 2026-04-17：Ability lookup 用 const slice 而非新表（Phase 3d 不碰效果實作）
- 2026-04-17：Mood decay 曲線固定依 spec（+10/-8/+20/-15、6h idle）— 若偏離需 review 記錄
