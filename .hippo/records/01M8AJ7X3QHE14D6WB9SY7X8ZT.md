---
id: 01M8AJ7X3QHE14D6WB9SY7X8ZT
kind: issue
created: 2026-06-23T00:00:00.000Z
---
- **Since**: 2026-06-23
- **Problem**: 健康檢查的重新探測判斷中，快取過期那一支在現有常數下永遠被先判斷的 TTL 條件蓋住，不會被執行。
- **Impact**: 日後調高常數時語意可能出錯；目前無影響。
- **Trigger**: 調高 PROBE_TTL_MAX 超過 3600 秒，或決定清掉這道保險分支時
- **Source**: 程式碼審查
- **Pointer**: none
- **Context**: 範圍 mnemos/health.rs 的 should_refresh；程式碼已加註解
