# Phase 3d — 黏著度機制（Stickiness）

> Spec §3.1 / §3.2 / §3.4 / §3.8 | 依賴：Phase 2a/2b/2c（daemon + combat_log + TUI）都已完成

## Goal

在「玩家不主動打開遊戲」的前提下，把 Ferris 從 tick 狀態機升級成讓人想回來陪的寵物。四個機制從情感 / 目標 / 儀式感三軸下手。

## Success Criteria（Quantifiable）

| KR | 驗證方法 | 閾值 |
|----|---------|-----|
| KR1 | Welcome Back Report 出現在 `codeforge statusline` 首行（隔 >30 min 再叫）及 `codeforge tui` 首次 paint | 手動觸發 + 單元測試：缺席 2h 顯示 2-3 行摘要，缺席 <10min 完全隱藏 |
| KR2 | `pet_snapshot.mood` 欄位存在，整數 0-100，被 daemon tick 以 4 個 signal 更新 | 4 個新 unit test（activity +10 / 6h idle -8 / boss +20 / HP<30 -15）PASS |
| KR3 | `codeforge statusline` 與 TUI pet 面板顯示 `next: <Ability Name> (Lv X)`；Lv ≥ 30 時顯示 `next: Lv 50 Legendary` | 2 個整合測試 covering Lv 1 / 3 / 7 / 12 / 25 / 35 的查找邏輯 |
| KR4 | `first_events` table 存在，每個 `event_id` 只寫一次；重啟後不重複觸發 | 單元測試：連續呼叫 `try_trigger_first_event("first_boss_kill")` 只第一次回傳 true |
| KR5 | 品質 gate | `cargo check`、`cargo clippy --all-targets`、`cargo test` 全綠；新檔 0 clippy warning；≥ 15 new tests |

## Scope Boundary

### In-scope
- Schema v5：`pet_snapshot` 加 `mood` + `mood_tick_stamp`；新 `first_events` table；reuse `settings` 存 `last_player_seen_at`
- Mood state（數值 + tick 更新規則）
- Next unlock anchor（查表，不含 ability 效果實作）
- First-time events（觸發紀錄 + 一行訊息進 `LastMessage` pipeline）
- Welcome Back 摘要（SQL 聚合 + 渲染）

### Out-of-scope（明確 defer）
- Ability 效果本體（Quick Eye / Focus Strike / ...）→ 未排 phase，跟 combat 深化一起
- Mood 影響 commentary 語氣 → Phase 3c AI Commentary
- Zone Mastery 聲望（spec §3.3）→ Phase 3a（需要 World Map）
- Crafting / Active Item / Snapshot → Phase 3e / 3f

## Data Model Changes

### Schema v5 migration（inline in `src/db/schema.sql`）

```sql
-- v5: stickiness
ALTER TABLE pet_snapshot ADD COLUMN mood INTEGER NOT NULL DEFAULT 60;
ALTER TABLE pet_snapshot ADD COLUMN mood_tick_stamp INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS first_events (
    event_id     TEXT PRIMARY KEY,
    triggered_at TEXT NOT NULL,
    tick_count   INTEGER NOT NULL,
    payload      TEXT         -- JSON extras (zone_id, mob_name, etc.)
);

INSERT OR IGNORE INTO schema_version (version) VALUES (5);
```

`last_player_seen_at` 用 `settings(key='last_player_seen_at', value=<ISO>)`，不建新表。

### ECS component（`src/daemon/ecs.rs`）

```rust
pub struct Mood {
    pub value: u32,       // 0..=100
    pub tick_stamp: u64,  // TTL 用，確保重開時不會釘住舊值
}
const MOOD_TTL_TICKS: u64 = 60 * 60 * 24;  // 1 日無更新視為 stale
```

Serialize 時比對 `current_tick - tick_stamp`，若超過 TTL 就重新從 DB 預設 60 補回（防 feedback `ecs-component-ttl` 提到的釘死問題）。

