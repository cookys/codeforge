# Session Handoff — 2026-04-18

> CEO level 3 session 結束於 Phase 3b merge + archive。前一份 handoff 記錄了 Phase 3d；這份接著的是 Phase 3b Strategy Mode。

## 今日進度（autonomous CEO level 3）

| 工作 | 內容 | Commit |
|------|------|--------|
| Digest cleanup | 4 則 unprocessed digests 審查 + mark processed（無新 knowledge 要寫） | （無 commit） |
| Phase 3b P1 | Strategy enum（Aggressive/Defensive/Explorer/Scholar）+ schema v6 + ECS PetStrategy component + migration upgrade path | `3fc5e3f` |
| Phase 3b P2 | Combat tick 套用 ATK 乘子 + DEF 乘子 + MOB priority sort；compute_damage / compute_counter helpers 可獨立測試 | `472541d` |
| Phase 3b P3 | `codeforge strategy [name]` CLI、statusline row 4 `strat:<tag>`、TUI pet panel stats 行 `strat:<full>` | `8a85894` |
| Review r1 fixes | 2 IMPORTANT + 1 關鍵 race：CLI → daemon `refresh_strategy_from_db` 在 tick step 3b | `9a2c8af` |
| Review r2 fix | Stale comment 清理（race 已解，不再 deferred） | `86d941d` |
| Merge + archive | `feature/phase3b-strategy` → main；project 歸檔；INDEX 更新 | `ab1354d`, 後續 archive commit |

**Stats**：302 → 340 tests（+38），zero clippy regression（baseline 32 → 32），review round 2 clean。

## Git 狀態

- 分支：`main`（archive 尚未 commit，下一步處理）
- 最新 5 筆：
  ```
  ab1354d chore(merge): feature/phase3b-strategy → main
  86d941d fix(phase3b/review-r2): stale comment cleanup
  9a2c8af fix(phase3b/review): round 1 findings
  8a85894 feat(phase3b/P3): CLI + statusline + TUI
  472541d feat(phase3b/P2): combat multiplier + priority
  ```
- 無 remote，push 是 no-op。
- Feature branch 已刪除。

## Phase Roadmap 現況

- ✅ Phase 1（Memory CLI + Common Pet）
- ✅ Phase 2a（Daemon framework）
- ✅ Phase 2b（MOB + Auto-Combat + Loot）
- ✅ Phase 2c（TUI + Local Map）
- ✅ Phase 3d（黏著度：Welcome Back / Mood / Next Unlock / First-Time）
- ✅ **Phase 3b（Strategy Mode：4 打法 × ATK/DEF 乘子 + MOB 優先序）** ← new
- 下一步候選：
  - **3a World Map + Zone unlock**（需 L1 語言分佈；建議先跑幾次 `codeforge dream` 累積資料），L
  - **3c AI Commentary**（Haiku API，1/hour opt-in；需要 rate-limit 設計），L
  - **3e Loot Crafting + active item**（讓 loot_inventory 可互動），L
  - **3f codeforge snapshot**（ASCII 月報；依賴 3d mastery，現在可以做），M

## 啟動下一 session 要做的事

1. **`autopilot:dev-flow` session-start gate** — 開始任何 code 動作前必跑（CLAUDE.md HARD RULE）。
2. 選擇下一 Phase。推薦順序 **3c → 3a → 3f**：
   - 3c 加「人味」—— strategy 已在，Haiku 可以依 strategy 生成不同語氣；需 API + rate-limit
   - 3a 是 World Map UI + Zone unlock；需 L1 語言分佈 → 建議先 `codeforge dream` 幾次
   - 3f ASCII 月報；3d 黏著度資料 + 3b 策略歷史都在，現在做能產出最豐富報告
3. 或者選 3e 讓 loot_inventory 從靜態集合變成可互動（`codeforge craft` / `codeforge use`）。
4. 照 L-workflow 走：plan → project dir → feature branch → P1..N → QG → 2 輪 review → merge → archive。

## 關鍵 Session Rules（持久有效，跨 session，不變）

- **CEO level 3**：全自主執行到底，DOA 內不停、不問 continue。只在 Board Decision / circuit breaker / 天然斷點才停。（feedback: `no-collapse-prompt`）
- **CJK truncation**：`.chars().take(N).collect::<String>()`，**絕不**用 `&s[..N]`（panic）。（feedback: `cjk-truncation`）
- **dev-flow boundary**：code-touch 邊界必 re-invoke（PreToolUse hook 會提醒）。（feedback: `dev-flow-boundary`）
- **Cross-project sync**：CodeForge 設計一律落 `doc/specs/*.md`。（feedback: `cross-project-sync`）
- **Design decision method**：tradeoff 派多 agent 平行研究，不用「先做最小、未來升級」迴避決策。（feedback: `design-decision-method`）
- **ECS component TTL**：serialize 進 `pet_snapshot` 的 component 要考慮釘死風險；持久 state（Mood / Strategy）不需 TTL，per-tick ephemeral（LastMessage）要 TTL。（feedback: `ecs-component-ttl`）
- **RNG salt monotonic**：daemon RNG 用 `tick_count`，不用 `tick_at` 秒數。（feedback: `rng-salt-monotonic`）
- **Shutdown 通道**：tokio task 用 `mpsc::channel(1)`，不用 `Notify::notify_waiters`；`spawn_blocking` 用 `Arc<AtomicBool>` 合作退出。（feedback: `notify-vs-mpsc-shutdown`）
- **CLI ↔ Daemon 共寫 pet_snapshot**：使用者可寫欄位（如 `strategy`）daemon tick 必須先 refresh_from_db 再走 combat/serialize，否則 daemon 會用 stale ECS 覆寫（Phase 3b review r1 抓到的）。

## Phase 3b 留給後續的小尾巴

**不阻塞**但可以 follow-up：

- **Tome Sense ability (Lv 15)** Scholar loot rate +20% —— 等 Phase 2.5 ability 系統上線再補
- **Explorer cross-zone priority** —— spec 原意「優先未探索 Zone」，Phase 3b 只有 home zone degenerate 成 id order；Phase 3a multi-zone 後再 revisit
- **AliveMob.zone_id** 仍 `#[allow(dead_code)]` —— Phase 3a multi-zone raids 會 consume

## Spec 入口

- `doc/specs/codeforge-mud-engine.md`（878 行）— daemon / 戰鬥 / TUI / §2 Strategy（已實作）/ §3 黏著度 / §3.10 Nation Theme
- `doc/specs/nation-p2p-design.md`（353 行）— Nation / Organizer / P2P integrity
- `.claude/rpg-engine-spec.md` — daemon write ownership model
- `.claude/i18n-spec.md` — i18n 兩層設計

## Memory 索引

`~/.claude/projects/-home-codepower-projects-codeforge/memory/MEMORY.md` — 今日無新增 feedback，但 `project_phase2_roadmap.md` 更新：3b 標記 ✅ 2026-04-18、3d 補上 ✅ 2026-04-17。

## 一句話狀態

Phase 3b 全數 ship — pet 現在可由玩家選擇 4 種打法影響戰鬥結果，combat tick 套乘子 + MOB 優先序，statusline + TUI 即時顯示。Review round 1 抓到 CLI → daemon 的 serialize race，已在 tick step 3b 加 `refresh_strategy_from_db` 根治。下一步選 3c/3a/3e/3f 任一都行。
