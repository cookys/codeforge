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

## 驗證輪（PM 01M89W3F 裁 A：只驗 r3 的 3 條修正）

- codex（review/r3v-codex.md，對 edea1733b9d4）：
  - 第 3 條「送出預覽」：已修。
  - 第 1 條「fleet 數字」：寬畫面已修；80x24 最底列被截成「fleet…」，數字看不到。
  - 第 2 條「掃描指令」：指令裡的 `<autopilot>` 是佔位符，照抄會被 shell 當成重新導向。
  - 這兩條已在下一個 commit 修好：
    - 80 欄的 statusline 拿掉配額長條和重置時間，fleet 段可以完整顯示，例如「fleet 8 lane 執行中」、「fleet 8 lane ⏶ 11 分鐘前」、「fleet —」。
    - 指令改成實際路徑：`node ~/projects/autopilot/scripts/repo-residue-sweep.js scan --repo .`。
- opencode：沒有產出。dispatch 回報 question_suspected，agent exit 1，也沒有 commit。第二家改由 claude-native fable 5.2 審，材料直接貼在 brief 裡。
- 為什麼第 2、3 輪只有一家審：r2 的 agy 認證失敗（auth_failed），r3 的 grok 額度用完（quota_exhausted），驗證輪的 opencode 異常結束。
