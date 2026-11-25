---
id: 01M8AJ7WW0SFYYJVE92NFC2RWQ
kind: issue
created: 2026-11-18T00:00:00.000Z
---
- **Since**: 2026-11-18
- **Problem**: 花費頁只畫 5h／7d 額度百分比和「✗ seats --quota 沒有本席這一列」，判定「資料不足，無法判定」；160x40 帳號列還露出英文原始欄位「primary_owner」。80x24 連本席錯誤列都被擠掉。
- **Impact**: 頁名是花費卻看不到花費，操作員不知道該看哪裡。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫
