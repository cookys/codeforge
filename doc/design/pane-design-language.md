# Pane 設計語言規範

> 本文件從既有規範、UX 閘門評審紀錄與實作規格抽出整理，作為後續所有 pane 設計稿與 brief 的單一設計語言參考。每條規則皆標註出處；找不到出處的規則不寫，找不到的來源標「未找到」。與現行實拍畫面不一致之處標註「**與現行不同**」。


## 操作員固定準則（唯一歸屬檔）

所有 pane、band、forge-stats 的畫面改動，動手前先讀這一節。這六條是操作員定的，每一輪設計稿、brief、審查都照這裡；別的文件只放一行指標連到這裡，不重抄內容。要改這一節要操作員本人決定（Goal 層級），lane 不能自己改。

| 條 | 準則 | 操作員日期／出處 |
|---|---|---|
| R1 | 操作區（分頁、熱鍵、選項、送出）固定在下方、靠近 prompt，資訊往上長；上半部只放不可點的分類標題。這一條優先於 ux-v7 原本把選單列放在 band 上方的做法。 | 2026-11-05（tui-guideline §13）；2026-11-20 重申；d-08e596ce（2026-11-25） |
| R2 | 從核准的 ux-v7 / v7.4 框架改進，不重新設計。偏離要列偏離清單並說明原因。 | d-02911087 A；PM 01M7ZH98；d-08e596ce（A2/B2 都不採用，2026-11-25） |
| R3 | 底部 band 左右分區、照重要性表排、收起時一行；展開鍵位置固定，不和 Claude Code 的 `[-]` 衝突。 | d-08e596ce（2026-11-25），PM 稽核 01M88C8ETJ 整理 |
| R4 | band 和右側 Pane 一起設計：band 是底部操作熱區，Pane 負責瀏覽和細節，兩者共用狀態；forge-stats 併入，不開第二個 pane。 | d-08e596ce（2026-11-25）；PM 01M78CB6、01M7MD5Z |
| R5 | CJK 算 2 格、框線對齊、深淺雙主題對比，都用腳本驗，不靠肉眼。 | PM 01M88BD7K5（2026-11-25，操作員第 4 次回報） |
| R6 | 選項鍵照字母對應：按 a 一定選 A。 | d-c7eb5dfb（2026-11-25） |

機械檢查（R1、R5、R6 已有腳本；新增的檢查登記在這裡）：
- R1：上半部可點元素為 0（autopilot `mods/live`，單 01M88CCDQD；現行頁面已知的違規列在測試的已知清單，下一張設計單要清掉）。
- R5：`mods/tux/grid-check.ts`、`mods/tux/contrast.test.js`、`mods/live/grid-frames.test.ts`（單 01M88BD7K5）。
- R6：`pickKeys` 測試（單 01M88CCDQD）。

## 現行畫面基準（209x50，待決頁 A 樣式）

來源：操作員 pane 實拍（209x50，待決頁 A 樣式；本機實拍檔）

```text
╭─ 待決 · 資料 fleet decisions list --json（schema fleet.decisions v1）· 10 秒 ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ [-]
│ 第 1 到 8 張，共 21 張 · 要你決定 21 張 · 目前第 4 張                                                                                                                                                    │
│    1. 合併 plan：要不要把 shared-tui plan 合進 develop？  · 建議 合併                                                                                                                                    │
│    2. 外掛更新時機：forge-stats 外掛今晚還是明早更新？  · 建議 明早                                                                                                                                      │
│    3. 預覽先拍幾欄：預覽先拍 80 欄還是 120 欄？  · 建議 120 欄                                                                                                                                           │
│ ▸  4. 驗收長卡：這張卡的背景有八十行，用來驗證 PgDn 的步長  · 建議 收到                                                                                                                                  │
│    5. 驗收測試卡：這是驗收腳本的測試卡  · 建議 收到                                                                                                                                                      │
│    6. 捲動測試卡 01：捲動測試 01：這張卡用來湊滿 21 張，選哪個？  · 建議 合併                                                                                                                            │
│    7. 捲動測試卡 02：捲動測試 02：這張卡用來湊滿 21 張，選哪個？  · 建議 合併                                                                                                                            │
│    8. 捲動測試卡 03：捲動測試 03：這張卡用來湊滿 21 張，選哪個？  · 建議 合併                                                                                                                            │
│ ↓ 還有 13 張 · j 往下                                                                                                                                                                                    │
│ ▸  4. 驗收長卡：這張卡的背景有八十行，用來驗證 PgDn 的步長                     █                                                                                                                         │
│     背景   第 01 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        █                                                                                                                         │
│            第 02 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 03 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 04 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 05 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 06 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 07 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 08 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 09 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 10 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 11 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 12 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│            第 13 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。        │                                                                                                                         │
│ ↓ 下面還有 71 行                                                                                                                                                                                         │
│ s: 預覽並送出已選 0 張 · 另 21 張未選                                                                                                                                                                    │
╰─  ↑↓ / PgUp/PgDn 捲卡 · p/n 換張 · j/k 換張 · a/b/c 選 · m 附註 · s 預覽 · 鍵盤在 pane · Esc  ────────────────────────────────  1 現況  2 待決  3 派工  4 審查  5 花費  6 殘留  7 艦隊  8 待辦  9 說明  ─╯
```

