# Proposal — Doppelganger Split Mechanic

**Status**: Awaiting user decision on 3 open parameters
**Blocks**: `SuppressDoppelgangerSplit` runtime consumer (storage 已就位於 Phase 3e)
**Spec touchpoint**: `doc/specs/codeforge-mud-engine.md` §2 line 130

## Context

Phase 3e MVP 已 ship `Doppelganger Ward`（`2× Abstract Gem → 2-day SuppressDoppelgangerSplit`）。Ward 的 CLI / craft / use 全通，effect row 正確寫入 `active_effects`，但 **Doppelganger 本身沒有 split 邏輯** —— `src/daemon/mob.rs:8` 標 "duplicate blocks (Phase 3 — deferred)"，daemon 沒有 on-defeat spawn、沒有 cascade、沒有 child-mob 繼承規則。

Spec §2 line 130 只給一行文字：「Duplicate code block → Doppelganger 🪞 (分裂，清不完)」。§3.5 line 349-350 說 Ward 「使用後：Doppelganger 不分裂，持續 2 天」。兩者加起來不足以實作 —— 五個關鍵參數沒有 user-level 決策。

## 三個必須決定的參數

user 這裡提供答案後，下個 session 可直接接 Ward runtime。預設值是 CTO 推薦的保守起點，不是強制。

### Q1. `split_trigger` — 何時分裂？

| 選項 | 描述 | 權衡 |
|------|------|------|
| **A. on_defeat**（預設推薦）| Doppelganger 被擊殺時才 spawn 子 mob | 最單純；combat loop 現有 `defeats` hook；spec 的「清不完」由 N 連鎖分裂達成 |
| B. on_tick_threshold | 每 X tick 若 Doppelganger 仍活 → spawn 一隻 | 實作複雜；需要 tick counter on mob row；與現有 scanner 重新 spawn 模式重疊 |
| C. on_hp_threshold | HP 降到 50% 時 spawn 一隻（僅一次）| 與 spec 「清不完」意象弱；需要在 mob row 加 "已分裂過" flag |

**答 A 時再決定**: 子 mob 的 `origin_path` 是 **(a) 複製 parent**（scanner 下次掃可能重新視為「解決」）還是 **(b) derived 一個衍生路徑**（例如 `parent-path#clone-1`，獨立於檔案系統）？推薦 (a) —— scanner-consistent 比較不會 state drift。

### Q2. `max_children` — 一次分裂幾隻？cascade 上限？

| 選項 | 描述 | 權衡 |
|------|------|------|
| **A. 每次分裂 1 隻，累積上限 2**（預設推薦）| 3 隻以內 decoupled；kill parent → 1 child；kill child → 1 grandchild；grandchild 不再分裂 | 可收斂；最多累積 3 隻同源；對應 spec「清不完」但給玩家終點 |
| B. 每次分裂 1 隻，無上限 | 真「清不完」—— 每 kill 都產生一隻 | 拒絕方法：要靠 Ward 才能收斂；玩家對 Ward 產生剛性需求（設計意圖對） |
| C. 每次 2 隻，總上限 3 | 分裂一次爆開 2 個 | 玩家體驗突兀；違反「溫和提升難度」的 Forger-Ruins 氛圍 |

### Q3. `child_stat_ratio` — 子 mob 的 stats 繼承多少？

| 選項 | parent stats × ratio | 權衡 |
|------|----------------------|------|
| **A. 0.7**（預設推薦）| 子 mob 比 parent 弱 30% | 戰鬥時間不會隨分裂 explode；XP 總產出 > 原本單隻 |
| B. 1.0 | 子 mob = parent 完整 stats | 真「清不完」感最強但 tick 預算受壓 |
| C. 0.5 | 子 mob 只有半血半攻 | 過度削弱；分裂幾乎沒戰鬥意義 |

**子 mob 的 XP / loot drop**：推薦繼承 parent 同 table（Pattern Fragment + 30% Abstract Gem，spec §2 loot table），stats 調降但 reward 不調降 —— 否則玩家砍子 mob 沒動機。

## 預設答案 = CTO 推薦

如果 user 不想逐題回答，直接回「全用預設」= **A / A / A** —— on_defeat、每次 1 隻累積上限 2、stats 0.7 繼承、origin_path 複製 parent、loot 同 parent。

## 下個 session 執行計畫（user 答完後）

**L-size**，估計 4 phases：
- **P1**: `mob.rs` 加 `split_config` + `spawn_child(parent, stat_ratio)` pure function + tests
- **P2**: `combat::run_tick` 在 defeats 迴圈內 check `SuppressDoppelgangerSplit` active → skip split；否則依 `max_children` rules call `spawn_child`
- **P3**: `mobs` schema 加 `parent_id` + `generation` 欄位（ALTER-guard，Phase 3a 模式）；用於 cascade 計數
- **P4**: 更新 `codeforge inventory` 的 Ward 描述 + 移除 "pending §2 expansion" 標註

估 +25 tests。

## 延伸議題（不 blocking，可先 defer）

- TUI Local Map 怎麼顯示 child mobs？共用 parent 的 glyph？
- `snapshot` 月報要不要統計「Doppelganger split 次數」當成黏著度指標？
- Doppelganger family tree 是否列入 badge trigger（"10 代以內清空一個 family"）？

---

**user 決定後請在這個 doc 底下加 `## Decision` 區塊，列出 Q1 / Q2 / Q3 答案**，下個 session 從該 section 讀。
