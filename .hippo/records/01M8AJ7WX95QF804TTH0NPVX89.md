---
id: 01M8AJ7WX95QF804TTH0NPVX89
kind: issue
created: 2026-11-19T00:00:00.000Z
---
- **Since**: 2026-11-19
- **Problem**: `src/dream/absorb.rs` 寫入 signal 時沒記讀的是哪個 Claude 專案目錄（slug）和檔名。8a5f30a 後只讀當前專案，但事後只能用名稱比對證明不是 PM 的記憶，證明不了全部是本專案的；換版前吸進來的跨專案條目也分不出。
- **Impact**: 隱私稽核無法只計數回答「有沒有跨專案」，得去讀操作員的 Claude 目錄（要卡）。（Minor：absorbed 列不會 ship）
- **Trigger**: ready；做法是 absorb 寫入時在 signal 上帶 `source_slug`（加檔名），加測試；再出一個只計數的檢查：L0 裡 absorbed 列的 slug 不等於當前專案 slug 的數量
- **Source**: PM 01M7R1SD 的只讀比對（41 列，39 個名稱，和 PM 記憶重疊 0）
- **Pointer**: none
- **Context**: 換版前吸進來的舊列沒有 slug，只能靠新的列往後查；舊列要不要清，另外問 PM
