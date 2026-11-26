# Console v5 實作計畫（209x49 寬版）

- Fleet-Task: 01M8BJCYYF7A1GPZ2KYMC58VD1（PM 01M8BJCY，Plan: plan-first）
- 設計稿：develop 47b7a88 合入的 `doc/design/tri/console/wide/`（稿 b1d004c939f0，overview ffb7e07e7851），68 步在 `out-wide-all/209x49-s<N>/screen.txt`
- 程式碼位置：autopilot plugin `mods/live/`，合併前交 autopilot 審（d-8046739d 分工）
- 驗收：hangar 用 209x49 開新視窗實拍，腳本逐列驗 CJK 寬度與雙主題對比（N+1）；換版後操作員日常使用（N+2）

## 1. 範圍

只做 209x49 這一個尺寸以上的寬版（下稱 W）。120x40、80x24 維持現行 v7.4 版面（稿 `gen_wide_all.py:9`：退化檢查沿用舊稿 `out/`，已過三輪審查），一行都不動。

切換條件（稿沒定，本計畫定）：`cols >= 200 && maxRows >= 17` 且 pane 已由操作員開啟、寬度 ≥ 90，才走 W；否則走現行版面。門檻做成一個常數，加遲滯 ±4 欄，避免拖拉視窗時來回跳。

不做：自己開 pane（owner 裁定 `register.ts:1230`：pane 由操作員開，mod 不主動開）；8 待辦的「退回原因」沒有資料來源，畫成「—」並登 backlog，不自己造資料。

## 2. 已知宿主限制（CC 2.1.331，實測於稿 `gen_wide.py:3-10`）

- 底部槽 `floor(rows/2)` 含 prompt → 209x49 時 band 最多 17 列。這是宿主硬上限；AP 沒有 0.6 之類的自訂比例（只有 askpm 的 0.4、待決清單 0.4 欄寬比，與高度無關）。mod 無法要求更多列（`mods/live/design.md:81`）。`design.md:79` 寫的「49x209: 19」是舊量測，P1 實測後更正。
- pane 寬 `min(floor(cols*0.45), cols-60)` = 94。現行 `openHostPane`（`register.ts:1719`、`:1792`）沒傳 `columns`，P1 先實測：操作員按鍵開 pane 時傳 `columns: 94`，讀回實際寬度。宿主若不吃 `columns`，W 照實際寬度排（≥90 才進 W）。

## 3. 分段（每段一個 AP 分支、一次 autopilot 審、一次合併）

| 段 | 內容 | 主要檔案 | 大小 | 驗收 |
|---|---|---|---|---|
| P1 | 量測與骨架：W 切換常數與遲滯、`columns` 實測、band 17 列框（頂框、12 列兩欄 38/73、`├┴┤`、狀態列、9 頁籤列、底框鍵提示）、`Tab` 循環換頁；頁內容先用現行頁面的清單與回覆區 | `register.ts` 2186-2360、`pages/frame.ts`、`pages/keys.ts`、新 `wide/band.ts` | M | 稿 2-N、1-N、7-N 三步 band 區逐字比對 screen.txt（金檔測試），狀態列不含「卡住」 |
| P2 | pane 94 欄：上半唯讀 30 列（溢出步驟、清單（續）、pin、來龍去脈收合與 `e` 展開、艦隊 12 產線、HIST、本機事件），下半「其他頁」8 列摘要可點、數字鍵跳頁 | `pane.tsx`、新 `wide/pane.ts`、各頁 summary 函式 | L | 68 步 pane 區金檔；`pane_fill.py` 同規則：每步 ≥32/40 列有字 |
| P3 | 待決頁 W：兩欄無捲動、互動狀態 2-I/P/Pc/R/F/Fl0/Fl 換皮、`g` 複製 URL、跨頁狀態 C1–C3 | `pages/decide.ts`、`pages/layout.ts` | M | 稿 2-* 與 C1–C3 共 23 步金檔；現有 decide 測試全綠 |
| P4 | 其他 7 頁 W：`*-M` 清單攤到 band 左欄＋pane「清單（續）」、`*-L` 步驟移 pane、7 艦隊接 forge 每 agent tok/s（`speedview.ts agentsColumn`）、9 說明鍵表（拿掉捲動鍵、加 `e`、`Tab`、`g`） | `pages/{now,dispatch,review,spend,hygiene,fleet,todo,help}.ts`、`forge.ts` | L | 其餘 45 步金檔 |
| P5 | 整合：X／S（缺資料、過期）狀態、真資料跑一輪、`run-tests.sh` 全綠、tmux 209x49 實拍 68 步與稿逐張並排＋差異清單 | 全部 | M | 並排圖交 PM（CLAUDE.md 畫面交件規則）；check3 對實拍 screen 全綠 |

P1 合併後 P2、P3 可平行；P4 依 P2（pane 區塊函式）；P5 最後。

## 4. 金檔怎麼比

稿的 `screen.txt` 是 209x49 純文字（含 CJK 雙寬）。測試在 AP 側渲染同一步的 fixture 資料，切出 band 區（列 25-41、欄 0-114）與 pane 區（列 0-41、欄 115-208）逐列比對；顏色角色另比 `roles.json`。fixture 由 `gen_wide_all.py` 的 `S` 匯出成 JSON（P1 寫匯出腳本，放 codeforge `doc/design/tri/console/wide/export_fixtures.py`，產物複製到 AP 測試目錄並記 sha）。

## 5. 實作與審查流程

- 實作：本席派 dispatch-hetero（local 優先，卡兩輪換 codex）到 AP worktree，一段一條分支 `feat/console-v5-p<N>`；本席跑 `mods/live/run-tests.sh` 與金檔，看過 diff 再交 autopilot 審。
- 合併：autopilot 審過、AP develop 測試綠，由 autopilot 合（AP 是它的 repo）。live 換 pin 照既有流程（PM go，hangar 推）。
- 回退：每段獨立合併；W 由切換常數守，關掉常數（或寬度不到）就回現行版面，舊版面測試不動。

## 6. 時程與證據

- 第一個證據：P1 合併 sha，11-26 11:00Z（PM 排定）。
- P2、P3：11-26 18:00Z；P4：11-27 06:00Z；P5 實拍並排：11-27 12:00Z，之後交 hangar 實拍（N+1）。

## 7. 風險

- 宿主不吃 `columns`：W 改用實際寬度，<90 不進 W（P1 第一件事就量）。
- band 實際只有 17 列，稿已按 17 排；若 tmux 狀態列或 client 尺寸讓 rows<49，`maxRows<17` 時不進 W。
- 金檔逐字比對太脆：只比結構列（框線、欄界、標籤），資料列比「有字／欄寬」，避免改一個字就全紅。
