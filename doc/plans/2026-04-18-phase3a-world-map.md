# Phase 3a — World Map + Zone unlock Plan

**Branch**: `feature/phase3a-world-map`
**Spec**: §1 (大小地圖系統) + §3.3 (Zone Mastery 聲望條) + §2 Strategy Mode (Explorer 補完)
**Depends on**: Phase 2a game_world table, Phase 2b combat_log aggregation, Phase 3b strategy

## Goal

> **Final goal**: 透過 L1 memory 掃描推算 user 在各語言 Zone 的活躍度，持久化到 `game_world` 表，並提供 `codeforge world` CLI 輸出 ASCII 世界地圖（含 unlock 狀態 + concept 統計 + Zone Mastery rank）。Explorer strategy 從 degenerate 的 id-order 升級為「依 unlock 狀態」排序 MOB 優先序（cross-zone multi-mob raids 留給未來 phase）。
>
> **Success criteria**（皆有量化門檻 + 驗證）：
> 1. L1 analyzer：給定 `{store_dir}/concepts/*.md`，正確累加每個 village 的 concept_count（keyword heuristic）— unit test 餵 fixture 驗證。
> 2. Zone unlock 規則：`(pet.village == zone_id) OR (concept_count ≥ UNLOCK_THRESHOLD)` → `unlocked=1` — test 覆蓋 threshold 邊界。
> 3. Zone Mastery rank：kill_count → rank（Traveler 0-49 / Forger 50-199 / Iron Crafter 200-499 / Veteran 500+）— boundary test。
> 4. `codeforge world` CLI：輸出 5-village ASCII map（unlocked 顯示色 + rank + kill_count + concept_count；locked 顯示 `???`）— 手動 smoke test + integration test 驗證輸出含每個 village 名稱。
> 5. Explorer strategy：`priority_order(kind, zone_id, unlocked_zones)` 在 unlocked zones 中優先「未經常 visit」者 — test 驗證 kill_count 低的 zone 排序靠前。
> 6. Schema v7 → v8 migration：新增 `game_world.concept_count INTEGER DEFAULT 0`，既有 DB 升級保留 rows。
>
> **Scope boundary**：
> - ✅ L1 keyword-based analyzer（rust/python/typescript/go/javascript 5 種語言）
> - ✅ `game_world` 新欄 + unlock 規則 + rank 計算
> - ✅ `codeforge world` CLI（ASCII map）
> - ✅ Explorer strategy 接 unlock state 排序（signature 改變）
> - ❌ 多 Zone 同時 spawn MOB（scanner 仍只掃 home zone） — defer 到 multi-zone raids phase
> - ❌ TUI world map panel（避免 layout 擁擠；`codeforge world` CLI 涵蓋 MVP UX）
> - ❌ Pet 跨 zone 移動（`pet.current_zone` 欄位 + 切換指令） — defer
> - ❌ L1 analyzer 改用 LLM 分類（MVP 走 keyword heuristic）

## 前置技術決策

| 決策 | 選擇 | 理由 |
|---|---|---|
| L1 analyzer 方法 | keyword heuristic（語言名 + build file + framework 名） | spec §1 說 "L1 memory 的 village 分佈"，關鍵字掃描已足夠；LLM 分類屬 over-engineering |
| 是否對同一 concept 多 language 計數 | yes，每個 concept 可同時+1 多個 language | "merge conflict" 等跨語言概念放棄 exclusive 歸類；分數為 soft metric |
| unlock 閾值 | `UNLOCK_THRESHOLD = 3` concepts | 避免單一 mention 即 unlock；與 Zone Mastery Traveler (0-49) 配對保留新手期 |
| zone rank 儲存 | 純計算（不 persist）— from `game_world.kill_count` | spec §3.3 "純 aggregation"；避免 rank 狀態漂移 |
| schema bump | v7 → v8，`game_world.concept_count INTEGER DEFAULT 0` | 遵守既有 ALTER-guard migration pattern |
| Explorer 升級 | `priority_order(kind, zone_id, ctx)` 接 `ZoneStats` slice | 原 `priority_order(kind)` 過時；改 signature 一勞永逸；callers 集中在 combat tick 更新易追 |

