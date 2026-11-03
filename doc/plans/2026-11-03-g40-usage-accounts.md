# G40 設計：每個 Claude 帳號的用量檢視

> 狀態：設計稿（PM G40，size S，只有文件，沒有程式碼）。2026-11-03。
> 相關：[`doc/specs/codeforge-usage.md`](../specs/codeforge-usage.md)（G36）、[`doc/specs/tui-guideline.md`](../specs/tui-guideline.md)（含 §3.5 間距規則）、autopilot G37（`scripts/claude-account.js`、`scripts/accounts-onboard.sh`）。

## 0. 目標與現況

operator 要的是：每個 Claude 帳號各一份「目前 5h／7d 用量 %、距離重置多久、token 何時到期、一張圖」，整合進 forge-stats。

現況兩個缺口：

1. G36 的配額快照只來自**互動 session 的 statusline**（`src/cli/statusline.rs:401-408` 讀 `rate_limits.five_hour|seven_day` 的 `used_percentage`／`resets_at`，`src/usage/quota.rs` 寫 `<live_base>/codeforge/usage/quota/<alias>.json`）。G37 的 headless 別名不跑 statusline，所以 autopilot 的 `account-pick`（`scripts/lib/account-pick.js:11,19-21`）永遠把它們分到 `class=unknown`。
2. token 到期日沒有任何紀錄。`accounts-onboard.sh` 只寫 token 檔（`:215`）和 `registry.json`。

證據標記：**VERIFIED** = 我在這台機器上跑過或讀過原始檔，附指令／行號；**UNVERIFIED** = 推論或沒測。

## 1. headless 別名怎麼拿到新鮮的 5h／7d %

### 1.1 查到的事實

| # | 事實 | 標記 | 證據 |
|---|---|---|---|
| 1 | `claude -p --output-format stream-json --verbose` 會輸出一則 `type: "rate_limit_event"` | VERIFIED | 在本機預設帳號跑兩次 `claude -p "reply ok" --model haiku --output-format stream-json --verbose --tools "" --strict-mcp-config < /dev/null`，兩次都各出現一則，位於 `assistant` 訊息之後、`result` 之前 |
| 2 | event 的形狀：`rate_limit_info.{status, resetsAt, rateLimitType, overageStatus, isUsingOverage, unifiedWindows.{five_hour?, seven_day?}.{utilization, resetsAt, status}}`；`utilization` 是 0–1 的小數，`resetsAt` 是 unix 秒 | VERIFIED | 實測輸出（本機帳號，數值不寫入本文）：`"unifiedWindows":{"seven_day":{"utilization":0.66,"resetsAt":<unix 秒>,"status":"allowed"}}`；binary 內的 schema 另有 `surpassedThreshold`，`rateLimitType` 列舉含 `five_hour`／`seven_day`／`seven_day_opus`／`seven_day_sonnet`／`seven_day_overage_included`／`overage`（2.1.317 的 bundled JS，以 `strings` 取出） |
| 3 | 數值來自回應標頭 `anthropic-ratelimit-unified-*`；「每個 window 每次觀察都被追蹤，event 在百分比（取整）、重置時間、狀態或門檻變動時送出」；「帳號狀態裡沒有的 window 就不出現」；API key／Bedrock／Vertex session 一律沒有 | VERIFIED | binary 內 `unifiedWindows` 欄位的說明字串 |
| 4 | 兩次探測都**只有 `seven_day`，沒有 `five_hour`**（探測當下這個帳號沒有 5h window 的資料） | VERIFIED（現象）／UNVERIFIED（原因） | 兩次輸出相同。猜測是 5h window 閒置時伺服器不送，沒有做對照測試。含意：schema 必須容許任一 window 為 `null` |
| 5 | `--output-format json`（單一 result 物件）**沒有**配額欄位 | UNVERIFIED（推論） | 我只跑了 stream-json。stream-json 的 `result` 訊息 key 全列為：`api_error_status duration_api_ms duration_ms … total_cost_usd ttft_ms usage uuid`，沒有 `rate_limit*`；`json` 格式輸出的就是這則 result，所以推論它也沒有。未實測 |
| 6 | 被拒絕的請求（usage limit reached）的 result 帶 `rate_limit_info`，形狀和 `rate_limit_event` 相同，含 `resetsAt`、`rateLimitType` | VERIFIED（binary 字串）／UNVERIFIED（未實際撞限額） | binary 內 `rate_limit_info (usage_limit_reached): … in the shape rate_limit_event carries` |
| 7 | 一次探測的成本：haiku、`--tools ""`，`total_cost_usd` 約 0.0016（API 等值），output 8 tokens，cache creation 約 7.9K tokens（主要是 system prompt） | VERIFIED | 探測的 `result` 訊息。訂閱方案不按 token 計費，但會推動 `utilization` |
| 8 | statusline 的輸入 JSON 有 `rate_limits.five_hour.used_percentage`（0–100）、`.resets_at`，七天同理；G36 除以 100 存成 0–1 | VERIFIED | `src/cli/statusline.rs:401-408` |
| 9 | headless 子行程的環境是 `CLAUDE_CONFIG_DIR=<alias config_dir>`、`AUTOPILOT_ACCOUNT=<alias>`，token 只以 `CLAUDE_CODE_OAUTH_TOKEN` 傳入 | VERIFIED | autopilot `scripts/lib/account-env.js:93,95`；`claude-account.js` 檔頭註解 |
| 10 | `claude-account.js run` 已經解析子行程輸出，能處理 stream-json（取最後一則 `type:"result"`） | VERIFIED | `scripts/claude-account.js:379-394`（`parseEnvelope`） |
| 11 | `claude --help` 沒有專門的「查用量」子指令；`claude setup-token --help` 沒有選項 | VERIFIED | 兩個 help 的輸出 |

