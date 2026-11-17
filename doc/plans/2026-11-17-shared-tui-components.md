# 計畫：forge-stats × autopilot-live 共用終端機元件（含寵物分頁）

> 來源：PM 工單 01M7MD5Z（操作員 11-17 17:2x「11/19? 為什麼要那麼久」），範圍 = PM 01M7M9J4 五點 + 01M7MD92 補兩點。
> 主導：codeforge lane。審查：hetero plan loop（最多兩代），autopilot lane 為其中一位審查者。
> 凍結交付：11-18 12:00（台北）前交 PM。寵物分頁 11-18 上線。
> 狀態：**草稿 g0**（11-17 晚）。凍結規則：rubric（同目錄 `.rubric.md`）在第一代審查前凍結，不再改。

## 0. 為什麼要做（一段話）

操作員看到同一個終端機裡兩套分頁列：forge-stats 的在上面、autopilot-live 的在下面，符號和顏色也各畫各的。原因不是誰畫錯，而是兩個 plugin 各自複製了一份分頁列、符號表、寬度計算和色彩。只要還是兩份，對齊就只能靠人眼。這份計畫把它們變成一份，並用測試讓任何一邊的偏移變紅。

## 1. 現況盤點（重複與獨有）

> 欄位：區塊 | forge-stats 位置 | autopilot 位置 | 重複／獨有 | 處置。行號以 codeforge develop 94c56f1、autopilot origin/develop b28dbc5e6 為準。


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
| F2 | 百分比取整三種：FS pane 四捨五入、FS panel 無條件捨去、AP 一律捨去 | FS `quota-view.ts:166`、`side.ts:44`、AP `ui.ts:204`、`band-contract.ts:57` | 用量 89.6% 在 pane 畫 90%、在 panel／autopilot 畫 89% | tux 定一個 `pct()`：`Math.floor(ratio*100 + 1e-9)`（照抄 AP 的 epsilon，防 0.29*100 掉成 28）；golden `width-cases.json` 旁加 `pct-cases.json`，含 0.29 → 29、0.896 → 89 |
| F3 | 警告門檻兩套：FS `tight` 0.9、band 契約 80/100 | FS `quota.ts:20`、AP `band-statusline-contract.json:2-4` | 同一池在帶上已黃、在 pane 仍白 | tokens.json 一份門檻 |
| F4 | FS 直接寫 theme key，AP 只准語意角色（lint L8）；FS 也直接寫符號字面（L2） | FS `model.ts:337`、`quota-view.ts:190`、v2 `overview-view.ts:9`；AP `ui-lint.test.js:179-233, 326` | vendor 進來後套 AP lint，FS 現有原始碼全紅 | P5 先改 FS 用 `PALETTE`/`GLYPHS`，再開 lint |
| F5 | `●` 在 FS 是「最新」，在 AP 是「執行中」 | FS `model.ts:35`、AP `ui.ts:67` | 操作員在兩處看到同一符號兩種意思（tux 一符一義） | FS 改用 vendor 的語意符號 |
| F6 | `PanelTone` 沒有 warning/error | FS `types/index.d.ts:70`、AP `shell.ts:150` | panel 列無法帶警示色，快耗盡的池在 autopilot 分頁裡是白的 | panel/1 加 tone（add-only）；額度最終改走 fleet.console/1（§6） |
| F7 | panel/1 的 schema 字串與 tone 表兩邊手抄 | FS `panel.ts:6`、AP `shell.ts:53` | 一邊改 tone 名、另一邊靜默丟列 | `tux/contracts/panel.ts` 型別一份，進 golden |
| F8 | 寬度級距兩套：FS 40/60/80/100/120、30/40/50；AP 80/120/160/200/240；tux §6.6 用 200/160/120/100/80 | FS `panel.ts:7`、`side.ts:9-11`；AP `shell.ts:11-12` | 同寬度下兩邊在不同欄寬換版面 | tux 的級距表為唯一；panel/1 的 tiers 改對它 |
| F9 | 時長格式兩種：`2h10m` 與 `8h05m` | FS `model.ts:576`、AP `ui.ts:207` | 同一個重置倒數兩種寫法 | 共用 `dur()` |
| F10 | `bandLayout`、`bandTier`、`BAND_MIN_COLUMNS` 只剩測試在呼叫；`summaryParts` 的 fleet 分支在 `register.tsx:790` 永遠收 `null` | grep：只有 `tests/band.test.tsx` | 無（死碼），但會被誤當成要共用的東西 | P5 刪除，不共用 |
| F11 | v2 分支早於 `pet-icon.ts`，`panel.ts` 會丟掉 `pet.icon`；v2 字串寫死中文 | v2 `panel.ts`、`pane-controls.ts:13,35` | v2 若直接合入，寵物分頁圖示消失 | P6 以 develop（含 a70f8fd）為底重做 v2，不直接合 v2 |
| F12 | 測試跑在兩個 runner：FS `claude plugin test`、AP `run-tests.sh`（node --test + plugin test） | `scripts/check-all.sh:84`、AP `run-tests.sh:19-45` | golden 只在一邊跑，另一邊的偏移不會紅 | §4：golden 寫成兩個 runner 都能讀的純資料，各自一支測試 |

