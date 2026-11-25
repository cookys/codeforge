---
id: 01M8AJ7X65MJ4SNB95RB2GJ4KM
kind: issue
created: 2026-10-31T00:00:00.000Z
---
- **Since**: 2026-10-31
- **Problem**: 目前只有 Rust 村的 Ferris 有像素圖，其他四個村落的寵物在面板上退回字元圖。
- **Impact**: 非 Rust 的寵物沒有像素圖，視覺不一致。
- **Trigger**: 使用者領養非 Rust 村落的寵物，或要求其他村落也有像素圖
- **Source**: pixel-pet 專案（2026-10-31）刻意縮小範圍
- **Pointer**: doc/plans/2026-10-31-pixel-pet.md
- **Context**: 每村一組 24x24 兩幀，驗證函式會自動涵蓋；畫法要補進繪圖 skill
