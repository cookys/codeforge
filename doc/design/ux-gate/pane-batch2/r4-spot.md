# pane 第 2 批：PM 裁決 A 之後的點驗（第 4 版，沒有評審）

- 受驗 sha：autopilot fix/pane-r4 @ 98cf58d03490（PM 01M7Q8H2 選 A：W1 先補證才能 pin，2–5 由 autopilot 修，codeforge 用截圖逐條點驗並重跑 keyprobe）
- 執行：`NO_REVIEW=1 bash scripts/pane-gate/run-gate-b2.sh <ap>` → 截圖 98 檔、本席卡畫得出來、keyprobe 6/6 ok
- 截圖（純文字）：`r4-shots/`；探針每次 render 一行，依數值計數後存在 `r4-shots/probe-summary.tsv`；keyprobe：`r4-keyprobe.log`

## W1：分頁列下那一列歸誰（真機量測，探針 AUTOPILOT_MODS_PANE_PROBE）

pane 第 1 列是 host 標題（✕），正文從第 2 列起，共 bodyRows 列。

| 版面 | placement | bodyRows | renderedRows | 最後一列是分頁列 | 正文列 | 正文之後那列 |
|---|---|---|---|---|---|---|
| 160x40 | dock（88 欄） | 33 | 33 | 是 | 2–34 | 35，在 host 給的高度之外 |
| 120x30 | dock（70 欄） | 23 | 23 | 是 | 2–24 | 25，同上 |
| 80x24 | inline | 6 | 6 | 是 | — | 圓角外框 |
| 80x20／80x16 | inline | 4／3 | 4／3 | 是 | — | 同上 |

mod 每次都畫滿 host 給的列數，分頁列就在最後一列。astra 說的空白列不屬 mod，結案。縮放瞬間有 1 個畫格是舊尺寸的 bodyRows（70 欄配 33），下一格就正確。

## 第 2–5 項

| # | 結果 | 截圖：列 |
|---|---|---|
| 2 band 修復指令不截斷 | ok | S5-stale-10min-p1-120x30、-p4-120x30：第 24 列「autopilot status runs --watch」完整 |
| 3 額度數字不截斷 | ok | S1-full-p4-120x30 第 11–13 列「浪費 91%／90%／75%」；160x40 同；80x24 行內欄名完整；S1-full-p1-80x16「✗ 額度紅 2（C 池、D 池）· ? 3」 |
| 4 80x16 待決頁看得到卡 | ok | S1-full-p2-80x16 第 9 列「▸ 測試機第二槽插電 第 1 張／共 3 張 · 1 天」 |
| 5 160x40 band 有「艦隊」區名 | ok | S1-full-p{0..4}-160x40 第 33 列「艦隊 ✗ 2  ⊘ 4」 |

本席可答卡（S6-own-p2-80x24）照常畫出；keyprobe 2 3 9 1 0 4 全部 ok。
