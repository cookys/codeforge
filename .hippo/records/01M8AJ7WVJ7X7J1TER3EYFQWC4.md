---
id: 01M8AJ7WVJ7X7J1TER3EYFQWC4
kind: issue
created: 2026-11-18T00:00:00.000Z
---
- **Since**: 2026-11-18
- **Problem**: 非 autopilot 專案的 session 開 pane，派工、審查、殘留三頁只畫「✗ 本席資料不是最新：沒有專案」加一條照做也無效的修復指令，和「資料不足，無法判定」；標題列每頁帶「✗ 槽位 無來源」，沒說要做什麼。
- **Impact**: 一般 session 是常態；三頁永遠是空的錯誤頁，修復指令誤導，紅色 ✗ 天天都在、失去警示作用。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫
