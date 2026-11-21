# v74-keys — v7.4 pane 互動驗收（PM 01M7XAJ7、01M7XARR、01M7XB0B、01M7XBE2）

pane 每次交件都要附上 `run.py` 的輸出（全綠）和擷取圖，PM 才會實按。

每個情境的沙盒都會設 `AUTOPILOT_V74_KEYLOG=<輸出目錄>/keylog-<情境>.jsonl`。autopilot r6 以後的 pane 會在這個檔逐行記下每個 v7.4 輸入 handler 收到的事件（不含輸入框文字），紅燈回報時連同擷取一起附上。

`--token-alias <alias>`：沙盒 session 改用 `~/.config/autopilot/accounts/<alias>/token` 這個池裡現有的權杖登入（檔案權限要 0600），不用預設登入。預設登入的池碰到週限額時，[待決回覆] 那一輪永遠不會回應，所以要換池。權杖在 tmux 裡的 shell 讀進 `CLAUDE_CODE_OAUTH_TOKEN`，值不進 argv，也不進閘門輸出或 log；不複製權杖，也不新登入。`CLAUDE_CONFIG_DIR` 會先 unset。

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

共 30 步：project 29 步（r10 起送出前多一步 send-preview：s 預覽，r11 起 y 確認送出）、noproject 1 步。

## UX 審查（PM 01M7XARR、01M7XBE2）

`run.py` 全綠之後才跑：

```
./ux-review.sh <run.py 的輸出目錄> <autopilot sha>     # 用 Bash 的 run_in_background 跑
```

- `ux-review.py` 把這次實按的逐鍵序列、每一步的擷取（只留 pane 的框到畫面底部，連續空白寫成 ⟨N⟩，相同的畫面只指向第一次出現的步驟）、從彩色擷取讀出的樣式（目前分頁、每個選項），和設計稿 `doc/design/ux-gate/v74/`（S1–S10、窄寬度下框線）組成評審包 `ux-review.md`。驗收是紅的就拒絕組包（`--allow-red` 只給演練用）。組包前跑跟 `build-b2.py` 一樣的私有名稱檢查，命中就不寫檔。
- 兩家異質引擎並行當嚴格 UX 總監：第二家（`V74_UX_SECOND`：預設 grok-4.7，PM 01M7Y0F8 在 agy 的 carbon 額度用完時改派；`carbon` 換回 gemini-4-carbon-high）和 gpt-6-astra（codex）。評四項：好不好用（各動作按幾個鍵）、資訊密度、與設計稿一致、閱讀舒適度；最後附 json（overall、axes、findings、verdict）。
- `V74_UX_SECOND=opus`（PM 01M7Y4HJ）：第二家改用 claude-native 的 claude-opus-5-5，結果要標明「同家意見」，過關門檻不變。token 席位需要 `CLAUDE_CODE_OAUTH_TOKEN`，所以設 `V74_UX_TOKEN_ALIAS=<池 alias>`：腳本在子 shell 裡從 `~/.config/autopilot/accounts/<alias>/token` 讀進 env，值不進 argv 也不進 log；alias 只從環境帶，不寫進 repo。`V74_UX_REF=glm` 另跑本地 GLM 當參考，不計分（目前被 cn_model_dispatch_barred 擋下，要等 PM 開豁免）。
- `ux-review.py --verdict` 讀兩份輸出：兩家都 overall ≥ 8 且沒有 Critical 才 exit 0；分數不夠或有 Critical exit 1；沒有可讀的 json exit 2。結果在 `ux-verdict.txt`。
- `ux-review.test.sh`（check-all 會跑）：不叫引擎，驗紅燈拒絕、組包內容、私有 email 擋下、四種判定。