### 1.2 選項

**(a) 被動：wrapper 把每次 headless 執行的 event 留下來。** 零額外成本、零額外風險。限制：只有呼叫端用 stream-json 時才有 event（事實 5）；大部分 dispatch 用 `json`，所以資料不穩定。可以改成「wrapper 偵測到 stream-json 才記錄」，但這等於資料量由呼叫端的格式決定。

**(b) 主動：每個別名定期探測一次。** 一次 haiku 呼叫約 0.0016 USD 等值（事實 7），每別名每小時一次，一天 24 次，對配額的影響可忽略；但有三個風險：
1. 它會增加 utilization，探測本身讓 5h window 的計時開始這點 **UNVERIFIED**（我沒有對閒置帳號測，也不能對別名測）。
2. 帳號被拒絕（429）時探測仍會回 event（事實 6，UNVERIFIED），但不應該在已 tight（≥90%）時再探測。
3. 探測需要 token，所以只能經 autopilot 的 wrapper，codeforge 不能自己讀 token。

**(c) 其他：** 讀 `claude auth status`／API 查詢。`claude auth status` 子指令存在（binary 字串「Show authentication status」），但它回答的是登入狀態不是用量，未測；直接打 OAuth 用量 API 要讀 token 並依賴未公開端點，不採用。

### 1.3 建議：(a) 為主、(b) 為補，資料落在 codeforge

