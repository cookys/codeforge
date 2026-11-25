# autopilot live pane 盤點表：分頁 7–9 與全頁共用

快照來源：`autopilot@23fd23038`（唯讀盤點，路徑皆相對於快照根目錄）。

---

## 7 艦隊（`fleet`，`mods/live/pages/fleet.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 7.1 | 本機待決卡區塊（`cardRows`）：標題列顯示卡數與更新年齡，逐筆列出 `◆ <title>` | `ctx.v74.decide`、`ctx.v74.ages.decide`（`mods/live/pages/fleet.ts:66-78`）；來源指令為 `fleet decisions list --json`（`mods/live/pages/frame.ts:287`） | 檢視本機目前有哪些待決卡需要處理（本頁為唯讀總覽，需切到 `2 待決` 操作） | 本頁不綁定任何頁面動作鍵（`mods/live/pane.tsx:168`） | **空的**：顯示標題列 `(0)`，無項目列（`fleet.ts:72-77`）<br>**很多筆要捲動**：由 `frameRows` 統一截斷並在末列顯示 `↓ 還有 N 列`（`frame.ts:264-270`）<br>**內容很長**：由 `frameRows` 以 `wrapRow` 折行，懸掛縮排 `WRAP_HANG`（`frame.ts:238`）<br>**出錯讀不到**：`ctx.v74.decide === null` 時顯示紅字 `✗` 錯誤列 `v74.f.cards_missing`（`fleet.ts:69`）<br>**資料過時**：`decideAge > TRACE_STALE_MS` 時顯示紅字 `✗` 過時提示 `trace.fleet_cards_stale`（`fleet.ts:73-75`） |
| 7.2 | 他機待決卡區塊（`othersRows`）：標題列顯示卡數與同步年齡，逐筆列出遠端標記、`c.id`、標題（截斷至 24 寬度）與來源主機目錄 | `ctx.others`（`OthersView`，`fleet.ts:80-95`；未查到 pane 接線處，見疑點） | 掌握其他主機上待決的遠端卡片與所屬位置 | 無 | **空的**：標題列 `(0)`，無項目列（`fleet.ts:87-94`）<br>**很多筆要捲動**：同 `frameRows` 捲動（`frame.ts:264-270`）<br>**內容很長**：`c.question` 截斷至 24 欄（`fleet.ts:91`），整列再由 `frameRows` 折行<br>**出錯讀不到**：`o === null` 顯示 `v74.f.others_missing`；`!o.present` 顯示 `v74.f.others_nosync`；`o.unreadable !== null` 顯示 `v74.f.others_bad`；`o.lastError !== null` 顯示 `fleet.others_error`（`fleet.ts:83-89`）<br>**資料過時**：`o.stale` 時顯示 `fleet.others_stale`（`fleet.ts:88`） |
| 7.3 | 執行中／失敗／已完成三區塊（`runBlocks`）：依序顯示執行中、失敗、已完成之 run 列表，每筆含 `run_id`、`runner/model`、`role` 與耗時／結束時間／失敗原因或 `rc` | `ctx.live.rows`（`fleet.ts:191-218`）；來源指令為 `autopilot status runs --json`（`frame.ts:288`） | 判斷哪些派工正在跑、哪些失敗或逾時、哪些已完成 | 無（`pane.tsx:168`） | **空的**：各區塊顯示 dim 字 `v74.f.none_row`（`fleet.ts:214`）<br>**很多筆要捲動**：同 `frameRows` 捲動（`frame.ts:264-270`）<br>**內容很長**：由 `frameRows` 折行（`frame.ts:238`）<br>**出錯讀不到**：`live === null` 顯示 `v74.f.runs_missing`（`fleet.ts:193`）；`live.rows === null` 顯示 `page.dispatch.no_runs`（`fleet.ts:204`）<br>**資料過時**：`live.state !== 'ok'` 或 `liveAge > TRACE_STALE_MS` 時顯示過時列（`fleet.ts:196-203`） |
| 7.4 | 產線摘要區塊（`lineSummaryBlock`）：顯示總產線數與各狀態計數（`✗` 失敗、`⊘` 閒置/卡住、`◉` 執行中、`⋯` 等待、`○` 待命/未知） | 依序嘗試 `ctx.v74.pm`（`fleet pm status --json`）、`ctx.console`（`fleet.console/1`）、`ctx.fleetFeed.totals`（`fleet tasks --tree --json`，`fleet.ts:35`、`fleet.ts:97-173`） | 快速掌握整體艦隊產線健康度與異常數量 | 無 | **空的**：若三者皆無且無錯誤訊息則不輸出任何列（`fleet.ts:104-172`）<br>**很多筆要捲動**：固定為摘要數列，無捲動問題<br>**內容很長**：錯誤訊息以 `missRows` 依 `size.cols` 折行（`fleet.ts:52`）<br>**出錯讀不到**：`pmErr` 或 `fleetErr` 非空時顯示錯誤列；`console.sources.tasks.state === 'no_source'` 時顯示 `frame.src_no_source`（`fleet.ts:128-130, 138-139, 154-156`）<br>**資料過時**：`pmAgeMs > TRACE_STALE_MS`、`tasksSrc.state === 'stale'|'expired'` 或 `fAgeMs > TRACE_STALE_MS` 時顯示過時提示列（`fleet.ts:120-122, 140-146, 162-164`） |

