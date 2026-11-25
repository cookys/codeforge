---
id: 01M8AJ7XAS02723PQYHSCBV8KN
kind: issue
created: 2026-11-05T00:00:00.000Z
---
- **Since**: 2026-11-05
- **Problem**: forge-stats pane 已改成操作區在底、內容往上長，但 MUD TUI 的戰鬥紀錄仍是最新在最上面，與準則相反。
- **Impact**: 兩個介面的閱讀方向不一致。
- **Trigger**: Goal 把 MUD 後段放回目前目標，下次動 MUD TUI 時
- **Source**: PM 決定 MUD TUI 這次不改（2026-11-05）
- **Pointer**: doc/specs/tui-guideline.md
- **Context**: 範圍 tui/panels/combat_log.rs 與 tui/layout.rs
