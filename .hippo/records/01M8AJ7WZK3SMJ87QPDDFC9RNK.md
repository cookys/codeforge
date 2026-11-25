---
id: 01M8AJ7WZK3SMJ87QPDDFC9RNK
kind: issue
created: 2026-04-16T00:00:00.000Z
---
- **Since**: 2026-04-16
- **Problem**: 全文搜尋的分詞器不會切中文，搜「重複」這類詞可能漏掉結果。
- **Impact**: 中文記憶搜尋有漏檢，條目變多後更明顯。
- **Trigger**: 使用者回報中文搜尋漏檢，或 FTS 語料超過 500 筆
- **Source**: 早期設計審查
- **Pointer**: none
- **Context**: 需 ICU 或 jieba 外部相依，要先 survey；範圍 memory_fts 與 memory/fts.rs
