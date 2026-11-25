---
id: 01M8AJ7XDGMM3RZT0Z6MV8GE0G
kind: issue
created: 2026-11-25T18:27:41.000Z
---
- **Since**: 2026-11-26
- **Problem**: 操作員在 mod 輸入框打 @@ 時，不會跳出席位名稱補全；打 p#<project> 也不會帶出該專案的負責人。v7.6 brief 已寫進這項需求，但到 autopilot develop 13af20c04 都還沒有實作（`git grep '@@' mods/` 只有 viewer 的 diff hunk）。同一份 brief 的第 (1) 行 askpm 已經在 develop。
- **Impact**: 操作員要問特定席位或專案負責人時，只能手打完整位址，容易打錯送錯人。
- **Trigger**: ready；目前里程碑（pane 實際使用驗收）結束後排入
- **Source**: autopilot 01M81Q2P；PM 裁定 01M81MFTV、01M81M4RYT
- **Pointer**: fleet 的 `:<project>` project-owner registry（不另建清單，BL-0404）；程式碼放 autopilot mods/live，合併前由 autopilot 審
- **Context**: 輸入時 a/b/c、s 都當文字的規則不變
