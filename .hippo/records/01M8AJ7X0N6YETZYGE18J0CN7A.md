---
id: 01M8AJ7X0N6YETZYGE18J0CN7A
kind: issue
created: 2026-04-16T00:00:00.000Z
---
- **Since**: 2026-04-16
- **Problem**: ingest 指令宣告支援 ChatGPT 匯出，但解析器可能處理不了所有格式變體（JSON 與 markdown）。
- **Impact**: 匯入 ChatGPT 對話時可能失敗或漏資料。
- **Trigger**: 有人要求 ChatGPT 遷移，或第二位使用者採用 CodeForge
- **Source**: 早期設計審查
- **Pointer**: none
- **Context**: 範圍 cli/ingest.rs
