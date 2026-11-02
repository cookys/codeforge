# Rubric — 2026-11-02-doppelganger-split.md

> Source plan: doc/plans/2026-11-02-doppelganger-split.md

R1: 兩個檔案含同一段非瑣碎程式碼（≥ 8 個保留行、≥ 200 bytes、≥ 4 行不同）時 scanner 產生分身怪；只有瑣碎行、import、屬性、註解、同檔內重複時不產生。
R2: 分身怪名稱與 origin_path 只由 scan root 相對路徑對決定，重複區塊長度改變或 re-scan 都不產生第二隻；結果不依賴 `read_dir` 或 HashMap 的迭代順序。
R3: 單次 scan 的讀檔量、重複索引量、每個 hash 的出現次數都有上限，病態輸入（大量相同片段）不會讓工作量變成平方或無上限。
R4: 分身怪名額保留不會讓既有四種怪在沒有重複時少於現行上限；總數 ≤ `MAX_MOBS_PER_SCAN`。
R5: 擊殺分裂嚴格照 A/A/A：on_defeat、每次 1 隻、第 2 代不再分裂、stats ×0.7 ceil ≥ 1、origin_path 與 name 同 parent、loot 同表。
R6: 子代 INSERT 在 parent `mark_defeated` 之後；同 tick 新生的子代不被攻擊；所有 mobs 寫入在同一個 tick transaction。
R7: `SuppressDoppelgangerSplit` 對該 zone 或全域生效時不分裂，過期後恢復；Ward 不擋 scanner 生成第 0 代。
R8: 舊 DB 升級不掉資料，新欄位 `parent_id`、`generation` 對舊列為 NULL／0；新裝 DB 結構與升級後一致；不 bump schema_version。
R9: 不新增 crate；scanner 錯誤不讓 tick 失敗；測試不碰 process-global env。
R10: 使用者看得到的訊息用正體中文；CJK 截斷用 `.chars().take(N)`；格式化只用 `./scripts/fmt.sh`。
R11: spec、proposal、inventory 警語、模組註解、CHANGELOG 與實作一致（check-doc-drift 綠）。
