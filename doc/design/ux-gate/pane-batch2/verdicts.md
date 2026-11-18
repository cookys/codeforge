# pane 第 2 批 UX 閘門判定

閘門規則：doc/specs/forge-stats-ux-gate.md（carbon 和 astra 都 ≥8 且沒有 Critical，最多 3 輪）。實作在 autopilot，codeforge 拍截圖、送審、寫處置。

| 輪 | autopilot sha | carbon | astra | 結果 | 檔案 |
|---|---|---|---|---|---|
| 1 | daedf7fbd（tree 同 13a61c0d3，截圖用 13a61c0d3 拍） | 4（C4 M7 N3） | 4（C3 M3 N5） | 未過 | r1-carbon.json、r1-astra.json、r1-dispositions.md、r1-keyprobe.log、r1-packet.md（送審包） |
| 2 | cc9bde3db（feat/pane-batch2-r4） | 5（C3 M5 N4；no_artifacts pass） | 5（C2 M8；no_artifacts fail） | 未過 | r2-carbon.json、r2-astra.json、r2-dispositions.md、r2-keyprobe.log、r2-packet.md（carbon 第一次 20 分鐘逾時無輸出，30 分鐘重派成功） |

B56（80x24 縮窄後數字鍵打進提示列）：第 1 輪 6/6 提示列乾淨；按 0 時鍛造頁捲在底部、自帶舊分頁列（處置 K1），第 2 輪 keyprobe 改看分頁列選中哪一頁，6/6 ok 才寫「B56 pass」。

B56 pass（第 2 輪 keyprobe 6/6 ok：分頁列選中的頁都對、提示列全空，r2-keyprobe.log）。