獨有、不共用：寵物 sprite（FS `model.ts:119-198`、`register.tsx:624-711` Raster）、記憶燈（`parseSessionView`）、fleet.brief/1 解析（v2 `brief-view.ts`）。AP 獨有：決策卡熱區、捲軸（`scroll.tsx`、`scrollmath.ts`）、lint 與 invariants。

## 2. 共用元件放哪、怎麼發版、怎麼釘版本（點 1）

**決定：autopilot repo 是唯一來源；codeforge 釘版本複製（vendor）。**

- 位置：autopilot `mods/tux/`，從 `mods/live/ui.ts` 抽出：`GLYPHS`（含 verified 註記）、`PALETTE`（8 theme key）、`cellWidth`/`displayWidth`、`strip()` 分頁列、`cutProse`、寬度退讓表，以及 `tokens.json`（§5）與 `golden/`（§4）。
- 為什麼不是第三個 repo：mod 執行期不能跨 plugin import，兩邊本來就要各帶一份檔案；多一個 repo 只多一個發版點，不少一次複製。autopilot 已經有 tux 準則、lint I5/L1–L18 和 glyph-check，來源放在規則旁邊，改規則和改元件是同一個 commit。
- 為什麼不是 codeforge 當來源：autopilot-live 是畫面主體（§3），forge-stats 併入後只剩資料和單機後備，畫得少的一邊不該擁有元件。
- autopilot 接受當擁有者（autopilot 01M7ME34，develop 33ca95225），條件照抄：抽出後 `ui.ts` 改從 `mods/tux/` import，不留第二份；`ui-lint`（含 `glyph-check.js`）改掃 `mods/tux/`。
- 發版：autopilot 每次改 `mods/tux/` 就在 `mods/tux/VERSION` 加一（整數），同 commit 更新 `golden/`；golden 測試鎖住 VERSION（改了內容沒加版號 → 紅）。
- 釘版本：codeforge `plugins/forge-stats/vendor/tux/` + `VENDOR.lock`：`{autopilot_sha, tux_version, files: {path: sha256}}`。
  - `scripts/tux-sync.sh <autopilot-sha>`：從 autopilot（本機 clone 或 GitHub）取該 sha 的 `mods/tux/`，寫檔、寫 lock，冪等。
  - check-all 加 `tux-lock`：vendor 檔的 sha256 必須等於 lock；手改 vendor 就紅。
- 釘版本的節奏：autopilot 發新 `tux_version` 時，codeforge 在下一個工作日內 sync（一個 PR），不必同步上線。

## 3. 分頁與熱區位置；併入還是獨立 pane（點 2、點 3）

### 3.1 位置（點 2）

以 autopilot 11-17 21:30 貼底版為基準：分頁列與要動手的東西貼底、靠近輸入框，內容往上長（tux v1.4 §1 熱區、§5 宿主限制）。forge-stats 不再有任何頂部分頁列。

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

forge-stats 保留一個**單行**狀態帶後備（現行 hotfix f58809b 的形狀：autopilot-live 存在就讓位），用 vendor 的符號和色彩。不保留獨立 pane。偵測方式沿用 hotfix：autopilot 的 `autopilot.fleet` 心跳在不在。

### 3.4 01M7KRP3（pane v2）怎麼辦

v2 的分支保留不刪。v2 的內容（overview-first、fleet.brief/1）在 A 之下變成 autopilot-live 的分頁內容；UX 閘門改對 autopilot-live 那幾個分頁重跑（carbon-high + astra 皆 ≥8）。這個改範圍要 PM 點頭（列在 §9 待決）。

## 4. 共用 lint 與 golden（點 4）

目標：任何一邊改了分頁位置或符號，兩邊的測試都紅。

- **golden 在 autopilot `mods/tux/golden/`**，純文字，每檔一個寬度：
  - `tabrow-{80,120,160}.txt`：固定 fixture 下的分頁列（含寵物圖示與後備兩種）；
  - `glyphs.json`：每個符號 → 意義 → 寬度；
  - `width-cases.json`：emoji、CJK、East Asian Ambiguous、ZWJ、VS16 的期望寬度。
