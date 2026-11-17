# 計畫：forge-stats × autopilot-live 共用終端機元件（含寵物分頁）

> 來源：PM 工單 01M7MD5Z（操作員 11-17 17:2x「11/19? 為什麼要那麼久」），範圍 = PM 01M7M9J4 五點 + 01M7MD92 補兩點。
> 主導：codeforge lane。審查：hetero plan loop（最多兩代），autopilot lane 為其中一位審查者。
> 凍結交付：11-18 12:00（台北）前交 PM。寵物分頁 11-18 上線。
> 狀態：**草稿 g1**（11-17 晚）：已折入第一代審查（astra、carbon、autopilot），處置見 §11。凍結規則：rubric（同目錄 `.rubric.md`）在第一代審查前凍結，不再改。

## 0. 為什麼要做（一段話）

操作員看到同一個終端機裡兩套分頁列：forge-stats 的在上面、autopilot-live 的在下面，符號和顏色也各畫各的。原因不是誰畫錯，而是兩個 plugin 各自複製了一份分頁列、符號表、寬度計算和色彩。只要還是兩份，對齊就只能靠人眼。這份計畫把它們變成一份，並用測試讓任何一邊的偏移變紅。

## 1. 現況盤點（重複與獨有）

> 欄位：區塊 | forge-stats 位置 | autopilot 位置 | 重複／獨有 | 處置。行號以 codeforge develop 94c56f1、autopilot origin/develop b28dbc5e6 為準；g1 對 autopilot develop 20958af2f 重核過下列行號：`ui.ts` PALETTE 在 87-88（不是 79-86）、`pct` 在 204、`band-contract.ts:57`、`tabs.ts:86`（`BAR_PANE_TABS`）、`:92`（`barStrip`）、`:109-113`（`usageTabFits`）。


| 區塊 | forge-stats | autopilot | 判定 | 處置 |
|---|---|---|---|---|
| 分頁列 | `register.tsx:683-705` 自繪 pane 分頁（寵物／用量／fleet；v2 為 `pane-controls.ts` 總覽／寵物），不經 `ui.strip` | `mods/live/tabs.ts` `barStrip()` → `ui.strip`，貼底版 21:30 | 重複 | 只留 autopilot 的；forge-stats 不再畫分頁列 |
| 狀態帶一列 | `register.tsx` band 摘要列（寵物 + 額度 chip；hotfix f58809b 起讓位給 autopilot-live） | `band.tsx` O4 帶 + `ϟ` 列 | 部分重複 | autopilot 有載入時 forge-stats 不畫；沒載入時 forge-stats 單機後備（§3.3） |
| 用量表 | `quota-view.ts`、`side.ts`（panel/1 side） | `shell.ts` `usageRows`/`usageDraw`（讀 panel/1）＋ tux §6 規劃的 fleet.console/1 額度 | 重複 | 額度表只在 autopilot-live，資料走 fleet.console/1（§6.2）；panel/1 `usage` 進入淘汰期 |
| 符號表 | `model.ts` / `strings.ts` 內嵌字元 | `ui.ts` `GLYPHS` + `glyph-check.js` + lint I5 | 重複 | 單一來源（§2） |
| 寬度計算 | `model.ts:11-25` `cellWidth`/`displayWidth`（手抄自 autopilot） | `ui.ts:18-52` | 逐字重複；forge-stats 少了 ANSI 剝除 | 單一來源；golden 鎖定 emoji、CJK、Ambiguous 三類 |
| 色彩 | 直接用 theme key／hex | `PALETTE` 八個 theme key，lint L1/L8 | 重複 | 單一來源，forge-stats 只准經 PALETTE |
| 寵物 sprite + 能力值 | `register.tsx` + `pet.json`（daemon 產） | 無 | **forge-stats 獨有** | 資料留 forge-stats（panel/1 `pet`），畫面搬進 autopilot-live 寵物分頁（§6） |
| fleet 摘要 | `fleet-view.ts`（fleet.brief/1） | `now` 分頁（autopilot.fleet 狀態） | 部分重複 | 併進 `now` 分頁，fleet.brief/1 由 autopilot 讀（須 autopilot 審查同意） |
| 寵物分頁圖示 | `hooks/pet-icon.ts`（a70f8fd，panel/1 `pet.icon`） | patch：`setPetTab`、`petRows` | forge-stats 決定、autopilot 畫 | 見 §6 |

### 1.1 盤點發現，各自落到哪一項

FS = `plugins/forge-stats`（pet-tab 分支），AP = autopilot `mods/live`（b28dbc5e6）。

