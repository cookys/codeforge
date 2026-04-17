# CodeForge Phase 2b — MOB 生成 + Auto-Combat + Loot

> 狀態：✅ **完成並合併**（2026-04-17，merge commit `9d15df2`）
> 建立：2026-04-17
> Branch：`feature/phase2b-combat`（已合併，待清理）
> 前置：Phase 2a ✅（daemon 框架 + event_inbox + live read path）
> Spec 參考：`doc/specs/codeforge-mud-engine.md` §2（戰鬥系統）

## Completion Summary

All 6 phases (P1-P6) + QG + 2 review rounds done in a single session.

- **Tests**: 156 passing (baseline 91 → +65). Stable across 3 consecutive runs.
- **Perf**: `tick_budget_under_10ms_average` passes in debug; release binary builds clean.
- **Clippy**: zero warnings on Phase 2b files. Pre-existing Phase 1 warnings untouched.
- **Review**: round 1 surfaced 4 IMPORTANT findings (defeated_at semantic, scanner OOM, read_tick_context error masking, LastMessage stuck forever); round 2 verified all RESOLVED. Merged with `--no-ff` to preserve phase history.

### Post-review fix highlights
- `mobs.defeated_at` now stores real unix seconds (was rng_salt aka tick_count).
- Scanner caps per-file reads at 2 MiB via stat-before-read.
- `read_tick_context` only collapses `QueryReturnedNoRows` to `salt=1`; other errors propagate.
- `LastMessage` carries `tick_stamp` + 5-tick TTL, so the statusline speech bubble clears after a few minutes instead of pinning forever.

## Project Goal

> **Final goal**: Daemon 在每個 tick 主動掃描 codebase 生成 MOB（boss/elite/zombie/ghost/doppelganger/void 六類），執行自動戰鬥並把結果落到 `combat_log` + `loot_inventory`。Pet 透過 kill MOB 累積 XP 與 loot；使用者透過 `codeforge pet` 看到最近戰鬥記錄與 inventory。
>
> **Success criteria**（可量化、可驗證）：
> 1. `cargo test` 全綠且 ≥ 110 個 test（目前 91 + 預計 +20）
> 2. Unit test 模擬「含 TODO × 6 + 長函式」的假 codebase → scanner 回傳 MOB 清單 ≥ 2 隻（驗證 scanner 有實際在分類）
> 3. Unit test 注入 HP=0 的 MOB → combat tick 執行 loot roll，`loot_inventory` 新增 row，pet XP 增加
> 4. E2E smoke：fresh DB → adopt → inject TODO file → daemon 手動 tick 1 次 → `mobs` table 有 row 且 combat_log 非空
> 5. `cargo clippy` 零新 warning（允許前期 pre-existing）
> 6. Code review round 2 無 CRITICAL/IMPORTANT finding
>
> **Scope boundary**：
> - **INCLUDE**：schema（mobs / loot_inventory）、scanner（TODO/FIXME count、function length heuristic）、戰鬥解算（hit/damage/defeat）、loot table（6 MOB kind → drop 對應表）、`codeforge pet` 顯示最近擊殺 + inventory
> - **EXCLUDE（延後）**：
>   - Strategy Mode 四模式 → Phase 3b
>   - Pet Ability 系統（Quick Eye / Focus Strike / Village Aura / Legendary）→ 延後（spec §2.5 新增）
>   - TUI / Local Map 渲染 → Phase 2c
>   - Dead-code 掃描（需 `cargo check` 輸出解析）→ 延後，先用 TODO + function length 兩種 heuristic
>   - Duplicate block 掃描（AST 比對太重）→ 延後
>   - Active item 使用（`codeforge use`）→ Phase 3e
>   - Loot Crafting 合成 → Phase 3e
>   - Mood Decay / Welcome Back report → Phase 3d
>   - Multi-zone（真正的 World Map）→ Phase 3a；P2b zone = pet 的 home village，固定

## Architecture Decisions

