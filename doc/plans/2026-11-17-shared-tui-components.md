# 計畫：forge-stats × autopilot-live 共用終端機元件（含寵物分頁）

> 來源：PM 工單 01M7MD5Z（操作員 11-17 17:2x「11/19? 為什麼要那麼久」），範圍 = PM 01M7M9J4 五點 + 01M7MD92 補兩點。
> 主導：codeforge lane。審查：hetero plan loop（最多兩代），autopilot lane 為審查者之一。
> 凍結交付：11-18 12:00（台北）前交 PM。寵物分頁 11-18 上線。
> 狀態：**草稿 g2**（11-17 晚）：已折入第一代審查與第二代（封頂）審查處置，見 §11。rubric（同目錄 `.rubric.md`）已凍結，不再改。

## 0. 為什麼要做

操作員在同一終端機看到兩套分頁列（forge-stats 在上、autopilot-live 在下），符號與顏色各畫各的。原因是兩個 plugin 各複製了分頁列、符號表、寬度計算和色彩。本計畫把它們變成一份，並用測試讓任一邊的偏移變紅。

## 1. 現況盤點

> 行號以 codeforge develop 94c56f1、autopilot origin/develop b28dbc5e6 為準；g1 對 autopilot develop 20958af2f 重核：`ui.ts` PALETTE 在 87-88（不是 79-86）、`pct` 在 204、`band-contract.ts:57`、`tabs.ts:86`（`BAR_PANE_TABS`）、`:92`（`barStrip`）、`:109-113`（`usageTabFits`）。

| 區塊 | forge-stats（FS） | autopilot（AP） | 處置 |
|---|---|---|---|
| 分頁列 | `register.tsx:683-705` 自繪（寵物／用量／fleet；v2 為 `pane-controls.ts`），不經 `ui.strip` | `mods/live/tabs.ts` `barStrip()` → `ui.strip`，貼底版 21:30 | 只留 AP |
| 狀態帶 | `register.tsx` band 摘要（hotfix f58809b 起讓位給 autopilot-live） | `band.tsx` O4 帶 + `ϟ` 列 | AP 在時 FS 讓位；否則後備（§3.3） |
| 用量表 | `quota-view.ts`、`side.ts`（panel/1 side） | `shell.ts` `usageRows`/`usageDraw`（讀 panel/1）＋ tux §6 規劃的 fleet.console/1 額度 | 只在 AP，走 fleet.console/1；panel/1 `usage` 淘汰期 |
| 符號表／寬度／色彩 | `model.ts`/`strings.ts` 內嵌符號；`model.ts:11-25` 寬度（手抄 AP，少 ANSI 剝除）；直接用 theme key／hex | `ui.ts` `GLYPHS` + `glyph-check.js` + lint I5；`ui.ts:18-52`；`PALETTE` 八個 theme key（lint L1/L8） | 各單一來源（§2）；golden 鎖 emoji、CJK、Ambiguous；FS 只經 PALETTE |
| 寵物 sprite + 能力值 | `register.tsx` + `pet.json`（daemon 產）；**FS 獨有** | 無 | 資料留 FS，畫面進 AP 寵物分頁（§6） |
| fleet 摘要 | `fleet-view.ts`（fleet.brief/1） | `now` 分頁（autopilot.fleet 狀態） | 併進 `now`，AP 讀 fleet.brief/1（須 AP 同意） |
| 寵物分頁圖示 | `hooks/pet-icon.ts`（a70f8fd，`pet.icon`） | patch：`setPetTab`、`petRows` | FS 決定、AP 畫（§6） |

獨有、不共用：寵物 sprite（FS `model.ts:119-198`、`register.tsx:624-711` Raster）、記憶燈（`parseSessionView`）、fleet.brief/1 解析（v2 `brief-view.ts`）；AP 的決策卡熱區、捲軸（`scroll.tsx`、`scrollmath.ts`）、lint 與 invariants。**`cutProse`** 不列共用項（FS 沒用它；FS 裁切用 `side.ts:61` `fitCells`、`pane-view.ts:52` `clipCells`，行為不同；P5 若換須附等價證據），只隨 `strip()` 當內部相依。

### 1.1 發現與落點