---

## 8 待辦（`todo`，`mods/live/pages/todo.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 8.1 | 待辦卡片列表（`todoCard`）：每張卡顯示序號 `k/n`、到期日（UTC `MM-DD` 或無期限）、選中游標 `‣` + `•` + 標題、到期說明（固定文字，依有無期限；沒有讀退回原因，todo.ts:35-43）、備註欄 | `ctx.v74.todo`、`ctx.v74.nav.cursor`、`ctx.v74.nav.notes`（`mods/live/pages/todo.ts:35-44, 52-68`）；來源指令為 `fleet todo --json`（`frame.ts:294`；註解指出目前 runtime 尚未接上，`todo.ts:1`） | 瀏覽暫存待辦卡片、確認到期日與備註、選擇要處理的卡片 | `j` / `ArrowDown` → 下一張卡（`keys.ts:41`、`pane.tsx:308-309`）<br>`k` / `ArrowUp` → 上一張卡（`keys.ts:42`、`pane.tsx:308-309`） | **空的**：`cards.length === 0` 顯示 `○` 與 `v74.d.todo_none` / `v74.d.todo_none_hint`（`todo.ts:53`）<br>**很多筆要捲動**：以 `blocks.slice(first)` 依剩餘列數 `room` 向上捲動確保 `cursor` 可見（`todo.ts:64-68`）<br>**內容很長**：`fit()` 依 `ctx.size.cols` 截斷每列（`todo.ts:20-31, 43`）；`frameRows` 亦會以最大 80 欄折行（`frame.ts:236`）<br>**出錯讀不到**：`v === undefined || v.todo === null` 顯示紅字 `✗` `v74.d.todo_no_source · v74.d.todo_recover`（`todo.ts:51`）<br>**資料過時**：`todo.ts` 本身未處理；若在 v7.4 box 內則由 `v74BodyStatusRow` 依 `v74.ages.todo` 顯示過時列（`frame.ts:383, 440-447`） |
| 8.2 | 動作列（`actions`）：顯示目前選中卡片之選項 `[a] <label>`、`[b] <label>`、`[c] <label>`（已選項目套用 `PALETTE.here` 高亮）與移動提示 `v74.d.todo_move` | `cur.options`、`ctx.v74.nav.picks`（`todo.ts:55-63`） | 為目前游標所在的待辦卡片選擇處置選項 | `a` / `b` / `c` → 對應 `pick` 索引 0 / 1 / 2，也可點擊 `[a]`/`[b]`/`[c]` 按鈕（`keys.ts:45-47`、`todo.ts:61`、`pane.tsx:163, 168`） | **空的**：無來源或無卡片時 `actions: []`（`todo.ts:51, 53`）<br>**很多筆要捲動**：最多顯示 `PICK_KEYS.length`（3 個）選項於同一列（`todo.ts:59`）<br>**內容很長**：選項標籤截斷至 `Math.max(4, Math.floor(cols / 3) - 6)`（`todo.ts:61`）<br>**出錯讀不到**：同 8.1<br>**資料過時**：同 8.1 |

---