| # | 發現 | 證據 | 會造成的錯 | 落點 |
|---|---|---|---|---|
| F1 | 寬度函式是手抄的，只差 ANSI 剝除 | FS `model.ts:8-25`、AP `ui.ts:18-52` | autopilot 加一個寬字區段，forge-stats 沒跟上，同一列兩邊算出不同寬度而錯位 | P4/P5 第一個抽出 |
| F2 | 百分比取整三種：FS pane 四捨五入、FS panel 無條件捨去、AP 一律捨去；AP 內部還有兩份（字串版 `pct`、數字版 `floorPercent`） | FS `quota-view.ts:166`、`side.ts:44`、AP `ui.ts:204`（`pct`，回 `"29%"`）、`band-contract.ts:57`（`floorPercent`，回 `29`） | 用量 89.6% 在 pane 畫 90%、在 panel／autopilot 畫 89%；tux 外面若仍留 `floorPercent` 的運算，就還是兩份 | tux `format.ts` 定整數版 `pctInt(ratio) = Math.floor(ratio*100 + 1e-9)`（epsilon 1e-9 照抄 AP，防 0.29*100 掉成 28）與字串版 `pct()`（建立在 `pctInt` 之上）。`floorPercent` 改成呼叫 tux 的 `pctInt`（保留它的 `round === 'floor'` 契約檢查，不再自己算）。`golden/pct-cases.json` 分開記兩個期望值：`pctInt(0.29) = 29`（整數）、`pct(0.29) = "29%"`（字串），另含 0.896 → 89 / `"89%"`。FS 兩處在 P5 改呼叫 vendor |
| F3 | 警告門檻兩套：FS `tight` 0.9、band 契約 80/100 | FS `quota.ts:20`、AP `band-statusline-contract.json:2-4` | 同一池在帶上已黃、在 pane 仍白 | `tokens.json` 加 `thresholds`（`warn: 80`、`critical: 100`，取自 band-statusline-contract.json）；P4a 同時加相等測試（tokens.json 的值 = band-statusline-contract.json 的值）。FS `quota.ts` 的 `tight: 0.9` 在 P5 換成讀 vendor 的 thresholds；行為變更（黃色由 90% 提前到 80%）列入 P5 驗收 |
| F4 | FS 直接寫 theme key，AP 只准語意角色（lint L8）；FS 也直接寫符號字面（L2） | FS `model.ts:337`、`quota-view.ts:190`、v2 `overview-view.ts:9`；AP `ui-lint.test.js:179-233, 326` | vendor 進來後套 AP lint，FS 現有原始碼全紅 | P5 先改 FS 用 `PALETTE`/`GLYPHS`，再開 lint |
| F5 | `●` 在 FS 是「最新」，在 AP 是「執行中」 | FS `model.ts:35`、AP `ui.ts:67` | 操作員在兩處看到同一符號兩種意思（tux 一符一義） | P5：FS `model.ts:35` 的「最新」改用 vendor `GLYPHS` 裡已有語意的符號（候選與替換清單由 P5 的 PR 列出，不得再用 `●`），並加 golden `glyphs.json` 的一符一義斷言 |
| F6 | `PanelTone` 沒有 warning/error | FS `types/index.d.ts:70`、AP `shell.ts:150` | 原計畫：panel 列帶不了警示色 | **不做（DROP）**。理由：額度最終離開 panel/1、改走 fleet.console/1（§6.2；panel/1 的 `usage` 進入淘汰期），警示色那一側由 fleet.console/1 與 autopilot 的 `PALETTE` 負責；寵物資料沒有「需要警告」的語意，不需要 warning tone。panel/1 維持現有 tone 集合，`contracts/panel.ts` 照現狀寫進去，不加 add-only 欄位 |
| F7 | panel/1 的 schema 字串與 tone 表兩邊手抄 | FS `panel.ts:6`、AP `shell.ts:53` | 一邊改 tone 名、另一邊靜默丟列 | `mods/tux/contracts/panel.ts`（型別、schema 字串、tone 集合、tiers）一份，P4b 抽出；`golden/panel-cases.json` 讓兩邊的寫入端與讀取端都對它跑（§4） |
| F8 | 寬度級距兩套：FS 40/60/80/100/120、30/40/50；AP 80/120/160/200/240；tux §6.6 用 200/160/120/100/80 | FS `panel.ts:7`、`side.ts:9-11`；AP `shell.ts:11-12` | 同寬度下兩邊在不同欄寬換版面 | `mods/tux/tiers.ts` 兩張**分開**的表，不混用：`PANE_TIERS`（整窗版面，對 tux §6.6）與 `SIDE_TIERS`（panel/1 側欄的窄級距：panel 40/60/80/100/120、side 30/40/50，沿用 FS 現值）。P4b 抽出時由 autopilot 對照 `shell.ts:11-12` 與 tux §6.6 選一套當 `PANE_TIERS` 並在 PR 說明（本計畫不替它決定數值）；P5 FS `panel.ts`、`side.ts` 改 import；`contracts/panel.ts` 的 tiers 鍵集合引用 `SIDE_TIERS` |
| F9 | 時長格式兩種：`2h10m` 與 `8h05m` | FS `model.ts:576`、AP `ui.ts:207` | 同一個重置倒數兩種寫法 | tux `format.ts` 的 `dur()`（以 AP `ui.ts:207` 為準，分鐘補零）；`golden/dur-cases.json`；FS `model.ts:576` 在 P5 改用 vendor |
| F10 | `bandLayout`、`bandTier`、`BAND_MIN_COLUMNS` 只剩測試在呼叫；`summaryParts` 的 fleet 分支在 `register.tsx:790` 永遠收 `null` | grep：只有 `tests/band.test.tsx` | 無（死碼），但會被誤當成要共用的東西 | P5 刪除（連同 `tests/band.test.tsx` 中對應的案例），不共用 |
| F11 | v2 分支早於 `pet-icon.ts`，`panel.ts` 會丟掉 `pet.icon`；v2 字串寫死中文 | v2 `panel.ts`、`pane-controls.ts:13,35` | v2 若直接合入，寵物分頁圖示消失 | P6 以 develop（含 a70f8fd）為底重做 v2，不直接合 v2 |
| F12 | 測試跑在兩個 runner：FS `claude plugin test`、AP `run-tests.sh`（node --test + plugin test） | `scripts/check-all.sh:84`、AP `run-tests.sh:19-45` | golden 只在一邊跑，另一邊的偏移不會紅 | §4：golden 寫成兩個 runner 都能讀的純資料，各自一支測試 |

獨有、不共用：寵物 sprite（FS `model.ts:119-198`、`register.tsx:624-711` Raster）、記憶燈（`parseSessionView`）、fleet.brief/1 解析（v2 `brief-view.ts`）。AP 獨有：決策卡熱區、捲軸（`scroll.tsx`、`scrollmath.ts`）、lint 與 invariants。**`cutProse`**：盤點沒有 forge-stats 使用它的證據（FS 裁切用的是 `side.ts:61` `fitCells`、`pane-view.ts:52` `clipCells`，行為不同），所以不列為共用項目，只隨 `strip()` 當內部相依。

## 2. 共用元件放哪、怎麼發版、怎麼釘版本（點 1）

**決定：autopilot repo 是唯一來源；codeforge 釘版本複製（vendor）。**

### 2.1 共用項目與唯一路徑

每項共用物只有一個擁有路徑（autopilot repo 內），沒有第二份來源。