---

## 1. 框與版面

### 規則
1. **上框線結構**：上框線格式為 `╭─ <標題> · <資料來源> · <秒數> ───╮ [-]`，右緣保留 `[-]` 給引擎收合按鈕（`current-pane-A-209x50.box.txt` L1；`doc/specs/tui-guideline.md` §14.1 R1）。
2. **下框線結構**：下框線左側為按鍵提示列，右側為頁籤列（`1 現況  2 待決  3 派工  4 審查  5 花費  6 殘留  7 艦隊  8 待辦  9 說明`）（`current-pane-A-209x50.box.txt` L28）。
3. **寬度與 `bodyColumns` 預留**：引擎回報的 `bodyColumns` 可能比實際可畫寬度多 2 格，行寬上限為 `bodyColumns - 2`（`doc/specs/tui-guideline.md` §1.1 規則 2、`doc/specs/codeforge-mod-ui.md` §4）。pane 預設寬度 76、最窄 20（`doc/specs/tui-guideline.md` §1.2）。
4. **高度規則**：
   - UX 閘門規範：總高度 ≤ `min(maxRows, floor(rows * 0.6))`，不得有整列空白補位（`doc/specs/forge-stats-ux-gate.md` 評分項目 3）。
   - **與現行不同**：實測現行待決框在 209x50 與 120x40 都是 28 列，在 120x30 是 24 列；而 `doc/specs/tui-guideline.md` §1.2 記載 pane 沒有高度分級、內容比 `scroll.bodyRows` 高時捲動。
5. **與 band／輸入框關係**：
   - AbovePrompt band 貼在輸入框正上方；`fullscreen` 且 ≥110 欄時 pane 停靠右側（docked），否則 inline 在 prompt 上方（`doc/specs/codeforge-mod-ui.md` §2）。
   - mod 自己開（unasked）需 ≥144 欄（`doc/specs/tui-guideline.md` §1.2、`doc/specs/codeforge-mod-ui.md` §2）。
   - **與現行不同**：`mods/live/design.md` §8 記載待決已移至 band shell，側邊 pane 的待決頁僅顯示 `待決在 band 上（d）`；但現行實拍 `current-pane-A-209x50.box.txt` 仍為完整待決頁。

### 最小 mock
```text
╭─ 待決 · 資料 fleet decisions list --json · 10 秒 ──────────────────────╮ [-]
│ （內容區）                                                             │
╰─ ↑↓ 捲卡 · j/k 換張 · Esc  ────────────────────  1 現況  2 待決  9 說明 ─╯
```

---

## 2. 底部錨定與熱區

### 規則
1. **操作區固定在底部**：選項列、按鈕、輸入列、按鍵提示放在 pane 最底部，不隨內容捲動，位置不變（`doc/specs/tui-guideline.md` §13 規則 1；`doc/specs/forge-stats-ux-gate.md` 評分項目 5）。
2. **底部錨點與捲動區**：內容超出時只捲內容區，操作區不動；捲動狀態（如 `↓ 下面還有 71 行`）顯示在內容區邊緣，不放在操作區（`doc/specs/tui-guideline.md` §13 規則 2–3）。
3. **操作順序不可擅改**：待決卡答題區順序為「背景→卡住→選項→建議→補充」，不得因底部貼齊而倒置（`doc/specs/forge-stats-ux-gate.md` 評分項目 14）。
   - **例外（雙欄，PM 裁定 2026-11-23）**：待決頁雙欄（左清單、右詳情）的右欄，選項排在題目之下、背景之前（題目→選項→背景→附註）。理由：操作員要長卡不用捲就看得到選項，這比上述順序重要；單欄的單卡 C 與其他版面仍照原順序。這是已裁定的設計，不算缺陷。
