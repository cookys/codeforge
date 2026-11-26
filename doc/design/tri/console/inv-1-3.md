# Autopilot Live Pane 盤點表：分頁 1 現況、2 待決、3 派工

快照基準：`autopilot@23fd23038`

---

## 分頁 1：現況（`now`，鍵 `1`）

`mods/live/pages/now.ts:1147` 定義 `nowPage = { id: 'now', key: '1', title: '現況', framed: true, render }`。`render` 依 `ctx.v74 !== undefined` 分流至 `renderV74`（`mods/live/pages/now.ts:934`）或舊版佈局（`mods/live/pages/now.ts:935-1143`）。

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | **v7.4 全艦隊總覽**（產線總數、失敗/卡住/未知/運作中計數、主機在線數、失敗/停靠/未知產線列、額度異常池） | 首選 `ctx.v74.pm`（`fleet pm status --json`，`mods/live/v74/contract.ts:3`、`mods/live/pages/now.ts:781-839`）；主機與額度來自 `ctx.console`（`fleet console --json`，`mods/live/pages/now.ts:795-803,815-817`）；降級依序為 `ctx.console`（`mods/live/pages/now.ts:740-759`）→ `ctx.v74.fleetSummary`（`mods/live/pages/now.ts:760-773`） | 判斷全艦隊是否有產線失敗、卡住、離線或額度風險需要介入 | 無本頁專屬列操作鍵；全局鍵 `1`–`9` 切頁、`Esc` 關閉、`w` 展開/收合（`mods/live/pages/keys.ts:27-39`） | **空的**：無異常時只顯示總覽列；**很多筆要捲動**：未處理（直接列出全部異常產線）；**內容很長**：以 `truncateSegs` 截斷單列（`mods/live/pages/now.ts:829-832`）；**出錯讀不到**：全部來源皆無時顯示紅字錯誤（`mods/live/pages/now.ts:774-778`）；**資料過時**：`pmAgeMs` 或 `fetchAgeMs > TRACE_STALE_MS` 時顯示紅字過時警告（`mods/live/pages/now.ts:805-808`） |
| 2 | **v7.4 本席狀態**（專案名、狀態標記與 verdict、經過時間、ctx%、session/host token 用量） | `ctx.live`（`mods/live/pages/now.ts:841-874`） | 確認當前工作席位是否正常運作、上下文與 token 消耗 | 同上 | **空的**：`live === null` 顯示 `no_live` 紅字（`mods/live/pages/now.ts:843-845`）；**很多筆要捲動**：固定 2 列，不需捲動；**內容很長**：未處理；**出錯讀不到**：`live.state !== 'ok'` 顯示紅字原因（`mods/live/pages/now.ts:848-852`）；**資料過時**：`seatAge > TRACE_STALE_MS` 顯示紅字過時警告（`mods/live/pages/now.ts:855-857`） |
| 3 | **v7.4 QC 審查摘要**（審查狀態、範圍 range、受保護檔案數） | `ctx.live.detail.qc`（`mods/live/pages/now.ts:876-887`） | 確認當前席位 QC 是否通過或需人工介入 | 同上 | **空的**：`qc === null` 顯示 `nodata.qc` 紅字（`mods/live/pages/now.ts:880`）；**很多筆要捲動**：固定 2–3 列；**內容很長**：未處理；**出錯讀不到**：同空的；**資料過時**：未處理 |
| 4 | **v7.4 代理決策紀錄**（已完成之代理決策數量與摘要、相對時間） | `ctx.live.detail.decisions`（`mods/live/pages/now.ts:889-907`） | 檢視代理已代為做出的決策與時間 | 同上 | **空的**：`dv === null` 或 `n === 0` 僅顯示空標題列；`decisionsNotWired === true` 顯示紅字（`mods/live/pages/now.ts:893-898`）；**很多筆要捲動**：未處理；**內容很長**：以 `truncateSegs` 截斷（`mods/live/pages/now.ts:904`）；**出錯讀不到**：同空的；**資料過時**：未處理 |
| 5 | **舊版現況（非 v7.4）**（待決提示列、全艦隊總覽、異常與額度池、本席狀態、最近對話、跳轉按鈕 `[2]` 待決 / `[6]` 殘留） | `ctx.console`、`ctx.live`、`seatQuota`（`fleet seats --quota --json`，60 秒快取，`mods/live/pages/now.ts:23-80,933-1143`） | 決定是否跳轉至 `[2]` 待決或 `[6]` 殘留處理異常 | `[2]` 按鈕或鍵 `2` 跳待決；`[6]` 按鈕（寬螢幕且 `residue > 0`）或鍵 `6` 跳殘留（`mods/live/pages/now.ts:940-942`） | **空的**：無異常顯示 `exc_none`（`mods/live/pages/now.ts:1093`）；**很多筆要捲動**：超出高度時截斷並於末列顯示 `另有 N 項` 摘要（`mods/live/pages/now.ts:1105-1141`）；極矮視窗（`avail <= 4`）切換為緊湊摘要模式（`mods/live/pages/now.ts:948-1070`）；**內容很長**：`wrapRow` 折行（`mods/live/pages/now.ts:1077,1111`）；**出錯讀不到**：`con === null` 或 `live === null` 顯示紅字（`mods/live/pages/now.ts:1079,1096-1098`）；**資料過時**：`isBriefStale` 顯示過期標記與分鐘數（`mods/live/pages/now.ts:156,176`） |

