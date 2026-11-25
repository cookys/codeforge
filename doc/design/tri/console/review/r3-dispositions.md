# 畫面第 3 輪處置（審稿 sha db3a5bf967d6；最後一輪）

審查：codex（review/r3-codex.md，FAIL，3 條 blocking）。grok 本輪沒有產出，dispatch 回報 engine_unavailable（quota_exhausted）。第 2 輪 agy 也沒有產出（auth_failed）。因此第 2、3 輪實際上只有 codex 一家審，沒有達到兩家異質審查。已到 3 輪上限，交 PM 裁定。

| 編號 | 等級 | 處置 | 怎麼改 |
|---|---|---|---|
| 1 | blocking | 修 | 最底 statusline 的 fleet 段改用該步 pane 底列艦隊的同一個數字。1-N 兩處都是 8，1-M 兩處都是 19；過時和讀不到的寫法沿用第 2 輪。 |
| 2 | blocking | 修 | 6-S 原本寫的 `codeforge hygiene --scan` 並不存在，改成 autopilot 實際的掃描指令：在主 clone 跑 `node <autopilot>/scripts/repo-residue-sweep.js scan --repo .`（autopilot@23fd23038 的 scripts/repo-residue-sweep.js:177 有 scan 子命令）。 |
| 3 | blocking | 修 | 送出預覽逐張列出：題目全文、選的字母、選項標籤和說明，沒選的卡標「沒選，不送」，第 4 張另列附註全文。超出回覆區時會捲動，並提示還有幾列；y 確認一直留在底部鍵列。 |
| note | note | 維持 | 其餘 note 都是確認第 1、2 輪已修，沒有新的 blocking。 |

檢查：check3 204/204 通過，R6 反向探針通過，render 204 張 BAD 0。
這 3 條修正本身還沒有經過異質審查。
