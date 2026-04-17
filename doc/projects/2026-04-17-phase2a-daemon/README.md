# CodeForge Phase 2a — Daemon Framework + Tick Loop

> 建立：2026-04-17
> Branch：`feature/phase2a-daemon`（待開）
> 前置：Phase 1 ✅
> IPC 決議：**Option D**（SQLite event_inbox + 500ms poll）— 見 `ipc-research.md`

## Goal

建立長駐 daemon 基礎：tick loop（60s）、game state SQLite schema、ECS 框架（hecs）、SQLite event_inbox 事件通道、daemon 啟動/關閉流程，作為 Phase 2b（戰鬥）+ Phase 2c（TUI）的底座。

## Architecture Decisions

| 決定 | 內容 | 出處 |
|------|------|------|
| IPC | SQLite `event_inbox` table + daemon 500ms poll | `ipc-research.md` |
| 兩寫者規則 | **縮窄**：只有 daemon 寫 derived state（pet_snapshot, combat_log, game_world）；event_inbox 共寫（欄位不重疊） | `ipc-research.md` §3、`.claude/rpg-engine-spec.md` |
| Tick 間隔 | 60s（per `rpg-engine-spec.md`） | Phase 1 |
| Catch-up cap | 240 ticks + sqrt tail（per `rpg-engine-spec.md`） | Phase 1 |
| ECS crate | hecs | Phase 1 |
| Daemon deploy | systemd user unit（Linux）/ launchd plist（macOS，Phase 2 後期） | `rpg-engine-spec.md` |

## Success Criteria (KR)

| KR | 驗證 | 狀態 |
|----|------|------|
| `codeforge daemon start/stop/status` 可啟停並回報狀態 | `systemctl --user status codeforge-daemon` active；`codeforge daemon status` 輸出 pid/uptime/last_tick | TBD |
| Daemon tick 每 60s 跑一次 | `pet_snapshot.updated_at` 每 60s 更新；連續 5 個 tick 無漏 | TBD |
| Tick 計算 < 10ms（per mud-engine §設計約束 5） | Criterion.rs benchmark | TBD |
| 240-tick catch-up cap + sqrt tail（模擬 2 週離線） | unit test：`missed=20000` → effective ticks ≈ 240 + sqrt(19760) ≈ 380 | TBD |
| hecs ECS：PetStats component + xp/regen/levelup/message systems | daemon 內能 read/write ECS world；systems 依賴關係明確 | TBD |
| SQLite schema + migrations：`pet_snapshot`、`game_world`、`combat_log`、`event_inbox`、`last_tick_at` | migrations 通過；空表 OK | TBD |
| Two-writer rule 符合新版：daemon 獨占 derived state 寫入 | grep：CLI 對 `pet_snapshot/combat_log/game_world` 無 INSERT/UPDATE | TBD |
| `codeforge emit <event>` 可 INSERT `event_inbox`（hook 通道） | unit test：emit 後 SELECT 得到 row，seen_at IS NULL | TBD |
| Daemon 500ms poll drain event_inbox | integration test：emit 10 events → daemon 內收到；`seen_at` 寫入 | TBD |
| Daemon 沒跑時 `codeforge emit` 照常成功、事件留存 | daemon 離線 → emit 5 events → 啟動 daemon → 全部被 drain | TBD |
| Daemon 停機不讓 `codeforge statusline` 壞 | kill daemon → statusline 仍正常輸出（read-only SQLite） | TBD |
| Multi-instance guard（兩個 daemon 進程不互踩） | pidfile + SQLite advisory lock；第二個 daemon 啟動 fail with 明確訊息 | TBD |
| Crash recovery：panic 後 `last_tick_at` 仍正確 | systemctl restart → tick 從 last_tick_at 接續（含 catch-up） | TBD |
| Event retention：`seen_at IS NOT NULL AND created_at < now-7d` 自動清理 | 灌 7 天以上舊資料 → tick 後被清 | TBD |

## Phases

| # | Phase | Activities | Status |
|---|-------|-----------|--------|
| P1 | SQLite schema + migrations | 新表：`pet_snapshot`、`game_world`、`combat_log`、`event_inbox`、`last_tick_at`；index；migrations | pending |
| P2 | Tokio runtime + tick loop | 60s interval；100ms burst；240-cap + sqrt tail；`last_tick_at` 持久化；panic 處理 | pending |
| P3 | hecs ECS 整合 | PetStats/VillageId/StatusEffect/LastMessage components；xp/regen/levelup/message systems；tick serialize → `pet_snapshot` | pending |
| P4 | Event inbox pipeline | `codeforge emit <event>` CLI subcommand；daemon 500ms poll；drain → game events；retention cleanup | pending |
| P5 | Daemon lifecycle CLI | `codeforge daemon start/stop/status/restart`；pidfile；advisory lock；signal handling（SIGTERM flush） | pending |
| P6 | Systemd unit | `~/.config/systemd/user/codeforge-daemon.service`；`WantedBy=default.target`；install instructions | pending |
| P7 | Claude Code hook integration | 改 `.claude/scripts/*.js` 改用 `codeforge emit`（session_start / session_end / file_saved / git_commit） | pending |
| QG | Quality gate | Code review + perf baseline（tick <10ms）+ spec sync check | pending |

## Open Questions（納入 P 對應階段處理）

1. **Daemon 退出策略** → P5：SIGTERM → 等當前 tick 完 → flush → exit。若 tick 進行中收 SIGKILL，下次啟動由 catch-up 接回。
2. **Multi-instance guard** → P5：`~/.codeforge/daemon.pid` + SQLite `BEGIN EXCLUSIVE` 開機短暫 probe。第二個 instance 偵測到會 exit 並印錯誤訊息。
3. **Crash recovery** → P2：`last_tick_at` 在**每個 tick 完成後**才寫，daemon panic 時該 tick 效果不落地，下次啟動從上個成功 tick 接續 catch-up。
4. **初次啟動 no Phase 1 DB** → P1：migrations 自帶 `CREATE IF NOT EXISTS`；daemon 啟動時跑 migrations，不需先 `codeforge init`（但 Phase 1 的 init flow 仍沿用）。

## Deferred（明確不在 Phase 2a）

- MOB 生成、Combat 計算（→ Phase 2b）
- TUI 渲染（→ Phase 2c）
- AI Commentary（→ Phase 3b）
- Pet 情緒衰減、welcome-back report、zone mastery（→ Phase 2b/2c）
- Nation theme（→ Phase 5c）
- macOS launchd deployment（→ Phase 2 後期，Linux MVP 先）
- Unix socket 即時通道（→ Phase 3b+ 若 TUI 需要 <50ms 鍵盤事件才加開第二通道）