| # | 發現 | 證據 | 落點 |
|---|---|---|---|
| F1 | 寬度函式手抄，只差 ANSI 剝除 | FS `model.ts:8-25`、AP `ui.ts:18-52` | P4/P5 第一個抽出 |
| F2 | 百分比取整三種：FS pane 四捨五入、FS panel 捨去、AP 捨去；AP 內還有兩份（字串 `pct`、數字 `floorPercent`） | FS `quota-view.ts:166`、`side.ts:44`、AP `ui.ts:204`（回 `"29%"`）、`band-contract.ts:57`（回 `29`） | tux `format.ts` 定 `pctInt(ratio) = Math.floor(ratio*100 + 1e-9)`（epsilon 照抄 AP，防 0.29*100 成 28）與建立其上的 `pct()`。`floorPercent` 改呼叫 `pctInt`（保留 `round === 'floor'` 契約檢查，不自己算）。`golden/pct-cases.json` 分記 `pctInt(0.29)=29`、`pct(0.29)="29%"`，另含 0.896 → 89 / `"89%"`。FS 兩處 P5 改呼叫 vendor |
| F3 | 警告門檻兩套：FS `tight` 0.9、band 契約 80/100 | FS `quota.ts:20`、AP `band-statusline-contract.json:2-4` | `tokens.json` 加 `thresholds`（`warn: 80`、`critical: 100`）；P4a 加相等測試（與 band-statusline-contract.json 同值）。`hooks/quota.ts` 比較 `pctInt(ratio) >= THRESHOLDS.warn`（`THRESHOLDS` 來自 `palette.ts`，§2.1）。FS `tight: 0.9` P5 換讀 vendor thresholds；行為變更（黃色由 90% 提前到 80%）列入 P5 驗收 |
| F4 | FS 直接寫 theme key 與符號字面，AP 只准語意角色（lint L8、L2） | FS `model.ts:337`、`quota-view.ts:190`、v2 `overview-view.ts:9`；AP `ui-lint.test.js:179-233, 326` | P5 先改 FS 用 `PALETTE`/`GLYPHS`，再開 lint |
| F5 | `●` 在 FS 是「最新」，在 AP 是「執行中」 | FS `model.ts:35`、AP `ui.ts:67` | P5：`model.ts:35` 改用 vendor `GLYPHS` 已有語意的符號（清單由 P5 PR 列出，不得再用 `●`），加 `glyphs.json` 一符一義斷言 |
| F6 | `PanelTone` 沒有 warning/error | FS `types/index.d.ts:70`、AP `shell.ts:150` | **不做（DROP）**：額度離開 panel/1、改走 fleet.console/1（§6），警示色由 fleet.console/1 與 AP `PALETTE` 負責；寵物資料無警告語意。panel/1 維持現有 tone 集合，`contracts/panel.ts` 照現狀寫，不加欄位 |
| F7 | panel/1 schema 字串與 tone 表兩邊手抄 | FS `panel.ts:6`、AP `shell.ts:53` | `mods/tux/contracts/panel.ts`（型別、schema 字串、tone、tiers）一份，P4b 抽出；`golden/panel-cases.json` 兩側都跑（§4） |
| F8 | 寬度級距兩套：FS 40/60/80/100/120、30/40/50；AP 80/120/160/200/240；tux §6.6 用 200/160/120/100/80 | FS `panel.ts:7`、`side.ts:9-11`；AP `shell.ts:11-12` | `mods/tux/tiers.ts` 兩張**分開**的表：`PANE_TIERS`（整窗，對 tux §6.6）與 `SIDE_TIERS`（panel/1 側欄：panel 40/60/80/100/120、side 30/40/50，沿用 FS 現值）。P4b 由 AP 對照 `shell.ts:11-12` 與 tux §6.6 選一套當 `PANE_TIERS` 並在 PR 說明（本計畫不決定數值）；P5 FS `panel.ts`、`side.ts` 改 import；`contracts/panel.ts` 的 tiers 鍵引用 `SIDE_TIERS` |
| F9 | 時長格式兩種：`2h10m` 與 `8h05m` | FS `model.ts:576`、AP `ui.ts:207` | `format.ts` 的 `dur()`（以 AP `ui.ts:207` 為準，分鐘補零）；`golden/dur-cases.json`；FS `model.ts:576` P5 改用 vendor |
| F10 | `bandLayout`、`bandTier`、`BAND_MIN_COLUMNS` 只剩測試呼叫；`summaryParts` 的 fleet 分支（`register.tsx:790`）永遠收 `null` | grep：只有 `tests/band.test.tsx` | P5 刪除（連同 `tests/band.test.tsx` 對應案例），不共用 |
| F11 | v2 分支早於 `pet-icon.ts`，`panel.ts` 會丟掉 `pet.icon`；v2 字串寫死中文 | v2 `panel.ts`、`pane-controls.ts:13,35` | P6 以 develop（含 a70f8fd）為底重做 v2，不直接合 |
| F12 | 測試跑在兩個 runner：FS `claude plugin test`、AP `run-tests.sh` | `scripts/check-all.sh:84`、AP `run-tests.sh:19-45` | §4：golden 為兩 runner 都能讀的純資料，各一支測試 |

## 2. 共用元件放哪、怎麼發版、怎麼釘版本（點 1）

**決定：autopilot repo 是唯一來源；codeforge 釘版本複製（vendor）。**

### 2.1 共用項目與唯一路徑