---

## 分頁 2：待決（`decide`，鍵 `2`）

`mods/live/pages/decide.ts:1232` 定義 `decidePage = { id: 'decide', key: '2', title: '待決', framed: true, render }`。鍵盤事件由 `routeKey`（`mods/live/pages/keys.ts:24-60`）分派至 `decideReduce`（`mods/live/pages/decide.ts:106-193`）。

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | **決策卡瀏覽與選擇**（依寬度自動切換雙欄 D 模式 `>=83` 欄、單欄 C 模式 `<83` 欄、或極矮緊湊列 `<10` 列；顯示標題、問題、背景、選項 `a`–`c` 與推薦標記、備註） | `ctx.v74.decide`（`fleet decision list --json`，`mods/live/v74/contract.ts:3`、`mods/live/pages/decide.ts:84,1157`）；舊版則讀 `ctx.live.pending` 與 `ctx.console.cards`（`mods/live/pages/decide.ts:53-74`） | 選擇每張決策卡的選項或輸入備註 | `a`–`c`（`PICK_KEYS`）→ 選取選項（`keys.ts:46-47`、`decide.ts:157-164`）；`p`/`n`（v7.4）或 `h`/`l` → 上/下一張卡（`keys.ts:31,56-57`、`decide.ts:135,184-185`）；`v` → 切換 D/C 版面（`keys.ts:32`、`decide.ts:121`）；`e` → 展開背景或緊湊模式（`keys.ts:33`、`decide.ts:123-134`）；`↑`/`↓`/`PgUp`/`PgDn` → 捲動詳情（`keys.ts:34`）；`f` → 填入建議選項（`keys.ts:57`） | **空的**：顯示 `v74.d.empty`（`decide.ts:1168`）；**很多筆要捲動**：雙欄左欄獨立捲動並顯示上/下還有 N 張與已選數，右欄詳情顯示捲軸與上/下更多提示（`decide.ts:996-1028,1070-1112`）；**內容很長**：`narrowAmbiguous` 轉 ASCII 後以 `wrapRow` 折行，左側標題以 `truncateToWidth` 截斷（`decide.ts:1062,1141-1150`）；**出錯讀不到**：`cards === null` 顯示紅字錯誤（`decide.ts:1159-1166`）；**資料過時**：顯示 `v74StaleBanners`（`decide.ts:1158`） |
| 2 | **備註輸入模式**（游標閃爍輸入備註文字） | 本頁狀態 `DecideNav.textMode` / `textBuf`（`mods/live/pages/decide.ts:32`） | 為當前焦點卡片補充文字說明 | `m` → 進入備註輸入（`keys.ts:48`、`decide.ts:156`）；輸入字元 / `Backspace` → 編輯（`decide.ts:149-153`）；`Enter` → 儲存備註（`keys.ts:26`、`decide.ts:154`）；`Esc` → 取消（`keys.ts:26`、`decide.ts:155`） | **空的**：顯示 `v74.d.note_empty`（`decide.ts:991`）；**內容很長**：`wrapRow` 折行（`decide.ts:992`）；**出錯讀不到**／**資料過時**：未處理（已送出或鎖定中之卡片禁止進入 `m`，`decide.ts:156`） |
| 3 | **送出預覽與批量送出**（動作列顯示 `[s] 送出 N 張`，按 `s` 展開預覽清單，按 `y` 送出進入 10 秒可撤回視窗） | `DecideNav.armed`、`snap`、`hold`（`mods/live/pages/decide.ts:94-101,139-147,1175-1198`）；送出與撤回邏輯在 `mods/live/decidebatch.ts`、`mods/live/decideundo.ts` | 確認預覽清單無誤後送出，或在 10 秒內撤回 | `s` → 開啟/重繪送出預覽（`keys.ts:49`、`decide.ts:139-144`）；`y` → 確認送出（`keys.ts:52`、`decide.ts:145-146`）；`Esc` 或 `x` → 取消預覽（`keys.ts:27,54`、`decide.ts:114`）；`u` → 10 秒內撤回（`keys.ts:53`、`decide.ts:178-183`） | **空的**：無可送出卡片時 `s` 不開啟預覽（`decide.ts:142`）；**很多筆要捲動**：動作區受 `frameActionRows` 限制，超出列數顯示 `…另 N 張`（`decide.ts:1189-1197`）；**內容很長**：標題截斷至 14 字、選項 12 字、備註 24 字（`decide.ts:1183-1185`）；**出錯讀不到**：未處理；**資料過時**：預覽期間若選項/備註/卡片變動，`previewStale` 為真，顯示 `v75.stale` 並拒絕 `y`（`decide.ts:101,116-119,145,1194`） |
| 4 | **等你動手（`awaiting_operator_action`）區塊**（已回覆但仍需操作員親自執行的步驟清單，位於 body 底部） | `ctx.v74.awaiting`（`DurableAwaiting[]`，`mods/live/pages/decide.ts:200-269,1200-1211`） | 依編號步驟完成人工操作 | 無專屬按鍵與按鈕（`mods/live/pages/decide.ts:204`） | **空的**：無步驟時不顯示（`decide.ts:238`）；**很多筆要捲動**：依 `awaitingBodyCap` 限制高度，容納不下時整張隱藏並顯示 `…另 N 張`；若會遮住焦點卡選項則自動摺疊為標題列（`decide.ts:242-249,1202-1210`）；**內容很長**：`wrapRow` 折行，絕不截斷（`decide.ts:211-215`）；**出錯讀不到**／**資料過時**：未處理 |