| 共用項目 | 唯一路徑（autopilot） | 備註 |
|---|---|---|
| 符號表 `GLYPHS`（含 `verified` 註記、`pet.icons`） | `mods/tux/glyphs.ts` | **只在這裡**。`tokens.json` 不含符號；`golden/glyphs.json` 是由 `GLYPHS` 衍生的 golden（測試重生成後比對），不是第二份來源 |
| 色彩角色 `PALETTE` | `mods/tux/palette.ts` | 內容由 `tokens.json` 的 `roles` 產生（hooks module 不能 import JSON，沿用 `gen-band-contract.js --check` 的做法，加 `gen-tux.js --check`）；golden 再逐項比對 `PALETTE` = `tokens.json.roles`。P4a→P4b 之間 `ui.ts:87-88` 的 `PALETTE` 還在，P4a 先加相等測試 |
| 寬度函式 `cellWidth`/`displayWidth` | `mods/tux/width.ts` | |
| 分頁列 `strip()` | `mods/tux/strip.ts` | `cutProse` 是 `strip()` 的內部相依（`ui.ts:567,572`），隨它搬，**不當獨立共用項目**（forge-stats 沒有用到它，見 §1.1 末段與 §11） |
| 取整、時長 `pctInt`/`pct`/`dur` | `mods/tux/format.ts` | F2、F9 |
| 寬度級距 `PANE_TIERS`/`SIDE_TIERS` | `mods/tux/tiers.ts` | F8 |
| panel/1 契約型別、schema 字串、tone、tiers 鍵 | `mods/tux/contracts/panel.ts` | F7 |
| 語意角色、用語、門檻 `tokens.json` | `mods/tux/tokens.json` | 欄位：`roles`、`terms`、`thresholds`（§5） |
| 版本與清單 | `mods/tux/VERSION`、`mods/tux/VERSION.manifest` | 見 2.2 |
| golden | `mods/tux/golden/` | §4 |

不屬於 tux 的：分頁**順序**（`BAR_PANE_TABS`，`tabs.ts:86`）與 `barStrip`（`tabs.ts:92`）留在 live，由 autopilot 的 golden 直接呼叫（§4）；codeforge 不畫分頁列，也就不需要它。

- 為什麼不是第三個 repo：mod 執行期不能跨 plugin import，兩邊本來就要各帶一份檔案；多一個 repo 只多一個發版點，不少一次複製。autopilot 已經有 tux 準則、lint I5/L1–L18 和 glyph-check，來源放在規則旁邊，改規則和改元件是同一個 commit。
- 為什麼不是 codeforge 當來源：autopilot-live 是畫面主體（§3），forge-stats 併入後只剩資料和單機後備，畫得少的一邊不該擁有元件。
- autopilot 接受當擁有者（autopilot 01M7ME34，develop 33ca95225），條件照抄：抽出後 `ui.ts` 改從 `mods/tux/` import，不留第二份。ui-lint 的 `modSources()`（`ui-lint.test.js:110-123`）本來就掃 `mods/` 下所有目錄，所以真正的工作是把 `except` 清單（`:214,:243,:327,:329,:367` 的 `'live/ui.ts'`）和 L3 對 `ui.ts` 的 import（`:13`）改指 tux 檔，歸 P4b。

### 2.2 發版與 VERSION 鎖

- autopilot 每次改 `mods/tux/` 就加一 `mods/tux/VERSION`（整數），同 commit 更新 `golden/` 與 `VERSION.manifest`。
- `VERSION.manifest` = `{version, files: {path: sha256}}`，涵蓋 `mods/tux/` 下除 `VERSION`、`VERSION.manifest` 以外的每個檔（含 `format.ts`、`tiers.ts` 這類 golden 不一定覆蓋到的檔）；由 `scripts/tux-manifest.js` 重算並自動加一版號。
- `mods/tux/golden.test.js` 鎖：① 重算的 sha 清單 = manifest（改了內容沒重算 → 紅）；② manifest.version = `VERSION`；③ 與合併基點（`origin/develop`）的 manifest 比，檔案 sha 不同而 version 相同 → 紅（手改 manifest 但沒加版號也紅）。
- **負向控制**：在 `format.ts` 改一個位元組不重算 → 紅；重算 manifest 但版號不加 → 紅。

### 2.3 釘版本（codeforge）

- codeforge `plugins/forge-stats/vendor/tux/` + `plugins/forge-stats/vendor/tux/VENDOR.lock`：`{autopilot_sha, tux_version, files: {path: sha256}}`。
  - `scripts/tux-sync.sh <autopilot-sha>`：從 autopilot（本機 clone 或 GitHub）取該 sha 的 `mods/tux/`，寫檔、寫 lock，冪等。
  - check-all 加 `tux-lock`：vendor 檔的 sha256 必須等於 lock，**且 `vendor/tux/` 下沒有 lock 沒列的多餘檔**；手改 vendor 或塞檔都紅。
- 節奏：autopilot 發新 `tux_version` 時，codeforge 在下一個工作日內 sync（一個 PR，進 develop）。sync 本身不必同步上線；**live tag 的版本相等門檻見 §7**（第一個 P5 之後的上線起算）。

## 3. 分頁與熱區位置；併入還是獨立 pane（點 2、點 3）

### 3.1 位置（點 2）

以 autopilot 11-17 21:30 貼底版為基準：分頁列與要動手的東西貼底、靠近輸入框，內容往上長（tux v1.4 §1 熱區、§5 宿主限制）。forge-stats 不再有任何頂部分頁列。

**11-18 上線時 forge-stats 只剩 band 版面**（pane v2 暫緩，PM 01M7M9J8）。develop 上的 v1 pane 分頁列（`register.tsx:683-705`）在 pane 版面才會畫，而 `session.start` 會讀 `$.store` 的 `layout`（`register.tsx:552-553`，存成 `pane` 就開 pane）。所以 P1b（codeforge，11-18，P3 之前）把已存的 `layout: pane` 在 session.start 時遷成 `band`、並讓 `layout pane` 指令（`:580-588`）不再接受 `pane`；這樣 11-18 的 tag 沒有任何路徑會在頂部畫分頁列。死碼（v1 pane 繪製）在 P6 刪。

### 3.2 建議：併入 autopilot-live 當分頁（點 3）