| 共用項目 | 唯一路徑（autopilot） | 備註 |
|---|---|---|
| 符號表 `GLYPHS`（含 `verified` 註記、`pet.icons`） | `mods/tux/glyphs.ts` | **只在這裡**。`tokens.json` 不含符號；`golden/glyphs.json` 由 `GLYPHS` 衍生，非第二份來源 |
| 色彩角色 `PALETTE` | `mods/tux/palette.ts` | 由 `roles` 產生（hooks module 不能 import JSON，沿用 `gen-band-contract.js --check`，加 `gen-tux.js --check`）；golden 比對 `PALETTE` = `roles`。`gen-tux.js --check` 也產 `THRESHOLDS`（取自 `tokens.json.thresholds`）到 `palette.ts`，列入 `VERSION.manifest`。P4a→P4b 間 `ui.ts:87-88` 仍在；P4a 只測子集，嚴格相等 P4b |
| 寬度 `cellWidth`/`displayWidth` | `mods/tux/width.ts` | |
| 分頁列 `strip()` | `mods/tux/strip.ts` | `cutProse` 是內部相依（`ui.ts:567,572`），隨它搬，不當獨立共用項 |
| 取整、時長 `pctInt`/`pct`/`dur` | `mods/tux/format.ts` | F2、F9 |
| 級距 `PANE_TIERS`/`SIDE_TIERS` | `mods/tux/tiers.ts` | F8 |
| panel/1 契約 | `mods/tux/contracts/panel.ts` | F7 |
| 語意角色、用語、門檻 | `mods/tux/tokens.json` | `roles`、`terms`、`thresholds`（§5） |
| 版本與清單 | `mods/tux/VERSION`、`VERSION.manifest` | §2.2 |
| golden | `mods/tux/golden/` | §4 |

不屬於 tux：分頁**順序**（`BAR_PANE_TABS`，`tabs.ts:86`）與 `barStrip`（`tabs.ts:92`）留在 live，由 AP golden 直接呼叫。

- 不用第三個 repo（多一個發版點；AP 已有 tux 準則、lint、glyph-check）；不用 codeforge 當來源（畫面主體在 AP-live，§3）。
- AP 接受當擁有者（autopilot 01M7ME34，develop 33ca95225），條件：抽出後 `ui.ts` 改從 `mods/tux/` import，不留第二份。ui-lint 的 `modSources()`（`ui-lint.test.js:110-123`）本來就掃 `mods/` 全部目錄，真正的工作是把 `except` 清單（`:214,:243,:327,:329,:367` 的 `'live/ui.ts'`）與 L3 對 `ui.ts` 的 import（`:13`）改指 tux 檔，歸 P4b。

### 2.2 發版與 VERSION 鎖

- AP 每次改 `mods/tux/` 就加一 `mods/tux/VERSION`（整數），同 commit 更新 `golden/` 與 `VERSION.manifest`。
- `VERSION.manifest` = `{version, files: {path: sha256}}`，涵蓋 `mods/tux/` 下除 `VERSION`、`VERSION.manifest` 外每個檔（含 `format.ts`、`tiers.ts`）；由 `scripts/tux-manifest.js` 重算並自動加版號。
- `mods/tux/golden.test.js` 鎖：① 重算 sha 清單 = manifest；② manifest.version = `VERSION`；③ 與合併基點（`origin/develop`）的 manifest 比，檔案 sha 不同而 version 相同 → 紅。
- **負向控制**：改 `format.ts` 一個位元組不重算 → 紅；重算 manifest 但不加版號 → 紅。

### 2.3 釘版本（codeforge）

- `plugins/forge-stats/vendor/tux/` + `vendor/tux/VENDOR.lock`：`{autopilot_sha, tux_version, files: {path: sha256}}`。
  - `scripts/tux-sync.sh <autopilot-sha>`：從 AP（本機 clone 或 GitHub）取該 sha 的 `mods/tux/`，寫檔、寫 lock，冪等。
  - check-all 加 `tux-lock`：vendor 檔 sha256 必須等於 lock，**且 `vendor/tux/` 下沒有 lock 未列的多餘檔**；手改或塞檔皆紅。
- 節奏：AP 發新 `tux_version` 後，codeforge 下一個工作日內 sync（一個 PR，進 develop）。sync 不必同步上線；**live tag 的版本相等門檻見 §7**。

## 3. 分頁與熱區位置；併入還是獨立 pane（點 2、點 3）

### 3.1 位置（點 2）

以 AP 11-17 21:30 貼底版為基準：分頁列與要動手的東西貼底，內容往上長（tux v1.4 §1、§5）。FS 不再有任何頂部分頁列。

**11-18 上線時 FS 只剩 band 版面**（pane v2 暫緩，PM 01M7M9J8）。v1 pane 分頁列（`register.tsx:683-705`）只在 pane 版面畫，而 `session.start` 讀 `$.store` 的 `layout`（`register.tsx:552-553`，存成 `pane` 就開 pane）。故 P1b（codeforge，11-18，P3 之前）在 session.start 把已存的 `layout: pane` 遷成 `band`，並讓 `layout pane` 指令（`:580-588`）不再接受 `pane`；11-18 的 tag 無任何路徑在頂部畫分頁列。v1 pane 繪製死碼 P6 刪。

### 3.2 建議：併入 autopilot-live 當分頁（點 3）