- **(a)+(b) 都做，但單一寫入點在 codeforge**：事實 3 的 event 是 headless 唯一不需憑證檔、不需猜測的來源。
- **(a) 機會式**：`claude-account.js run`（autopilot）在子行程輸出是 stream-json 時，取最後一則 `rate_limit_event`（已經過 token scrub），以 stdin 餵給 `codeforge usage ingest --alias <alias> --root <hash8>`；失敗一律忽略、不改 exit code、不寫 audit。對呼叫端格式沒有要求。
- **(b) 補洞**：autopilot 新增 `claude-account.js usage-probe [--alias A|--all]`，用該別名的 token 跑上面那個 haiku stream-json 呼叫，**只**印出 `{alias, root, event}` 的 JSON 行（沒有 token、沒有 prompt 內容）。codeforge 的 `usage collect` 在「該別名最新觀察超過 60 分鐘、且沒有 window ≥90%」時呼叫它，每別名最多每 60 分鐘一次。預設**關閉**，由 `codeforge usage accounts --probe`（單次）或設定檔 `usage.probe_aliases = true` 開啟，因為它會花配額。statusline 熱路徑絕不探測（tui-guideline §8-5）。
- **為什麼不只用 (a)**：別名閒置時沒有任何 run，面板會變成「？」，而 `account-pick` 正好最需要知道閒置帳號還剩多少。
- **為什麼不只用 (b)**：沒有 run 的時候探測才是浪費；有 run 時 (a) 免費。
- ingest 的合併規則：以 window 為單位，新觀察覆蓋舊的；`resets_at` 已過的 window 視為未知；每個 window 記 `observed_at_ms` 和來源（`statusline`／`event`／`probe`）。寫入同一個 `quota/<alias>.json`（沿用 `codeforge.quota/1`，欄位不變），所以 G37 的 `account-quota.js` **不用改就能**把 headless 別名從 `unknown` 分成 `ok`／`tight`。

## 2. token 到期

| 事實 | 標記 | 證據 |
|---|---|---|
| `claude setup-token` 產生的是 **1 年**期的 OAuth token | VERIFIED | 官方文件 code.claude.com/docs/en/authentication，「Generate a long-lived token」：`generate a one-year OAuth token with claude setup-token`；binary 內 `$ye=31536000`（秒）、`uAs=Math.round($ye/86400)`、顯示字串 `t.days===365?"1-year":…-day` |
| 指令不儲存 token，只印出來 | VERIFIED | 同一份文件：`It does not save the token anywhere` |
| binary 的 setup-token 處理函式接受 `expiresInDays` | VERIFIED（程式碼）／UNVERIFIED（有沒有對外的 CLI 旗標） | `claude setup-token --help` 沒列出任何選項，所以預設 365 天；不能假設別名都是 365 天，要把實際天數存起來 |
| token 被撤銷或帳號被停用時會提早失效 | UNVERIFIED | 所以到期日只是預估；`claude-account.js` 的 exit 6（auth stop，401）要蓋過日期，直接顯示「失效」 |

**issued-at 存哪**：onboard 時 token 還沒寫入前，`accounts-onboard.sh` 並不知道 token 是什麼時候被 `setup-token` 簽發的（那一步在另一個窗格），所以「貼上時間」比真正簽發時間晚幾分鐘到幾十分鐘，對 365 天可忽略。

- 檔案：`<live_base>/codeforge/usage/accounts/<alias>.json`，0600、目錄 0700，**不含 token**，只有 `{schema:"codeforge.usage-account/1", alias, token_issued_at, token_lifetime_days}`。
- 不放進 autopilot 的 `registry.json`：`account-registry.js:17,145` 的 `ROW_KEYS = ['alias','enabled','priority']` 會對多餘的 key 直接丟 `registry_unknown_key`，加欄位要改 autopilot 的驗證和測試，也會讓舊版 registry reader 整個壞掉。
- 寫入：`accounts-onboard.sh` 在 token 寫入成功（`:215` 之後）呼叫 `codeforge usage account set <alias>`（`--issued-at` 預設現在、`--lifetime-days` 預設 365），best-effort。已經 onboard 的別名：operator 手動 `codeforge usage account set <alias> --issued-at <YYYY-MM-DD>`（G37 的指南 O3 本來就要他記下 lifetime）。沒有紀錄的別名顯示「到期 —」，不猜。
- 警告門檻：**剩 30 天**顯示黃色字樣（`到期 28 天`）；**剩 7 天**與已過期改成 `▲`（要人處理，tui-guideline §3.1）；30 天夠 operator 排一次互動式 `setup-token`（需要瀏覽器授權），7 天是最後期限。依 §3.3-5 黃色每列最多一個，所以多個別名同時進入警告時只畫最近到期的那個。

## 3. 顯示