## Phase Breakdown

### P1 — L1 analyzer + schema v8

- `src/memory/l1_stats.rs` — `pub fn language_distribution(store_dir) -> HashMap<String, u32>`
  - 掃 `{store_dir}/concepts/*.md` + `{store_dir}/qa/*.md`
  - 關鍵字表 per village（e.g. rust: `["rust", "cargo", "rustc", "Cargo.toml"]`; python: `["python", "pip", "django", "flask"]`; ...）
  - 若 body 含任一關鍵字 → 該 village count += 1（每個 concept 最多 +1 per village）
- `src/db/schema.sql` 新增 `game_world.concept_count INTEGER NOT NULL DEFAULT 0`
- `src/db/mod.rs` migration upgrade path v7 → v8
- Unit test fixture 驗證分布計算
- Commit: `feat(phase3a/P1): L1 language analyzer + schema v8`

### P2 — Zone unlock + rank calculator

- `src/world/mod.rs` — new module
  - `pub fn refresh_from_l1(conn, store_dir) -> Result<()>` — L1 distribution → game_world upsert + unlock 決定
  - `pub fn rank_for(kill_count) -> ZoneRank` — 純函數，Traveler/Forger/Iron Crafter/Veteran 四檔
  - `pub struct ZoneStats { village_id, unlocked, kill_count, concept_count, rank }`
  - `pub fn load_all(conn) -> Result<Vec<ZoneStats>>`
- Unit test: unlock 邊界（concept_count = 2 vs 3）、rank 邊界（49/50/199/200/499/500）、home zone 永 unlock
- Commit: `feat(phase3a/P2): zone unlock + mastery rank`

### P3 — `codeforge world` CLI

- `src/cli/world.rs` — `codeforge world [--refresh]`
  - 預設：load_all → 繪製 ASCII map（5-village grid + 每格含 name / rank / kill_count / concept_count）
  - `--refresh`：先跑 `refresh_from_l1` 再繪
- ASCII 佈局：參考 spec §1 格式，5 village 2×3 網格（第 6 格 `???` 保留 Nation P2P extension）
- 色彩：用 village.rgb() 讓 unlocked zone 上色，locked zone 灰階
- Integration test: CLI 呼叫產出字串含所有 5 village display_name + rank 名稱
- Commit: `feat(phase3a/P3): codeforge world CLI + ASCII map`

### P4 — Explorer strategy revisit

- `src/daemon/strategy.rs` — `priority_order` 簽名改
  - 新增 `Strategy::priority_order_ctx(kind, zone_id, zone_stats: &HashMap<String, &ZoneStats>) -> u8`
  - 舊 `priority_order(kind)` 保留作 fallback（無 zone context 時，用於測試）
  - Explorer: 計算 target zone 的 `kill_count`，低 → 高優先（越新探索越重要）
  - 其他 strategy 保持現有語意（與 zone 無關）
- `src/daemon/combat.rs` — 更新 call site：傳入 ZoneStats
- Handoff 的 "Phase 3b 留給 3a revisit" 這次處理
- Unit test: Explorer 分數：同 kind、兩 zone，kill_count 低的 zone 優先
- Commit: `feat(phase3a/P4): Explorer cross-zone priority via unlock state`

### QG + Review

- `cargo check + cargo clippy + cargo test`（目標：427 → ≥ 450 tests，zero clippy regression）
- `superpowers:requesting-code-review` 2 輪；Critical + Important fix

### Merge + Archive

- Merge `feature/phase3a-world-map` → main
- Archive plan + project dir + INDEX
- 更新 project_phase2_roadmap.md 3a ✅

## 不阻塞的 backlog

- Multi-zone MOB spawning（scanner 掃 all unlocked zone）
- Pet 跨 zone 移動（`pet_snapshot.current_zone` + `codeforge travel <zone>`）
- TUI world map panel 整合
- L1 analyzer 換 LLM 分類（提升準確度）
- `???` 未探索 Nation zone 顯示（等 Nation P2P phase 5）