| | A. 併入 autopilot-live（建議） | B. 保留獨立 pane、共用元件 |
|---|---|---|
| 操作員看到 | 一個窗、一條分頁列 | 兩個窗、兩條分頁列（就算元件共用，位置仍是兩處） |
| tux §6 | 符合「不開第二窗」 | 要改準則並寫理由 |
| 宿主捲動／焦點 | 一份（§5.5、§5.6 的修補只做一次） | 兩份 pane 各自處理捲動偏移與焦點 |
| 寬度 | 一個 pane 吃側欄寬度 | 兩個 pane 分側欄寬度，80 欄時兩邊都退讓到不可讀 |
| codeforge 成本 | v2 pane 的畫面程式退場（約 `pane-view.ts`、`fleet-view.ts`、`quota-view.ts` 的繪製部分）；v2 的資料整理（overview、fleet.brief/1 解析）改輸出到 panel/1 或交給 autopilot | 維持 v2 pane，再改用 vendor 元件、改成貼底 |
| autopilot 成本 | 多一個寵物分頁（patch 已有，約 170 行含測試）；fleet.brief/1 讀取併入 `now` | 幾乎無 |
| 對齊風險 | 只剩資料契約 | 每次畫面調整都有兩份要對 |

不改 §6。B 唯一的優點是 forge-stats 可以單獨被裝；這由 §3.3 的後備涵蓋。

### 3.3 沒裝 autopilot 的使用者

forge-stats 保留一個**單行**狀態帶後備（現行 hotfix f58809b 的形狀：autopilot-live 存在就讓位），用 vendor 的符號和色彩。**這一列貼底：它是輸入框上方的最後一列**，不在頂部，也不是分頁列。不保留獨立 pane。偵測方式沿用 hotfix：autopilot 的 `autopilot.fleet` 心跳在不在。

### 3.4 01M7KRP3（pane v2）怎麼辦

v2 的分支保留不刪。v2 的內容（overview-first、fleet.brief/1）在 A 之下變成 autopilot-live 的分頁內容；UX 閘門改對 autopilot-live 那幾個分頁重跑（carbon-high + astra 皆 ≥8）。這個改範圍要 PM 點頭（列在 §9 待決）。

### 3.5 現有 `forge` 分頁與 `pet` 分頁的關係

autopilot 現有 `forge` 分頁（`tabs.ts:84,95`；`band.tsx:108` 說它是開 forge-stats 完整檢視的短分頁，內容是速度與近期請求表）。**決定：共存，各管各的內容。** `forge` 分頁維持現狀（速度、近期請求、記憶）；`pet` 是新的窗格分頁（加進 `BAR_PANE_TABS`，位置在 `usage` 之後、`forge` 之前），只畫 sprite 和能力值。理由：取代 `forge` 會刪掉寵物以外的內容，超出 11-18 的範圍；共存只加一個分頁、不改既有分頁的語意，是最小改動。容納規則見 §6 的 `petTabFits`。

## 4. 共用 lint 與 golden（點 4）

目標：任何一邊改了分頁位置、符號、寬度規則或 panel 契約，兩邊的測試都紅。

### 4.1 golden（autopilot `mods/tux/golden/`，純文字／JSON）

- `tabrow-{80,120,160}.txt`、`tabrow-console-{80,120}.txt`：**用 live 的 `barStrip`**（`tabs.ts:92`，順序來自 `BAR_PANE_TABS` `tabs.ts:86`；console 模式走 `consoletabs.ts`）在固定 snapshot 下畫出，不是 fixture 專用的 `strip()`。含寵物圖示與後備兩種（`tabrow-80` 同時鎖 `petTabFits` 與欄寬 < 80 的後備）。改 `BAR_PANE_TABS` 順序 → golden 紅。
- `layout-bottom-{80,120}.txt`：整窗畫面，斷言**最後一列是分頁列／band**、內容列在它上面（貼底位置 golden）。把分頁列移到頂部 → 紅。
- `glyphs.json`（由 `GLYPHS` 衍生）、`width-cases.json`（emoji、CJK、East Asian Ambiguous、ZWJ、VS16）、`pct-cases.json`、`dur-cases.json`。
- `panel-cases.json`：panel/1 的案例（正常、含 `pet.icon`、未知 tone、缺欄位、`usage`/`side` 內含毒值）。**寫入端與讀取端都跑它**：codeforge 的 `tests/panel-contract.test.ts` 要求 forge-stats 產出等於案例；autopilot 的 `mods/live/panel-contract.test.js` 要求 `panel.ts` 讀取案例得到預期結果。tone 名、schema 字串、tiers 鍵任何一邊改 → 兩邊都紅。負向控制：把 tone 名改掉 → 兩邊皆紅。

### 4.2 每種變更各側哪個測試紅

| 變更 | autopilot 側紅的測試 | codeforge 側紅的測試 | 另一側怎麼跑 |
|---|---|---|---|
| 分頁順序／位置 | `mods/tux/golden.test.js`（`tabrow-*`、`layout-bottom-*`） | `tests/tux-golden.test.tsx` 用 vendor `strip()` 跑 `tabrow-*` 裡的 strip 案例；`tests/no-top-tabrow.test.tsx`（band 版面與已存 `layout: pane` 都不畫頂部分頁列，負向控制：還原 pane → 紅） | `tux-cross`（4.3） |
| 符號 | 同上（`glyphs.json`）；ui-lint L3/I5/I20、`petIconViolations` | `tests/tux-golden.test.tsx`（vendor `GLYPHS` 對 `glyphs.json`） | `tux-cross` |
| 寬度規則（含 `pct`、`dur`） | 同上（`width-cases`、`pct-cases`、`dur-cases`） | 同上；FS 自己的 `displayWidth` 已刪（P5） | `tux-cross` |
| panel 契約 | `mods/live/panel-contract.test.js`（讀取端） | `tests/panel-contract.test.ts`（寫入端） | `tux-cross` |

### 4.3 兩邊都紅的機制（`tux-cross`）