### 3.1 符號選擇（不新增符號）

| 意思 | 用什麼 | 註 |
|---|---|---|
| 用量 | 數字 `62%`；pane 用量表 `▰▱`（§3.2） | 登記過 |
| 重置倒數 | `◷`＋空格＋精簡時間（`2h05m`） | `◷`（時間）已登記。`↻` **沒有**登記，`↺` 是「審查輪次」，所以不用任何旋轉箭頭 |
| 7 天走勢 | `▁…█` | 登記過 |
| 要人處理（token 過期／失效） | `▲` | 狀態表 need_you |
| 「沒資料」 | `—` | 結構符號 |

### 3.2 forge-stats band 晶片

依 §3.5：晶片內「符號＋1 空格＋值」，晶片間 2 空格，次要資訊 `' · '`。別名是文字不是符號，所以直接接值。

```
艦隊 ▲ 1  ⊘ 2  ● 4 · work 62% ◷ 2h05m  home 18% ◷ 4h40m · Ferris Lv.3
```

- 位置：優先序 §2.1 第 4 級（進度、用量），在艦隊計數之後、寵物之前。用量晶片寫在 forge-stats 這一列（有 autopilot 時是第 2 列）。
- 只畫**限制較緊的那個 window**（沿用 `Snapshot::constrained`）：`%` 取該 window，倒數是該 window 的重置；pane 才畫兩個 window。
- 緊的晶片（≥90%，與 autopilot `TIGHT_THRESHOLD` 一致）：`%` 與倒數用 warning 色，每列最多一個（第二個以後保持預設色）。
- token 剩 ≤7 天或已過期／失效：`▲ work 到期 6d`、`▲ work 失效`，排在用量晶片之前（`▲` 本身就是要人處理）。
- 資料過期（超過 6 小時，`quota.rs` `MAX_AGE_MS`）或 window 已重置：`work —`。
- **窄寬度捨棄順序**（從尾端拿，§2.2 規則）：寵物 → 非緊別名的晶片（保留最緊的一個）→ 所有倒數（`◷ …`）→ 只剩 `work 62%`（別名一定保留，否則看不出是誰）→ 整段用量交回（原樣 `next(e)`）。token 的 `▲` 晶片屬於要人處理，排在艦隊的 `▲`／`✗`／`⊘` 之後、所有用量晶片之前，所以最後才會被拿掉。

### 3.3 `codeforge usage accounts`（完整檢視，≤100 欄）

資料取自收集器（收集 + 讀 `quota/`、`accounts/`、歷史取樣），單次呼叫不探測（`--probe` 才探測並說明會花配額）。

```
帳號用量                                          更新 3m 前（來源 statusline）
──────────────────────────────────────────────────────────────────────────────
work                                                      token 到期 214 天
  5h  ▰▰▰▰▰▰▱▱▱▱  62%    ◷ 2h05m 後重置
  7d  ▰▰▰▱▱▱▱▱▱▱  31%    ◷ 3d 4h 後重置     ▁▂▂▃▃▄▄▅▅▅▆▆▆▆▇▇▇  7 天走勢（7d，6h 一點）

home                                                      token 到期 18 天
  5h  ▰▱▱▱▱▱▱▱▱▱  12%    ◷ 4h40m 後重置
  7d  ▰▰▰▰▰▰▰▰▰▱  91%    ◷ 1d 2h 後重置     ▃▄▅▅▆▆▆▇▇▇▇▇██████  7 天走勢（7d，6h 一點）

lab                                                       ▲ token 失效（401，6h 前）
  5h  —                                      （無資料：最後觀察 9h 前，來源 event）
  7d  —

說明：7d 走勢每個字是 6 小時內的最高值；缺資料的時段畫空白。
```

