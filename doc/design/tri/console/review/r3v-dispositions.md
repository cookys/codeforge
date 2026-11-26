# 驗證輪處置（PM 01M89W3F 裁 A）

- codex（r3v-codex.md，審 edea1733b9d4）：FAIL，兩條 blocking，已在 fb40b3ebd7bb 修好（細節見 r3-dispositions.md 的「驗證輪」一節）：
  - 80x24 statusline 的 fleet 段被截掉。
  - 掃描指令裡有 `<autopilot>` 佔位符。
- fable 5.2（claude-native，r3v-fable.md，審 fb40b3ebd7bb）：PASS，5 條都確認已修，沒有 blocking。note 的處置如下：
  - 1(a) 1-M 80x24 上方看不到「執行 19」：80 欄放不下艦隊清單，原本就是這樣設計的。不改。
  - 1(b) 7-S 的「執行中 3」和「fleet 8 lane」單位不同：記為下一輪措辭候選。本輪範圍只限 3 條修正，不改。
  - 2 「主 clone」沒寫絕對路徑：每台主機路徑不同，repo 也不能放本機路徑（private-leaks）。不改。
  - 3 2-P 第一屏看不到「（沒選，不送）」：左邊清單同時顯示 ·3、✓4。記為候選，本輪不改。
  - 4 2-A 在 80x24 時鍵列沒有「s 送出」：超出本輪範圍。交 PM 決定要不要另外開單。
- opencode：沒有產出（question_suspected，exit 1），由 fable 補上第二家。

## PM 看稿後修正（01M89WRV、01M89WW3）

- 80x24 的 2-L2、2-L3、2-A 鍵列補上「s 送出」。80 欄鍵列原本就沒列這個鍵，fable 的第 4 條 note 指出了這點。
- 2-P 送出預覽只列已選的卡，標題改成「3 張（1、2、4）」；沒選的第 3 張不再列入預覽。2-Pc、2-R 已確認，沒有沿用這份資料，所以不用改。
- check3 新增 R8：預覽標題的張數和編號、預覽列出的卡，都必須和清單打勾（✓N）的卡一致。
  - 反向探針 `brief/tools/check3-r8.test.sh`，三種尺寸都跑：原樣要綠；清單少勾一張、或預覽多列一張沒選的卡，都要轉紅。
  - 拿上一版 f7a65c026e86 的 out/ 跑新的 check3，三種尺寸的第 60 步都 FAIL。
