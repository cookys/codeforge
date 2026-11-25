# 盤點表（autopilot@23fd23038）

三份分頁盤點由 agy 產出（inv-1-3、inv-4-6、inv-7-9），本席做了兩項驗證：179 個 file:line 全部存在、行號在檔案範圍內；抽 8 處對照原始碼，內容都對得上。

## 設計要注意的程式現況
- 2 待決在程式裡仍有 u 撤回（keys.ts:53）。操作員裁定不做收回，設計稿照裁定不畫 u，實作時再移除。
- 8 待辦的資料來源在 runtime 還沒接上（todo.ts:1），目前只會顯示出錯列；設計稿兩種狀態都畫。
- 7 艦隊的他機卡片（ctx.others）還沒接線（fleet.ts:3）。
- 3 派工、4 審查、5 花費的焦點或折疊狀態宣告了但沒接上（dispatch.ts:18-20、review.ts:250、spend.ts:19-22）。新骨架的 index 需要有焦點，實作時要接。

- [inv-1-3.md](inv-1-3.md)
- [inv-4-6.md](inv-4-6.md)
- [inv-7-9.md](inv-7-9.md)

## 第 0 輪審查後的更正
- 待決和待辦的選項鍵只有 a–c（keys.ts:45-47、v74/contract.ts:72）。inv-1-3 寫成 a–e、inv-7-9 寫「含 d」都是錯的。
- 待決頁還有 f 鍵：填入建議選項（keys.ts:57）。inv-1-3 漏列。
- 寫成「可捲動」的段落，依據的 frameRows（frame.ts:264-269）只是照傳入的 scroll 切片，再顯示剩幾列。能不能用鍵捲，要看 pane 有沒有接線；盤點表裡的「可捲動」都只代表有截斷提示。
- 每個 agent 的 tok/s 來源是 speedview.ts:118 的 agentsColumn，目前畫在鍛頁（pages/forge.ts），不在 9 個分頁裡。
- 第 0 輪第二代：上面的 a–c、f 已直接改進 inv-1-3 和 inv-7-9 的原列。待辦的「退回原因」沒有資料來源（todo.ts:35-43），原列已更正。
