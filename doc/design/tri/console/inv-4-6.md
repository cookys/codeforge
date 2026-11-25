# autopilot live pane 功能盤點：4 審查、5 花費、6 殘留

> 快照來源：`autopilot@23fd23038`

---

## 分頁 4：審查（`mods/live/pages/review.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | 程式碼審查列（`<branch/sha>  <X 引擎 VERDICT>  <Y 引擎 VERDICT> · 第 N 輪 · <age>`，若與 QC head 不同則加 `⏶` 提示） | `ctx.live.review.code`（`CodeReviewView`，`mods/live/pages/review.ts:202,208-216`） | 判斷各引擎審查結論是否收斂、是否需要修正，以及程式碼審查與 QC 的 commit head 是否一致 | `4` → 切換至審查頁（`mods/live/pages/review.ts:247`, `mods/live/pages/keys.ts:38-39`）；`j`/`ArrowDown`、`k`/`ArrowUp` → 捲動（`mods/live/pages/keys.ts:41-42`）；`Esc` → 關閉 pane（`mods/live/pages/keys.ts:27`） | **空的**：若整頁 `body.length === 0` 顯示 `○ 沒有審查資料`（`mods/live/pages/review.ts:240`）<br>**很多筆要捲動**：由 `frameRows` 固定高度並在溢位時於最後一列顯示 `↓ 還有 N 列`（`mods/live/pages/frame.ts:6`）<br>**內容很長**：`reviewRow` 使用 `truncateSegs` 依 `cols` 截斷（`mods/live/pages/review.ts:28-41,91`）<br>**出錯讀不到**：`live === null` 時顯示紅色 `✗` 錯誤訊息（`mods/live/pages/review.ts:186-188`）；`actions` 顯示 `資料不足，無法判定`（`mods/live/pages/review.ts:185,242`）<br>**資料過時**：`live.state !== 'ok'` 或 `liveAgeMs > TRACE_STALE_MS` 時顯示紅色 `✗` 與過時閾值說明（`mods/live/pages/review.ts:190-201`） |
| 2 | 計畫審查列（`<plan/branch>  <X 引擎 VERDICT> · 第 N 輪 · <age>`，終止時顯示凍結或 verdict 文字） | `ctx.live.review.plan`（`PlanReviewView`，`mods/live/pages/review.ts:205,217-225`） | 確認計畫審查輪次、各引擎結論與是否已凍結 | 同上（無專屬操作鍵） | **空的**：`plan === null` 時不輸出該列（`mods/live/pages/review.ts:217`）<br>**很多筆要捲動**：同上（`mods/live/pages/frame.ts:6`）<br>**內容很長**：`reviewRow` 以 `truncateSegs` 截斷（`mods/live/pages/review.ts:91`）<br>**出錯讀不到／資料過時**：同第 1 項（`mods/live/pages/review.ts:186-201`） |
| 3 | 推送前檢查（QC 區塊：`▾ 推送前檢查：<狀態> · <age>`、範圍 `range`、受保護檔案數） | `ctx.live.detail.qc`、`ctx.live.detail.qc_at`（`QcView`，`mods/live/pages/review.ts:120-131,203,228-234`） | 確認 QC 狀態（通過／尚欠／不需／未知）與受保護檔案數量，決定是否可推送 | 同上（無專屬操作鍵） | **空的**：`qc === null` 且未啟用 `showMissingQc` 時不顯示（`mods/live/pages/review.ts:228-229`）<br>**很多筆要捲動**：同上<br>**內容很長**：`qcRows` 以 `truncateSegs` 截斷（`mods/live/pages/review.ts:128-129`）<br>**出錯讀不到**：`showMissingQc`（`ctx.v74 !== undefined \|\| ctx.box === true`）且 `qc === null` 時顯示紅色 `✗` 缺檔提示（`<project_key>.qc.json`），並將 `actions` 設為 `資料不足，無法判定`（`mods/live/pages/review.ts:228-234,242`）<br>**資料過時**：依 `live` 整體過時檢查處理（`mods/live/pages/review.ts:190-201`） |
| 4 | 我替你決定的事（已決策清單：`▾ 我替你決定的事（N）`，列出 `✓ <id> <第一行決策> · <age>`） | `ctx.live.detail.decisions`（`DecisionsView`，`mods/live/pages/review.ts:143-157,204,236-238`） | 檢視系統已代為決定的事項與時間 | `Enter` / `z` → 折疊或展開已決策清單（`view.expanded`，`mods/live/pages/review.ts:4,237`、`mods/live/pages/keys.ts:29,43`） | **空的**：`rows.length === 0 && count <= 0` 時不顯示（`mods/live/pages/review.ts:146`）<br>**很多筆要捲動**：同上（`mods/live/pages/frame.ts:6`）<br>**內容很長**：`firstLine` 取第一行後以 `truncateSegs` 截斷（`mods/live/pages/review.ts:23,153`）<br>**出錯讀不到**：`decisions === null && live.sections?.decisionsNotWired === true` 時顯示紅色 `✗` 未接線訊息並將 `actions` 設為 `資料不足，無法判定`（`mods/live/pages/review.ts:236,238,242`）<br>**資料過時**：依 `live` 整體過時檢查處理（`mods/live/pages/review.ts:190-201`） |