- **runner**：autopilot 沒有 repo 根目錄的 `run-tests.sh`，live mod 的 runner 是 `mods/live/run-tests.sh`（純 JS 測試走 `node --test`，`*.test.ts` 走 `claude plugin test`，後者由 `lib/real-claude-env.sh` 的 `mods_run_plugin_test` 呼叫）。本計畫的做法：
  - autopilot 側：`node --test mods/tux/golden.test.js` 與 `mods/live/panel-contract.test.js` 加進 `mods/live/run-tests.sh`；要跑 codeforge 的 tsx 測試時，用同一個 `mods_run_plugin_test` 對 `$CODEFORGE_DIR/plugins/forge-stats` 跑 `claude plugin test`（codeforge 自己的 check-all 也是這樣跑，`scripts/check-all.sh:84`）。
  - codeforge 側：check-all 的 `tux-cross` 步驟用 `claude plugin test`；`tests/tux-golden.test.tsx` 讀環境變數 `TUX_GOLDEN_DIR`（預設 `vendor/tux` 對應的 pinned golden；`tux-cross` 設成 `$AUTOPILOT_DIR/mods/tux/golden`，預設 `../autopilot`）。
- **判定規則**：
  - codeforge 側**硬紅**：vendor（或 lock 釘的 golden）與 autopilot HEAD 的 golden 不一致就紅，包括 autopilot 已升版、codeforge 還沒 sync 的情況。後果是 autopilot 升版後 codeforge 的 check-all 會紅，直到下一個工作日內的 sync PR 合入；這是刻意的，用來逼 §2.3 的節奏。
  - autopilot 側**只在「`tux_version` 相同但輸出不同」時紅**；autopilot 版號較新（codeforge 仍釘舊版）時只印 `notice: codeforge pins tux N, autopilot is M`，不紅，否則 autopilot 升版的那個 commit 一定自己打架。
- **兄弟 repo 不在**：開發時印 `skip: sibling absent`，check-all 總結列把它記為 SKIP、不算過。**release 一律硬失敗**（hangar live pin 之前的 gate 與 `scripts/release-local.sh` 都必跑 `tux-cross`；兄弟不在 = 失敗）。這條在全文只在此處定義，§7 引用它。
- 防的失敗：autopilot 改了分頁順序並更新自己的 golden，codeforge 仍釘舊版、各自綠，上線後操作員看到兩種順序。
- 另有 lint：forge-stats 原始碼中非 ASCII 的符號只准出現在 vendor `GLYPHS` 或 `pet-icon.ts` 的暫時表裡（P5 後只剩 vendor）。

## 5. 對齊 fleet 主控台網頁設計系統（點 5，01M7M7Y7）

- `mods/tux/tokens.json`（autopilot 擁有，唯一檔案）：
  - `roles`：語意名 → 終端機 theme key（沿用 `ui.ts:87-88` 的八個語意：attention→warning、here→claude、broken→error、done→success、running→suggestion、idle→inactive、structure→subtle、action→merged；加上 fleet 審查補的角色）；
  - `terms`：用語表（zh-TW / en）：`待決`、`派工`、`審查`、`用量`、`寵物`、`陳舊`、`過期`、`無來源`……；
  - `thresholds`：`warn: 80`、`critical: 100`（F3；取自 `band-statusline-contract.json`）。
  - 沒有 `glyphs`（符號只在 `mods/tux/glyphs.ts`，§2.1）。
- 網頁那邊用同一份 `tokens.json` 的 `roles` 與 `terms`；hex 色值由 fleet 網頁自己定（終端機沒有 hex），但**語意名必須一一對到**。
- **機械檢查什麼：**
  1. fleet repo 複製 `tokens.json`，一個測試把 sha256 釘住（`path@sha`；升版 = 換檔、換 sha、重跑 gate，執行期不抓）；
  2. fleet 的 cargo 測試檢查 `tokens.json.roles` 的角色名與網頁色表（`console_brief.rs` 一帶）的角色名**一對一**：每個角色有 hex，網頁色表沒有 tokens.json 以外的角色；`terms` 的 zh-TW/en 鍵齊全；
  3. autopilot 側：`PALETTE` = `tokens.json.roles` 逐項相等（§2.1）；
  4. codeforge 的 `tux-lock` 對照 fleet 釘的 sha，不同時印出差異（不擋，因為那是 fleet 的 repo）。
- **機械檢查不到什麼（明寫）：** hex 色值本身，以及終端機主題色與網頁色看起來是否一致（對比、色差）；fleet 網頁自己的 CSS（版面、字級、間距）；沒有放進 `terms` 的自由文字；兩個 repo 在升版之間的非同步落差（只靠 sha 釘住與通知）。
- **locale 與 `terms`**：autopilot 的 `locales.ts` 是產生檔（`gen-locales.js --check` 已在 `mods/live/run-tests.sh` 裡）。屬於共用用語的 key（分頁名 `tab.*` 與上列用語）改成由 `tokens.json.terms` 產生，擴充 `gen-locales.js`，`--check` 不符就紅；其餘 locale key 仍由 locales 自有。
- fleet 的回覆（01M7ME32）：01M7M7Y7 還沒定版（設計系統計畫 11-19 12:00 交）；過渡主控台只有一組**色名**（bg、fg、card、line、muted、link、red、amber、blue、grey、slate、down、bar、chipline、shadow，fleet develop e8c67ba2 `console_brief.rs`），沒有用語表。fleet 決定**以 `tokens.json` 的 roles/terms 為唯一來源**，網頁只定 hex。缺的角色（例如 stale、overdue、waiting_on）**由 fleet 在本計畫的第二代審查前提出**，P4a 凍結（開始寫第一版）前一併納入，不在網頁自加。
- 定版時由 autopilot 通知 fleet `mods/tux/tokens.json@<sha>`。
- 時程含意：`tokens.json` 的 roles/terms/thresholds 第一版要在 11-19 12:00（fleet 設計系統交件）之前存在，所以 P4 拆成 P4a（tokens.json，11-18）與 P4b（其餘抽出，11-19）。

## 6. 寵物分頁（點 6）

- **畫面**：autopilot-live 新分頁 `pet`（共存於 `forge` 分頁，§3.5），位置在 `用量` 之後、`ϟ`（forge）之前。內容：sprite（半格 Raster，沿用 `doc/specs/codeforge-mod-ui.md` 的 Windows 結論，不用 mod `Image`）＋能力值列。
  - **內容貼底**：分頁內容用 `justifyContent="flex-end"`（tux-guidelines §1：內容緊貼分頁列，中間不留空列），分頁列與 band 在最後一列。
  - **容納規則 `petTabFits(columns, t, forgeTab)`**：仿 `usageTabFits`（`tabs.ts:109-113`），剩餘欄數扣掉 pet 分頁的寬度後，仍須 ≥ `USAGE_LABEL_ROOM`（速度標籤最小 11 格），否則不畫 pet 分頁、用 dim `›` 收尾，pet 分頁不得擠掉 `forge` 分頁或速度標籤。`tabrow-80.txt` golden 鎖定這個邊界（兩側各一案）。
