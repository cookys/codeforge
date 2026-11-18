# pane 第 2 批 UX 閘門判定

閘門規則：doc/specs/forge-stats-ux-gate.md（carbon 和 astra 都 ≥8 且沒有 Critical，最多 3 輪）。實作在 autopilot，codeforge 拍截圖、送審、寫處置。

| 輪 | autopilot sha | carbon | astra | 結果 | 檔案 |
|---|---|---|---|---|---|
| 1 | daedf7fbd（tree 同 13a61c0d3，截圖用 13a61c0d3 拍） | 4（C4 M7 N3） | 4（C3 M3 N5） | 未過 | r1-carbon.json、r1-astra.json、r1-dispositions.md、r1-keyprobe.log、r1-packet.md（送審包） |
| 2 | cc9bde3db（feat/pane-batch2-r4） | 5（C3 M5 N4；no_artifacts pass） | 5（C2 M8；no_artifacts fail） | 未過 | r2-carbon.json、r2-astra.json、r2-dispositions.md、r2-keyprobe.log、r2-packet.md（carbon 第一次 20 分鐘逾時無輸出，30 分鐘重派成功） |
| 3（最後一輪） | 91bd6682d（feat/pane-batch2-r5） | 7（C0 M3 N3；no_artifacts pass） | 5（C2 M2 N1；no_artifacts fail） | 未過，交 PM 裁決 | r3-carbon.json、r3-astra.json、r3-keyprobe.log（6/6 ok）、r3-packet.md（附 autopilot 的 W1 說明） |

B56（80x24 縮窄後數字鍵打進提示列）：第 1 輪 6/6 提示列乾淨；按 0 時鍛造頁捲在底部、自帶舊分頁列（處置 K1），第 2 輪 keyprobe 改看分頁列選中哪一頁，6/6 ok 才寫「B56 pass」。

B56 pass（第 2 輪 keyprobe 6/6 ok：分頁列選中的頁都對、提示列全空，r2-keyprobe.log）。

第 3 輪後兩家仍有的問題（交 PM，見 fleet 01M7N307 回報）：W1 分頁列下那列空白歸屬未定（astra Critical；carbon pass；autopilot 以 host 程式碼靜態證據說是 host 外框，沒量真機 bodyRows）；120x30 band 修復指令截成「autopilot …」（astra Critical、carbon Major）；額度頁 120x30／160x40 新加的欄名讓判定硬截成「浪費 1」（實為 12%，兩家 Major）；80x16 待決頁只剩操作列（兩家 Major）；160x40 band 少「艦隊」區名（U1 未修）。

PM 裁決 A 後的點驗（autopilot 98cf58d03，沒有評審，r4-spot.md）：W1 真機量測 bodyRows＝正文全高、分頁列在最後一列，下面那列屬 host，結案；第 2–5 項逐條 ok；keyprobe 6/6 ok。可以照 rollout 流程 pin。