- **autopilot 側**：`mods/tux/golden.test.js` 用 `strip()` 重畫並比對；改分頁順序、位置或符號而不更新 golden → 紅。
- **codeforge 側**：
  - `tests/tux-golden.test.tsx` 用 vendor 的 `strip()`、`displayWidth` 跑同一份 golden → vendor 與 golden 不一致就紅；
  - forge-stats 自己的 `displayWidth` 刪掉，改用 vendor 的（寬度規則不同是現在就存在的 bug 來源）；
  - lint：forge-stats 原始碼中非 ASCII 的符號只准出現在 vendor `GLYPHS` 或 `pet-icon.ts` 的白名單裡。
- **「兩邊都紅」的做法**：check-all 加 `tux-cross`：若 `$AUTOPILOT_DIR`（預設 `../autopilot`）存在，用它 HEAD 的 `mods/tux/golden/` 跑 codeforge 的 golden 測試；autopilot 的測試腳本同樣用 `$CODEFORGE_DIR` 跑 codeforge 的 `tests/tux-golden.test.tsx`。另一邊不在本機時印一行 `skip: sibling absent` 並不算過（release 腳本裡改成必跑）。
  - 防的失敗：autopilot 改了分頁順序並更新自己的 golden，codeforge 仍釘舊版、各自綠，上線後操作員看到兩種順序。

## 5. 對齊 fleet 主控台網頁設計系統（點 5，01M7M7Y7）

- `mods/tux/tokens.json`（autopilot 擁有）：
  - `roles`：語意名 → 終端機 theme key（`attention`→`warning` 等，沿用 `ui.ts:79-86` 的八個語意）；
  - `glyphs`：符號 → 語意名（同 `glyphs.json`）；
  - `terms`：用語表（zh-TW / en）：`待決`、`派工`、`審查`、`用量`、`寵物`、`陳舊`、`過期`、`無來源`……
- 網頁那邊用同一份 `tokens.json` 的 `roles` 與 `terms`；hex 色值由 fleet 網頁自己定（終端機沒有 hex），但**語意名必須一一對到**。
- 機械檢查：fleet 網頁 repo 也釘 `tokens.json` 的 sha256；兩邊的 lock 不同時，codeforge 的 `tux-lock` 印出差異（不擋，因為那是 fleet 的 repo）。
- fleet 的回覆（01M7ME32）：01M7M7Y7 還沒定版（設計系統計畫 11-19 12:00 交）；過渡主控台只有一組**色名**（bg、fg、card、line、muted、link、red、amber、blue、grey、slate、down、bar、chipline、shadow，fleet develop e8c67ba2 `console_brief.rs`），沒有用語表。fleet 決定**以 `tokens.json` 的 roles/terms 為唯一來源**，網頁只定 hex；缺的角色（例如 stale、overdue、waiting_on）由 fleet 在本計畫審查裡提出，不在網頁自加。
- fleet 的機械檢查：主控台 HTML 編進 Rust binary，所以 fleet 複製 `tokens.json` 進 fleet repo，加一個 sha256 等於釘住值的測試，執行期不抓；升版 = 換檔、換 sha、重跑 gate。定版時由 autopilot 通知 fleet `mods/tux/tokens.json@<sha>`。
- 時程含意：`tokens.json` 的 roles/terms 第一版要在 11-19 12:00（fleet 設計系統交件）之前存在，所以 P4 拆成 P4a（tokens.json，11-18）與 P4b（其餘抽出，11-19）。

## 6. 寵物分頁（點 6）

- **畫面**：autopilot-live 新分頁 `pet`，位置在 `用量` 之後、`ϟ` 之前。內容：sprite（半格 Raster，沿用 `doc/specs/codeforge-mod-ui.md` 的 Windows 結論，不用 mod `Image`）＋能力值列。
- **資料**：forge-stats 寫 `codeforge.panel/1` 的 `pet`：`{title, sprite, tiers, icon}`。`icon = {id, emoji, fallback, text}`，`text` 是要畫的那一個（已實作，codeforge a70f8fd，7 個測試）。
- **圖示**：一個寵物 emoji，固定 2 格。候選三個（🐱 cat、🐣 chick、🦀 crab），由操作員從截圖頁挑；預設 cat。
  - 後備：`TERM` 是 linux/dumb/vt*/ansi/cons*、locale 不是 UTF-8、或 `CODEFORGE_PET_ICON=plain` 時，畫兩個 ASCII 字（`:3`、`o<`、`}{`）。
  - autopilot 只畫 `pet.icon.text`，畫之前用 vendor `displayWidth` 驗是不是正好 2 格，不是就不開這個分頁（不畫壞的）。