---

## 分頁 3：派工（`dispatch`，鍵 `3`）

`mods/live/pages/dispatch.ts:257-263` 定義 `dispatchPage = { id: 'dispatch', key: '3', title: '派工', framed: true, render }`。本頁為純唯讀頁面，無動作按鈕與列操作鍵。

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 1 | **全艦隊產線摘要列**（來源名稱、年齡、門檻與停滯/運作/失敗/等待/總數計數） | 首選 `ctx.v74.pm`（`fleet pm status --json`），降級依序為 `ctx.console`（`fleet console --json`）→ `ctx.v74.fleetSummary`（`mods/live/pages/dispatch.ts:119-217`） | 快速掌握全艦隊各產線狀態分佈 | 無本頁專屬按鍵；僅全域切頁/關閉/展開鍵（`mods/live/pages/keys.ts:27-39`） | **空的**：全無來源時計數顯示佔位符並附紅字原因（`dispatch.ts:212-216`）；**很多筆要捲動**：固定 2–4 列；**內容很長**：`truncateSegs` 截斷計數列，錯誤原因以 `brokenRows` 折行（`dispatch.ts:47-48,143`）；**出錯讀不到**：顯示紅字錯誤訊息（`dispatch.ts:172,182,215`）；**資料過時**：超過 `TRACE_STALE_MS` 顯示紅字過時警告（`dispatch.ts:165-168,187-194,206-208`） |
| 2 | **派工執行清單**（單一區塊 `派工 N 筆`，欄位為 `線` / `引擎` / `工作` / `狀態`，執行中優先、其次近期結束且未過期者） | `ctx.live.rows`（`mods/live/pages/dispatch.ts:223-254`） | 檢視目前正在執行或剛結束的派工任務與耗時/逾時狀態 | 同上 | **空的**：`all.length === 0` 顯示 `page.dispatch.none`（`dispatch.ts:243-246`）；**很多筆要捲動**：未處理（直接列出全部 `all`，由外框處理）；**內容很長**：各欄依 `W_LINE`/`W_ENGINE`/`wWork` 以 `truncateToWidth` 截斷，整列再以 `truncateSegs` 截斷（`dispatch.ts:111-116,248`）；**出錯讀不到**：`live === null` 或 `live.rows === null` 顯示紅字原因（`dispatch.ts:223-226,233-236`）；**資料過時**：`live.state !== 'ok'` 顯示紅字 `stale_live`，且每列狀態欄加上紅色過期標記（`dispatch.ts:110,229-232`） |

