# 第一輪修正回應與證據

任務：`01M7KRP3A17982XHW7VPJTXSGD`。起點 commit：`3d05eb7`；工作分支：`feat/fs-pane-v2-r1`（派發的 worktree）。本次未啟動 UX 評審，分數與放行由操作員決定。

歷史證據定位（第二輪修訂）：`cf3f9a7` 已移除 `round-1-evidence/`，以下 receipt 路徑指向上一輪提交 `6f08a98772a8a1eacd0e835eee61bb851d23ffa5` 的歷史檔案，不是目前工作樹的連結。可用 `git show 6f08a98772a8a1eacd0e835eee61bb851d23ffa5:doc/design/ux-gate/2026-11-17-pane-v2/round-1-evidence/<檔名>` 查閱；本輪不恢復日誌或 receipt 檔案。

## 1. 任務重述與驗收

依操作員的高度及去重裁定修正 C1、C2、M1–M4、K1–K4。S1–S3 全部八條卡住／離線（含 pm）首屏可見，S3 不超過 14 列；原因、時長、備註不可被低優先內容擠掉；缺資料與過舊必須明示；配色對應可按性與嚴重度。畫面使用正體中文，fixture 使用假名，不新增 brief 欄位。所有指定命令須 exit 0，截圖與本文件一併提交。

## 2. 定位與修改前的精確原文

以下原文可用 `git show 3d05eb7:<檔案>` 重現，行號是修改前版本。

| 檔案與行號 | 原文 |
|---|---|
| `plugins/forge-stats/hooks/brief-view.ts:50` | `const subjects = new Set(decisions.map(i => i.subject.display))` |
| `plugins/forge-stats/hooks/brief-view.ts:54` | `subjects.add(a.who)` |
| `plugins/forge-stats/hooks/brief-view.ts:62` | `if (subjects.has(i.subject.display) \|\| (i.ids.task_id !== undefined && tasks.has(i.ids.task_id)) \|\| seen.has(i.subject.display)) return false` |
| `plugins/forge-stats/hooks/overview-view.ts:46` | `const state = i.text + ' · ' + elapsed(i.since_ms, now)` |
| `plugins/forge-stats/hooks/overview-view.ts:73` | `if (cols < 110) return [...lead, s(cut(detail, rest), tone)]` |
| `plugins/forge-stats/hooks/overview-view.ts:79` | `if (a.note && inner > width(fields)) fields.push(s(cut(a.note, inner - width(fields))))` |
| `plugins/forge-stats/hooks/overview-view.ts:121` | `const head: PaneRow[] = status.length >= 2 ? [row('status', '狀態', statusSegs, cols)] : []` |
| `plugins/forge-stats/hooks/overview-view.ts:151` | `const minimum = { blocked: Math.min(3, blocked.length), pools: Math.min(1, pools.length), assignments: Math.min(1, assignmentRows.length) }` |
| `plugins/forge-stats/hooks/pane-controls.ts:20` | `if (o.decisions > 0 && !o.autopilotHint) control('d', '開啟方式')` |

## 3–4. 逐項紅測、修正、驗證

下列每組都執行 `claude plugin test plugins/forge-stats`。JSON receipt 記錄真正的子命令 exit code、測試日誌名稱及原始輸出 SHA256；日誌略去大量 `UX_SHOT`／fixture JSON（完整畫面另存 shots）。check-all 的產物暫存路徑在提交版日誌中替換為 `<build-temp>`，避免帶入本機路徑；原始輸出 SHA256 保留，未使用 hook override。每個行為修正均在下一項實作前驗證。