- 每個別名一個區塊，區塊標題列右邊放 token 狀態。7d 超過 90% 的百分比用 warning 色（一列一個）。
- 無資料用 `—`，並說明原因（過期、沒有觀察、window 不在伺服器回應裡，見事實 4）；絕不補零。
- 寬度 <80 時：先拿掉走勢，再拿掉重置倒數的「後重置」字樣，最後才拿掉量表只留數字；每行依顯示寬度截（§6.3）。
- `--json` 輸出同 `accounts.json` 的內容。
- 走勢資料：`usage.db` 新增一張取樣表，每次寫入 quota 時，同一個 6 小時 bucket 內只保留 `max(pct)`，保留 14 天（歷史以後想要更細才改）；`accounts.json` 帶最近 28 個 bucket。這是**新儲存**，需要 migration；`usage.db` 的 O_NOFOLLOW 等檢查沿用。

## 4. 責任切分與資料契約

**codeforge 擁有**：收集（ingest、probe 排程）、合併、取樣歷史、token 到期紀錄、所有顯示（band、pane、`usage accounts`）。**autopilot 只消費與提供入口**：`claude-account.js` 繼續讀 `quota/<alias>.json`（契約不變）；新增兩件事：run 結束後 best-effort 呼叫 `codeforge usage ingest`，以及 `usage-probe` 子指令。token 永遠不離開 autopilot，codeforge 不讀任何憑證或 token 檔。

### 4.1 檔案

| 檔案 | schema | 寫入者 | 讀取者 | 離開本機？ |
|---|---|---|---|---|
| `<live_base>/codeforge/usage/quota/<alias>.json` | `codeforge.quota/1`（**不變**） | codeforge（statusline、`usage ingest`、probe） | autopilot `account-quota.js`、codeforge | 否 |
| `<live_base>/codeforge/usage/accounts/<alias>.json` | `codeforge.usage-account/1`（新） | codeforge `usage account set` | codeforge | 否 |
| `<live_base>/codeforge/usage/accounts.json` | `codeforge.usage-accounts/1`（新） | codeforge `usage collect` | forge-stats band／pane | 否 |
| `<live_base>/codeforge/usage/summary.json` | `codeforge.usage-summary/1`（**不變**） | codeforge | forge-stats 用量分頁、`usage push` | 是 |

`accounts.json` 欄位（每層驗證，同 G36 的嚴格風格；整數 0..=2^53−1）：

```json
{"schema":"codeforge.usage-accounts/1","generated_at_ms":0,
 "accounts":[{"alias":"work","observed_at_ms":0,"source":"statusline|event|probe",
   "five_hour":{"pct":0.62,"resets_at":0},"seven_day":{"pct":0.31,"resets_at":0},
   "token":{"issued_at":0,"expires_at":0,"state":"ok|warn|critical|expired|revoked|unknown"},
   "spark_7d":[0.2,null,0.3]}]}
```

`pct` 沿用 0–1（`quota.rs`、`account-quota.js` 都是這個尺度；event 的 `utilization` 也是 0–1，事實 2）。`token.state` 由 codeforge 算好，band 不重算門檻。`revoked` 來自 autopilot exit 6 的紀錄（經 `usage account set --state revoked`，best-effort）。`spark_7d` 最多 28 個、元素是 0–1 或 `null`。

### 4.2 為什麼是新檔而不是擴充 `summary.json/1`

- `summary.json` 是「唯一會離開本機的東西」（G36 §summary）；別名走勢、token 到期日屬於帳號資訊，不應該進 `usage push` 的 blob。
- `/1` 的每一層是嚴格驗證（未知欄位整份不顯示），加欄位必須升 `/2`，forge-stats 的 `USAGE_CONTRACT` 和 Rust `contract()` 要同步，舊版 plugin 讀到 `/2` 整個用量分頁會變成「讀不懂」。獨立新檔讓用量分頁不受影響，band 讀新檔、分頁讀舊檔，各自升級。
- 代價：兩個檔案的百分比來源相同（同一次 collect 一起寫），不會分叉；`accounts.json` 的 pct 與 `quota/` 一致由測試守住。
- forge-stats 讀檔的上限沿用（一般檔案、不是 symlink、≤256 KiB），並加 `ACCOUNTS_CONTRACT` 區塊，cargo test 比對（同 `USAGE_CONTRACT` 作法）。

