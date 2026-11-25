# Rubric — 2026-11-25-r1-bottom-controls.md

> Source plan: doc/plans/2026-11-25-r1-bottom-controls.md

R1: R1 上半部可點元素＝0：PAGES＋EXTRA_PAGES × docked／boxed × 80x24／120x40／200x50 × 宿主框高 5／13／17，`KNOWN` 清空，反向探針在舊碼上是紅的。
R2: 下半部完整：每種尺寸和模式下 `key:a`、`key:s`、換卡入口都在下半部；退化階梯 L0–L5 各有一條測試斷言選對階。
R3: R5 寬度與雙主題對比腳本在新版面全綠。
R4: R6 字母對應在合成單列（L3、L4）時照樣成立，含 `[B, A]` 案例。
R5: 三種尺寸 × docked／boxed 真終端截圖與第 4 節 mock 逐張並排，附差異清單。
R6: 全套失敗數 ≤ 基線 10；band 與 Pane 共用游標有測試；`/forge-stats` 不再開第二個 Pane，三頁內容對照無缺。
R7: 只延伸 ux-v7／v7.4，不重新設計；偏離一律列在第 6 節（R2）。
R8: band 維持一列、左右分區、展開鍵固定在右端，不和 Claude Code 的 `[-]` 衝突（R3）。
R9: 不出第二個 pane；forge-stats 的內容併進 live pane（R4）。
R10: 上下半分界用 01M88CCDQD 已上線的定義（框高的一半），不另立。
R11: 資料來源、band 單列排版、配色 token 值都不在範圍內。
R12: 測試和截圖只用沙箱 HOME／`AUTOPILOT_LIVE_DIR`／`CODEFORGE_DATA_DIR`，不碰操作員的 live 目錄。
R13: D1：v7.4 docked 待決頁 S3 每張卡由上往下排，游標那張卡的 a/b/c、m 跟著卡片 → 本計畫：卡片內容在資訊區，a/b/c、m、送出固定在下半部控制區。原因：R1 優先於 ux-v7（準則表 R1 明寫）。
R14: D2：v7.4 C 雙欄左欄清單可點選換卡 → 本計畫：左欄清單不可點，換卡改由下半部換卡器（點選或 p／n、j／k）。原因：R1 規定上半部不能有可點元素；同一份清單一半可點、一半不可點，會讓操作員混淆。
R15: D3：ux-v7 把選單列放在 band 上方 → 不變，band 仍是最下面一列。R1 已寫明優先，這裡只記錄 v7 原文和 R1 的關係。
R16: D4：forge-stats 可開自己的 Pane → `/forge-stats` 改開 live pane 的對應頁籤。原因：R4。