## 9 說明（`help`，`mods/live/pages/help.ts`）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| 9.1 | 狀態圖示說明區（`v74.b.help.status_title`）：列出 `✗` 失敗、`⊘` 閒置、`?` 未知、`⏶` 要你處理、`◉` 執行中、`✓` 完成、`○` 待命、`⋯` 等待、`◆` 待決、`▸` 回答、`✓` 代理 | 靜態字面與 `GLYPHS.state` / `GLYPHS.structure`、`ctx.t(...)`（`mods/live/pages/help.ts:25-28`），無外部資料來源（`help.ts:1`） | 查閱各狀態圖示含義 | 無頁面專屬動作鍵，僅全域切換／關閉／捲動鍵（`pane.tsx:168`） | **空的**：靜態內容不會為空<br>**很多筆要捲動**：由 `frameRows` 依 `opts.scroll` 捲動並顯示 `↓ 還有 N 列`（`frame.ts:264-270`）<br>**內容很長**：`frameRows` 以最大 80 欄折行（`frame.ts:236`）<br>**出錯讀不到**：不適用（無資料來源）<br>**資料過時**：不適用 |
| 9.2 | 符號說明區（`v74.b.help.symbol_title`）：列出 `▾` 展開、`▸` 收合、`ℹ` 資訊、`ϟ` 速度、`⚑` 殘留、`★` 推薦 | 靜態 `GLYPHS.structure` 與 `ctx.t(...)`（`help.ts:30-32`） | 查閱介面結構與輔助符號含義 | 同 9.1 | 同 9.1 |
| 9.3 | 按鍵與操作說明區（`v74.b.help.keys_title`）：列出基本操作鍵、原因／數字切換／點擊折疊說明；若 `ctx.v74.todo === null` 額外顯示待辦關閉提示 | `ctx.t(...)` 與 `ctx.v74?.todo === null`（`help.ts:34-41`） | 學習鍵盤快捷鍵與滑鼠點擊操作方式 | 同 9.1 | **內容很長**：`labelled()` 以 12 欄標籤寬度、總寬 `Math.max(20, cols - 4)`、懸掛縮排 14 欄折行（`help.ts:14-18`） |

---

## 全頁共用：底列狀態與分頁條、header、band（prompt 上方那條）