A 一窗一條分頁列、符合 tux §6「不開第二窗」、宿主捲動／焦點修補（§5.5、§5.6）只做一次；B 兩窗兩條分頁列、要改 tux 準則、80 欄時都退讓到不可讀。A 讓 v2 pane 畫面程式（`pane-view.ts`、`fleet-view.ts`、`quota-view.ts` 的繪製部分）退場，v2 資料整理改輸出到 panel/1 或交給 AP；AP 多一個寵物分頁（patch 約 170 行含測試）。

B 唯一的優點是 FS 可單獨被裝，由 §3.3 後備涵蓋。

### 3.3 沒裝 autopilot 的使用者

FS 保留**單行**狀態帶後備（hotfix f58809b 的形狀：autopilot-live 存在就讓位），用 vendor 的符號與色彩。**這一列貼底，是輸入框上方最後一列**，不在頂部、不是分頁列。不保留獨立 pane。偵測沿用 hotfix（`autopilot.fleet` 心跳）。

### 3.4 01M7KRP3（pane v2）怎麼辦

v2 分支保留不刪。其內容（overview-first、fleet.brief/1）在 A 之下變成 autopilot-live 分頁內容；UX 閘門改對該些分頁重跑（carbon-high + astra 皆 ≥8）。改範圍須 PM 點頭（§9）。

### 3.5 現有 `forge` 分頁與 `pet` 分頁

AP 現有 `forge` 分頁（`tabs.ts:84,95`；`band.tsx:108`：開 forge-stats 完整檢視的短分頁，內容速度與近期請求表）。**決定：共存。** `forge` 維持現狀（速度、近期請求、記憶）；`pet` 是新窗格分頁（加進 `BAR_PANE_TABS`，在 `usage` 之後、`forge` 之前），只畫 sprite 與能力值。容納規則見 §6 `petTabFits`。

## 4. 共用 lint 與 golden（點 4）

目標：任一邊改了分頁位置、符號、寬度規則或 panel 契約，兩邊測試都紅。

### 4.1 golden（AP `mods/tux/golden/`，純文字／JSON）

- `tabrow-{80,120,160}.txt`、`tabrow-console-{80,120}.txt`：**用 live 的 `barStrip`**（`tabs.ts:92`，順序來自 `BAR_PANE_TABS` `tabs.ts:86`；console 模式走 `consoletabs.ts`）在固定 snapshot 畫出，不用 fixture 專用 `strip()`。含寵物圖示與後備兩種（`tabrow-80` 同時鎖 `petTabFits` 與欄寬 < 80 後備）。改順序 → 紅。**`tabrow-*` 只在 AP 側**（vendor 沒有 `barStrip`），不在 codeforge 的 golden 測試清單；codeforge 靠 `tux-cross`（§4.3）執行 AP 的測試而紅。
- `layout-bottom-{80,120}.txt`：整窗畫面，斷言**最後一列是分頁列／band**。移到頂部 → 紅。
- `glyphs.json`（由 `GLYPHS` 衍生）、`width-cases.json`（emoji、CJK、East Asian Ambiguous、ZWJ、VS16）、`pct-cases.json`、`dur-cases.json`。
- `panel-cases.json`：正常、含 `pet.icon`、未知 tone、缺欄位、`usage`/`side` 內含毒值。**寫入端與讀取端都跑**：codeforge `tests/panel-contract.test.ts` 要求 FS 產出等於案例；AP `mods/live/panel-contract.test.js` 要求 `panel.ts` 讀案例得預期結果。tone 名、schema 字串、tiers 鍵任一邊改 → 兩邊紅。負向控制：改掉 tone 名 → 兩邊皆紅。

### 4.2 每種變更各側哪個測試紅

每種變更各側哪個測試紅（另一側靠 `tux-cross`，§4.3）：

- 分頁順序／位置：AP `mods/tux/golden.test.js`（`tabrow-*`、`layout-bottom-*`）；codeforge 靠 `tux-cross` 執行 AP 的 `tabrow-*`；另加 `tests/no-top-tabrow.test.tsx`（band 版面與已存 `layout: pane` 都不畫頂部分頁列；還原 pane → 紅）。
- 符號：AP 同上（`glyphs.json`）、ui-lint L3/I5/I20、`petIconViolations`；codeforge `tests/tux-golden.test.tsx`（vendor `GLYPHS` 對 `glyphs.json`）。
- 寬度規則（含 `pct`、`dur`）：AP 同上（`width-cases`、`pct-cases`、`dur-cases`）；codeforge 同上，FS 自己的 `displayWidth` P5 已刪。
- panel 契約：AP `mods/live/panel-contract.test.js`（讀取端）；codeforge `tests/panel-contract.test.ts`（寫入端）。

### 4.3 兩邊都紅的機制（`tux-cross`）

