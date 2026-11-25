---
id: 01M8AJ7X163AJ3KYSJHXADCRGJ
kind: issue
created: 2026-06-15T00:00:00.000Z
---
- **Since**: 2026-06-15
- **Problem**: ship 的端到端只在隔離測試庫驗過；真實 session 結束後對正式 Mnemos 的整條鏈還沒跑過一次。
- **Impact**: 正式環境的 ledger 是否真的落地沒有證據，且失敗佇列會持續累積。
- **Trigger**: 下次 Mnemos server 在跑時，確認真實 session-end 的 ledger 進了正式腦
- **Source**: ship-online 專案上線後的收尾缺口
- **Pointer**: doc/specs/codeforge-ship.md
- **Context**: 驗完後用 ship --resend 清掉 ship-failed 佇列；失敗只排隊不阻塞，非 blocker