---

## 分頁 5：花費（`mods/live/pages/spend.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | 5h / 7d 額度進度條（`meterRow`：百分比、10 格量表、重置倒數 `resetIn`、來源時間或失敗警示） | `seatQuota.view()`（背景執行 `fleet seats --quota --json`，60 秒快取，`mods/live/pages/now.ts:23-24,52-80`）與 `ctx.live.account.quota`、`ctx.console.quota.pools`（`mods/live/pages/spend.ts:183-199`） | 監控當前 seat 的 5 小時與 7 天額度使用率與重置時間，評估是否接近上限 | `5` 或 `u` → 切換至花費頁（`mods/live/pages/spend.ts:253`, `mods/live/pages/keys.ts:21,36-39`）；無列內點擊或展開動作（`mods/live/pages/spend.ts:19-22`） | **空的**：`pct === null` 時不畫空條，改顯示 `— 未接線`（`mods/live/pages/spend.ts:43-45`）<br>**很多筆要捲動**：固定 2 列（5h / 7d）＋ 狀態列<br>**內容很長**：未處理（固定寬度格式化）<br>**出錯讀不到**：全部無資料時顯示紅色 `✗`（`mods/live/pages/spend.ts:213-215`）；指令失敗但有快取時顯示黃色 `⏶` 提示（`mods/live/pages/spend.ts:205-206,210`）<br>**資料過時**：`row.stale === true` 或 `ageMs > SEATS_RED_AFTER_MS`（10 分鐘）時量表轉紅加 `✗` 並附紅色過時說明（`mods/live/pages/spend.ts:49,189,200-204`） |
| 2 | 額度池總覽（兩種模式擇一）：<br>A. `poolEtaRows`：各池狀態、`%`、燃燒率 `%/h`、重置與耗盡預估時間<br>B. `poolRows`（當 `ctx.poolEta === undefined`）：依危險度排序的各池狀態與 `shortVerdict` | A. `ctx.poolEta`（`capacity.pool-eta/1` JSON，`parsePoolEta`，`mods/live/pages/spend.ts:87-139,216`）<br>B. `ctx.console.quota.pools` 與 `ctx.console.sources.quota_forecast`（`mods/live/pages/spend.ts:60-83,216`、`mods/live/pages/quota-pick.ts:37-97`） | 判斷哪個額度池最危險（耗盡、受困、浪費或過時），決定是否切換池或調整派工 | 同上（無專屬操作鍵） | **空的**：`eta === null`、`con === null` 或 `pools.length === 0` 時顯示紅色 `✗` 缺來源列（`mods/live/pages/spend.ts:62,64,114`）<br>**很多筆要捲動**：由 `frameRows` 處理溢位（`mods/live/pages/frame.ts:6`）<br>**內容很長**：未處理（直接輸出組裝字串）<br>**出錯讀不到**：`p.state === 'no_source'` 顯示 `?` 與未量測（`mods/live/pages/spend.ts:124`）；`!paceOk` 時顯示 `?` 缺預測來源（`mods/live/pages/spend.ts:65,76`）<br>**資料過時**：`eta` 超過 `stale_after_s` 顯示紅色 `✗` 與每列紅色過時標記（`mods/live/pages/spend.ts:115-120,135`）；`poolRows` 依 `rankPools` 標記 `stale` / `stale_seat` 並加註過時閾值（`mods/live/pages/spend.ts:78-79`） |
| 3 | 今日用量（僅顯示 token 數 `tokLabel`，不顯示金額） | `ctx.live.detail.spend.tok.host` 或 `tok.session`（`mods/live/pages/spend.ts:218-232`） | 掌握今日 token 消耗量 | 同上 | **空的**：`tok` 為 `null` 時顯示灰字未知與未接線提示（`mods/live/pages/spend.ts:226-227`）<br>**很多筆要捲動**：單列顯示<br>**內容很長**：未處理<br>**出錯讀不到**：`live === null` 或 `sp === null` 時顯示紅色 `✗`（`mods/live/pages/spend.ts:222-223`）<br>**資料過時**：`live.state !== 'ok'` 或 `published_at` 超過 `BAND_STALE_MS`（5 分鐘）時數值轉紅加 `✗` 並附紅色過時說明（`mods/live/pages/spend.ts:221,228,230-231`） |
| 4 | 24 小時趨勢圖（`sparkText`，最多取最後 24 筆 hourly 資料並附來源說明） | `ctx.live.detail.spend.hourly`（`mods/live/pages/spend.ts:234-245`） | 觀察近 24 小時用量波動趨勢 | 同上 | **空的**：無 `hourly` 資料時依 `live`/`sp` 狀態顯示 `—` 與原因（`mods/live/pages/spend.ts:242-244`）<br>**很多筆要捲動**：固定顯示 2–3 列<br>**內容很長**：固定取 `.slice(-24)`（`mods/live/pages/spend.ts:237`）<br>**出錯讀不到**：同「空的」分支顯示原因（`mods/live/pages/spend.ts:243`）<br>**資料過時**：未處理（僅依 `live` 整體狀態） |
| 5 | 輸出速度區塊（最新 `tok/s`、近 5 筆 sparkline、平均／最佳 `tok/s` 與請求數） | `ctx.steps`（由 `register.ts` 從 `turn.step` 串流計時並存於 `$.state`，`mods/live/speed.ts:1-2,9-28`、`mods/live/pages/spend.ts:141-179,247`） | 觀察模型輸出速度是否異常變慢 | 同上 | **空的**：`steps.length === 0` 時顯示 `—` 無資料（`mods/live/pages/spend.ts:145-147`）；無 `tps` 時依 `decodeMs` 顯示未串流／視窗過短／無 token（`mods/live/pages/spend.ts:157-163`）<br>**很多筆要捲動**：固定取最近 5 筆畫 sparkline（`mods/live/pages/spend.ts:171`）<br>**內容很長**：未處理<br>**出錯讀不到**：同「空的」分支<br>**資料過時**：最後一筆超過 `BAND_STALE_MS`（5 分鐘）時轉紅加 `✗` 並附紅色過時列（`mods/live/pages/spend.ts:152,156,165-167`） |