- **runner**：AP 的 live mod runner 是 `mods/live/run-tests.sh`（純 JS 走 `node --test`，`*.test.ts` 走 `claude plugin test`，由 `lib/real-claude-env.sh` 的 `mods_run_plugin_test` 呼叫）。
  - AP 側：兩支測試（`mods/tux/golden.test.js`、`mods/live/panel-contract.test.js`）加進該 runner；用 `mods_run_plugin_test` 對 `$CODEFORGE_DIR/plugins/forge-stats` 跑 codeforge 的 tsx 測試（同 `scripts/check-all.sh:84`）。
  - codeforge 側：check-all 的 `tux-cross` 步驟用 `claude plugin test`；`tests/tux-golden.test.tsx` 讀 `TUX_GOLDEN_DIR`（預設 vendor 對應的 pinned golden；`tux-cross` 設 `$AUTOPILOT_DIR/mods/tux/golden`，預設 `../autopilot`）。
- **`tux-cross` 內容（P5 起）**：在 `$AUTOPILOT_DIR` 跑 `autopilot node --test`（`mods/tux/golden.test.js`、`mods/live/panel-contract.test.js`），以環境變數停用遞迴（AP 側不再回頭跑 codeforge）；並核對 `VENDOR.lock` 的 `tux_version` 與各檔 sha256 等於 AP `mods/tux/`。示範：改 AP 分頁順序、不重生 fixture → codeforge 紅。
- **`pet-cross`（P3 專用，P5 後由 `tux-cross` 取代）**：此時還沒有 vendor。對 sibling 跑 AP `panel-contract` 與 pettab 測試，加 codeforge panel golden。擁有者 P2（AP 側）/P1b（codeforge 側）。sibling 不在 = 紅。hangar live pin 與 `release-local.sh` 都跑它。
- **判定規則**：
  - codeforge 側**硬紅**：vendor（或 lock 釘的 golden）與 AP HEAD 的 golden 不一致就紅，含 AP 已升版而 codeforge 未 sync。後果是 AP 升版後 codeforge check-all 紅，直到一個工作日內的 sync PR 合入；刻意，逼 §2.3 節奏。
  - AP 側**只在「`tux_version` 相同但輸出不同」時紅**；AP 版號較新時只印 `notice: codeforge pins tux N, autopilot is M`，免得 AP 升版 commit 自己打架。
- **兄弟 repo 不在**：開發時印 `skip: sibling absent`，check-all 總結列記為 SKIP、不算過。**release 一律硬失敗**（hangar live pin 前的 gate 與 `scripts/release-local.sh` 都必跑 `tux-cross`（P3 為 `pet-cross`）；兄弟不在 = 失敗）。§7 引用此處。
- 防：AP 改分頁順序並更新 golden、codeforge 仍釘舊版而各自綠，上線後兩種順序。
- 另有 lint：FS 原始碼的非 ASCII 符號只准在 vendor `GLYPHS` 或 `pet-icon.ts` 暫時表（P5 後只剩 vendor）。

## 5. 對齊 fleet 主控台網頁設計系統（點 5，01M7M7Y7）

- `mods/tux/tokens.json`（AP 擁有，唯一檔案）：
  - `roles`：語意名 → 終端機 theme key（沿用 `ui.ts:87-88` 八語意：attention→warning、here→claude、broken→error、done→success、running→suggestion、idle→inactive、structure→subtle、action→merged；加 fleet 審查補的角色）；
  - `terms`：用語表（zh-TW / en）：`待決`、`派工`、`審查`、`用量`、`寵物`、`陳舊`、`過期`、`無來源`；
  - `thresholds`：`warn: 80`、`critical: 100`（F3）。沒有 `glyphs`（§2.1）。
- 網頁用同一份 `roles`/`terms`；hex 由網頁自定，**語意名必須一一對到**。
- **機械檢查：**
  1. fleet repo 複製 `tokens.json`，測試釘 sha256（`path@sha`；升版 = 換檔、換 sha、重跑 gate）；
  2. fleet cargo 測試：`roles` 角色名與網頁色表（`console_brief.rs` 一帶）**一對一**（每個角色有 hex、網頁色表沒有 tokens.json 以外的角色）；`terms` 的 zh-TW/en 鍵齊全；
  3. AP 側：P4a 只測現有八個 `PALETTE` 是 `roles` 的子集、`thresholds` 等於 band 契約；嚴格 `PALETTE` = `roles` 在 P4b（§2.1）；
  3b. `roles` 每個值必須是 CC 八個主題鍵之一（測試斷言）；
  4. codeforge `tux-lock` 對照 fleet 釘的 sha，不同時印差異（不擋，那是 fleet 的 repo）。
