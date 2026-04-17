# Phase 3c — AI Commentary Plan

**Branch**: `feature/phase3c-ai-commentary`
**Spec**: §3.9 (frequency override) + §4 (triggers, generation, display) + §3.5/§3.8 (commentary tone modes)
**Depends on**: Phase 3b Strategy (tone selector), Phase 3d Mood/FirstEvents (trigger data already emitted)

## Goal

> **Final goal**: Pet 在戰鬥 / level-up / 長時間 idle / session 超長時能說話。全域 1/hour、opt-in、strategy-aware 語氣、30 天不重複同一 phrase，有 API key 走 Haiku，無 key/離線 rule-based。
>
> **Success criteria**（皆有量化門檻 + 驗證）：
> 1. `CODEFORGE_COMMENTARY=1` 啟用時，Boss kill / Level up / session ≥ 4h / long idle (≥ 30 min) 各自觸發對應 commentary — 由 unit test 模擬 trigger 驗證。
> 2. Rate limit：同一小時內只 emit 1 條，第 2+ 條需在 `commentary_budget.last_emit_at` ≥ 3600s 才放行 — boundary test `t0 / t0+3599 / t0+3600 / t0+3601`。
> 3. Phrase dedup：同 phrase 在 30 天內不重複 emit — history table 命中即走 fallback pool，test 覆蓋命中/超期/多 kind 同 phrase。
> 4. Haiku API failure (timeout / non-200) → 走 rule-based，絕不 panic，絕不中斷 tick — mock reqwest failure test 覆蓋。
> 5. Opt-out (`CODEFORGE_COMMENTARY` 未設) → 完全不 emit commentary（`commentary_feed` row count 為 0）— integration test。
> 6. Tick latency：commentary 處理 (sync 部分) < 5ms — criterion benchmark 或 debug_assert 時計。
> 7. `codeforge commentary list` / `on` / `off` / `test` CLI 齊全 — CLI integration test 驗證輸出格式。
>
> **Scope boundary**：
> - ✅ Trigger: Boss kill / Level up / Session ≥4h / Long idle / Zone unlock (placeholder — 等 3a multi-zone 才有實作事件)
> - ✅ Display: Statusline footer 5% 機率 / TUI combat_log 區顯示最近 5 條
> - ❌ Git-based trigger（short commit / file 3+ edits / TODO++）→ 需要 L1 analyzer，defer 到 Phase 3f
> - ❌ Nation-specific tone overrides（Nation Plugin）→ Phase 5
> - ❌ 背景 phrase pool LLM 生成 / offline batch — 保持 rule-based pool 靜態 curated

## 前置技術決策

| 決策 | 選擇 | 理由 |
|---|---|---|
| HTTP call 模型 | Haiku call spawn 為 `tokio::spawn` detached task | 不阻塞 tick（spec §設計約束 865：tick < 10ms，LLM async 不阻塞） |
| Rate limit state | 新 table `commentary_budget`（single row id=1） | pet_snapshot 寫入 ownership 只給 daemon 的規則 (`.claude/rpg-engine-spec.md`) 照用，feed / budget / history 單行表 |
| Phrase dedup window | 30 天，SHA-1 hash + `last_used_at` filter | spec §3.9：「已說過的 phrase 至少 30 天不重複」 |
| Offline fallback | 靜態 rule-based pool (`locales/zh-TW.yaml` commentary section) | spec §設計約束 861 + i18n 既有 infra |
| Schema bump | v6 → v7；新增 3 表 `commentary_feed` / `commentary_history` / `commentary_budget` + `settings.commentary_opt_in` (bool) | 遵守既有 ALTER-guard migration pattern (`src/db/mod.rs:83`) |
| Tone selection | village × strategy 二維 key → rule pool；Haiku prompt 含兩維提示 | Handoff 提到「strategy 已在，Haiku 依 strategy 生成不同語氣」 |

## Phase Breakdown

### P1 — Schema v7 + repo

- `src/db/schema.sql` 新增 3 table + `settings.commentary_opt_in` (bool, default 0)
- `src/db/mod.rs` schema_version bump 6 → 7 + ALTER-guard for existing installs
- `src/commentary/repo.rs` — CRUD for feed / history / budget
- 單元測試：schema migration upgrade path、budget single-row invariant
- Commit: `feat(phase3c/P1): commentary schema v7 + repo`

