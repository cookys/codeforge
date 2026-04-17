# Phase 3f — `codeforge snapshot` Plan

**Branch**: `feature/phase3f-snapshot`
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.6 (可分享 ASCII 輸出)
**Depends on**: Phase 2b combat_log, Phase 3a world/game_world, Phase 3d badges/milestones (optional)

## Goal

> **Final goal**: Ship `codeforge snapshot` — zero-friction shareable ASCII monthly report. Pure stdout, no account/OAuth, box-drawing card covering a rolling 30-day window (combat stats + mini zone map + pet identity). Users copy-paste to Slack/Discord.
>
> **Success criteria**:
> 1. `codeforge snapshot` prints a box-drawn ASCII card to stdout (exit 0).
> 2. Card contains: pet name + date header, 2×2 mini zone grid with rank, monthly kill counts by mob_kind (Boss/Elite/Zombie/Ghost), longest consecutive kill streak in any zone, pet identity line + village tagline quote — each verified by string-contains in integration test.
> 3. `--days N` flag overrides the 30-day window; test proves the combat query uses this value.
> 4. CJK-safe rendering: card width is bounded to ≤60 visible columns on every line, verified by unit test running `unicode-width` over every output line.
> 5. Empty DB (no pet adopted, no combat) prints a valid card with sensible defaults — no panic, tested.
> 6. 100% pure function: `render(SnapshotData) -> String` — separable from DB, testable offline.
>
> **Scope boundary**:
> - ✅ Monthly combat stats (kills by mob_kind, longest consecutive-kill streak in a zone)
> - ✅ Mini zone grid (uses `world::load_all`, 2×2 top 4 zones by kill_count + 5th row if space)
> - ✅ Pet identity + village tagline footer
> - ✅ `--days N` flag (default 30)
> - ❌ `--clipboard` flag (defer — arboard/cli-clipboard adds heavy deps for marginal UX)
> - ❌ Legendary commits progress (depends on Phase 5a Nation P2P)
> - ❌ Loot crafting count (depends on Phase 3e)
> - ❌ Image output (Phase 4 Zoa)

## 前置技術決策

| 決策 | 選擇 | 理由 |
|---|---|---|
| Aggregation module 位置 | `src/snapshot/mod.rs` (new top-level) | spec §3.6 專屬範圍；與 world/ 平行；避免塞進已肥的 cli/ |
| Window 預設 | 30 天 | spec 說 "本月戰績"；滾動窗口比月份邊界友善 |
| Time filter | combat_log.occurred_at ≥ datetime('now', '-N days') | 輕量 SQL；沒 timezone 複雜度 |
| Streak 定義 | 單 zone 內連續 N 個 boss 擊殺（無失敗/無間斷）| spec 範例 "Rust 8 個 Boss 連殺"；簡單定義 |
| 無 pet 時的輸出 | 佔位卡（"no adopt yet"）+ empty 戰績 | spec 沒明說但 pet.rs 已有 graceful null 路徑 |
| Zone grid 數量 | 4 固定（2×2）+ 第 5 zone 在下方單 row | 排版穩定；spec 範例就 4 |
| CJK width | unicode-width，沿用 cli/world.rs 同模式 | 避免再裝 dep；與 world map 一致 |
| Card width | 60 visible cols | spec 範例 58 cols；60 給 bordering |
| clipboard | 不做 | 減少 cross-platform deps；用戶用終端 copy |

## Phase Breakdown

### P1 — Data aggregation

- `src/snapshot/mod.rs`:
  - `pub struct SnapshotData { pet, month, kills, top_streak, zones, generated_at }`
  - `pub fn collect(conn, days: i64) -> Result<SnapshotData>`
  - `combat_log` aggregation grouped by mob_kind (HashMap<String, u32>)
  - Longest streak: query combat_log ordered by occurred_at, group consecutive same-zone kills
  - Pet identity + level from `pet_snapshot` (or None if unadopted)
  - Zone stats from `world::load_all`
- Unit tests: fixture rows, empty DB, window boundary (29d vs 30d vs 31d), streak correctness

### P2 — ASCII card renderer

- `src/snapshot/render.rs` (same module):
  - `pub fn render(data: &SnapshotData) -> String`
  - Box-drawing with ╔═╗ ║ ╠═╣ ╚═╝
  - Sections: header (pet name + date), zone grid (2×2), combat stats (kill breakdown + streak), footer (identity + tagline quote)
  - CJK-safe padding via local pad/clip helpers (or import from `cli/world.rs`'s pattern)
  - Unit tests: width invariants (every line ≤ card width), empty data variant, CJK padding
  - Stateless — `render` takes `&SnapshotData`, returns `String`; no I/O

### P3 — `codeforge snapshot` CLI

- `src/cli/snapshot.rs` — glue `collect → render → println`
- `--days N` flag (default 30), validated N > 0
- Wire into `cli/mod.rs::Commands::Snapshot { days: i64 }`
- Integration test: seed in-memory DB + pet_snapshot + combat_log, run `run(ctx, days)`, assert output contains pet name, "擊殺", village names

### QG + Review

- `cargo check + cargo clippy + cargo test` (target: 466 → ≥ 490 tests)
- `requesting-code-review` 2 rounds; Critical + Important fixes

### Merge + Archive

- Merge `feature/phase3f-snapshot` → main
- Archive plan + project dir + INDEX
- Update `project_phase2_roadmap.md` 3f ✅

## Deferred backlog

- `--clipboard` flag (arboard crate)
- Legendary commits progress (Phase 5a Nation)
- Loot crafting count (Phase 3e)
- Badges line when Phase 3d badge counts are meaningful (already shipped; can add later)
- Image/PNG export (Phase 4)