| # | 功能（畫面上顯示什麼） | 資料來源（指令／檔案／API，附 file:line） | 操作員在這裡要做什麼決定 | 按鍵（鍵 → 效果，附 file:line） | 狀態：空的／很多筆要捲動／內容很長／出錯讀不到／資料過時，各怎麼顯示（附 file:line；程式沒處理就寫「未處理」） |
|---|---|---|---|---|---|
| C.1 | **Header（非 boxed 模式全域標頭）**：`headerLines` 顯示判決狀態字、專案名、經過時間、Session 花費、Host 今日花費、Context 使用率與 watcher 備註 | `snap.band`、`snap.detail.spend`、`snap.metrics.ctx`、`snap.watcher_note`（`mods/live/tabs.ts:46-61`）；由 `Pane` 在每頁上方繪製（`pane.tsx:471-476`） | 確認目前整體判決狀態、花費與上下文用量 | 無 | **空的／出錯讀不到**：`snap.band === null` 時僅顯示 `snap.header` 狀態文字（`tabs.ts:48`）；`sp === null` 顯示 `$—`、`metrics === undefined` 顯示 `—`（`tabs.ts:52-56`）<br>**內容很長**：`wrapLines(..., ctx.cols)` 折行（`tabs.ts:60`）；若有溢出提示 `withOverflowHint` 會在空間不足時省略提示以保護花費數字（`tabs.ts:179-189`） |
| C.2 | **Header（v7.4 boxed 模式頂框與首列狀態）**：頂框顯示 `<頁名> · 資料 <source>（schema <schema>）· <age>`；若有錯誤／缺源／過時／console 異常，於 body 第一列顯示 `v74BodyStatusRow` | `V74_SOURCES`（`frame.ts:285-295`）、`v74PageAgeMs`（`frame.ts:379-391`）、`v74PageFailure`（`frame.ts:305-375`）、`v74ConsoleSevere`（`frame.ts:299-303`）；由 `framedView` 繪製（`pane.tsx:320-337`） | 確認目前分頁資料來源、schema、新鮮度與是否異常 | 無 | **內容很長**：`v74TitleSegs` 依序退化為短來源名、截斷來源名、僅頁名+年齡（`frame.ts:418-435`）；`v74BodyStatusRow` 依 `maxCols` 優先保留失敗與過時資訊（`frame.ts:481-497`）<br>**出錯讀不到／資料過時**：`v74BodyStatusRow` 顯示紅字失敗／缺源原因與黃字 `stale` 提示（`frame.ts:452-472`） |
| C.3 | **底列狀態與分頁條（非 boxed 模式）**：`tabStrip` 繪製 `━` 底線與分頁按鈕（`現況`、`待決` 等），有待處理事項時名稱以警告粗體標示 | `stripOf`（`tabs.ts:82-86`）、`tabNeeds`（`tabs.ts:74-80`）、`stripView`（`pane.tsx:136-154`） | 切換分頁或辨識哪個分頁有待處理事項 | 數字鍵 `1`–`9`、`0` 或點擊分頁按鈕切換（`pane.tsx:123, 146`） | **內容很長**：`flexWrap="wrap"` 自動換行（`pane.tsx:143`）；`consoleStrip` 依寬度逐級隱藏 `△` 與 `⊘` 數字，最窄退化為僅顯示作用中分頁與總 `⏶N ✗N`（`consoletabs.ts:72-81`） |
| C.4 | **底列狀態與分頁條（v7.4 boxed 模式）**：`╰─ <按鍵提示> ──── <分頁1>  <分頁2> ... ─╯`，當前分頁反白（`inverse`）；同時註冊隱藏數字熱鍵與 `w` 展開鍵 | `frameHintText` / `frameBottomFit`（`frame.ts:123-162`）、`stripRows`（`frame.ts:164-199`）、`framedView`（`pane.tsx:351-391`） | 查看目前可用按鍵提示、目前所在分頁，並切換分頁 | 數字鍵 `1`–`9`、`0` 切換分頁；`w` 切換展開／收合；點擊分頁名稱切換（`pane.tsx:361-389`） | **內容很長**：`frameBottomFit` 先將提示縮為 `hintShort`，再將非當前分頁縮為純數字，最後縮為 `hintMin` 或空字串（`frame.ts:147-162`）；在 `<= 3` 列（`TINY_FRAME_ROWS`）時省略提示框線列以保留內容列（`frame.ts:212`、`pane.tsx:218`） |
| C.5 | **Band（prompt 上方單列狀態條 `OneRowBand`）**：固定單列，左側顯示提醒（`‣ ...` 或閒置／缺資料），右側依序為 `3 產線`、`6 來源`、`2 待決`、`4 QC`、`5 ϟ tok/s`、sparkline、寵物與 `▾展開`／`▴收合` 切換鈕；並綁定 `1`–`9`、`0`、`w` 熱鍵 | `OneRowInput`、`layoutOneRowBand`（`mods/live/band-onerow.ts:40-476`）、`OneRowBand`（`mods/live/band.tsx:114-171`） | 一眼掌握提醒、產線異常、殘留數、待決數、QC 警示與即時速度，並點擊區段直接跳轉對應分頁 | 點擊區段跳轉：`remind`→`decide`、`line`→`dispatch`、`source`→`hygiene`、`pend`→`decide`、`qc`→`review`、`tps`→`spend`（`band-onerow.ts:133-147`）；`1`–`9`、`0` 切分頁、`w` 或點擊切換鈕展開／收合（`band.tsx:146-169`） | **空的**：無提醒且 `pend.count === 0` 顯示 `band.onerow.idle`；`spark`／`pet` 無資料時直接省略以把寬度還給提醒（`band-onerow.ts:344-346, 385`）<br>**內容很長**：依序丟棄 `pet` → `spark` → 非警示 `qc` → `source` → `tps` → `line`（有警示則改為緊湊格式 `<=14` 欄）→ `pend`，確保不換行且提醒至少保留 `REMIND_MIN_COLS`（28）或 `REMIND_ALERT_MIN_COLS`（20）（`band-onerow.ts:405-441`）<br>**出錯讀不到／資料過時**：`missing` 顯示 `—` 與 idle 色調；`stale` 加上 `~` 前綴與 `?` 後綴（`band-onerow.ts:111-128, 166-354`） |

---

## 全域按鍵表（`mods/live/pages/keys.ts`、`mods/live/pane.tsx`、`mods/live/band.tsx`）

