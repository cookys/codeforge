# Phase 3c — AI Commentary

> Pet 說垃圾話 / 感性評論系統。Haiku-driven，opt-in，全域 1/hour，strategy-aware 語氣，30 天 phrase dedup。

**Plan**: [../../../plans/2026-04-18-phase3c-ai-commentary.md](../../../plans/2026-04-18-phase3c-ai-commentary.md)
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.9 + §4
**Branch**: `feature/phase3c-ai-commentary`（merged, deleted）
**Started**: 2026-04-18
**Completed**: 2026-04-18 ✅

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 Schema v7 + repo | ✅ | `6e69beb` |
| P2 Trigger + rate limit | ✅ | `caf158a` |
| P3 Generator (Haiku + rule + dedup) | ✅ | `4e6ffa1` |
| P4 Daemon integration (async) | ✅ | `858e09d` |
| P5 Display: statusline + TUI | ✅ | `f80af63` |
| P6 CLI `codeforge commentary` | ✅ | `a07693e` |
| QG | ✅ | 427 tests / 32 clippy warnings (= baseline) |
| Review r1 → fix | ✅ | `b374786` (4 IMPORTANT findings fixed) |
| Review r2 → clean | ✅ | clean — ready to merge |
| Merge + archive | ✅ | `0890258` |

## Final Goal Recap

Pet 在 Boss kill / Level up / Session ≥4h / Long idle 時說話，opt-in、rate-limited、風格隨 strategy 變化。無 API key 走 rule-based pool。

## Success Criteria Tracker

- [x] opt-in flag 控制可啟用（env `CODEFORGE_COMMENTARY=1` OR settings flag）
- [x] rate limit boundary test pass（`can_emit_boundary_exactly_at_window`）
- [x] 30 天 dedup test pass（`seen_within_30_day_window_matches_spec`）
- [x] Haiku failure → rule fallback（`execute_dispatch_empty_api_key_still_rule_based`）
- [x] opt-out = 0 commentary emitted（`decide_returns_none_when_opt_out`）
- [x] tick latency < 5ms（LLM call 移出 tick tx，走 `tokio::spawn`）
- [x] CLI 4 指令齊全（on / off / list / test）

## Review r1 findings

| # | File | Fix |
|---|------|-----|
| 1 | `commentary/dispatch.rs` | 補上 `PRAGMA foreign_keys=ON`，與 `db::Context::open_db` 一致 |
| 2 | `cli/commentary.rs test()` | 移除 `record_history`，避免 debug 時污染 30-day dedup window；加 2 個 lock-in test |
| 3 | `commentary/generator.rs` | 新 `Candidate { rendered, hash }` + `swap_remove` → 消除 double-render trap |
| 4 | `commentary/trigger.rs` + callers | 新增 `Trigger::ManualTest` unit variant，正確 route 到 ManualTest phrase pool |

## Follow-up（不阻塞）

- Haiku prompt injection surface：`mob_name` 來自 MOB scanner。風險低（cosmetic 頻道、60 char 截斷）。Phase 5 Nation plugin 整合時再評估。
- 未來新 trigger：Phase 3a 多 zone 後補 `Trigger::ZoneUnlock` 真實偵測路徑，目前是 placeholder。
- Explicit 測試：current commentary dispatch 的 integration test 缺 end-to-end daemon tick → feed row 驗證（spawn 非同步測試成本高，已靠 unit test + CLI test 覆蓋）。