4. **鍵提示寫法**：固定於下框線左側，以 ` · ` 分隔，最後標註焦點狀態與退出鍵（如 `鍵盤在 pane · Esc`）（`current-pane-A-209x50.box.txt` L28）。

### 最小 mock
```text
│ ↓ 下面還有 71 行                                                        │
│ s: 預覽並送出已選 0 張 · 另 21 張未選                                   │
╰─ ↑↓ / PgUp/PgDn 捲卡 · j/k 換張 · 鍵盤在 pane · Esc  ──  1 現況  2 待決 ─╯
```

---

## 3. 色彩語意

### 規則
1. **只用 `PALETTE` 主題鍵**：全部透過 `PALETTE` 角色取用（`warning`、`error`、`success`、`suggestion`、`inactive`、`subtle`、`claude`、`merged`），不用原始 RGB、不自畫背景色、不用 `inverse`（`doc/specs/tui-guideline.md` §3.3 規則 4）。
2. **角色對應**：
   - 要你處理／建議 `★`：`warning`（黃色）（`doc/specs/tui-guideline.md` §3.1；`doc/specs/forge-stats-ux-gate.md` 評分項目 10）。
   - 失敗 `✗`／疑似卡住 `⊘`：`error`（紅色）（`doc/specs/tui-guideline.md` §3.1）。
   - 完成 `✓`：`success`（綠色）（`doc/specs/tui-guideline.md` §3.1）。
   - 執行中 `●`：`suggestion`（藍色）（`doc/specs/tui-guideline.md` §3.1）。
   - 可按操作：`merged`（洋紅）（`doc/specs/forge-stats-ux-gate.md` 評分項目 10）。
   - 結構線／捲軸軌道：`structure` / `subtle`；捲軸滑塊：`here`（`mods/live/design.md` §2）。
   - 閱讀次要文字／未開始：`inactive`（`doc/specs/tui-guideline.md` §3.1；`doc/specs/forge-stats-ux-gate.md` 評分項目 10）。
3. **每列最多一個黃色焦點**：黃色只用在要人處理的那件事，一列最多一個（`doc/specs/tui-guideline.md` §3.3 規則 5）。
4. **不靠單色表意**：狀態的符號、顏色、字樣三者同時出現；要人處理的事除黃色外，還要有粗體或 `▲`（`doc/specs/tui-guideline.md` §10 規則 1、4）。

### 最小 mock
```text
│ ▲ 1. 合併 plan：要不要合進 develop？  ★ 建議 合併 (warning)             │
│ ● 2. 執行中任務 (suggestion)          ✓ 已完成 (success)               │
```

---

## 4. 符號與詞彙

### 規則
1. **符號表（每符號僅一義、單格寬、禁用 emoji）**（`doc/specs/tui-guideline.md` §3.1–3.3）：
   - `▸`：收合／目前焦點指示（`doc/specs/tui-guideline.md` §3.2；`current-pane-A-209x50.box.txt` L6、L12）。
   - `★`：推薦選項（`doc/specs/tui-guideline.md` §3.2）。
   - `✓`：完成（`doc/specs/tui-guideline.md` §3.1）。
   - `○`：未開始（`doc/specs/tui-guideline.md` §3.1）。
   - `↑` / `↓`：領先上游／落後上游，或捲動方向提示（`doc/specs/tui-guideline.md` §3.2、`current-pane-A-209x50.box.txt` L11、L26）。
   - `█`：捲軸滑塊（`mods/live/design.md` §2、`current-pane-A-209x50.box.txt` L12–13）。
   - `┃` / `│`：結構分隔線／捲軸軌道（`doc/specs/tui-guideline.md` §3.2、§3.5）。
2. **頁籤一律寫名稱**：`1 現況  2 待決  3 派工  4 審查  5 花費  6 殘留  7 艦隊  8 待辦  9 說明`（`current-pane-A-209x50.box.txt` L28；`mods/live/locales/zh-TW.json` L108–116）。
3. **zh-TW 用字慣例**：
   - 使用台灣用語（設定、回報、檔案、預設、專案、資料、執行中、已進行），禁用簡體或中國用語（`doc/specs/tui-guideline.md` §4 規則 2）。
   - 白話表達，技術名詞保留英文並與中文空半形空格（`3 個 worktree 要你處理`）（`doc/specs/tui-guideline.md` §4 規則 1）。
   - 符號後空半形 1 格再接數字或字（`⊘ 12`），晶片之間空 2 格（`doc/specs/tui-guideline.md` §3.5）。

