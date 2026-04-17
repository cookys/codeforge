# Phase 3b — Strategy Mode

> Spec: `doc/specs/codeforge-mud-engine.md` §2 Strategy Mode (lines 147-159)
> Depends on: Phase 2b (combat) + Phase 2a (ECS) — both archived
> Size: L | Branch: `feature/phase3b-strategy`

## Final goal

玩家可透過 `codeforge strategy <name>` 選擇 4 種打法，daemon 戰鬥 tick 套用對應的 ATK/DEF 乘子與 MOB 優先序，讓 Phase 2b 戰鬥資料有策略意義。

## Success criteria（可驗證）

1. **DB**：`pet_snapshot.strategy` 欄位存在，預設 `'explorer'`；migration v6 + 舊 DB upgrade path 各一測 PASS。
2. **Combat 乘子**：4 種策略 × (ATK mult / DEF mult) 各驗：damage 隨 atk_mult 比例變；counter 受 def_mult 反比例影響。單元測 PASS。
3. **MOB 優先序**：Aggressive 的 Boss > Zombie、Defensive 的 Ghost > Elite、Scholar 的 Boss > Zombie 在 mixed zone 均有對應 test：`summary.defeats[0].kind` 符合預期。
4. **CLI**：`codeforge strategy scholar` 寫入、`codeforge strategy` 回印目前策略；無效值回錯。整合測 PASS。
5. **Statusline**：含 `strat:<name>` 段（短名稱，最多 8 chars）。
6. **TUI pet panel**：加一行 `策略: <full_name>`。
7. **zero regression**：302 → ≥302 tests PASS、clippy baseline 32 → ≤32 warnings。
8. **CEO level 3 DOA**：整 Phase 完成到歸檔無中途停。

## Scope boundary

**IN**：
- 4 個策略 enum + 乘子 + 優先序
- `pet_snapshot.strategy` 欄位 + migration
- `codeforge strategy` CLI command
- combat tick 套用乘子 + 排序
- statusline + TUI 顯示
- Welcome Back Report **不**加策略（已持久顯示於 statusline）

**OUT（defer）**：
- Tome Sense ability（Lv 15）+ Scholar loot rate bonus — 依賴 Phase 2.5 ability 系統
- Strategy 切換 analytics / history log
- AI Commentary 對策略的回應（Phase 3c）
- Cross-zone raid（AliveMob.zone_id 已預備 #[allow(dead_code)]，但 Phase 3a 才有 multi-zone）

## Architectural decisions

| 決策 | 選擇 | 理由 |
|------|------|------|
| 持久化位置 | `pet_snapshot` 欄位 | 沿用 Mood 前例；co-located 單 UPSERT |
| 預設值 | `explorer` (1.0x / 1.0x，無優先偏好) | Phase 2b 既有行為 backward-compatible |
| Enum 表示 | `Strategy` enum + `as_str()` / `from_str` | 同 `MobKind` pattern |
| DB 欄位型別 | `TEXT NOT NULL DEFAULT 'explorer'` | 好 debug、未來擴充不需要 schema change |
| 優先序實作 | 在 Rust 載入後 sort | SQL CASE 24 分支難讀；MAX_ATTACKS_PER_TICK=50 > MAX_MOBS_PER_SCAN=20 → LIMIT 已涵蓋全部 alive mobs |
| Active pet 鎖 | 主寵套用（spec §232-238） | Phase 2 only 1 pet；未來多寵再改 |
| Scholar + Tome Sense | defer | ability 系統尚未上線 |

## Phase breakdown

### P1 — Schema + state model（~5 files）

