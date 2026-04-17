# Phase 3b — Strategy Mode

> Plan: `doc/plans/2026-04-18-phase3b-strategy.md`
> Spec: `doc/specs/codeforge-mud-engine.md` §2 Strategy Mode
> Branch: `feature/phase3b-strategy`
> Started: 2026-04-18

## Status

| Phase | Desc | Status |
|-------|------|--------|
| P1 | Schema v6 + Strategy enum + ECS component | ⏳ |
| P2 | Combat multiplier + MOB priority sort | ⏳ |
| P3 | `codeforge strategy` CLI + Statusline + TUI | ⏳ |
| QG | cargo check + clippy + test | ⏳ |
| Review r1 | superpowers:requesting-code-review | ⏳ |
| Review r2 | verify fixes | ⏳ |
| Merge | feature/phase3b-strategy → main | ⏳ |
| Archive | 歸檔 + INDEX 更新 | ⏳ |

## Final goal

`codeforge strategy <name>` 可切換 4 種打法，daemon 戰鬥套用 ATK/DEF 乘子與 MOB 優先序。

## Summary

延續 Phase 2b 建立的戰鬥骨架，加入 spec §2 定義的 4 種策略（Aggressive / Defensive / Explorer / Scholar）。不新表、不新 zone、不新 MOB 類型；純乘子 + 排序 + CLI 開關。