- **機械檢查不到（明寫）：** hex 值本身與終端機／網頁色看起來是否一致；網頁 CSS（版面、字級、間距）；未入 `terms` 的自由文字；兩 repo 升版間的非同步落差（只靠 sha 釘住與通知）。
- **locale**（P4b，擁有者 autopilot）：AP `locales.ts` 是產生檔（`gen-locales.js --check` 已在 `mods/live/run-tests.sh`）。共用用語的 key 改由 `tokens.json.terms` 產生，擴充 `gen-locales.js`／`locales.ts`，`--check` 不符就紅。P4a 不碰。
- fleet 回覆（01M7ME32）：01M7M7Y7 未定版（設計系統計畫 11-19 12:00 交）；過渡主控台只有色名（fleet develop e8c67ba2 `console_brief.rs`），沒有用語表。fleet 決定**以 `tokens.json` 的 roles/terms 為唯一來源**，網頁只定 hex。缺的角色由 fleet 提供（01M7MHMP），P4a 照此寫入：
  - **新增五個角色**：`stale`→inactive、`overdue`→error、`waiting_on`→subtle、`needs_human`→merged、`link`→suggestion。
  - **fleet 用名對到既有角色，不另開名字**：ok=`done`、warn=`attention`、fail=`broken`、text_muted=`structure`；`running`、`idle` 同名。網頁一律用 tokens 名。
  - 同鍵角色（overdue/broken 皆 error，stale/idle 皆 inactive，needs_human/action 皆 merged）靠文字標籤＋圖形分開，不靠顏色（逾期用 ◈，網頁側）。
  - `offline`（fleet cb5bef5e §2.6，01M7MK4H）：色彩用既有 `broken`，`terms` 加 `offline`（離線），P4a 寫入。符號放 `GLYPHS.state.offline`，不放 tokens.json；⊘ 已是 `stalled`（`ui.ts:67`），不可重用，由 autopilot 另選過 lint 的符號並通知 fleet（P2 或 P4b）。
- 定版時由 AP 通知 fleet `tokens.json@<sha>`。第一版須在 11-19 12:00 前存在，故 P4 拆成 P4a（11-18）與 P4b（11-19）。

## 6. 寵物分頁（點 6）

- **畫面**：AP-live 新分頁 `pet`（§3.5），內容：sprite（半格 Raster，依 `doc/specs/codeforge-mod-ui.md`，不用 `Image`）＋能力值列。
  - **內容貼底**：`justifyContent="flex-end"`（tux-guidelines §1：內容緊貼分頁列，不留空列）。
  - **`petTabFits(columns, t, forgeTab)`**：仿 `usageTabFits`（`tabs.ts:109-113`），剩餘欄數扣掉 pet 分頁寬度後仍須 ≥ `USAGE_LABEL_ROOM`（速度標籤最小 11 格），否則不畫 pet 分頁、用 dim `›` 收尾；pet 分頁不得擠掉 `forge` 分頁或速度標籤。`tabrow-80.txt` 鎖此邊界（兩側各一案）。
- **資料**：FS 寫 `codeforge.panel/1` 的 `pet`：`{title, sprite, tiers, icon}`。`icon = {id, emoji, fallback, text}`，`text` 是 FS 依環境決定要畫的那一個（已實作，a70f8fd，7 個測試）。
- **圖示**：一個寵物 emoji，固定 2 格。候選三個（🐱 cat、🐣 chick、🦀 crab），由操作員從截圖頁挑；預設 cat。
  - **後備**（任一成立就畫兩個 ASCII 字 `:3`、`o<`、`}{`）：`TERM` 是 linux/dumb/vt*/ansi/cons*、locale 非 UTF-8、`CODEFORGE_PET_ICON=plain`（以上由 FS 在 `pet.icon.text` 決定，已有測試），**或實際欄寬 < 80**（FS 看不到欄數，由 AP 渲染端決定）。< 80 的標籤**從白名單條目推出（該條目的 fallback），不讀 payload**。欄寬案例進 `tabrow-80.txt`（79 與 80 各一案）。
  - **reader 驗證**：`readPet()` 以整個 icon tuple `{id, emoji, fallback, text}` 比對**單一** `GLYPHS.pet.icons` 條目，四欄都要符合，否則不開分頁。兩個寬度級（>= 80、< 80）都檢查**實際繪出的標籤** `displayWidth === 2`。**負向控制**：`⚠️`（2 格但不在清單）或 CJK 兩字 → 不開；`text` 合法但 `fallback` 不合法，在 79 欄 → 不開。
- **glyph lint 白名單**：emoji 在 lint I5 / `EMOJI_BANNED` 禁止；例外**只給 `GLYPHS.pet.icons` 這一個 key**，`tableGlyphs()`（`ui.ts:149`）不納入它，其他 key 檢查不變。
  - `verified`：挑定前寫 `pending-owner`，挑定後寫 `2026-11-18 <terminal>`；符合 `verifiedOk`（`ui.ts:147`），不用 `owner-11-18` 這種格式。
  - `pet.icons` 有**獨立斷言** `petIconViolations`（不併入要求寬度 1 的 `glyphTableViolations`）：每個 emoji 寬 2、單一 Emoji_Presentation 碼位、無 U+FE0F；每個 fallback 兩個可印 ASCII、非 East Asian Ambiguous。沿用 `ui-lint.test.js` M7 的 planted 表測紅（植入 `⚠️`、帶 FE0F 的 emoji、寬 1 的字 → 各自紅）。
  - 過渡期（P2→P5）：AP `GLYPHS.pet.icons` 是**接收白名單**（權威）；codeforge `hooks/pet-icon.ts` 的表是**發送端**暫時表；P5 把 `pet-icon.ts` 改讀 vendor 的 `GLYPHS.pet.icons` 並刪自己的表。
