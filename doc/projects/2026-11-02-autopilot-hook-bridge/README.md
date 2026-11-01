# autopilot-hook-bridge — hooks 改由 autopilot bridge 呼叫（v0.0.8）

計畫：[`doc/plans/2026-11-02-autopilot-hook-bridge.md`](../../plans/2026-11-02-autopilot-hook-bridge.md) · BACKLOG：B31（hooks 段提前，owner 卡 d-d9be662c 選 A） · Branch：`feature/autopilot-hook-bridge`

## Project Goal

> **Final goal**: autopilot plugin 的 bridge 每個事件只呼叫一條 `codeforge hook on <event>` 就跑完該事件的全部 codeforge 工作；install／bootstrap 偵測到 bridge 時不寫 hooks 進 settings.json，並清掉舊項目；發布 v0.0.8。
> **Success criteria**: 計畫 §2 的 KR1–KR8，每條都有門檻與驗證方式。
> **Scope boundary**: 包含 `hook on` 子指令（session-start 合併輸出、session-end／pre-compact detach、冪等）、install 偵測與 `--hooks-via`、文件與 v0.0.8 發布；不含 autopilot 端 bridge、statusLine 搬家、B28、B31 其他段、任何機器上實際跑 install／bootstrap。

## Progress

| Phase | Size | Status |
|---|---|---|
| Plan + hetero plan review | — | 進行中 |
| P1 `hook on` 骨架 + session-start 合併 | M | — |
| P2 session-end／pre-compact detach + 冪等 | M | — |
| P3 install 偵測 bridge + `--hooks-via` | M | — |
| P4 文件、版本、e2e、發布 | S | — |

Last updated: 2026-11-02
