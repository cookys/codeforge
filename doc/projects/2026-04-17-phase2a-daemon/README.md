# CodeForge Phase 2a — Daemon Framework + Tick Loop

> 建立：2026-04-17
> Branch：`feature/phase2a-daemon`（待開）
> 前置：Phase 1 ✅

## Goal

建立長駐 daemon 基礎：tick loop（60s）、game state SQLite schema、ECS 框架（hecs）、daemon 啟動/關閉流程，作為 Phase 2b（戰鬥）+ Phase 2c（TUI）的底座。

## ⚠️ 未決議題：IPC 方案（BLOCKS 所有 KR）

兩份 spec 對 Phase 2a 的 IPC 描述**互相矛盾**。必須先收斂才能鎖定範圍。

| Spec | 立場 |
|------|------|
| `doc/specs/codeforge-mud-engine.md` §6 | Unix socket `~/.codeforge/daemon.sock`，newline JSON，hook 推事件 |
| `.claude/rpg-engine-spec.md` 決策欄 | 「No IPC required, polling via SQLite」「No IPC (polling SQLite)」「Two-writer rule」 |

### 選項 A：SQLite-only（rpg-engine-spec 路線）

- Daemon 只 tick + 寫 `pet_snapshot`；CLI 永遠 read-only
- Hook 事件（session_start, git_commit 等）落地：
  - 方案 A1：寫入 `event_inbox` 表，daemon tick 時 drain → 違反 two-writer rule（CLI 變成寫者）
  - 方案 A2：寫入 JSONL 檔（e.g. `.codeforge/events/YYYY-MM-DD.jsonl`），daemon 用 inotify watch → 沿用 L0 signals 的模式
- 優點：零 socket 生命週期複雜度；fits existing infra
- 缺點：事件延遲 = 一個 tick（60s），不即時

### 選項 B：Unix socket（mud-engine-spec 路線）

- Daemon 跑 accept loop；hook 連上推 JSON
- 優點：即時（事件立刻進 game engine）
- 缺點：socket server + protocol + error handling + 路徑權限（跨 user？）+ socket 殘留清理

### 選項 C：混合（socket **optional**）

- Daemon 預設 SQLite-only（選項 A2）
- Socket 是 opt-in enhancement（Phase 3+），不在 2a 範圍
- 把 mud-engine-spec §6 的 IPC 段改註記為 "Phase 3+ optional"

### 建議

傾向 **選項 C**：延續 Phase 1 的「L0 JSONL + inotify」模式，hooks 把事件當 signal 寫 JSONL，daemon 定期 drain。這樣 Phase 2a 範圍純粹（daemon + tick + schema），IPC socket 留到確實需要即時性時再做。

但這由你拍板。選定後要做的事：
1. 更新 `doc/specs/codeforge-mud-engine.md` §6：改寫 IPC 段符合選擇
2. 更新 `.claude/rpg-engine-spec.md` Decision Log：註記衝突已解
3. 鎖定下方 KR 與 Phase 分解

---

## Success Criteria (KR) — 待 IPC 方案決定後鎖定

| KR | 驗證 | 狀態 |
|----|------|------|
| `codeforge daemon` 可啟動並 tick | `ps` 看到 process + `last_tick_at` 每 60s 更新 | TBD |
| Tick 計算 < 10ms（per 設計約束 §5） | benchmark | TBD |
| 240-tick catch-up cap + sqrt tail | 模擬 2 週離線 → tick 數合理 | TBD |
| hecs ECS：pet entity + PetStats component | daemon 內能 read/write ECS world | TBD |
| SQLite schema: `pet_snapshot`、`game_world`、`combat_log`（空表 OK） | migrations 通過 | TBD |
| Two-writer rule 不破（daemon 獨占 game state 寫） | grep CLI 無 INSERT/UPDATE game_* | TBD |
| systemd user unit（Linux）可啟動 | `systemctl --user status codeforge-daemon` active | TBD |
| daemon 停機不讓 `codeforge statusline` 壞 | kill daemon → statusline 仍正常輸出 | TBD |
| **（IPC 決議後補）** 事件傳遞機制 works | 視選項而定 | TBD |

## Phases（待 IPC 決議後補細節）

| # | Phase | Status |
|---|-------|--------|
| P1 | SQLite schema + migrations（game_world, pet_snapshot, combat_log, last_tick_at） | pending |
| P2 | Tokio + tick loop（60s interval, 100ms burst, 240-cap, sqrt tail） | pending |
| P3 | hecs ECS 整合（PetStats component + xp/regen/levelup systems） | pending |
| P4 | Event ingestion（依 IPC 決議決定） | pending |
| P5 | systemd user unit + launchd plist + `codeforge daemon start/stop/status` | pending |
| P6 | Daemon↔CLI contract 驗證（WAL reader never blocked） | pending |
| QG | Code review + perf baseline + spec sync | pending |

## Open Questions（除 IPC 外）

1. **Daemon 退出策略**：SIGTERM → flush tick → exit？最後 tick 還沒完成呢？
2. **Multi-instance guard**：跑兩個 daemon 會互相踩 SQLite WAL，要 pidfile 還是 SQLite advisory lock？
3. **crash recovery**：daemon panic 後重啟，`last_tick_at` 該從哪裡算？（延伸自 catch-up 邏輯）
4. **初次啟動 no Phase 1 DB**：要 `codeforge init` 先跑？還是 daemon 自己會建？

## Deferred（明確不在 Phase 2a）

- MOB 生成（→ Phase 2b）
- Combat 計算（→ Phase 2b）
- TUI 渲染（→ Phase 2c）
- AI Commentary（→ Phase 3b）
- Nation theme（→ Phase 5c）