### 4.3 過期（staleness）

- 單一 window：`resets_at` 已過 → 未知（沿用）。
- 整個別名：`observed_at_ms` 超過 6 小時 → `—`（沿用 `MAX_AGE_MS`，與 autopilot 的 6 小時一致）。
- `accounts.json` 本身 `generated_at_ms` 超過 1 小時：band 把用量晶片整段拿掉（不顯示舊數字），pane 提示執行 `codeforge usage`（同現有用量分頁行為）；band 不呼叫 codeforge（§8-1）。

### 4.4 要和 `autopilot--claude` 協調的事

1. `claude-account.js run` 在 stream-json 輸出時 best-effort 呼叫 `codeforge usage ingest`（介面：stdin 一則 event JSON，參數 `--alias`、`--root`；`root` 用 `account-quota.js` 的 `hash8(config dir)`，與 codeforge `alias.rs` 一致）。
2. 新子指令 `claude-account.js usage-probe`（輸出格式見 §1.3），並在它的測試裡確認 stdout 不含 token。
3. `accounts-onboard.sh` 在 token 寫入成功後呼叫 `codeforge usage account set`（codeforge 不存在時略過，不算失敗）。
4. exit 6（auth stop）時 best-effort 呼叫 `codeforge usage account set <alias> --state revoked`。
5. `account-quota.js` 不用改；若要把 `source: probe`／`event` 的資料標示出來（`status` 輸出多一欄），由 autopilot 決定。
6. `codeforge` 對 autopilot 路徑的依賴：由 `AUTOPILOT_ROOT`（或 PATH 上的 wrapper）找 `claude-account.js`；找不到就不探測。

## 待解問題

1. 5h window 閒置時伺服器是否不送（事實 4 的原因）？這決定閒置別名的 `five_hour` 要顯示 `—` 還是 `0%`。需要在 operator 授權下對一個別名做對照測試（本次依規定沒有碰別名）。
2. 探測是否會「啟動」5h window 的計時？如果會，閒置別名被定期探測會把 5h 重置時間往後推，反而影響 `account-pick`。需要先測再決定 (b) 預設開或關（目前預設關）。
3. `--output-format json` 是否真的沒有配額欄位（事實 5 只是推論）？若有，(a) 的涵蓋率會大增，不必要求 stream-json。
4. 探測頻率 60 分鐘、緊的門檻 90%、token 警告 30／7 天是否合適？operator 決定；探測是否預設開也是 operator 決定。
5. 已 onboard 的別名沒有簽發日期：operator 手動補，還是用 token 檔的 mtime 回填？mtime 是 metadata 不是內容，但我沒有碰那個檔，要 operator 同意再做。
6. tui-guideline §3.3-5「黃色每列最多一個、只給要人處理的事」與「緊的配額晶片用 warning 色」衝突：配額緊要不要算要人處理？若不算，改用粗體或 `error` 色？需要 PM／規範持有人裁定。
7. `codeforge usage ingest` 與 `usage-probe` 的呼叫方向：是否接受 codeforge 去呼叫 autopilot（排程在 collect 內），還是改成 autopilot 的 cron 驅動、codeforge 只被動 ingest？後者耦合較少，但 autopilot 必須擁有排程。
8. per-model 週限（`seven_day_opus`／`seven_day_sonnet`）要不要顯示？event 的 `rateLimitType` 列舉有它們，但 unifiedWindows 只列 `five_hour`／`seven_day`／`seven_day_overage_included`（binary 字串），v1 先不做。
9. Team／Enterprise 帳號的 token 簽發在別的組織時（`forceLoginOrgUUID` 對 `setup-token` 不生效，文件 §Restrict login）會不會有不同的有效期？沒有資料，v1 一律當 365 天並讓 operator 可覆寫。
10. 7d 走勢的 6 小時 bucket 與 14 天保留是否夠？要不要同時畫 5h 走勢（24 個 1 小時 bucket）？v1 只畫 7d。