- **資料**：forge-stats 寫 `codeforge.panel/1` 的 `pet`：`{title, sprite, tiers, icon}`。`icon = {id, emoji, fallback, text}`，`text` 是 forge-stats 依環境決定要畫的那一個（已實作，codeforge a70f8fd，7 個測試）。
- **圖示**：一個寵物 emoji，固定 2 格。候選三個（🐱 cat、🐣 chick、🦀 crab），由操作員從截圖頁挑；預設 cat。
  - **後備條件**（任一成立就畫兩個 ASCII 字 `:3`、`o<`、`}{`）：`TERM` 是 linux/dumb/vt*/ansi/cons*、locale 不是 UTF-8、`CODEFORGE_PET_ICON=plain`（以上由 forge-stats 在 `pet.icon.text` 決定，已有測試），**或實際欄寬 < 80**（forge-stats 看不到終端機欄數，所以由 autopilot **渲染端**用真實 columns 決定：< 80 就畫 `icon.fallback`，不畫 `icon.text`）。欄寬案例進 `tabrow-80.txt` golden（79 與 80 各一案）。
  - **reader 驗證**：autopilot 只在 `text ∈ GLYPHS.pet.icons 的 emoji ∪ 其 fallback 集合` 時才開這個分頁，否則不開（不畫壞的）；「正好 2 格」只是附加條件，不再是唯一條件。**負向控制**：資料給 `⚠️`（2 格但不在清單）→ 不開分頁；給 CJK 兩個字 → 不開。
- **glyph lint 白名單**：emoji 在 lint I5 / `EMOJI_BANNED` 是禁止的。例外**只給 `GLYPHS.pet.icons` 這一個 key**，`tableGlyphs()`（`ui.ts:149`）不納入它，其他所有 key 的 emoji 檢查不變。
  - `verified`：未挑定前寫 `pending-owner`，挑定後寫 `2026-11-18 <terminal>`；符合 `verifiedOk`（`ui.ts:147`），不使用 `owner-11-18` 這種格式。
  - `pet.icons` 有**獨立的斷言函式** `petIconViolations`（不併入 `glyphTableViolations`，那個要求寬度 1）：每個 emoji 寬 2、單一 Emoji_Presentation 碼位、無 U+FE0F；每個 fallback 是兩個可印 ASCII、非 East Asian Ambiguous。沿用 `ui-lint.test.js` M7 的作法，用 planted 表測紅（植入 `⚠️`、植入帶 FE0F 的 emoji、植入寬 1 的字 → 各自紅）。
  - 分頁標籤來自資料，不在原始碼字面裡；白名單的作用是 reader 與 golden 可查的清單。
  - 實測：patch 在 autopilot b28dbc5e6 上跑完整測試，glyph lint（L3、M7）綠；唯一紅的是 `parsePanel` 的形狀斷言，已修。
  - 過渡期（P2→P5）寫實：autopilot `GLYPHS.pet.icons` 是**接收白名單**（權威，不符就不開分頁，安全失敗）；codeforge `hooks/pet-icon.ts` 的表是**發送端**暫時表，P2→P5 之間兩邊各一份，P5 把 `pet-icon.ts` 改成讀 vendor 的 `GLYPHS.pet.icons` 並刪掉自己的表。
- **與 tux §6.2 / L19 的衝突**：L19（`tux-guidelines.md:404`）禁止「額度繪製模組」出現 `codeforge.panel`。現況是 `shell.ts:46-53` 的 `parsePanel`/`PANEL_SCHEMA = 'codeforge.panel/1'` 就在額度繪製檔裡，L19 一接上 lint，`shell.ts` 先紅。所以 **P2 就做**，不等 §6.2：
  - 把 panel 讀取搬出 `shell.ts`，放進新檔 `mods/live/panel.ts`（唯一出現 `codeforge.panel` 字樣的讀取檔）；`pettab.ts` 只 import 它的 `readPet()`（回傳型別只有 `pet`，不暴露 `usage`/`side`）；`shell.ts` 的 `usageRows` 改吃 `panel.ts` 解析後的資料，檔內不再有 `codeforge.panel`。
  - tux-guidelines L19 那一列補**檔名清單**：繪製模組 = `shell.ts`、`band.tsx`、`consolepages.ts`（P2 實作時以實際的額度繪製檔為準並寫進這一列）；例外 = `panel.ts`（只讀取）、`pettab.ts`（只准用 `readPet()` 的 `pet` 欄位）。L19 本文不放寬。
  - **反向檢查在 P2 進 lint**：`pettab.ts` 不得出現 `pace.v1`、`quota`、`usage`、`.side` 字樣；負向控制：在 `pettab.ts` 植入 `panel.usage` → 紅；測試另給 `readPet()` 一個 `usage`/`side` 內含毒值的 panel，斷言輸出不受影響（關閉 `panel.side` 外洩）。
  - `shell.ts` 的 `usageRows` 最終改讀 fleet.console/1（§6.2）是 autopilot 側後續工作，不在 11-18 範圍；P2 只做上面的讀取搬家。

## 7. 操作員 clone 回到發版節奏（點 7）

- 現況：操作員帳號的 codeforge clone detached 在 47a2cf2（hotfix），forge-stats 直接從那份 clone 的 `plugins/forge-stats` 載入。
- 規則：操作員的 clone 只停在 **live tag**，不跟 develop 的 tip，也不從分支 tip 上線。codeforge 的 tag 形狀 `live-YYYY-MM-DD[-n]`；autopilot 現有的形狀是 `live-YYYYMMDD-N`（例如 `live-20261117-1`），各用各的，不統一。
  - 每次上線：PM go → 打 tag → hangar 的 live pin 腳本在各主機 `git fetch --tags && git checkout <tag>`，記 sha16 與舊狀態（回滾見下）。hangar 的 live pin 腳本與紀錄檔路徑**由 hangar 提供**，本計畫不替它定路徑。