---

## 跨頁行為

1. **切分頁**：`routeKey`（`mods/live/pages/keys.ts:36-40`）依 `V74_KEY` 數字鍵 `1`–`9` 與殼層快捷鍵（`r`/`u`/`p`，其中 `u` 在待決頁保留給撤回）回傳 `{ kind: 'page', to }`；若目標即當前頁則回傳 `{ kind: 'none' }`。
2. **焦點與輸入保護**：`paneFocus === false` 時所有按鍵回傳 `{ kind: 'pass' }` 交還給 Claude Code（`mods/live/pages/keys.ts:25`）。在待決頁進入 `textMode` 時，僅 `Enter`（儲存）與 `Escape`（取消）與字元輸入生效，數字鍵不會誤觸切頁（`mods/live/pages/keys.ts:26`）。
3. **捲動狀態**：待決頁的雙欄捲動（`ctx.pendingScroll.list` 與 `ctx.pendingScroll.detail`）透過 `keyedGrow` / `keyed` 綁定卡片 ID 與游標位置（`mods/live/pages/decide.ts:1002-1003,1078-1085`）；現況與派工頁無獨立捲動狀態。
4. **開著卡／預覽時切頁再切回**：待決頁的 `DecideNav`（含 `cursor`、`picks`、`notes`、`hold`、`sent`、`armed`、`snap`）為頁面外部傳入的狀態（`mods/live/pages/decide.ts:32-34,1169-1170`），只要宿主保留 `DecideNav` 物件，切頁再切回不會遺失選取與備註；但若期間卡片清單變動，`previewStale` 會將已開啟的送出預覽標記為過時（`mods/live/pages/decide.ts:101`）。

---

## 疑點

1. **雙軌渲染並存**：`now.ts`（`mods/live/pages/now.ts:934`）與 `decide.ts`（`mods/live/pages/decide.ts:54`）皆同時保留了 `ctx.v74` 路徑與舊版（`ctx.console` / `ctx.live`）路徑，且 `tabs.ts`（`mods/live/tabs.ts:1-1157`）仍存在另一套舊版 tab builder，兩者邊界與退場時機在程式中未明言。
2. **派工頁 `DispatchView` 未使用**：`mods/live/pages/dispatch.ts:18-20,219` 宣告了 `DispatchView = { focus: number; expanded: boolean }`，但 `dispatchView(ctx, _view)` 並未讀取 `_view`，實際上沒有游標焦點或折疊功能。
3. **跨頁狀態持久化未見於頁面層**：`decide.ts` 的 `DecideNav` 與 `pendingScroll` 皆由呼叫端（`register.ts` 等宿主）注入，頁面本身不負責跨頁切換時的狀態保存或重置。