| 按鍵 | 有效範圍 | 效果 | 來源（file:line） |
|---|---|---|---|
| 任何鍵（`!st.paneFocus`） | 全域（焦點不在 pane） | `pass` 交還 Claude Code 處理 | `mods/live/pages/keys.ts:25` |
| `Enter` / `Escape` / 字元（`st.textMode`） | `2 待決` 備註輸入模式 | `Enter` 儲存、`Escape` 取消、其餘字元輸入 | `mods/live/pages/keys.ts:26` |
| `Escape` | 全域（`paneFocus`） | 在 `decide` 若展開背景／緊湊視圖則收合；若在送出預覽（`armed`）則取消預覽；否則關閉 pane | `mods/live/pages/keys.ts:27` |
| `w` | 全域（`paneFocus` 及單列 band） | 切換展開／收合 pane（`toggle_expand`） | `mods/live/pages/keys.ts:28`、`mods/live/band.tsx:165-167`、`mods/live/pane.tsx:388` |
| `Enter` | 全域（`paneFocus`） | 若按鈕有焦點則觸發按鈕（`button`），否則切換折疊（`fold`） | `mods/live/pages/keys.ts:29` |
| `1`–`9`、`0` | 全域（非 `textMode`） | 切換至對應分頁（`1`–`9` 對應 `V74_KEY`，`0` 對應 `pet`）；已在該頁則無動作 | `mods/live/pages/keys.ts:38-40`、`mods/live/band.tsx:151-163`、`mods/live/pane.tsx:374-380` |
| `r` / `u` / `p` | 全域（`u` 在 `decide` 為復原） | 切換至 `room` / `spend` / `pet` 分頁 | `mods/live/pages/keys.ts:21, 36-37` |
| `j` / `ArrowDown`、`k` / `ArrowUp` | 全域（`decide` 的方向鍵改為捲動） | 向下／向上移動游標（`move`） | `mods/live/pages/keys.ts:34, 41-42` |
| `z` | 全域（`fleet` 除外） | 切換折疊（`fold`） | `mods/live/pages/keys.ts:43`、`mods/live/pane.tsx:162, 168` |
| `o` | 非卡片頁（`decide`、`todo` 除外；實際僅 `quota` 綁定） | 展開／收合焦點列（`open`） | `mods/live/pages/keys.ts:44`、`mods/live/pane.tsx:166-168` |
| `a` / `b` / `c` | `2 待決`、`8 待辦` | 選擇對應選項（`pick` 0–2） | `mods/live/pages/keys.ts:45-47`、`mods/live/pane.tsx:163-164, 168` |
| `m` / `s` / `y` / `u` / `x` / `h` / `l` / `f` / `n` / `p` / `v` / `e` / `PageUp` / `PageDown` | 僅 `2 待決` | 備註、送出、確認、復原、取消預覽、上／下頁、填入推薦、上／下卡、切換樣式、展開、翻頁捲動 | `mods/live/pages/keys.ts:30-35, 48-58`、`mods/live/pane.tsx:164` |

---

## 疑點

1. **`8 待辦` 資料來源尚未接上 runtime**：`mods/live/pages/todo.ts:1` 註解明確寫明 `the source is not live yet: null at runtime -> the broken row; tests build it from fixtures/v74/todo.json`，因此實際執行時進入 `8 待辦` 會固定顯示 `v74.d.todo_no_source` 錯誤列。
2. **`7 艦隊` 他機卡片 `ctx.others` 的接線狀態**：`mods/live/pages/fleet.ts:3` 註解寫 `(fleet.ts othersView, to be wired by the pane)`，而 `mods/live/pane.tsx:206` 傳入 `frame.others ?? null`，若 host 未填入 `FramePane.others`，畫面上會顯示 `v74.f.others_missing`。
3. **非 boxed 模式 `DIGIT_TAB` 的 `'0'` 映射**：`mods/live/pane.tsx:124` 中 `DIGIT_TAB` 將 `'0'` 映射到 `'now'` 而非 `'pet'`，與 `PAGE_DIGIT`（`pet: '0'`）及 `keys.ts`（`PET_KEY` → `'pet'`）不一致。
4. **`band-onerow.ts` 中 `formatSpark` 與 `formatPet` 的點擊性**：`band-onerow.ts:305-327` 將 `spark` 與 `pet` 設為 `clickable: false, page: null`，且 `bandSlotPage`（`band-onerow.ts:141-145`）亦回傳 `null`，但 `pet` 仍可透過數字鍵 `0` 或 `p` 開啟（`keys.ts:15, 21`）。