### 最小 mock
```text
│ ▸  4. 驗收長卡  · 建議 收到 ★                                        █  │
╰─ ↑↓ 捲卡 · Esc  ────────────────  1 現況  2 待決  3 派工  4 審查  9 說明 ─╯
```

---

## 5. 數字和單位寫法

### 規則
1. **秒數與時間**：經過時間與年齡用精簡格式，最多兩個單位（`8h05m`、`3m`、`10 秒`），年齡加「前」（`{age} 前`）（`doc/specs/tui-guideline.md` §4 規則 6；`mods/live/locales/zh-TW.json` L2）。
2. **計數與卡片位置**：「第 1 到 8 張，共 21 張 · 要你決定 21 張 · 目前第 4 張」（`current-pane-A-209x50.box.txt` L2）；或「第 n 張，共 N 張」（`mods/live/design.md` §8）。
3. **百分比與金額**：百分比取整數（`12%`），金額取兩位小數（`$175.33`），sha 取 8 位（`doc/specs/tui-guideline.md` §4 規則 6）。
4. **tok/s**：單位寫作 `tok/s`（`mods/live/locales/zh-TW.json` L27）。

### 最小 mock
```text
│ 第 1 到 8 張，共 21 張 · 要你決定 21 張 · 目前第 4 張 · 10 秒前        │
│ 進度 75%（3/4） · 速度 42 tok/s                                         │
```

---

## 6. 狀態記號與空／錯誤狀態

### 規則
1. **資料誠實**：數字取自原檔不另算；過舊標年紀；缺資料說缺什麼，不得在缺資料或過舊時看起來像正常狀態（`doc/specs/forge-stats-ux-gate.md` 評分項目 8、Critical 條件）。
2. **過期與無資料**：
   - 過期標示：「資料過舊 · 上次發佈 {at}」或「資料 N 分鐘前」（`mods/live/locales/zh-TW.json` L73–74；`mods/live/design.md` §4）。
   - 無資料：「沒有資料 · {what}」或 `—`（`mods/live/locales/zh-TW.json` L97；`doc/specs/tui-guideline.md` §3.2）。
   - 零張卡：「目前沒有待你回答的問題」（`mods/live/locales/zh-TW.json` L131）。
3. **不得截斷原因**：說明文字與原因必須折行顯示（`wrapLines`），不得以 `…` 截斷原因（`doc/specs/tui-guideline.md` §6 規則 2）。

### 最小 mock
```text
│ 目前沒有待你回答的問題                                                  │
│ 資料過舊 · 上次發佈 12 分鐘前 · watcher 沒有在發佈                      │
```

---

## 7. 窄寬度退化順序

### 規則
1. **退化順序**：寬度不夠時從優先序尾端開始拿掉（裝飾 → 進度／用量 → 正在跑／等待中 → 卡住 → 要人處理），最先保留要你處理的事（`doc/specs/tui-guideline.md` §2.1）。
2. **單列項目縮減順序**：先將本文截到最少 8 格 → 丟掉尾端 ` · meta` → 截最後一段 meta → 繼續截本文；右側 `▸`／`›` 永遠保留（`doc/specs/tui-guideline.md` §6 規則 1）。
3. **絕不能省的項目**：頁籤名稱不能只剩數字，必須保留中文名稱（`1 現況  2 待決`）；判定符號、`ⓘ`、`⚑N` 與待決標示不捨（`doc/specs/tui-guideline.md` §2.1；`current-pane-A-209x50.box.txt` L28）。
4. **CJK 顯示寬度與 `…` 截斷**：中文佔 2 格，一律依顯示寬度（`displayWidth`／`cutProse`）計算，不可依字元數截斷；一般文字右截加 `…`，id 左截保留尾巴（`…2031601-0e0c`）（`doc/specs/tui-guideline.md` §6 規則 3–4）。

### 最小 mock
```text
╭─ 待決 · 10 秒 ────────────────────────────────────────────────────────╮ [-]
│ ▸ 4. 驗收長卡…  · 建議 收到                                           │
╰─ j/k 換張 · Esc  ────────────────────────────  1 現況  2 待決  9 說明 ─╯
```

---

## 8. 鍵盤與焦點