---

## 分頁 6：殘留（`mods/live/pages/hygiene.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | 要你處理區塊（`▾ 要你處理 · <age>`，列出需人工判斷的 worktree／分支等殘留項目與完整原因；若有可自動清理數則先顯示總覽拆分） | `ctx.live.detail.residue`（`ResidueView`，經 `residueTally` 計算，`mods/live/pages/hygiene.ts:182,205-221`） | 檢視需人工介入的髒 worktree 或未整合分支，決定後續手動清理或處理 | `6` → 切換至殘留頁（`mods/live/pages/hygiene.ts:232`, `mods/live/pages/keys.ts:38-39`）；`j`/`ArrowDown`、`k`/`ArrowUp` → 捲動（`mods/live/pages/keys.ts:41-42`）；無 `[o]/[x]/[z]` 按鈕（`mods/live/pages/hygiene.ts:1,3`） | **空的**：`tl.judgeCount === 0` 時顯示綠色 `✓` 與無需處理文字（`mods/live/pages/hygiene.ts:219`）<br>**很多筆要捲動**：若 `tl.unlisted > 0` 顯示 `還有 N 筆`（`mods/live/pages/hygiene.ts:221`），其餘由 `frameRows` 捲動（`mods/live/pages/frame.ts:6`）<br>**內容很長**：使用 `judgeRow(..., wide=true)` 輸出完整描述，由 frame 自動折行不截斷（`mods/live/pages/hygiene.ts:204,220`）<br>**出錯讀不到**：`live === null` 或 `residue === null` 時顯示紅色 `✗` 錯誤訊息（`mods/live/pages/hygiene.ts:167-169,198-201`）<br>**資料過時**：`residue.at` 超過 `TRACE_STALE_MS` 時顯示紅色 `✗`（`residue.json` 過時）（`mods/live/pages/hygiene.ts:210,213-215`） |
| 2 | 自動清理區塊（`▾ 自動清理 · <age>`，顯示已自動移除的 worktree 與封存分支數量） | `ctx.live.detail.residue`（`auto_removed_count`、`auto_archived_count`、`auto_ran_at`、`by_class`，`mods/live/pages/hygiene.ts:60-88,223-226`） | 確認背景自動清理是否正常運作及最近清理成果 | 同上（無專屬操作鍵） | **空的**：`auto === null` 時顯示灰字無自動清理紀錄（`mods/live/pages/hygiene.ts:225`）<br>**很多筆要捲動**：固定 1 列摘要<br>**內容很長**：由 frame 折行<br>**出錯讀不到**：`residue === null` 時略過此區塊（`mods/live/pages/hygiene.ts:198-202`）<br>**資料過時**：同第 1 項（`mods/live/pages/hygiene.ts:213-215`） |
| 3 | 外掛來源區塊（`▾ 外掛來源 · <age>`，顯示載入來源如 `develop` 或 `cache:<v>`、異常問題如落後上游／殘留 cache PID、以及 `flags` 清單） | `ctx.live.detail.load_source`（`LoadSourceView`，`mods/live/pages/hygiene.ts:90-154,183-197,228`） | 確認外掛是否從正確的開發來源載入，排除 cache 殘留或落後上游問題 | 同上（無專屬操作鍵） | **空的**：`ls === null` 時顯示紅色 `✗` 缺 `load_source` 訊息（`mods/live/pages/hygiene.ts:145`）<br>**很多筆要捲動**：`ls.flags` 超過 10 個時截斷並加 `…`（`mods/live/pages/hygiene.ts:193`）<br>**內容很長**：錯誤／破損來源使用 `brokenRows` 以 `wrapPlain` 完整折行不截斷（`mods/live/pages/hygiene.ts:28-29,145,149,152`）<br>**出錯讀不到**：`source` 為 `unknown`/`missing` 或 `isLoadSourceBroken(ls)` 時整段轉紅加 `✗`（`mods/live/pages/hygiene.ts:120-125,149,152`）<br>**資料過時**：`ls.at` 超過 `TRACE_STALE_MS` 時顯示紅色 `✗`（`load-source.json` 過時）（`mods/live/pages/hygiene.ts:185,188-190`） |

