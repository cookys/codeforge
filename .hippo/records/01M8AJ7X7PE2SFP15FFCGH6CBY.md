---
id: 01M8AJ7X7PE2SFP15FFCGH6CBY
kind: issue
created: 2026-11-02T00:00:00.000Z
---
- **Since**: 2026-11-02
- **Problem**: 同一行同時有跨行續行與未閉合的區塊註解時，續行狀態蓋掉註解狀態；註解本文若含結束符，續行提前結束，可能生出假的分身怪。
- **Impact**: 只會多生假怪，不影響資料或 tick。
- **Trigger**: 下次動 dup_detect.rs 的行分類，或實際看到這種假分身怪
- **Source**: B10 hetero code review 第 3 輪延後項（finding d06d04b2）
- **Pointer**: none
- **Context**: 加續行內註解狀態並補兩檔負例測試；範圍 daemon/dup_detect.rs 的 classify