| 發現 | 修改與最終檔案:行號 | 紅測 → 綠測 receipt（exit code） |
|---|---|---|
| C2／carbon C1 | item 只按 `id` 去重；待決排除卡住須同 `subject.display` 且同 task/card ID；交辦不過濾卡住，不同線共用 task 不排除。pm 逾時任務保留。`plugins/forge-stats/hooks/brief-view.ts:47`；設計 §1.3 `doc/design/forge-stats-pane-v2.md:45`。 | 01 紅 `round-1-evidence/01-c2-red.json` 1 → 01 綠 `round-1-evidence/01-c2-green.json` 0 |
| C1 | 狀態併頁籤；每區先留一列，再依卡住、各池、交辦分配。S3 為 1+8+2+1+2=14 列。各池其餘項目合計摘要，交辦同列 `+2 件`。`plugins/forge-stats/hooks/overview-view.ts:149`、`:169`、`:197`、`:217`；八條明確斷言 `plugins/forge-stats/tests/ux-shots.test.tsx:113`。 | 02 紅 `round-1-evidence/02-c1-red.json` 1 → 02 綠 `round-1-evidence/02-c1-verified.json` 0 |
| M1 | offline 移除 title；原因先取寬度，下一步只用剩餘寬度；80 欄保留「疑似 courier 連線問題」。`plugins/forge-stats/hooks/overview-view.ts:48`；`plugins/forge-stats/tests/ux-shots.test.tsx:109`。 | 03 紅 `round-1-evidence/03-m1-red.json` 1 → 03 綠 `round-1-evidence/03-m1-green.json` 0 |
| K1／carbon M1 | 先預留完整時長，再截原因並接上時長；S1–S3 每條皆驗。`plugins/forge-stats/hooks/overview-view.ts:51`；`plugins/forge-stats/tests/ux-shots.test.tsx:117`。 | 04 紅 `round-1-evidence/04-k1-red.json` 1 → 04 綠 `round-1-evidence/04-k1-verified.json` 0 |
| M2 | 備註先保留寬度，依序縮方案、bar、重置文字；120 欄完整保留範例備註，80 欄保留開頭；長備註保留至少八格與省略號。`plugins/forge-stats/hooks/overview-view.ts:67`；`plugins/forge-stats/tests/overview.test.tsx:86`。 | 05 紅 `round-1-evidence/05-m2-red-isolated.json` 1 → 05 綠 `round-1-evidence/05-m2-green.json` 0 |
| M3 | `d 決策卡`；按下顯示「待決卡在 autopilot pane：輸入 /autopilot」，窄版仍保留全部有效鍵位與說明。`plugins/forge-stats/hooks/pane-controls.ts:20`、`:35`；`plugins/forge-stats/tests/pane-v2.test.tsx:28`。 | 06 紅 `round-1-evidence/06-m3-red.json` 1 → 06 綠 `round-1-evidence/06-m3-green.json` 0 |
| M4 | short 或週用量 ≥95% 的 bar 用 warning；不夠敘述 warning，用不完與閱讀文字 inactive，各池無 error。`plugins/forge-stats/hooks/overview-view.ts:72`、`:91`、`:94`、`:115`；`plugins/forge-stats/tests/overview.test.tsx:101`。 | 07 紅 `round-1-evidence/07-m4-red.json` 1 → 07 綠 `round-1-evidence/07-m4-green.json` 0 |
| K2／carbon M3 | 缺檔／損毀使用「摘要」及指定診斷。`plugins/forge-stats/hooks/overview-view.ts:155`。原本已讀 `$HOME/.local/state/fleet/brief.json` 並忽略未知 key，保留實作並加測 pools、lines、retired_seats 與巢狀未知 key；未新增資料模型欄位。`plugins/forge-stats/hooks/brief-view.ts:6`、`:25`；`plugins/forge-stats/hooks/register.tsx:275`；`plugins/forge-stats/tests/brief.test.tsx:54`。 | 08 紅 `round-1-evidence/08-k2-red.json` 1 → 08 綠 `round-1-evidence/08-k2-green.json` 0；未知欄位／路徑測試在紅階段已通過 |
| K3／carbon M4 | 分頁、按鍵與目前頁籤 inactive；目前頁籤 bold；merged chrome 皆有 action，`p` 提示也可按；無 Button 時摘要提示 subtle；各池與交辦主體 bold，空槽 subtle。`plugins/forge-stats/hooks/pane-controls.ts:8`、`:54`；`plugins/forge-stats/hooks/overview-view.ts:90`、`:136`、`:222`；`plugins/forge-stats/hooks/register.tsx:703`、`:733`。 | 09 紅 `round-1-evidence/09-k3-red.json` 1 → 09 綠 `round-1-evidence/09-k3-verified.json` 0 |
| K4／carbon m1 | 狀態皆保留來源名與「過舊」，如「摘要過舊 6m」「各池過舊 6m」；三種寬度驗證。`plugins/forge-stats/hooks/overview-view.ts:145`；`plugins/forge-stats/tests/overview.test.tsx:158`。 | 10 紅 `round-1-evidence/10-k4-red.json` 1 → 10 綠 `round-1-evidence/10-k4-green.json` 0 |
| M2 對齊補測 | 備註寬度由全池共用，避免 smith1 的名稱欄與 bar 位置／長度跟其他池不同。`plugins/forge-stats/hooks/overview-view.ts:78`、`:194`；`plugins/forge-stats/tests/overview.test.tsx:167`。 | 12 紅 `round-1-evidence/12-m2-alignment-red.json` 1 → 12 綠 `round-1-evidence/12-m2-alignment-green.json` 0 |
| C1 溢位補測 | 真的放不下時，省略提示併在最後一筆，七個槽顯示七筆、只隱藏一筆；附上 j/k。`plugins/forge-stats/hooks/overview-view.ts:130`、`:178`；`plugins/forge-stats/tests/overview.test.tsx:184`。 | 13 紅 `round-1-evidence/13-c1-overflow-red-exact.json` 1 → 13 綠 `round-1-evidence/13-c1-overflow-verified.json` 0 |
| C1／M2 單列池補測 | 80 欄只分到一列各池時，仍保留 smith1、百分比、八格備註與剩餘分類數。`plugins/forge-stats/hooks/overview-view.ts:105`、`:210`；`plugins/forge-stats/tests/overview.test.tsx:193`。 | 14 紅 `round-1-evidence/14-c1-single-pool-red.json` 1 → 14 綠 `round-1-evidence/14-c1-single-pool-green.json` 0 |
| 行尾空白 | 去除輸出行尾的多餘欄位 padding，保留欄間對齊。`plugins/forge-stats/hooks/overview-view.ts:41`；`plugins/forge-stats/tests/overview.test.tsx:203`。 | 15 紅 `round-1-evidence/15-row-padding-red.json` 1 → 15 綠 `round-1-evidence/15-row-padding-green.json` 0 |