### P2 — Trigger detector + rate limit

- `src/commentary/trigger.rs` — `Trigger` enum + `detect_triggers(world, tick, defeats, level_delta, session_duration)`
- `src/commentary/budget.rs` — `GlobalBudget::check_and_reserve(conn, now) -> bool` (1h window)
- Tick integration：tick.rs 在 first_events 之後、serialize 之前呼叫 `commentary::process_tick`
- 測試：rate limit boundary (t+3599 / t+3600 / t+3601)、multiple triggers 同 tick 只通過 1
- Commit: `feat(phase3c/P2): trigger detector + global rate limit`

### P3 — Generator (Haiku + rule-based fallback + dedup)

- `src/commentary/generator.rs`
  - `async fn generate_haiku(api_key, pet, trigger, context) -> Result<String>`
  - `fn generate_rule_based(pet, trigger, context, rng_salt) -> String`
  - `fn normalize_for_dedup(phrase) -> String` + `phrase_hash`
- `locales/zh-TW.yaml` + `locales/en.yaml` 補 commentary pool（每 kind × 4 strategy 至少 4 phrase）
- 離線 / key 缺 / 2xx 失敗 / 超時 → fallback；所有路徑過 dedup 檢查
- 測試：Haiku mock fail→rule / phrase dedup hit → fallback / 四 strategy 語氣差異
- Commit: `feat(phase3c/P3): Haiku generator + rule fallback + 30-day dedup`

### P4 — Daemon integration (async spawn, no tick block)

- tick.rs：sync pre-check (budget + dedup) 後決定 emit。有 API key + opt-in → `tokio::spawn(generator::emit_haiku_async)`；無 key 或 opt-in 關 → sync rule-based 直接寫 feed
- Shutdown-safe：async task 走 `Arc<AtomicBool>` cooperative cancel (feedback: `notify-vs-mpsc-shutdown`)
- commentary_budget.last_emit_at 在 spawn 前 reserve，Haiku 失敗則 unreserve（避免「預訂了卻沒 emit」卡死 1h 窗）
- 測試：sync path tick latency < 5ms、async path 不阻塞 tick、spawn task 在 shutdown 時不洩漏
- Commit: `feat(phase3c/P4): daemon integration with async Haiku dispatch`

### P5 — Display: Statusline footer + TUI combat log

- `src/cli/statusline.rs` — footer 行 5% 機率（基於 `rng(tick_count)`）替換成 `commentary_feed` 最新 row，否則保持原 footer
- `src/tui/panels/combat_log.rs` — 新增 commentary line（icon prefix `💬` 區隔戰鬥 `⚔`）；保留最近 5 條 commentary
- 測試：statusline 5% 機率分佈（seed 固定驗證）、TUI combat_log 混合排序正確
- Commit: `feat(phase3c/P5): statusline footer + TUI commentary display`

### P6 — CLI `codeforge commentary`

- `src/cli/commentary.rs`
  - `commentary on` → `settings.commentary_opt_in = 1`（等同 `CODEFORGE_COMMENTARY=1` 的持久化版本）
  - `commentary off`
  - `commentary list [--n 10]` → tail feed
  - `commentary test [kind]` → 繞過 rate limit 觸發一條（開發 aid）
- env `CODEFORGE_COMMENTARY` 仍有效，與 settings flag 做 OR
- 測試：CLI integration test 覆蓋四指令輸出
- Commit: `feat(phase3c/P6): codeforge commentary CLI`

### QG + Review

- `cargo check + cargo clippy + cargo test`（目標：340 → ≥ 370 tests，zero clippy regression）
- `superpowers:requesting-code-review` 2 輪；Critical + Important 全 fix
- Commit: `fix(phase3c/review-r1): ...`

### Merge + Archive

- Merge `feature/phase3c-ai-commentary` → main
- Archive plan + project dir
- INDEX.md 更新
- Post-merge: 手動觸發 `codeforge commentary test boss_kill` 驗證顯示

## 不阻塞的 backlog

- Haiku prompt 語料擴充（pet personality system 才會完善）
- TUI commentary 動態動畫 (Phase 4)
- Week Streak trigger（需 Nation credential）
- Zone unlock trigger 真實實作事件（等 Phase 3a）
