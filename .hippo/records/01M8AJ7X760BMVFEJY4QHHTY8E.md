---
id: 01M8AJ7X760BMVFEJY4QHHTY8E
kind: issue
created: 2026-11-01T00:00:00.000Z
---
- **Since**: 2026-11-01
- **Problem**: 本機知識庫整個被 gitignore，多機開發彼此看不到；專案範圍的 lesson 計畫改由 pm CLI 寫入並跟著 repo 走。
- **Impact**: 不做的話專案知識無法跨機器共享。
- **Trigger**: 操作員對 pm 計畫拍板、fleet 通知 codeforge 寫遷移計畫；之後先開決策卡，不先動程式
- **Source**: fleet 討論 2026-11-01，codeforge 已提出六項限制
- **Pointer**: doc/specs/codeforge-memory-contract.md
- **Context**: 需新增 atom 的 scope 欄位與來源 repo 欄位；遷移計畫與程式全部未開始