### Ability lookup table（純 Rust const）

```rust
// src/pet/ability.rs
pub const ABILITY_UNLOCKS: &[(u32, &str, &str)] = &[
    (5,  "Quick Eye",      "passive"),
    (10, "Focus Strike",   "active"),
    (15, "Tome Sense",     "passive"),
    (20, "Village Aura",   "passive"),
    (30, "Memory Recall",  "passive"),
    (50, "Legendary",      "village"),  // 顯示名稱後面會夾 village-specific
];

pub fn next_unlock(level: u32) -> Option<(u32, &'static str)> { ... }
```

## Sub-phases

| P | 內容 | 估時 | 關鍵檔 |
|---|------|------|-------|
| **P1** | Schema v5 + ECS `Mood` component + ability 常數表 | 30m | `src/db/schema.sql`, `src/db/migrations.rs`, `src/daemon/ecs.rs`, `src/pet/ability.rs`（新） |
| **P2** | Mood Decay system（daemon tick 計算）+ serialize-with-TTL + live_state 曝露 | 45m | `src/daemon/systems.rs`, `src/daemon/ecs.rs`, `src/pet/live_state.rs` |
| **P3** | Next Unlock Anchor（statusline + TUI 渲染） | 30m | `src/cli/statusline.rs`, `src/tui/panels/pet.rs` |
| **P4** | First-Time Events（daemon 偵測 + 寫入 + 一行感性 commentary 進 `LastMessage`） | 45m | `src/daemon/systems.rs` (new `first_event.rs`), `src/db/schema.sql` |
| **P5** | Welcome Back Report（last_player_seen 更新 + summary 聚合 + statusline/TUI 渲染） | 60m | `src/cli/statusline.rs`, `src/tui/panels/` (new `session_summary.rs`), `src/pet/live_state.rs` |
| **P6** | QG + 2 輪 review + merge + archive | 45m | — |

## 風險與緩解

| 風險 | 緩解 |
|------|------|
| `mood` 欄位每 tick 寫入 → DB 寫量爆 | Mood 只在值變化 ≥ 1 時才 serialize（write coalescing），參考 `LastMessage` 的 skip-NULL 做法 |
| `first_events` 在多 daemon 副本下重複寫入 | `PRIMARY KEY(event_id)` + `INSERT OR IGNORE`，天然 idempotent |
| Welcome Back 計算 "缺席期間" 需要時間基準 | 用 `settings.last_player_seen_at`，statusline / TUI 進入時更新；若 NULL 視為首次，跳過摘要 |
| Tick loop 加 mood 運算會增加 hot path CPU | 4 signal 都是 O(1) 比對；feedback `rng-salt-monotonic` 不相關（不用 rng） |
| 改動跨 `src/pet/` + `src/daemon/` + `src/cli/` + `src/tui/` → skill routing 多處觸發 | Review loop 正好覆蓋 — 單輪 `superpowers:requesting-code-review` 可一次到位 |

## 決策原則

- Mood decay 曲線 / 觸發時間閾值用 spec 標準值，**不另外調**：`+10 / -8 / +20 / -15`、`6h idle`。偏離需明確理由記在 review。
- Welcome Back 的「缺席判定下限」設 **10 分鐘**（<10 min 視為同一 session，不顯示摘要）。
- First-event 的清單先鎖 5 個（spec §3.8 原文表格），不擴充。
- Mood 預設值 **60**（50-79 正常區間中央，確保新 pet 落在「正常」而非「疲憊」）。

## Non-goals（保險）

- 不改動 pet_snapshot 舊欄位
- 不改 tick 長度或 inbox schema
- 不引入新 crate dep（所有工具 std / rusqlite / chrono 已有）
- 不碰 `import/`、`dream/`、`brain/`（out of scope）

## 完成後的下一步

依 handoff 建議順序 → Phase 3b（Strategy Mode）或 Phase 3a（World Map）。由 CEO 在 3d 結束前重新評估優先序。
