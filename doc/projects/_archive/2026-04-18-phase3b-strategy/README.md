# Phase 3b — Strategy Mode ✅

> Plan: `doc/plans/2026-04-18-phase3b-strategy.md`
> Spec: `doc/specs/codeforge-mud-engine.md` §2 Strategy Mode
> Branch: `feature/phase3b-strategy` (merged 2026-04-18)
> Merge commit: `ab1354d`

## Status

| Phase | Desc | Status | Commit |
|-------|------|--------|--------|
| P1 | Schema v6 + Strategy enum + ECS component | ✅ | `3fc5e3f` |
| P2 | Combat multiplier + MOB priority sort | ✅ | `472541d` |
| P3 | `codeforge strategy` CLI + Statusline + TUI | ✅ | `8a85894` |
| Review r1 | code-review — 2 CRIT (re-classified) + 4 IMP | ✅ | `9a2c8af` |
| Review r2 | Verification + 1 stale doc fix | ✅ | `86d941d` |
| Merge | feature/phase3b-strategy → main | ✅ | `ab1354d` |

## Final goal — met

`codeforge strategy <name>` 切換 4 種打法；daemon 戰鬥套用 ATK/DEF 乘子 + MOB 優先序。

## Success criteria — all PASS

1. ✅ DB：`pet_snapshot.strategy` v6 migration + upgrade path tested
2. ✅ Combat 乘子：4 × 策略 × (ATK mult, DEF mult) 單元測 PASS
3. ✅ MOB 優先序：Aggressive / Defensive / Scholar / Explorer 在 mixed zone 驗證
4. ✅ CLI：`codeforge strategy [name]` 讀寫 + invalid error
5. ✅ Statusline：row 4 `strat:<tag>` 顯示當前策略
6. ✅ TUI pet panel：stats 行加 `strat:<full>`
7. ✅ Tests：302 → 340（+38）；clippy 32（baseline stable）
8. ✅ CEO level 3 DOA：P1-P3 + QG + 2 輪 review + merge 無中途停

## Notable deltas from plan

- 計劃估 ~320 tests，實際 340（+38 vs +20-30 plan）—— review r1 race regression test +1、migration assert +1、strategy enum 單測比計劃多。
- **Round 1 race finding**: CLI write 可能被 daemon 的下一 tick `serialize_to_db` 蓋回去，用戶切換策略後下一 tick 靜默復原。已加 `GameWorld::refresh_strategy_from_db` 在 tick step 3b（tx 內、combat 前）把 DB 寫入吃進 ECS，regression test 已覆蓋。Round 2 確認乾淨。

## Deferred / follow-ups

- **Tome Sense ability (Lv 15)** Scholar loot rate +20% —— 依賴 Phase 2.5 ability 系統
- **Explorer cross-zone priority** —— spec 原意是「優先未探索 Zone」，Phase 3b 只有 home zone，degenerate 成 within-zone id order；code comment 已標 Phase 3a revisit
- **AliveMob.zone_id** 仍 `#[allow(dead_code)]` —— Phase 3a multi-zone raids 會 consume