- `src/db/schema.sql`：`pet_snapshot` inline 加 `strategy TEXT NOT NULL DEFAULT 'explorer'`，seed version 6
- `src/db/mod.rs`：`upgrade_pet_snapshot_strategy()`，走 `column_exists` guard；tests：v6 seeded / greenfield has column / upgrade from v5 adds column
- `src/daemon/ecs.rs`：`PetStrategy { value: Strategy }` component；`load_or_init` 新增 `strategy` 讀取（default Explorer 回退）；`serialize_to_db` 寫入
- `src/daemon/strategy.rs`（新）：`Strategy` enum（`Aggressive` / `Defensive` / `Explorer` / `Scholar`）+ `as_str()` / `from_str()` / `atk_mult()` / `def_mult()` / `priority_order(MobKind) -> u8`；unit tests
- 歸 P1 commit：`feat(phase3b/P1): strategy enum + schema v6`

### P2 — Combat integration（~2 files）

- `src/daemon/combat.rs`：
  - `run_tick` 讀取 `PetStrategy` component
  - damage 公式：`((pet_atk as f64) * strategy.atk_mult() * rng(0.8..1.2)).ceil()`
  - counter 公式：`mob.atk - (pet_def * strategy.def_mult() / 2)` 向下取整，saturating_sub
  - 載入後 `mobs.sort_by_key(|m| (strategy.priority_order(m.kind), m.id))`
- 測試：
  - 4 × (atk_mult 高的策略打更重 damage)
  - 4 × (def_mult 低的策略吃更多 counter)
  - Aggressive Boss > Zombie、Defensive Ghost > Zombie、Scholar Boss > Zombie、Explorer 維持 id 排序
- 歸 P2 commit：`feat(phase3b/P2): strategy multiplier + mob priority in combat`

### P3 — CLI + Statusline + TUI（~4 files）

- `src/cli/mod.rs`：新 `Strategy { name: Option<String> }` subcommand
- `src/cli/strategy.rs`（新）：`run(ctx, name)` — name=None 顯示當前；name=Some 更新 `pet_snapshot.strategy`（INSERT OR IGNORE 確保 row 存在 + UPDATE，與 mood 同模式）；無效 name 回 anyhow error 列出 4 種有效值
- `src/pet/live_state.rs`：`LiveState` 加 `pub strategy: Option<Strategy>`，從 snapshot 讀
- `src/cli/statusline.rs`：格式加 `| strat:<short>`（aggressive→agg / defensive→def / explorer→exp / scholar→sch）
- `src/tui/render.rs`：pet panel 加一行 `策略: <full_name>`
- 測試：
  - `codeforge strategy` 整合（無 daemon 也能讀寫）
  - invalid name → error
  - statusline / TUI render 字串含 strat segment
- 歸 P3 commit：`feat(phase3b/P3): codeforge strategy CLI + statusline + TUI`

## Risk register

| Risk | Mitigation |
|------|-----------|
| schema v6 migration 在已有 Phase 3d DB 上 apply 失敗 | `column_exists` guard + `migrations_are_idempotent` existing test 會接住 |
| Strategy::from_str 大小寫差異 | 統一 ToLower 比對（user 可能輸入 Aggressive / AGGRESSIVE） |
| TUI panel 高度超出 | mood + next_unlock + strategy 三行；P3 view 上已有 mood，加一行 size 不爆 |
| CEO 收斂失速 | 每 P 完成後 QG 自動跑；review 兩輪 hard 上限 |

## Quality gate

- `cargo check`（zero error）
- `cargo clippy --all-targets -- -D warnings` 不適用（repo baseline 32 warnings）；用 `cargo clippy -- -W clippy::all` 比對總數 ≤ 32
- `cargo test`（target ≥302 → 預期新增 ~15-20 tests → ~320 total）
- `superpowers:requesting-code-review`（max 3 rounds）

## Merge + archive

- `git merge --no-ff feature/phase3b-strategy`
- push（no-op，無 remote）
- `doc/projects/2026-04-18-phase3b-strategy/` → `_archive/`
- `doc/projects/INDEX.md` 加 archive row
- `.claude/handoff.md` 更新 Phase 3b 完成註記