---

## 跨頁（與 band／底列的連動）

1. **分頁名稱轉黃警示（`tabNeeds`）**：
   - **審查（`review`）**：當 `snap.detail.qc !== null` 且 `ui.isNeedYou(ui.stateOf('qc', snap.detail.qc.state))` 為真（即 QC 尚欠）時，`tabNeeds` 將 `out.review` 設為 `{}`，底列分頁名稱會以 warning 粗體（黃色）顯示（`mods/live/tabs.ts:69-70,76,85`）。
   - **殘留（`hygiene`）**：當 `judgeCountOf(snap) > 0`（`residueTally(residue).judgeCount > 0`）或 `snap.detail.load_source !== null && ui.isNeedYou(sourceStateOf(snap.detail.load_source))` 為真時，`tabNeeds` 將 `out.hygiene` 設為 `{}`，底列分頁名稱轉黃（`mods/live/tabs.ts:69-70,73,77,85`）。
   - **花費（`spend`）**：`tabNeeds` 未對 `spend` 設定黃色警示；但在 `pageBarStrip` 中，收合態 band 會顯示所有分頁名稱，且「額度」資訊在 `pageBarStrip` 仍保留獨立計數標記（`mods/live/tabs.ts:114,120-125,137-144`）。
2. **`⚑` 計數與 band 連動**：
   - `judgeCountOf(snap)`（即 `residueTally(snap.detail.residue).judgeCount`）是共用的單一數字來源，同時驅動殘留分頁名稱轉黃、現況頁摘要列與 band 上的 `⚑N` 計數（`mods/live/tabs.ts:72-73`）。
3. **其他分頁與底列提示**：
   - `frame.ts` 底部邊框提示依 `V74_BORDER_KEY` 對應 `v74.b.border.review`、`v74.b.border.spend`、`v74.b.border.hygiene`，窄寬度時三頁共用 `v74.b.short.scroll`（`mods/live/pages/frame.ts:101-112`）。

---

## 疑點

1. **花費頁（`spend.ts`）與舊額度頁（`quota.ts`）的雙重模式**：
   - `spend.ts` 註解說明已將舊 `額度` 頁折疊進來（`mods/live/pages/spend.ts:1`），`layout.ts` 也將 `quota` 的按鍵對映到 `'5'`（`mods/live/pages/layout.ts:12`）；但 `tabs.ts` 的 `BAND_PAGE_TABS` 仍同時列出 `'quota'` 與 `'spend'`（`mods/live/tabs.ts:90`），且 `spend.ts` 依 `ctx.poolEta === undefined` 切換 `poolRows` 與 `poolEtaRows` 兩種完全不同的呈現邏輯（`mods/live/pages/spend.ts:216`）。
2. **審查頁（`review.ts`）的折疊預設值與 `keys.ts` 互動**：
   - `DEFAULT_REVIEW_VIEW` 將 `expanded` 設為 `true`（`mods/live/pages/review.ts:15`），但 `reviewPage.render` 呼叫 `renderReview(ctx)` 時未傳入任何 `view` 狀態（`mods/live/pages/review.ts:250`），代表實際渲染時永遠使用 `DEFAULT_REVIEW_VIEW`（展開狀態），`Enter` / `z` 折疊切換並未接上外部狀態保存。
3. **花費頁（`spend.ts`）的 `SpendViewOpts` 保留但無作用**：
   - `spend.ts` 明確註解無可點擊列，卻仍保留 `SpendViewOpts` 與 `spendFocusRows`（永遠回傳 `0`）以相容 pane view table（`mods/live/pages/spend.ts:19-22`）。
