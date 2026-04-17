# Plan — Phase 2a Daemon Framework

> 建立：2026-04-17
> 專案：[phase2a-daemon](../projects/2026-04-17-phase2a-daemon/README.md)
> Branch：`feature/phase2a-daemon`
> Size：**L**（multiple escalation triggers：schema migration + new module + IPC protocol + Phase 2 daemon architecture）

## 策略概觀

建 daemon 的**骨幹**（schema → runtime → ECS → event channel → lifecycle → systemd → hook integration），不做戰鬥、MOB、TUI、commentary。這些留給 2b/2c/3b。

## 依賴序

```
P1 (schema)  ─┬─▶ P2 (tick loop)  ─┬─▶ P3 (ECS)  ──▶ P7 (hook wire-up)
              │                    │
              └─▶ P4 (event inbox) ┘
              
  P2 ──▶ P5 (daemon lifecycle CLI) ──▶ P6 (systemd unit)
```

- P1 獨立，先做（已完成：commit `f3f983b`）
- P2/P4 可並行起手（P4 需 P1 的 event_inbox table，已在；不需 P2 的 tick loop）
- P3 需 P2（tick loop 才有 ECS 運行時機）
- P5 需 P2（daemon process 存在）
- P6 需 P5（CLI subcommand 存在）
- P7 需 P4（event emit subcommand 存在）+ P5（daemon 在跑）

## 關鍵技術決策（已鎖定）

| 決策 | 值 | 出處 |
|------|---|------|
| IPC | SQLite `event_inbox` + 500ms poll（Option D） | `ipc-research.md` |
| Tick 間隔 | 60s | `rpg-engine-spec.md` |
| Catch-up | 240 cap + sqrt tail | `rpg-engine-spec.md` |
| Catch-up burst | 100ms per tick | `rpg-engine-spec.md` |
| ECS crate | `hecs` | `rpg-engine-spec.md` |
| DB 位置 | `$CODEFORGE_DIR/codeforge.db`（固定 global） | Phase 2a README §Arch Decisions |
| Daemon deploy | systemd user unit（Linux MVP）；launchd 延後 | `rpg-engine-spec.md` |
| 兩寫者規則 | 縮窄版：daemon 獨占 derived state；`event_inbox` 共寫（欄位不重疊） | `mud-engine-spec` §6 |

## 驗證策略

每個 phase 交付含：
- unit tests（至少覆蓋新表/新函式主要路徑）
- 手動 smoke test（CLI 可呼叫）
- `cargo check` + `cargo clippy` 乾淨

L-workflow QG：`cargo check` + `cargo clippy` + `cargo test` + `superpowers:requesting-code-review`

## Risk / Open Items

1. **hecs + rusqlite interop**：兩者都不是 serde-first，serialization 層可能需要手動映射。→ P3 碰到時處理，若成本過大先用純 SQL（無 ECS），ECS 滑到 2b。
2. **tokio tick precision**：60s drift over 長時間運行 → 使用 `tokio::time::interval` + `MissedTickBehavior::Burst` 驗證。
3. **systemd user unit 權限**：第一次安裝需 `systemctl --user daemon-reload`——使用者可能困惑。→ P6 提供 one-liner install script 或 `codeforge daemon install`。
4. **Phase 1 `pet` table vs Phase 2 `pet_snapshot`**：statusline 的 migration 路徑需明確。→ P3 決定（statusline 改讀 snapshot、還是 daemon 寫回 pet）。

## Backlog 風險（Phase 2a 收斂後回頭看）

- systemd 以外的 Linux（NixOS/runit）？→ 等使用者反饋
- macOS launchd？→ Phase 2 後期
- Windows？→ Phase 5+ 或永不