- **與 tux §6.2 / L19 的衝突**：L19（`tux-guidelines.md:404`）禁止額度繪製模組出現 `codeforge.panel`，而 `shell.ts:46-53` 的 `parsePanel`/`PANEL_SCHEMA = 'codeforge.panel/1'` 就在額度繪製檔裡，L19 接上 lint 時 `shell.ts` 先紅，故 **P2 就做**，不等 fleet.console/1：
  - panel 讀取搬出 `shell.ts`，放新檔 `mods/live/panel.ts`（唯一出現 `codeforge.panel` 字樣的讀取檔），分兩個出口：`readPet()`（回傳型別只有 `pet`；**永久例外，只限 `pettab.ts`**，有反向 lint）與 `readLegacyUsage()`（**過渡白名單，只限 `shell.ts` 的 `usageRows`，P6b 刪**）。檔內不再有 `codeforge.panel`。
  - tux-guidelines L19 補**檔名清單**：繪製模組 = `shell.ts`、`band.tsx`、`consolepages.ts`（P2 以實際檔為準）；例外 = `panel.ts`（只讀取）、`pettab.ts`（只准用 `readPet()` 的 `pet` 欄位）。L19 本文不放寬。**L19 同時擋兩種**：字面 `codeforge.panel`，以及對 `mods/live/panel.ts` 的 import／re-export（繪製模組內）。
  - **反向檢查在 P2 進 lint**：`pettab.ts` 不得出現 `pace.v1`、`quota`、`usage`、`.side`；負向控制：在 `pettab.ts` 植入 `panel.usage` → 紅；另給 `readPet()` 含 `usage`/`side` 毒值的 panel，斷言輸出不受影響。
  - `usageRows` 改讀 fleet.console/1 歸 P6b（同時刪 `readLegacyUsage()` 與其白名單）；P2 只搬讀取。

## 7. 操作員 clone 回到發版節奏（點 7）

- 現況：操作員 clone detached 在 47a2cf2（hotfix），forge-stats 直接從它的 `plugins/forge-stats` 載入。
- 規則：操作員 clone 只停在 **live tag**，不跟 develop tip。codeforge tag 形狀 `live-YYYY-MM-DD[-n]`；AP 形狀 `live-YYYYMMDD-N`（如 `live-20261117-1`），各用各的。
  - 每次上線：PM go → 打 tag → hangar live pin 腳本在各主機 `git fetch --tags && git checkout <tag>`，記 sha16 與舊狀態。腳本與紀錄檔路徑**由 hangar 提供**。
- **11-18 上線（P3）條件**：兩個 live tag——codeforge `live-2026-11-18`（內容 ≥ develop 1c630a4 的 pet.icon，加 P1b）與 AP `live-20261118-1`（內容含 P2；當天重打則 `-2`）——加兩個 sha16 紀錄。**P3 不要求 `tux_version` 相等**（`VERSION` 到 P4b 才有）。
- **自 P3（11-18）起**，hangar live pin 腳本拒絕非 tag 的 ref。
- **AP live tag 的 `mods/tux/VERSION` > 1** 要等 codeforge P5 帶相符 `VENDOR.lock` 才可上。
- **`tux_version` 相等門檻**：自**第一個 P5 之後的上線**起算——兩個 tag 內的 `mods/tux/VERSION` 必須等於 `VENDOR.lock.tux_version`，由 hangar live pin 腳本檢查並拒絕非 tag 的 ref；`tux-cross` release 時不可略過（§4.3）。
- **回滾**：
  - 第一次（detached 47a2cf2，無舊 tag）：`git -C <操作員 clone> checkout 47a2cf2`，並還原前一個 binary（遷移前備份成 `.bak.<epoch>`；hangar 記 .bak 位置與 sha16）。
  - 之後每次：`git checkout <舊 tag>` 加前一個 binary 的 `.bak`。
  - AP 側：`git -C <AP clone> checkout <AP 前一個 live tag>`（如 `live-20261117-1`），與 codeforge 的 `47a2cf2`（或舊 tag）並列記錄。

## 8. 實作分工與時程（點 3 的「並行」）

並行前提是寫入檔案不重疊：P1b 只寫 `register.tsx` 一帶；P2 只寫 AP `mods/live/*`；P4a 只寫 `tokens.json` 與獨立的 `tokens.test.js`。P4b 會動 P2 動過的檔（`ui.ts`、`ui-lint.test.js`、`tabs.ts`），必須排在 P2 合入後。