- **11-18 上線（P3）的條件**：兩個 live tag——codeforge `live-2026-11-18`（內容 ≥ develop 1c630a4 的 pet.icon，加上 P1b）與 autopilot `live-20261118-1`（內容含 P2；若當天重打則 `-2`）——加上兩個 sha16 紀錄。**P3 不要求 `tux_version` 相等**：`mods/tux/VERSION` 要到 P4b 才存在。
- **`tux_version` 相等的門檻**：從**第一個 P5 之後的上線**起算——兩個 tag 內的 `mods/tux/VERSION` 必須等於 codeforge `VENDOR.lock.tux_version`，由 hangar 的 live pin 腳本檢查並拒絕非 tag 的 ref；`tux-cross` 在 release 時不可略過（§4.3）。
- **回滾**：
  - 第一次遷移（起點是 detached 47a2cf2，沒有舊 tag）：`git -C <操作員 clone> checkout 47a2cf2`，並把前一個 binary 還原（遷移前備份成 `.bak`，例如 `codeforge` binary 的 `.bak.<epoch>`；hangar 記下 .bak 位置與 sha16）。
  - 之後每次：`git checkout <舊 tag>` 加上前一個 binary 的 `.bak`。
- 11-18 之後就是正常節奏。

## 8. 實作分工與時程（點 3 的「並行」）

寫入檔案不重疊是並行的前提：P1b 只寫 codeforge `register.tsx` 一帶；P2 只寫 autopilot `mods/live/*`（含 `ui.ts`、`ui-lint.test.js`、`tabs.ts`、新檔 `pettab.ts`、`panel.ts`）；P4a 只寫 `mods/tux/tokens.json`。P4b 會動 P2 動過的檔，所以它必須排在 P2 合入之後。

| 序 | 項目 | 誰 | 依賴 | 時程 |
|---|---|---|---|---|
| P1 | `pet.icon` 合入 develop | codeforge | 無 | **已完成**：develop 1c630a4（11-17，check-all 綠） |
| P1b | forge-stats 只剩 band 版面：session.start 把已存的 `layout: pane` 遷成 `band`、`layout pane` 指令不再接受 `pane`（`register.tsx:552-556,580-588`）；`tests/no-top-tabrow.test.tsx`（§4.2） | codeforge | 無 | 11-18，P3 之前 |
| P2 | 寵物分頁正式 PR：`pettab.ts`、`panel.ts`（panel 讀取搬出 `shell.ts`）、`GLYPHS.pet.icons`＋`petIconViolations`＋planted 測試、reader 白名單與負向控制、`petTabFits`＋`tabrow-80`（golden 先放 `mods/live/golden/`，P4b 搬進 `mods/tux/golden/`）、L19 檔名清單與反向檢查、欄寬 < 80 後備（§6） | autopilot | 21:30 貼底版 | 11-18 |
| P3 | 11-18 上線：codeforge `live-2026-11-18` + autopilot `live-20261118-1`，兩個 sha16 紀錄（**不要求 `tux_version`**，§7） | PM go，hangar 執行 | P1、P1b、P2 | 11-18 晚 |
| P4a | `mods/tux/tokens.json` 第一版（`roles` 沿用 `ui.ts:87-88` 八語意 + fleet 審查補的角色；`terms` zh-TW/en；`thresholds` 80/100 + 與 band 契約相等的測試；`PALETTE` = `roles` 相等測試），通知 fleet path@sha | autopilot | 計畫凍結、fleet 補的角色 | 11-18 |
| P4b | 抽出 `mods/tux/`：`glyphs.ts`（含 `pet.icons` 與 pettab 例外一併帶進來）、`palette.ts`（由 tokens 產生）、`width.ts`、`strip.ts`、`format.ts`（`pctInt`/`pct`/`dur`，`floorPercent` 改呼叫）、`tiers.ts`（F8）、`contracts/panel.ts`（F7）+ `VERSION`/`VERSION.manifest` + golden；`ui.ts` 改 import；ui-lint 的 `except`（`:214,:243,:327,:329,:367`）與 L3 import（`:13`）改指 tux 檔 | autopilot | **P2 已合**、P4a | 11-19 |
| P5 | `tux-sync.sh`、`VENDOR.lock`、`tux-lock`/`tux-cross` 進 check-all；FS 改用 vendor 並刪自己的：F1 寬度函式（`model.ts:8-25`，改用 vendor `displayWidth`）、F2 `quota-view.ts:166`/`side.ts:44` 改 `pctInt`、F3 `quota.ts` `tight 0.9` → vendor thresholds、F4 theme key／符號字面改 `PALETTE`/`GLYPHS` 後才開 lint、F5 `model.ts:35` 的 `●` 換 vendor 語意符號、F8 `panel.ts`/`side.ts` 改 import `tiers.ts`、F9 `model.ts:576` 改 `dur()`、F10 刪死碼；`pet-icon.ts` 改讀 `GLYPHS.pet.icons` 並刪暫時表；`tests/panel-contract.test.ts`、`tests/tux-golden.test.tsx` | codeforge | P4b | 11-19～20 |
| P6a | codeforge：刪 v1 pane 繪製死碼；v2 內容改成輸出到 panel/1 與 fleet.brief/1 的資料；UX 閘門重跑所需的截圖 | codeforge | P4b、P5、PM 對 §3.4 點頭 | 11-20 起 |
| P6b | autopilot：把 v2 內容畫成 autopilot-live 分頁；`shell.ts` `usageRows` 的 panel/1 讀取（P2 已搬到 `panel.ts`）遷到 fleet.console/1（§6.2） | autopilot | P6a 的資料契約、P4b | 11-20 起 |
| P7 | fleet 網頁釘 tokens.json（sha256 測試）與角色／用語一對一的 cargo 測試 | fleet | P4a | 11-19 |

P1–P3 不依賴 P4a 以後的任何東西：寵物分頁先用 autopilot 現有的 `GLYPHS`/`cellWidth`，P5 時再換成 vendor；11-18 的上線只要求兩個 tag 與 sha16 紀錄（§7）。P6 拆成各只有一個擁有者的兩列，不再由兩個 repo 共同寫同一項。