中途失敗亦保留：02-c1-green（舊狀態位置與不再溢位的捲動情境）、04-k1-green（courier 原因少一字）、05-m2-red（初次含前一項失敗，後以 isolated 重現）、09-k3-green（新增單空白 Text）、13-c1-overflow-green（原測試已捲到底，無 j）皆 exit 1，詳見同名 receipt，未作成功證據。

既有斷言沒有刪除或降低門檻：卡住可見下限從 ≥3 收緊為精確 8；無來源摘要改用新措辭但維持恰好一列；C2 將錯誤的 subject 去重預期改為精確 ID 清單。舊文案斷言同步使用指定新文案。捲動測試增加四筆假名資料／降低測試終端高度，仍要求 j/k/u/a/i、上下界、重新調整尺寸與底部位置；過舊斷言改檢查主體加頁籤狀態，保留 warning 要求。

## 5. 最終驗證、變更清單與保留範圍

最終命令全部 exit 0，彙整見 [驗證收據](round-1-verification.json)，完整逐次記錄見 索引 `round-1-evidence/index.md`。

| 最終指定命令 | exit code | 證據 |
|---|---|---|
| `claude plugin test plugins/forge-stats` | 0（212 pass、0 fail） | receipt `round-1-evidence/final-plugin.json` |
| `UX_SHOTS_DIR=<repo>/doc/design/ux-gate/2026-11-17-pane-v2/shots python3 plugins/forge-stats/tests/export-ux-shots.py` | 0（十四份） | receipt `round-1-evidence/final-shots.json` |
| `env -u NO_COLOR ./scripts/check-all.sh` | 0 | receipt `round-1-evidence/final-check-all.json` |

截圖涵蓋 S1–S6、淺深主題共十四份，雜湊與列數見 [manifest](shots/manifest.json)。S3 的 14 列與八筆卡住可在 [80x24](shots/S3-80x24-dark.txt) 逐列核對；[160x40](shots/S1-160x40-dark.txt) 供操作員放行檢視。

每個修改檔案與一行原因見 [本輪逐檔清單](round-1-changed-files.md)。

未做：未啟動 UX 評審、未部署／合併／推送；未改 band、Rust、autopilot repo 或使用者設定目錄；未新增 brief 欄位。正式交辦來源尚無 operator-origin 欄位，因此維持正式輸入空陣列，截圖與測試使用假交辦。互動終端外框高度、自訂色盤與上線放行仍由操作員確認，本次只交測試 harness 的畫面與色盤證據。
