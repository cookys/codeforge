# Phase 3c — AI Commentary

> Pet 說垃圾話 / 感性評論系統。Haiku-driven，opt-in，全域 1/hour，strategy-aware 語氣，30 天 phrase dedup。

**Plan**: [../../plans/2026-04-18-phase3c-ai-commentary.md](../../plans/2026-04-18-phase3c-ai-commentary.md)
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.9 + §4
**Branch**: `feature/phase3c-ai-commentary`
**Started**: 2026-04-18

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 Schema v7 + repo | pending | — |
| P2 Trigger + rate limit | pending | — |
| P3 Generator (Haiku + rule + dedup) | pending | — |
| P4 Daemon integration (async) | pending | — |
| P5 Display: statusline + TUI | pending | — |
| P6 CLI `codeforge commentary` | pending | — |
| QG | pending | — |
| Review r1 → fix | pending | — |
| Review r2 → clean | pending | — |
| Merge + archive | pending | — |

## Final Goal Recap

Pet 在 Boss kill / Level up / Session ≥4h / Long idle 時說話，opt-in、rate-limited、風格隨 strategy 變化。無 API key 走 rule-based pool。

## Success Criteria Tracker

- [ ] opt-in flag 控制可啟用
- [ ] rate limit boundary test pass
- [ ] 30 天 dedup test pass
- [ ] Haiku failure → rule fallback
- [ ] opt-out = 0 commentary emitted
- [ ] tick latency < 5ms (sync path)
- [ ] CLI 4 指令齊全

## Notes

詳細決策與 out-of-scope 見 plan 文件。