- **glyph lint 白名單**：emoji 在 lint I5 / `EMOJI_BANNED` 是禁止的。例外寫成 `GLYPHS.pet.icons`（三個候選，`verified: owner-11-18`），lint 只對這一個 key 放行，且斷言每個都是單一 Emoji_Presentation 碼位、無 U+FE0F、`cellWidth === 2`。分頁標籤來自資料，不在原始碼字面裡；白名單的作用是讓 golden 和 reader 的寬度驗證有一份可查的清單。
  - 實測：patch 在 autopilot b28dbc5e6 上跑完整測試，glyph lint（L3、M7）綠；唯一紅的是 `parsePanel` 的形狀斷言，已修。
- **與 tux §6.2 / L19 的衝突**：L19 禁止「額度繪製模組」出現 `codeforge.panel`。寵物不是額度，所以寵物分頁放在獨立模組 `mods/live/pettab.ts`。L19 目前只是準則條文、還沒接進 lint（tux-guidelines.md:607），所以例外要先寫下來，免得接 lint 時變成漏洞：
  - tux-guidelines L19 那一列補一句：「`pettab.ts` 例外，只准讀 `codeforge.panel/1` 的 `pet` 欄位」；
  - 反向檢查（同時進 lint）：`pettab.ts` 不得出現 `pace.v1`、`quota`、`usage` 字樣，也不得讀 panel/1 的 `pet` 以外欄位；負向控制：在 `pettab.ts` 植入 `panel.usage` → 紅。
  - `shell.ts` 的 `usageRows` 讀 panel/1 的現況照 §6.2 改讀 fleet.console/1（autopilot 側工作，不在 11-18 範圍）。

## 7. 操作員 clone 回到發版節奏（點 7）

- 現況：操作員帳號的 codeforge clone detached 在 47a2cf2（hotfix），forge-stats 直接從那份 clone 的 `plugins/forge-stats` 載入。
- 規則：操作員的 clone 只停在 **live tag**（`live-YYYY-MM-DD[-n]`），不跟 develop 的 tip。
  - 每次上線：PM go → 打 tag → hangar 的 live pin 腳本在各主機 `git fetch --tags && git checkout <tag>`，記 sha16、舊 tag（回滾指令 `git checkout <舊 tag>`）。
  - autopilot 同樣以 tag 釘；兩個 tag 一起寫進上線紀錄（`tux_version` 必須相同，否則不上）。
- 11-18 的第一個 tag：`live-2026-11-18`，內容 ≥ develop 1c630a4（pet.icon 已合入）；操作員 clone 從 47a2cf2 移到這個 tag。之後就是正常節奏。

## 8. 實作分工與時程（點 3 的「並行」）

| 序 | 項目 | 誰 | 依賴 | 時程 |
|---|---|---|---|---|
| P1 | `pet.icon` 合入 develop | codeforge | 無 | **已完成**：develop 1c630a4（11-17，check-all 綠） |
| P2 | 寵物分頁（patch → autopilot 正式 PR，含白名單與 reader 寬度驗證） | autopilot | 21:30 貼底版 | 11-18 |
| P3 | 11-18 上線：codeforge `live-2026-11-18` + autopilot tag，同 `tux_version` | PM go，hangar 執行 | P1、P2 | 11-18 晚 |
| P4a | `mods/tux/tokens.json` 第一版（roles 沿用 `ui.ts:79-86` 八語意 + fleet 審查補的角色；terms zh-TW/en），通知 fleet path@sha | autopilot | 計畫凍結 | 11-18 |
| P4b | 抽出 `mods/tux/` 其餘（GLYPHS、PALETTE、寬度、strip、cutProse、pct、dur）+ `VERSION` + golden；`ui.ts` 改 import | autopilot | P4a | 11-19 |
| P5 | `tux-sync.sh`、`VENDOR.lock`、`tux-lock`/`tux-cross` 進 check-all；刪 forge-stats 自己的寬度與符號 | codeforge | P4b | 11-19～20 |
| P6 | forge-stats v2 pane 退場；內容改成 autopilot-live 分頁的資料；重跑 UX 閘門 | codeforge + autopilot | P4b、P5、PM 對 §3.4 點頭 | 11-20 起 |
| P7 | fleet 網頁釘 tokens.json（sha256 測試） | fleet | P4a | 11-19 |

P1–P3 不依賴 P4a 以後的任何東西：寵物分頁先用 autopilot 現有的 `GLYPHS`/`cellWidth`，P5 時再換成 vendor。

## 9. 待決（交 PM）

1. §3.2 選 A（併入）並據此把 01M7KRP3 的範圍改成「v2 內容進 autopilot-live 分頁」。建議：A。
2. 寵物圖示三選一（操作員從截圖頁挑）。建議：cat（:3 後備最好認）。

## 10. 不做的事

- 不改 tux v1.4 §6「不開第二窗」。
- 不在 codeforge 擁有任何共用元件的原始版本。
- 不做 MUD 後段畫面、不新增配額資料來源（G40 not-goals）。
