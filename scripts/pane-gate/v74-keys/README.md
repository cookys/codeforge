# v74-keys — v7.4 pane 互動驗收（PM 01M7XAJ7、01M7XARR、01M7XB0B、01M7XBE2）

pane 每次交件都要附上 `run.py` 的輸出（全綠）和擷取圖，PM 才會實按。

```
./run.py <autopilot-checkout> [--out DIR] [--only project|noproject]
```

- 私有 tmux server、209x49，跟操作員一樣開：`claude --plugin-dir <checkout>`，再打 `/live-pane`。
- HOME 是沙盒，只連結 `~/.claude` 和 `~/.claude.json`（登入用）；資料全是假的（`gen-data.py` 的卡片庫和 live 檔、`bin/fleet` 讀 checkout 的 `mods/live/fixtures`）。`bin/fleet` 把每次呼叫記到 `fleet-calls-<情境>.jsonl`。
- 每按一次鍵就存 `NN-<情境>-<步驟>.txt`（純文字）和 `.ans`（彩色），結果逐步寫進 `results.jsonl`；任何一步失敗 exit 1。一步失敗後會清掉誤打進輸入框的字、關掉誤開的對話框，後面的步驟照常判斷。
- 按鍵序列：`/live-pane` → 1–9 每一頁（頁標題、輸入框保持空、分頁列在框的下框線裡、目前這頁反白或有底色、框下緊接 band）→ 2 待決（4 張：fleet 3 張加卡片庫的測試卡、不截斷、內文欄 ≤90、選項間空一行、★ 選項加粗或換色）→ 縮到 100 欄（下框線只有一列、以 ╯ 結尾、目前頁保留名字、沒有獨立的分頁列，再放回 209）→ ctrl+x Tab 拿鍵盤 → b 選（送出 ≥1、輸入框空）→ ↓↓↑（▸ 2、3、2）→ a → m 附註 → 4、2 換頁保留選擇 → 移到測試卡 `d-keys0004` 選 a（長背景全文、有換行）→ s 送出（畫面出現 `[待決回覆]` 和 `d-keys0004`，卡片庫或 fleet 收到）→ Esc 還給輸入框 → 輸入框有字時數字不換頁（平台規定）、ctrl+x Tab 之後才換。
- `noproject` 情境：cwd 不對應任何專案（PM 席的情況），band 要有艦隊、待決、ⓘ、tok/s，不能出現「沒有專案」。
- 送出只用測試卡 `d-keys0004`，絕不用真的卡。

`parse.test.sh`（check-all 會跑）：用存下來的紅燈擷取（autopilot 94100764e）和手寫的綠燈框，檢查 `run.py` 的畫面解析；另用設計稿 ux-v7.4 的 S3 彩色擷取（`fixtures/design-S3.ans`）驗顏色判讀，用舊版設計稿（`fixtures/design-r3-S3.txt`，分頁列獨立一行）當反例。反向探針（沒換行的長句、還有空列卻截斷、★ 那行沒樣式、★ 那行樣式和 a: 一樣、純文字擷取判不出反白）必須判失敗。

共 29 步：project 28 步、noproject 1 步。