## 9. 待決（交 PM）

1. §3.2 選 A（併入）並據此把 01M7KRP3 的範圍改成「v2 內容進 autopilot-live 分頁」。建議：A。
2. 寵物圖示三選一（操作員從截圖頁挑）。建議：cat（:3 後備最好認）。

## 10. 不做的事

- 不改 tux v1.4 §6「不開第二窗」。
- 不在 codeforge 擁有任何共用元件的原始版本。
- 不做 MUD 後段畫面、不新增配額資料來源（G40 not-goals）。

## 11. 第一代審查處置

指紋為 g1b 輸出的前 8 碼；「AP」= autopilot 擁有者審查（plan@54db9f7）。全部採納，沒有不採納項目。

| 來源 | 發現 | 處置 |
|---|---|---|
| astra 4ed92112（R9） | 11-18 上線要求 tux_version 相同，但 VERSION 到 P4b 才有 | 採納：P3 只要求兩個 tag 與 sha16；相等門檻自首次 P5 之後的上線起算（§7、§8） |
| astra 4c586d87（R2） | 沒有 panel 契約案例與貼底位置測試 | 採納：`panel-cases.json` 兩側都跑、`layout-bottom-*` golden、`tests/no-top-tabrow.test.tsx`（§4.1、§4.2） |
| astra 57591e5d（R7） | F3、F6–F8 沒有工作項 | 採納：F3→tokens thresholds、F6 DROP（附理由）、F7→`contracts/panel.ts`、F8→`tiers.ts` 兩張表，並排進 P4a/P4b/P5（§1.1、§8） |
| astra 43c0dfeb（R8） | 第一次遷移沒有可用的回滾目標 | 採納：回滾 = `checkout 47a2cf2` + 前一個 binary 的 `.bak`（§7） |
| carbon 011d3f7c（R1） | 路徑不明確、缺 panel 契約、符號重複 | 採納：§2.1 逐項唯一路徑；glyph 只在 `glyphs.ts`，`tokens.json` 去掉 glyphs；`VENDOR.lock` 路徑固定，`tux-lock` 也抓多餘檔（§2） |
| carbon caa19319（R2） | 單行 `strip(fixture)` golden 抓不到順序、無 panel 案例 | 採納：golden 改呼叫 live `barStrip`（含 console 模式）；panel 案例兩側跑（§4.1） |
| carbon 76c96979（R3） | develop 上的 v1 pane 頂部分頁列在 11-18 仍可達 | 採納：新增 P1b，已存的 `layout: pane` 遷成 `band`；11-18 只有 band 版面；死碼 P6a 刪（§3.1、§8） |
| carbon 11101d3b（R5） | reader 只驗 2 格、emoji 例外外溢、L19 未排程 | 採納：reader 白名單＋負向控制、例外只給 `pet.icons` 且有獨立斷言、P2 做 `panel.ts` 搬家與反向檢查（§6） |
| carbon dae0d4b6（R6） | 沒寫哪些不檢查、fleet 只釘 sha、缺角色 | 採納：§5 列出機械／非機械兩份清單、fleet 加一對一測試、缺角色在審查前提出（§5、P7） |
| carbon a8ac9b60（R7） | F3/F6/F7/F8/F9 未對應、P5 缺具體遷移、tiers 混在一起 | 採納：§8 P5 逐項列 F1–F5、F8–F10；F8 分兩張表；F6 DROP（§1.1、§8） |
| carbon c5d59793（R8） | §2 與 §7 對版本一致的規則互相矛盾，缺 autopilot tag 與 hangar 路徑 | 採納：sync 不必同步上線、門檻只看 live tag 且自 P5 起算；autopilot tag `live-20261118-1`；hangar 路徑由 hangar 提供（§2.3、§7） |
| carbon 1f5975e7（R9） | P2 與 P4b 寫同檔卻無依賴；P6 雙擁有者 | 採納：P4b 依賴 P2 已合；P6 拆成 P6a／P6b 各一擁有者並對映 `usageRows` 遷移（§8） |
| AP R1 | 缺 panel 型別與 tiers；`pct`/`floorPercent` 兩份；PALETTE 與 roles 兩份；VERSION 鎖無機制；行號 | 採納：§2.1、F2（`pctInt`＋`pct`、`floorPercent` 改呼叫）、`palette.ts` 由 `roles` 產生、VERSION.manifest＋負向控制、行號改 `ui.ts:87-88` |
| AP R2 | golden 沒案例；tab 順序在 live；`tux-cross` 自打架；runner；兄弟不在 | 採納：§4.1–§4.3（`barStrip`、版號規則、runner 為 `mods/live/run-tests.sh` ＋ `claude plugin test`、release 硬失敗） |
| AP R3 | pet 內容貼底、後備位置、`petTabFits`、forge 與 pet 的關係 | 採納：§6（`flex-end`、`petTabFits`＋`tabrow-80`）、§3.3 最後一列、§3.5 共存 |
| AP R5 | 欄寬 < 80 後備、白名單擋資料、`verified` 格式、獨立斷言、`shell.ts` 的 L19 衝突、檔名清單 | 採納：§6 逐項 |
| AP R6 | `terms` 與 locales 兩份；門檻不在 tokens | 採納：locale 由 `terms` 產生並 `--check`；`thresholds` 進 tokens（§5） |
| AP R7 | F3/F5/F6/F8 無列；`cutProse` 無使用證據 | 採納：§8 列出；F6 DROP；`cutProse` 不當獨立共用項，只隨 `strip()` 當內部相依（FS 實際用的是 `fitCells` `side.ts:61` 與 `clipCells` `pane-view.ts:52`，行為不同，本計畫不替換，P5 若要換須另附等價證據） |
| AP R8 | tux_version 與 11-18 上線矛盾 | 採納：同 astra 4ed92112 |
| AP R9＋附帶 | P4b 依賴 P2；ui-lint 的 `except`／L3 import 要改指 tux | 採納：§8 P4b；§2.1 說明 `modSources()` 本來就掃全部，工作是改 `except` 與 import |