### 規則
1. **Pane 取得鍵盤方式**：透過 `ctrl+x tab`、點擊或 `$.ui.open({ focus: true })` 取得鍵盤；session 開始時自動開的窗不帶 `focus`，不搶輸入框（`doc/specs/codeforge-mod-ui.md` §3；`doc/specs/tui-guideline.md` §13 規則 7）。
2. **一個 Esc 一層**：`Esc` 還給輸入框或關閉當前層（`mods/live/design.md` §9；`current-pane-A-209x50.box.txt` L28）。
3. **鍵衝突規則**：
   - `Button.hotkey` 只能是一個數字或小寫字母；`?` 是 Claude Code 說明鍵，不可當熱鍵（`doc/specs/tui-guideline.md` §14.2）。
   - 取焦的 pane 裡方向鍵用於捲動，不用於在按鈕間移動（`doc/specs/codeforge-mod-ui.md` §3）。
   - 數字鍵在輸入框空著時會直接觸發 band 按鈕，因此 band 選單數字僅作標籤、不設為 hotkey（`mods/live/design.md` §11）。

### 最小 mock
```text
╰─ ↑↓ / PgUp/PgDn 捲卡 · j/k 換張 · 鍵盤在 pane · Esc  ──  1 現況  2 待決 ─╯
```

---

## 9. 新 pane／新頁 brief 必附清單

寫新 pane 或新頁 brief 前，必須逐條對照以下 10 條：

1. **高度與空白**：總高度 ≤ `min(maxRows, floor(rows * 0.6))`，無整列空白補位（`doc/specs/forge-stats-ux-gate.md` 評分項目 3）。
2. **寬度安全**：所有輸出行寬 ≤ `bodyColumns - 2`，80／120／160 欄無水平捲動、無錯位（`doc/specs/tui-guideline.md` §1.1、§7）。
3. **CJK 截斷**：使用顯示寬度（中文 2 格）截斷，文字右截加 `…`，id 左截加 `…`（`doc/specs/tui-guideline.md` §6 規則 3–4）。
4. **底部錨定**：操作區與鍵提示固定在底部，不隨內容捲動（`doc/specs/tui-guideline.md` §13 規則 1）。
5. **操作順序**：沿用既有操作順序（如「背景→卡住→選項→建議→補充」），未經確認不擅改（`doc/specs/forge-stats-ux-gate.md` 評分項目 14）；唯一已裁定的例外是待決頁雙欄右欄（選項在背景前，見第 2 章規則 3）。
6. **色彩語意**：只用 `PALETTE` 主題鍵，每列最多一個黃色焦點，淺色／深色終端皆可讀（`doc/specs/tui-guideline.md` §3.3、§5）。
7. **符號與間距**：只用共用符號表，禁用 emoji，符號後空 1 格、晶片間空 2 格（`doc/specs/tui-guideline.md` §3.1–3.5）。
8. **全正體中文**：標籤、狀態、提示全用台灣正體中文，無簡體字（`doc/specs/tui-guideline.md` §4；`doc/specs/forge-stats-ux-gate.md` 評分項目 6）。
9. **資料誠實**：缺資料或過期明確標示，不偽裝成正常狀態；render 時不跑子行程（`doc/specs/tui-guideline.md` §8；`doc/specs/forge-stats-ux-gate.md` 評分項目 8）。
10. **頁籤與焦點**：頁籤保留中文名稱，支援 `ctrl+x tab` 與 `Esc` 焦點切換，不搶輸入框（`doc/specs/codeforge-mod-ui.md` §3；`current-pane-A-209x50.box.txt` L28）。

---

## 10. 出處索引

- `doc/specs/tui-guideline.md`：終端介面規範（空間合約、寬度分級、符號表、間距、正體中文、深淺主題、截斷、刷新、無障礙、底部錨定、疊層與焦點）。
- `doc/specs/codeforge-mod-ui.md`：Claude Code mod UI 開發依據（能力階梯、鍵盤與焦點限制、`bodyColumns` 預留 2 格）。
- `doc/specs/forge-stats-ux-gate.md`：UX 閘門規範（評分項目 1–14、Critical 條件、必交截圖）。
- `doc/design/ux-gate/pane-batch2/verdicts.md`：pane 第 2 批 UX 閘門判定與各輪結果。
- `autopilot:mods/live/design.md`：O4 band shell 設計與捲動／座標／r5 規則。
- `autopilot:mods/live/locales/zh-TW.json`：zh-TW 實際用字與頁籤名稱。
- `doc/plans/evidence/2026-11-18-pane-batch1`：未找到（未執行 shell 搜尋）。
- `git log` / `git show b84dd83b`（carbon r3）：未找到（未執行 shell 指令）。
- `autopilot:mods/live/pages/*.ts` 及測試檔（v74-r10b 等）：未找到（未執行 shell 搜尋）。
