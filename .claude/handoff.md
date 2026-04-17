# Session Handoff — 2026-04-17

> 上次 session 結束於 Phase 2c merge + archive。下一 session 讀這份 + `.claude/knowledge/INDEX.md` + memory 即可恢復進度。

## 今日進度（autonomous CEO level 3）

一次 session 內完成 3 個 Phase：

| Phase | 內容 | Merge commit |
|-------|------|--------------|
| 2a | Daemon 框架 + event_inbox + live read path | `174fe54` |
| 2b | MOB 生成 + auto-combat + Loot + CLI/statusline | `9d15df2` |
| 2c | TUI + Local Map（`codeforge tui` 指令） | `c9d91fc` |

**Stats**：baseline 91 → 233 tests，全綠；debug + release 都乾淨；零 clippy warning 在新檔。

## Git 狀態

- 分支：`main`（clean，無 uncommitted）
- 最新 5 筆：
  ```
  bbc1856 chore(docs): archive Phase 2c
  c9d91fc chore(merge): feature/phase2c-tui → main
  b714309 fix(phase2c): review round-1 fixes
  42703da feat(phase2c/P6): codeforge tui command
  cd251c0 feat(phase2c/P5): event loop
  ```
- 沒有 remote，所以 push 是 no-op（本地 only）。
- Feature branch 已刪除。

## 啟動下一 session 要做的事

1. **`autopilot:dev-flow` session-start gate** — 開始任何 code 動作前必跑。
2. 確認 MEMORY.md / `.claude/knowledge/INDEX.md` 沒有 staleness。
3. 決定下一 Phase（見下方 Next 選項）。
4. 若決定 L-size，照 L-workflow：plan → project dir → feature branch → P 階段 → QG → 2 輪 review → merge → archive。

## 關鍵 Session Rules（持久有效，跨 session）

這些是使用者多次明確交代過的規則，**不要忘記**：

- **CEO level 3**：收到「讓 CEO 接手」類語句 → 全自主執行到底，DOA 內不停、不問 continue。只在 Board Decision / circuit breaker / 天然斷點才停。（feedback: `no-collapse-prompt`）
- **CJK truncation**：`.chars().take(N).collect::<String>()`，**絕不**用 `&s[..N]`（panic）。（feedback: `cjk-truncation`）
- **dev-flow boundary**：觸發在 code-touch 邊界（Edit/Write `src/**/*.rs`、Bash `cargo ...` / `git checkout -b`），不只 session 開頭。已裝 PreToolUse hook 提醒。（feedback: `dev-flow-boundary`）
- **Cross-project sync**：CodeForge 設計（含 CodePower session 內的）一律落 `doc/specs/*.md`，單一真實來源，不靠 conversation log 搬。（feedback: `cross-project-sync`）
- **Design decision method**：tradeoff 派多 agent 平行研究彙整；**不要**用「先做最小、未來升級」迴避決策。（feedback: `design-decision-method`）
- **ECS component TTL**：每 tick serialize 進 `pet_snapshot` 的 component 必須帶 `tick_stamp` + TTL，否則釘死 DB 欄位。（feedback: `ecs-component-ttl`）
- **RNG salt monotonic**：daemon 所有 RNG seed 用 `tick_count`，不能用 `tick_at` 秒數。（feedback: `rng-salt-monotonic`）
- **Shutdown 通道**：tokio task 間用 `mpsc::channel(1)`，不用 `Notify::notify_waiters`（會丟 wakeup）。`spawn_blocking` 用 `Arc<AtomicBool>` 合作退出。（feedback: `notify-vs-mpsc-shutdown`）

## Spec 入口（Phase 2+ 必讀）

- `doc/specs/codeforge-mud-engine.md`（878 行）— daemon / 戰鬥 / TUI / §3 黏著度 / §3.10 Nation Theme
- `doc/specs/nation-p2p-design.md`（353 行）— Nation / Organizer / P2P integrity / credential schema
- `.claude/rpg-engine-spec.md` — daemon write ownership model
- `.claude/i18n-spec.md` — i18n 兩層設計

## 下一 Phase 候選（使用者挑）

依 roadmap 順序 + 依賴關係：

| 選項 | Phase | 內容 | 為何選 | 估 size |
|------|-------|------|--------|---------|
| **A** | 3a | World Map + Zone unlock | 延伸 2c TUI 到多 zone；需要 L1 memory 語言分佈 | L |
| **B** | 3b | Strategy Mode（4 種打法）| 擴 combat 行為，資料量小，純策略乘子 | M-L |
| **C** | 3c | AI Commentary（Haiku，1/hour opt-in）| 加人味；需要 Anthropic API，有 rate-limit 設計 | L |
| **D** | 3d | 黏著度（welcome-back / mood decay / mastery / milestones）| 長期保留；依賴 2b+2c 資料已在 | L（拆 4 個 PR） |
| **E** | 3e | Loot Crafting + active item | 把 2b loot_inventory 變可互動 | L |
| **F** | 3f | `codeforge snapshot` ASCII 月報 | 分享型；依賴 3d mastery 才有意思 | M |

**建議順序**：D（黏著度）→ 3b Strategy → 3a World Map → 3c AI Commentary。理由：2b/2c 已經打好資料基礎，3d 可以直接消費現有 data 提升留存；3b 加深度但不需新資料源；3a 需要 L1 語言分佈，可能還要先跑 `dream` 幾次才有東西展示。

但使用者可能有他的偏好，**問他**再決定。

## 功能驗證建議（optional）

Phase 2c TUI 尚未在真 terminal 跑過。想手動試：

```bash
cargo install --path .
# 在 daemon 有跑的環境
codeforge tui
# 按 q 退出
```

如果 review 認為需要 live resize / 更多 UX polish，開 Phase 2c.1 hotfix，別塞進 3a。

## Memory 索引

`~/.claude/projects/-home-codepower-projects-codeforge/memory/MEMORY.md` 現有：
- 3 條 project（phase1、phase2-roadmap）
- 1 條 reference（live-read-pattern）
- 7 條 feedback（見上方 rules 區，都是已 violated 過的）

## 一句話狀態

Phase 2 大致收尾（2a daemon + 2b combat + 2c TUI 都跑完），codebase 已經是可以跑的 MUD 雛形；下一步決定是走深（3b/3d/3e）還是走廣（3a）。