| 序 | 項目 | 誰 | 依賴 | 時程 |
|---|---|---|---|---|
| P1 | `pet.icon` 合入 develop | codeforge | 無 | **已完成**：develop 1c630a4（11-17，check-all 綠） |
| P1b | FS 只剩 band 版面：session.start 把 `layout: pane` 遷成 `band`、`layout pane` 指令不再接受 `pane`（`register.tsx:552-556,580-588`）；`tests/no-top-tabrow.test.tsx`（§4.2） | codeforge | 無 | 11-18，P3 前 |
| P2 | 寵物分頁正式 PR（§6 全部）：`pettab.ts`、`panel.ts`、`GLYPHS.pet.icons`＋`petIconViolations`＋planted 測試、reader tuple 比對與負向控制（§6）、`readPet()`/`readLegacyUsage()` 分離、`petTabFits`＋`tabrow-80`（golden 先放 `mods/live/golden/`，P4b 搬進 `mods/tux/golden/`）、L19 檔名清單（含 panel.ts import/re-export）與反向檢查、欄寬 < 80 後備、`pet-cross`（AP 側） | autopilot | 21:30 貼底版 | 11-18 |
| P3 | 11-18 上線：codeforge `live-2026-11-18` + autopilot `live-20261118-1`，兩個 sha16 紀錄（不要求 `tux_version`，§7）；`pet-cross` 綠；hangar live pin 拒絕非 tag ref | PM go，hangar 執行 | P1、P1b（含 `pet-cross` codeforge 側）、P2 | 11-18 晚 |
| P4a | 只寫 `mods/tux/tokens.json`（`roles` 八語意 + fleet 補的角色；`terms`；`thresholds` 80/100）與獨立 `mods/tux/tokens.test.js`（現有八個 `PALETTE` 是 `roles` 子集、`thresholds` 等於 band 契約、`roles` 值皆為 CC 八主題鍵）；**不接 `run-tests.sh`**；通知 fleet path@sha | autopilot | 計畫凍結、fleet 補的角色 | 11-18 |
| P4b | 抽出 §2.1 全部項目（`glyphs.ts` 帶 `pet.icons` 與 pettab 例外；`floorPercent` 改呼叫 `pctInt`）+ `VERSION`/`VERSION.manifest` + golden；`ui.ts` 改 import；嚴格 `PALETTE` = `roles`、`tokens.test.js` 接進 `run-tests.sh`、`gen-locales.js`/`locales.ts`（§5）；`gen-tux.js --check` 含 `THRESHOLDS`；ui-lint 的 `except`（`:214,:243,:327,:329,:367`）與 L3 import（`:13`）改指 tux 檔 | autopilot | **P2 已合**、P4a | 11-19 |
| P5 | `tux-sync.sh`、`VENDOR.lock`、`tux-lock`/`tux-cross` 進 check-all；FS 改用 vendor 並刪自己的：F1、F2、F3、F4（改 `PALETTE`/`GLYPHS` 後才開 lint）、F5、F8、F9、F10（行號見 §1.1）；`pet-icon.ts` 改讀 `GLYPHS.pet.icons` 並刪暫時表；`tux-cross` 取代 `pet-cross`；`tests/panel-contract.test.ts`、`tests/tux-golden.test.tsx`（不含 `tabrow-*`） | codeforge | P4b | 11-19～20 |
| P6a | codeforge：刪 v1 pane 繪製死碼；v2 內容改輸出到 panel/1 與 fleet.brief/1 的資料；UX 閘門重跑所需截圖 | codeforge | P4b、P5、PM 對 §3.4 點頭 | 11-20 起 |
| P6b | autopilot：把 v2 內容畫成 autopilot-live 分頁；`usageRows` 的 panel/1 讀取（P2 已搬到 `panel.ts`）遷到 fleet.console/1（§6），刪 `readLegacyUsage()` 與過渡白名單 | autopilot | P6a 的資料契約、P4b | 11-20 起 |
| P7 | fleet 網頁釘 tokens.json（sha256 測試）與角色／用語一對一 cargo 測試 | fleet | P4a | 11-19 |

P1–P3 不依賴 P4a 以後：寵物分頁先用 AP 現有 `GLYPHS`/`cellWidth`。P6 拆成各只有一個擁有者的兩列。

## 9. 待決（交 PM）

1. §3.2 選 A（併入），並把 01M7KRP3 的範圍改成「v2 內容進 autopilot-live 分頁」。建議：A。
2. ~~fleet 補充角色~~ 已結（01M7MHMP，§5）。
3. 寵物圖示三選一（操作員從截圖頁挑）。建議：cat（:3 後備最好認）。

## 10. 不做的事

- 不改 tux v1.4 §6「不開第二窗」。
- 不在 codeforge 擁有任何共用元件的原始版本。
- 不做 MUD 後段畫面、不新增配額資料來源（G40 not-goals）。

## 11. 審查處置

- 第一代：`2026-11-17-shared-tui-components.dispositions-g1.md`（全部採納）。
- 第二代（封頂）：`2026-11-17-shared-tui-components.dispositions-g2.md`（全部採納）。