| 決定 | 內容 | 理由 |
|------|------|------|
| Zone 模型 | P2b 單 zone = pet 的 home village（rust/python/...）；scanning dir 由 `CODEFORGE_SCAN_DIR` env 決定，預設 daemon 啟動時的 `$PWD` | 真正的 multi-zone World Map 是 Phase 3a；不值得為 P2b 加複雜度 |
| MOB 來源 | Scanner 每 10 tick 跑一次（不是每 tick），cap 20 MOBs/zone。掃 `.rs/.py/.ts/.js/.go` 檔：TODO/FIXME 次數、function length（brace count heuristic）、dead import（簡單 regex） | 掃 code 是 IO + compute 重的操作；每 tick 跑會違反 <10ms budget |
| 戰鬥解算 | Daemon owns writes；Spec §2 公式：`hit_chance = (pet.atk + pet.ver) / (mob.def + difficulty)`；damage `= pet.atk * rng(0.8..1.2)`；pet 也吃傷害 | 維持 Phase 2a 的 "daemon owns derived state" 原則 |
| Loot 儲存 | 新表 `loot_inventory`：id / kind / name / quantity / acquired_at；quantity 聚合重複 loot（e.g. XP potion × 3） | 避免單品 row 爆炸，item kind-level 聚合 |
| Randomness | seed 來源：`tick_at` unix ts（純確定性，unit test 可控） | 跨機器可重現；不需 `rand` crate，用簡單 LCG |
| RNG implementation | 自寫 u64 LCG（Xorshift64*），避免新增 `rand` crate dependency | Cargo.toml 最小化；LCG 對遊戲戰鬥隨機已足夠 |

## Success Criteria (KR)

| KR | 驗證 | 狀態 |
|----|------|------|
| Schema migration 新增 `mobs` / `loot_inventory` table 通過 | `cargo test migrations_*` 全綠 | ✅ |
| Scanner 對含 TODO × 6 的假 codebase → 產生 ≥ 1 Zombie MOB | `src/daemon/mob_scanner.rs` unit test | ✅ |
| Scanner 對含 120 行函式的假檔 → 產生 1 Boss MOB | scanner unit test | ✅ |
| Combat tick 對 hp=1 的 MOB 在 pet.atk 正常下 > 50% 機率擊殺 | 重複 tick 解算，統計 kill rate | ✅ |
| MOB 死亡後 loot roll 至少一筆 entry 進 loot_inventory | combat + loot integration test | ✅ |
| Boss 死亡保證掉 Rare Item + XP；Zombie 掉 XP + TODO Cleaner | loot table unit test per kind | ✅ |
| Pet 吃 MOB 傷害 HP 下降但不會 < 0 | combat unit test，注入強 MOB | ✅ |
| `codeforge pet` 輸出含 "最近擊殺" + "Inventory" 兩段 | smoke test | ✅ |
| Tick budget < 10ms 在 100 MOB 下仍達標（spec §設計約束 5） | perf test with 100 mobs | ✅ |
| Scanner 不會每 tick 跑（cap：10 tick 一次）| scanner call counter test | ✅ |
| Full tick transaction atomic：combat + loot + snapshot 一起 commit 或一起 rollback | tx test with forced error injection | ✅ |

## Phases

| # | Phase | Activities | Status |
|---|-------|-----------|--------|
| P1 | Schema + migrations | `mobs` table（id/zone_id/kind/name/hp/hp_max/atk/def/difficulty/spawned_at/defeated_at）+ `loot_inventory`；migration version 3 | ✅ |
| P2 | MOB scanner | `src/daemon/mob_scanner.rs`：glob source files、count TODO/FIXME、function length、dead import heuristic；產出 MobSpec vec；rate-limit（every 10 ticks）| ✅ |
| P3 | Combat system | `src/daemon/combat.rs`：per-tick attack/damage/defeat；integrate into tick.rs；deterministic RNG | ✅ |
| P4 | Loot system | `src/daemon/loot.rs`：per-kind loot table；insert into `loot_inventory`；apply XP to ECS | ✅ |
| P5 | CLI integration | `codeforge pet` 擴充：最近擊殺 + inventory；optional `codeforge mobs` list | ✅ |
| P6 | Statusline integration | daemon 寫 last_message = 最新戰鬥敘述；statusline 已經會讀（Phase 2a 已有 last_message 欄） | ✅ |
| QG | Quality gate | `cargo check + clippy + test` + perf KR | ✅ |

## Known risks / open questions

1. **Scanner path resolution**：daemon 通常被 systemd 啟動於 `$HOME`；`CODEFORGE_SCAN_DIR` env 必須設，否則掃不到 code。預設行為：env 未設 → skip scanner，warn once。
2. **Function length heuristic**：brace count 對 Rust 有效，對 Python（indent-based）無效。P2b MVP 支援 brace 語言；Python 用獨立 heuristic（dedent back to col 0）或跳過。
3. **Determinism**：RNG seed from `tick_at`。跨 tick 持續性如何？每個 MOB 擊殺事件獨立 seed = `(tick_at, mob_id)`。

## Deferred（明確不在 Phase 2b）

- Dead-code 掃描需 `cargo check --message-format=json` 輸出解析 → Phase 3a
- Duplicate block 需 AST / token hash 比對 → 延後
- Strategy Mode → Phase 3b
- Pet Ability 系統 → 延後（spec §2.5 新增）
- TUI → Phase 2c
- Active item + Loot Crafting → Phase 3e
- Multi-zone（掃 workspace members / 按檔案語言分 zone）→ Phase 3a
