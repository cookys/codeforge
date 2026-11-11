# G40 設計：配額檢視（codeforge 只負責顯示）

> 狀態：設計稿 v3（PM G40 重新定界，size S，只有文件，沒有程式碼）。2026-11-04；PM 同日裁定已折入（見「PM 裁定」一節）。v1 是 `98c7a8b`，那一版把「收集」放在 codeforge，已被取代（見 §1）。
> 相關：[`doc/specs/codeforge-usage.md`](../specs/codeforge-usage.md)（G36）、[`doc/specs/tui-guideline.md`](../specs/tui-guideline.md)（§2 優先序、§3.1／3.2 符號表、§3.5 間距、§8 刷新與成本）。
> 資料契約（唯一依據）：autopilot `develop` @ `591f5918c`，`project/plans/2026-11-03-g40-agent-lane-quota.md`，主要是 §18（單一契約、單一儲存、本機 mirror、reporter）與 §20（以 `(provider, alias)` 為 key 的 `fleet.quota/1`、manifest、可見度 L0–L3）；10 個 provider 的能力實測在同 commit 的 `docs/plans/evidence/2026-11-03-g40-agent-lane-quota/capability-matrix.md`。契約與本文衝突時以契約為準，並回頭修本文。

## 0. 定界

operator／PM 的決定（不在本文討論）：

- **使用者線**（statusline、codeforge，給人看）與**代理線**（dispatch，給 `account-pick` 用）是兩條路。
- **收集移出 codeforge**：autopilot 的 G37 wrapper 負責觀察與探測，courier 的 reporter（零 token、常駐）把觀察正規化成同一份契約，推到 fleet hub 的 quota 表；hub 是儲存，本機 `<live_dir>/quota/` 是它的 mirror。
- **codeforge 只讀、只畫**：讀 mirror（hub 的本機副本），不收集、不探測、不排程、不寫任何配額檔。
- **statusline 快照只是其中一個來源**：codeforge 現有的互動 session 快照（`codeforge.quota/1`，G36）繼續照舊寫出；reporter 讀它、轉成契約記錄、`source: passive`。沒有第二條管線。

## 1. 被取代的 v1 設計（保留紀錄，不再實作）

v1（`98c7a8b`）的下列部分作廢，由 autopilot 契約的對應項目取代：

| v1 項目 | 為什麼作廢 | 取代者（契約 @ `591f5918c`） |
|---|---|---|
| `codeforge usage ingest`（wrapper 把 event 餵進來） | 收集不屬於 codeforge | §5.1 wrapper passive capture 寫 mirror；§18.2 reporter |
| `codeforge usage collect` 的 probe 排程、`--probe`、`usage.probe_aliases` | 會花 token，必須經 wrapper 才能碰憑證；排程是代理線的事 | §5.3、§19（probe 實驗決定預設）、§20.3 manifest 的 `tinyprobe` |
| 寫 `quota/<alias>.json`、`accounts.json` 聚合檔、`accounts/<alias>.json`（token 到期紀錄） | 單一儲存、單一契約；codeforge 不再寫這些 | §18.1／§20.2 `fleet.quota/1`（record 自帶 `token_issued_at`／`token_expires_at`）；§5.6 token meta 由 autopilot 擁有 |
| `usage.db` 取樣表與 7d 走勢 | 取樣只為收集服務；契約沒有歷史欄位 | 無（走勢 v1 不做，見「PM 裁定」第 5 項） |
| `usage-probe`、`account set`、`--state revoked` 等 autopilot 協作項 | 方向反了：現在是 autopilot 擁有它們 | §13／§20.6 phases P2、P2b、P3、P4 |

**不受影響、照舊**：G36 的 `summary.json`、`codeforge.quota/1` 快照的寫入（statusline 熱路徑，使用者線）、`usage push`。

### 1.1 留下來的已驗證事實（背景）

這些事實 v1 查過、契約也引用（契約 V1–V3、V8、V9），顯示端要靠它們理解來源資料的樣子。

| 事實 | 標記 | 證據 |
|---|---|---|
| `claude -p --output-format stream-json --verbose` 會輸出 `type:"rate_limit_event"`；`rate_limit_info.unifiedWindows.{five_hour?, seven_day?}.{utilization, resetsAt, status}`，`utilization` 是 0–1，`resetsAt` 是 unix 秒；window 不在帳號狀態裡就不出現（兩次實測都只有 `seven_day`） | VERIFIED | v1 §1.1 事實 1–4（本機實測，數值不入文）；契約 §4 V1–V3 |
| statusline 輸入 JSON 的 `rate_limits.five_hour\|seven_day.{used_percentage, resets_at}`：`used_percentage` 是 0–100，`resets_at` 是 epoch 秒。這是**來源端**的尺度，顯示端不會直接看到，reporter 在邊界除以 100 一次 | VERIFIED | `src/cli/statusline.rs:401-408`；契約 §18.1 轉換規則 |
| `claude setup-token` 簽發的是 **1 年**期 token；實際天數可能不同，所以到期日只是預估 | VERIFIED（官方文件）／UNVERIFIED（提早撤銷） | 官方 authentication 文件「Generate a long-lived token」；契約 §20.3 `token_lifetime` |
| 被拒絕的請求（usage limit reached）的 result 帶形狀同 event 的 `rate_limit_info` | VERIFIED（binary 字串）／UNVERIFIED（未實際撞限額） | v1 事實 6 |
| 缺的 window 是 `null`，不是 0；來源沒回就不能當作沒用 | VERIFIED | 契約 V3、§20.2 |

## 2. 顯示要處理的資料（契約對照）

### 2.1 記錄與 key

顯示的最小單位是一筆 `fleet.quota/1` 記錄，**key 是 `(provider, alias)`**，不是 alias，也不是 harness（契約 §20.1）：opencode 底下花的 glm 帳號，記在 `glm`。

```json
{"schema":"fleet.quota/1","provider":"claude","alias":"work","visibility":"L3",
 "windows":[{"kind":"rolling_5h","utilization":0.62,"used":null,"limit":null,"unit":null,
             "resets_at":0,"source":"passive","observed_at":0}],
 "host_role":"agent","token_issued_at":0,"token_expires_at":0}
```

顯示端要吃的欄位與單位（全部以契約為準）：

| 欄位 | 單位／值域 | 顯示端怎麼用 |
|---|---|---|
| `utilization` | 0–1 小數，收 `[0, 1.5]`；大於 1.5 契約視為 `quota_invalid` | 乘 100、四捨五入成 `%`；>1.0 也畫成 `100%`。顯示端**不做 0–100 判斷**，0–100 只存在於 statusline 來源端 |
| `resets_at` | epoch **秒**整數，或 `null` | 格式化成倒數（`2h05m`、`3d 4h`）；`null` 寫「重置時間未知」，不是 0 |
| `observed_at` | epoch **毫秒**，每個 window 各一個 | 算資料年齡；v1 的 record 級 `observed_at` 已不存在 |
| `kind` | `rolling_5h`／`weekly`／`daily`／`monthly`／`credits`／`rpm`／`tpm`，每個 record 每種最多一個 | 決定標籤與畫法（§3） |
| `used`／`limit`／`unit` | 非負數，兩者同時有或同時無；`unit` ∈ `requests`／`tokens`／`credits` | 有才畫「剩餘數字」，沒有就只畫百分比 |
| `source` | `passive`／`endpoint`／`probe`／`estimated`／`429` | `estimated` 與 `429` 有專屬畫法（§4）；其餘三種顯示端不區分，只在 `--json`／詳細檢視註明 |
| `visibility` | `L0`–`L3`，是**最近一次觀察實際達到**的等級（劣化會如實下降） | 決定能不能畫量表（§4） |
| `token_issued_at`／`token_expires_at` | epoch 秒或 `null` | 到期顯示（§5）；`null` = 未知，不畫 |
| `host_role` | 字串 | **只在同一 `(provider, alias)` 從多於一台 host 送達時才顯示**（pane 區塊標題列；band 不畫）；單一 host 時不畫（PM 裁定 6；多 host 如何進 mirror 見 §9 待解 7） |

### 2.2 provider 清單

契約列出的 10 個營運者自有帳號（契約 §20 開頭；能力等級取自 capability matrix）：

| provider（契約用字） | matrix 觀察到的等級 | 預期的 window 組 |
|---|---|---|
| `claude`（claude code） | L3 | `rolling_5h`、`weekly` |
| `codex` | L3（僅 weekly 觀察到） | `weekly`；另有 credits 餘額，但契約沒有只帶餘額的欄位（§9 待解 2；顯示規則已定，見 §3） |
| `minimax` | L3 | `rolling_5h`、`weekly` |
| `glm` | L2 | kind 對照是推論（未驗證），所以要標 `estimated`／推論 |
| `qoder` | L2（vendor 文件，未實測） | `credits`，重置時間未知 |
| `kimi` | L1 | 只有 429；文件說有 `weekly`／`rolling_5h`／`monthly` |
| `opencode` | L1（opencode-go） | 只有 limit 錯誤文字 |
| `qoder-cli-cn` | L0 | 無 |
| `agy` | L0 | 無 |
| `grok` | L0 | 無 |

**顯示端不寫死這張表（PM 裁定 7：provider 清單永不寫死）**：上表只是 matrix 實測的範例。契約 §20.8 Q12 明說清單只是範例、不是 registry，`qoder-cli-cn`、`opencode`、`agy` 到底是 provider 還是 harness 由 matrix 逐列決定。顯示端對任何符合 `^[a-z0-9-]{1,24}$` 的 `provider` 一視同仁，畫它的字面名稱；window 組完全由記錄的 `windows[]` 決定，不由 provider 推測。

## 3. 各 window 怎麼畫

標籤（固定欄寬 6 格，CJK 字佔 2 格）：

| `kind` | pane 標籤 | band 標籤 | 畫法 |
|---|---|---|---|
| `rolling_5h` | `5h` | `5h` | 量表 `▰▱`（10 格）＋ `%` ＋ `◷` 倒數 |
| `weekly` | `週` | `週` | 同上。契約的 `weekly` 不保證恰好 7 天（codex 觀察到 `window_minutes=10080`），所以不叫 `7d` |
| `daily` | `日` | `日` | 同上 |
| `monthly` | `月` | `月` | 同上 |
| `credits` | `額度` | `額度` | 有 `used`／`limit`：量表＋「剩 N unit」＋已用 %；有已用比例但無數量：量表＋「已用 %」；**只有餘額、沒有分母（codex）：只寫「剩 N unit」，不畫量表、不畫 %**。**量表永遠畫已用比例**，和其他 window 同方向；「剩」是另外一個數字 |
| `rpm`／`tpm` | `rpm`／`tpm` | 不畫 | 只在 pane 的次要行畫（短時節流，契約不列入 ok／tight 判定）；沒有量表，只寫 `1.00` 這類數值或 `100%` 加 `◷` |

只有餘額的 `credits` 沒有 `utilization`，不參與 band 的「最緊 window」挑選（§6.1），只在 pane 畫；契約目前無法承載這種 window（§9 待解 2，已交 autopilot），規則先寫定，欄位到位即生效。

倒數用 `◷`（時間，§3.2 已登記）＋1 空格＋精簡時間；**不用** `↻`／`↺`（`↺` 是審查輪次）。`resets_at` 已過的 window 不畫，視為「已重置，等新觀察」（契約 §5.2 規則 1）。

## 4. 可見度、估算、過期、429：每一種怎麼畫

這一節是顯示端最重要的規則。總原則：**沒有量到的東西不畫量表**；量表只代表一個有來源的 `utilization`。

| 狀況 | 判定（契約依據） | pane（完整檢視） | band 晶片 |
|---|---|---|---|
| **L0** | `visibility: L0`、沒有記錄、或 `windows: []`（§20.4）。已登記但從未觀察的別名由 reporter 補一筆 L0 記錄（PM 裁定 4），顯示端不自己列別名 | 一行 `無資料`，下一行小字說明（沒有任何可見來源）。**不畫量表、不畫 0%** | 不畫個別晶片；併入尾端的 `無資料 N` 計數晶片（可捨棄） |
| **L1** | `windows` 只有 `source:"429"` 的項目（§20.4） | `429 冷卻`；有 `resets_at` 時接 `◷ 倒數後重試`，沒有時寫「重試時間未知」。**不畫量表** | `provider:alias 429 冷卻`，有 `resets_at` 時加 `◷ 倒數` |
| **L2** | 有 `utilization`，部分 `resets_at: null`（§20.4） | 有值的 window 畫量表；`resets_at` 是 `null` 的 window 寫「重置時間未知」 | 同 L3，沒有倒數就省略 `◷ …` |
| **L3** | 全部 window 有 `utilization` 與 `resets_at` | 量表＋倒數 | 最緊 window 的 `%`＋倒數 |
| **`source:"429"` 的 window** | 契約規定 `utilization` 固定 1.0，是合成值（§20.2） | 不論 record 等級，這個 window 一律畫 `429 冷卻`，**不畫成 100% 的量表**（因為 1.0 是合成的，不是量測；PM 裁定 3） | 同 L1 的 band 寫法 |
| **`estimated`** | `source:"estimated"`，本機計數對方案上限（§20.2：只能讓別名「看起來更緊」，不是量測） | 量表照畫，百分比前面加字樣 `估`（`估 41%`），pane 底下一行註明「估 = 本機計數，不是供應商回報」 | `估 41%`；倒數照常 |
| **過期（年齡）** | window 的 `observed_at` 距今 > 顯示門檻 | 該行尾端加年齡（`· 2h10m 前`） | 晶片尾端加年齡（`2h10m 前`） |
| **過期（硬上限）** | 距今 > **6 小時**（契約 §5.2 規則 2，與 `MAX_AGE_MS` 同）或超前 > 5 分鐘（`quota_invalid`） | 當作無資料：`—  過期（最後觀察 7h 前，未計入）`，**不畫量表** | 不畫個別晶片，併入 `無資料 N` |
| **credits 類型** | `kind:"credits"`（§3 表） | `額度` 行：量表＋「剩 N unit」（有 `used`／`limit` 時）＋已用 %；重置時間常未知 | `額度 剩 3,100`；沒有 `used`／`limit` 時 `額度 已用 38%` |
| **緊**（≥ 0.90，與 autopilot `TIGHT_THRESHOLD` 一致） | 該 window 的 `utilization` ≥ 0.90 | `%` 後接字樣 `緊`（`91% 緊`）；**不改顏色**（PM 裁定 1；§6.3） | 同左 |

要點：

- **「估」不是新符號。** PM 裁定 2：估算一律用中文字 `估`，不登記 `≈`，不新增任何 glyph。
- **過期的顯示門檻契約沒有給。** 契約只有 6 小時的硬上限（超過就丟）。「幾分鐘開始顯示年齡」是顯示端自己的決定：**30 分鐘**（PM 裁定 3）。理由：reporter 不送 heartbeat、不偽造新鮮度（契約 §18.2），所以閒置別名的資料本來就會變舊，這是正確行為，要讓使用者看見年齡而不是隱藏。
- 同一個 record 內各 window 的 `observed_at` 可能不同，年齡**按 window 算**，不按 record 算。
- 一個 record 內全部 window 都因 `resets_at` 已過或超過 6 小時而被丟掉時，整個別名當 L0 畫（`無資料`），說明寫「已重置或過期，等新觀察」。

## 5. token 到期

契約的 record 自帶 `token_issued_at`／`token_expires_at`（epoch 秒，未知時為 `null`；§20.2、§20.3 `token_lifetime`）。顯示端因此**不需要**自己存到期日（v1 的 `accounts/<alias>.json` 作廢）。

- `token_expires_at` 為 `null`：不畫任何到期字樣（契約：`days: null` 表示未知，並抑制到期警告）。
- 剩餘 > 30 天：pane 區塊標題列右側畫 `token 到期 214 天`；band 不畫。
- 剩餘 ≤ 30 天、> 7 天：同上，數字改成實際天數；band 仍不畫（要人處理的才進 band）。30 天對應契約 §5.6 的 `accounts.token_warn_days` 預設值。
- 剩餘 ≤ 7 天或已過期：`▲` 晶片（`▲ claude:home token 到期 6 天`、`▲ claude:home token 已過期`），這是 §3.1 的 `need_you`，唯一允許黃色（warning）的地方；排在所有用量晶片之前。
- 7／30 天門檻：契約把 30 天放在 autopilot 設定 `accounts.token_warn_days`，7 天只寫在 §5.6 文字；codeforge 先用常數 30／7；7 天已交 autopilot 列為契約常數（§9 待解 4），契約落定後改由 `QUOTA_CONTRACT` 同步。
- **缺口**：契約 §5.6 說「記錄到的 401 蓋過日期，`status` 顯示 `invalid`」，但這個狀態只出現在 autopilot 的 `status`，**沒有進 `fleet.quota/1`**。顯示端因此無法畫「token 失效」（§9 待解 1，已交 autopilot）；v1 的 `▲ token 失效` 暫時拿掉。

## 6. 畫面

### 6.1 forge-stats band 晶片

依 §3.5：晶片內「符號＋1 空格＋值」，晶片之間 2 空格，次要資訊 `' · '`，band 的組與組 `' │ '`。別名與 provider 是文字不是符號，直接接值。

```
艦隊 ▲ 1  ● 4 · claude:work 5h 62% ◷ 2h05m  claude:home 週 91% 緊 ◷ 1d 2h  kimi:main 429 冷卻 ◷ 1h20m  無資料 3 · Ferris Lv.3
```

- **位置**：§2.1 第 4 級（進度、用量），在艦隊計數之後、寵物之前。有 autopilot 時是第 2 列，沒有時是第 1 列；沒有任何可畫晶片時原樣 `next(e)`，不畫空白列（§2.2）。
- **不畫 `host_role`**（PM 裁定 6，只在 pane 標題列顯示）。
- **每個 `(provider, alias)` 一個晶片**，畫它「最緊的 window」：在 `rolling_5h`／`weekly`／`daily`／`monthly`／`credits` 中取 `utilization` 最大者（與契約 pick 的 `used` 定義一致，`rpm`／`tpm` 不參與）。晶片內寫 window 標籤，讓人知道是哪個。
- **標籤**：`provider:alias`。窄寬度時先拿掉 `provider:` 前綴（保留 alias），前提是畫面上別名不重名；重名時保留前綴（不同 provider 可以有同名別名）。
- **晶片排序（保留優先序，由高到低）**：① `▲` token 晶片 → ② 緊（≥0.90）→ ③ `429 冷卻` → ④ 其餘依 `utilization` 由高到低 → ⑤ `無資料 N` 計數晶片（L0、過期、已重置的別名合計）。
- **窄寬度捨棄順序**（從尾端拿，§2.1 規則）：寵物 → `無資料 N` → 所有年齡字樣（`2h10m 前`）→ 所有倒數（`◷ …`）→ 由排序尾端往前拿掉整個晶片，直到只剩一個（最緊的）→ 只剩 `alias 62%`（別名一定保留）→ 整段用量交回。`▲` token 晶片屬於要人處理，最後才拿掉；不拆開符號和數字（§3.5 規則 4）。
- **資料來源與成本**：只讀 mirror（§7）；render 不跑子行程、不碰 hub（§8-1）。

### 6.2 `codeforge usage accounts`（完整檢視，≤100 欄）

單次呼叫、只讀 mirror，不探測（沒有 `--probe`）。**沒有走勢圖（sparkline）**：v1 不做，見「PM 裁定」第 5 項。下面是示意（數字與別名皆為虛構，glm 那行是為了示範 `estimated`，真實的 glm kind 對照仍待 matrix 驗證；`claude:work` 標題列的 host 示範「同一 key 來自多台 host」；codex 的 `額度` 行示範只有餘額、無分母，待契約欄位到位）。

```
配額檢視                                       更新 12s 前（reporter · mirror 7 筆）
──────────────────────────────────────────────────────────────────────────────────
claude:work  L3 · host agent、desk · token 到期 214 天
  5h    ▰▰▰▰▰▰▱▱▱▱  62%   ◷ 2h05m 後重置
  週    ▰▰▰▱▱▱▱▱▱▱  31%   ◷ 3d 4h 後重置

claude:home  L3 · ▲ token 到期 6 天
  5h    ▰▱▱▱▱▱▱▱▱▱  12%   ◷ 4h40m 後重置
  週    ▰▰▰▰▰▰▰▰▰▱  91% 緊   ◷ 1d 2h 後重置

codex:main  L3
  週    ▰▱▱▱▱▱▱▱▱▱  17%   ◷ 3d 後重置 · 2h10m 前
  額度  剩 12.50 credits

glm:main  L2
  日    ▰▰▰▰▱▱▱▱▱▱  估 41%   重置時間未知

qoder:main  L2
  額度  ▰▰▰▰▱▱▱▱▱▱  剩 3,100 credits · 已用 38%   重置時間未知

kimi:main  L1 · 429 冷卻
  429   冷卻中   ◷ 1h20m 後重試

agy:main  L0 · 無資料
  沒有任何可見的配額來源

minimax:main  L3 · 過期
  —     過期（最後觀察 7h 前，未計入）

說明：估 = 本機計數，不是供應商回報；緊 = 已用 ≥ 90%。年齡超過 30 分鐘才顯示（· 2h10m 前）。
```

- 每個 `(provider, alias)` 一個區塊，標題列：`provider:alias`、`L<n>`（暗色）、`host <role>`（**只有同一 key 來自多於一台 host 時才出現**，host 之間用 `、`）、`token 到期`；`▲` 與到期字樣之間照 §3.5 規則 1 隔 1 格。
- 區塊順序：先 `▲`、緊、429，其餘依 provider、alias 字母序；L0 與過期放最後。
- `rpm`／`tpm` 的 window 列在該區塊最下面，不畫量表。
- 每個 window 行：標籤（6 格）、量表、`%`（含 `估`／`緊` 字樣）、倒數、年齡；欄與欄之間 2 空格，倒數 `◷ ` 後 1 空格。
- 寬度 <80：先拿掉年齡，再拿掉「後重置」，再拿掉量表只留數字；每行依顯示寬度截（§6）。<60 時每個 `(provider, alias)` 縮成單行。
- `--json`：輸出顯示端解讀後的清單（已套用 §4 的過期與丟棄規則），不是 mirror 檔案的原樣轉存；欄位名跟契約一致。
- 讀不到、schema 不符的 mirror 檔：不顯示該列，區塊最底下寫 `N 筆無法讀取（schema 或內容不符）`。

### 6.3 顏色（§3.3 規則 4、5）

- 黃色（warning）**只**給 `▲` 的要人處理（token ≤7 天、已過期）。
- 緊（≥0.90）、`429 冷卻`、`估`、過期、無資料：不用黃色。緊用字樣 `緊` 標示，`無資料`／過期／`429 冷卻` 用 `inactive`，`估` 用預設文字色。
- 這與 v1 的「緊的配額用 warning 色」不同：PM 裁定 1 已定案，緊只用 `緊` 字樣，黃色維持專屬於要人處理。

## 7. 資料存取

顯示端只讀契約定義的 **mirror**（契約 §18.4、§20.1）：

| 項目 | 值 |
|---|---|
| 路徑 | `<live_dir>/quota/<provider>/<alias>.json`（hub key 為 `quota/<provider>/<alias>`；舊的 `accounts/quota/<alias>.json` 已被契約改掉，沒有出貨過，不讀） |
| schema | `fleet.quota/1`。其他版本（含 `/2`）整份不顯示，算「無法讀取」，不嘗試相容 |
| 誰寫 | wrapper（passive、probe，寫入 mirror）與 reporter（從 hub 拉回、把 statusline 快照轉換後推送）。**codeforge 絕不寫這個目錄** |
| 對 hub | 顯示端不連 hub。reporter 是唯一的 hub client；hub 不存在時 mirror 單獨運作（契約 §18.4 `hub=absent`），顯示端行為不變 |

讀取規則（遵守 §8 刷新與成本，數字在設計時寫明）：

1. **render 路徑不跑子行程、不啟動 reporter、不呼叫 `claude-account.js`**。statusline 一次性行程只讀檔；forge-stats 在計時器裡讀，不在 render 裡讀。
2. 讀法與 G36 `summary.json` 相同的嚴格風格：一般檔案、不是 symlink、`O_NOFOLLOW`、單檔 ≤ **4 KiB**（契約要求 payload <1 KiB）、整個 mirror 最多 **64 個 provider 目錄、256 筆記錄**，超過只計數不顯示（§8-3）。
3. TTL：forge-stats 每 **15 秒**重讀一次，且只在 summary 列有讀者時（心跳新鮮時）才讀（§8-2）；statusline 依自己的 `refreshInterval`，每次只讀幾個小檔。
4. 逐筆驗證：`schema` 必須是 `fleet.quota/1`；路徑裡的 `provider`／`alias` 必須與內文一致（契約 N24，不符就丟棄）；`kind` 必須在列舉內、同 record 內不重複；`utilization` ∈ `[0, 1.5]`；`used`／`limit`／`unit` 成組。任一不符：丟棄該筆（契約 N25），計入「無法讀取」。
5. **缺檔行為**：
   - `<live_dir>/quota/` 不存在，或存在但沒有任何有效記錄：band 不畫用量晶片；pane 寫「尚未收到配額資料（由 autopilot 的 reporter／wrapper 寫入）」。**不報錯、不補零、不去生成資料。**
   - 單一檔案不存在：該別名不出現。**codeforge 不自己維護別名清單**（PM 裁定 4）：別名清單就是 autopilot G37 的 registry，reporter 對每個已登記但沒有資料的別名補一筆 L0 記錄，所以真正的 L0 別名會以 L0 記錄的形式出現在 mirror，顯示端照 §4 畫 `無資料`。
6. **過期行為**：套用契約 §5.2 規則 1、2（window 的 `resets_at` 已過就丟；record 的觀察年齡 > 6 小時就丟，超前 > 5 分鐘視為無效），再套 §4 的 30 分鐘年齡顯示。不做 per-window 合併（合併是 reader／reporter 的事，mirror 已是合併後的結果）。
7. **過渡期**：reporter 還沒上線時（契約 P2b 之前），`<live_dir>/quota/` 不存在。此時顯示端**維持現況**（G36 的 `codeforge.quota/1` 快照 chip，只有互動別名），不會把兩個來源疊在一起。mirror 一出現就只讀 mirror（契約 §18.1：「mirror 出現後沒有任何顯示端直接讀 `codeforge.quota/1`」）。這條過渡規則已獲 PM 核准（PM 裁定 3）。
8. TS（forge-stats）與 Rust（statusline、`usage accounts`）各一份讀取器，契約區塊用 `QUOTA_CONTRACT` 常數同步，cargo test 比對 TS 與 Rust 的 schema 名、window 列舉、單位、上限值（同 G36 `USAGE_CONTRACT` 作法）。

## 8. 責任切分與要協調的事

| 誰 | 負責 |
|---|---|
| autopilot | 觀察（wrapper passive／probe）、manifest 與 adapter、reporter、mirror 的寫入、token 日期的紀錄與 401 狀態、pick 讀取 |
| fleet | hub 的 quota 表、key `quota/<provider>/<alias>`、跨 host 衝突規則 |
| codeforge | 讀 mirror、畫 band 晶片與 `usage accounts`、statusline 快照照舊寫出（一個來源）、`QUOTA_CONTRACT` 同步測試、不變式測試（§3.5 規則 3） |

向 autopilot、fleet 提的要求在 §9 標「協調」或「已交 autopilot（PM 轉）」。

## PM 裁定（2026-11-04）

設計稿已獲接受，並附下列裁定；每項已折入上文，原待解項目已移出 §9。

1. **緊**：配額緊一律用字樣 `緊`，不上色；黃色維持專屬於要人處理（`▲`）。
2. **估**：估算保留中文字 `估`，不登記 `≈`，不新增任何 glyph。
3. **照原案核准**：過渡規則（mirror 出現前維持 G36 快照晶片，出現後只讀 mirror）、年齡顯示門檻 30 分鐘、`429` window 一律畫 `429 冷卻` 的覆蓋規則。
4. **從未觀察的別名**：別名清單就是 G37 registry；reporter 對每個已登記但沒有資料的別名補一筆 L0 記錄；codeforge 自己不維護別名清單（PM 轉 autopilot）。
5. **走勢圖（sparkline）**：v1 不做（沒有歷史的擁有者）；hub 表出現後再重新評估。本文所有示意畫面與介面描述均不含走勢圖。
6. **host_role**：只在同一 `(provider, alias)` 從多於一台 host 送達時才顯示。
7. **provider 清單**：永不寫死；顯示端畫記錄的字面名稱，window 組由 `windows[]` 決定。

## 9. 待解問題

1. **（已交 autopilot（PM 轉））** token 失效／被撤銷的狀態沒有進 `fleet.quota/1`（契約 §5.6 的 `invalid` 只在 `status`）。顯示端無法畫「token 失效」；契約需新增 `token_state`（`ok`／`invalid`／`unknown`）或等價欄位。
2. **（已交 autopilot（PM 轉））** codex 的 credits 只有餘額、沒有分母（matrix codex 欄 5），契約的 `used`／`limit` 成組規則無法表達。**顯示規則已定**（§3）：只有餘額時只畫「剩 N unit」，不畫量表、不畫 %；待契約允許只帶餘額的 window。
3. **（已交 autopilot（PM 轉））** `<live_dir>`（autopilot `resolveLiveDir()`）與 codeforge 現有的 `<live_base>/codeforge/...` 是否同一個根；顯示端需要確定的 mirror 絕對路徑解析規則。
4. **（已交 autopilot（PM 轉））** token 7 天門檻要成為契約常數（30 天已是設定 `accounts.token_warn_days`），否則顯示端的常數會與設定脫鉤。
5. **（已交 autopilot（PM 轉））** 契約欄位名與 codeforge 檔案名稱漂移（契約 Q11：`resets_at`／`utilization` vs `pct`）。顯示端鎖定 `fleet.quota/1`，遇到其他名字就整份不顯示；契約凍結版本號時通知 codeforge。
6. **（顯示端自定）** `rpm`／`tpm` 只在 pane、不在 band；契約 §20.8 Q13 本來就在問其他 lane 顯不顯示。
7. **（協調：fleet）** 同一個 `(provider, alias)` 從多台 host 觀察時，mirror 路徑是一個 key 一個檔；多 host 資料如何呈現（多筆、合併或 `host_role` 清單）要由 hub 的衝突規則（契約 §5.4）決定，顯示端才知道何時算「來自多於一台 host」（PM 裁定 6 的觸發條件）。
8. **（待 matrix）** glm 的 kind 對照、kimi 的 reset、qoder-cli-cn／agy／grok 的 L0 何時能升等：升等時 band／pane 不需要改程式，但範例畫面與 §2.2 表需要跟著更新。

## 10. 契約對齊（v4，2026-11-04；autopilot develop 1f221ae63）

autopilot 的 G40R2 已收斂，PM 指定以 `project/plans/2026-11-03-g40-agent-lane-quota.md` 在 1f221ae63 的版本為準（§5 記錄形狀、§22 mirror view）。下列各項**取代**上文對應段落；上文保留作為決策紀錄。§9 待解 1–5、7 由此結案。

| # | 上文說法 | 契約現況（§） | 顯示端做法 |
|---|---|---|---|
| 1 | mirror 檔是一筆 `fleet.quota/1` 記錄（§7） | 檔案是 `fleet.quota-view/1`：`{schema, namespace, hub_offset, rows[]}`，每列 `{writer_courier, host_role, observed_at_hub, skewed, record}`（§22.2）；另有 `quota/namespace.json` | 解析 view，逐列驗 `record`；namespace 守門：view 與 `namespace.json` 的 namespace 都非 null 且相等才用 peer 列，否則只留本機列 |
| 2 | 「不做 per-window 合併，mirror 已合併」（§7 規則 6） | 「Readers merge all rows per window with 5.2」（§22.2） | 照 §5.2 實作合併：只比年齡（本機列 `now - t`，peer 列 `(now + hub_offset) - t`，`t = observed_at_hub - (B - x)`）；> 6 h、超前 > 300 s、`resets_at <= now + hub_offset` 丟棄；每個 kind 取年齡最小的量測 window，平手本機勝、兩 peer 平手取 `utilization` 大者；`estimated` 另算，只能拉高；refusal 取年齡最小的未過期者 |
| 3 | `observed_at` 是毫秒（§2.1） | 全部 epoch **秒**；`*_ms` 欄位、> 1e11 的值都無效（§6） | 一律秒；倒數 helper 用毫秒時乘 1000 |
| 4 | 429 是 `source:"429"`、`utilization` 1.0 的合成 window；L1 = 只有這種 window | 429 是記錄層的 `refusal: {observed_at, retry_at, reason}`，絕不是 window；L1 = `windows: []` + `refusal`（§20.4、§21.8） | `429 冷卻` 從 `refusal` 畫；冷卻到 `retry_at`，沒有時 `observed_at + 30 分`（G37 的冷卻終點顯示端拿不到）；過期 = 冷卻結束或 `observed_at + 6 h` 取早者；只有過期能清除，較低的 window 不能。window `source` 是 `429`／`none`／`statusline` 的記錄整筆丟棄 |
| 5 | token 失效無法表達（§5 缺口、§9-1） | `token_state`（`ok`／`invalid`／`unknown`，缺省 = `unknown`）＋`token_state_at`（§21.1） | `invalid` 畫 `▲ provider:alias token 已失效`，排第一、黃色、不畫量表與百分比；不被 6 h 年齡規則淘汰，只有狀態改變才清除 |
| 6 | codex 只有餘額時欄位不足（§9-2） | `credits` window 可帶 `remaining`、`utilization: null`（§21.2） | 規則照 §3 不變：只寫「剩 N unit」；`remaining == 0` 是唯一的「空」訊號；不參與最緊 window 挑選 |
| 7 | `<live_dir>` 解析未定（§9-3） | `<live_dir>` = autopilot 解析器，其他 repo 的讀取端「must reproduce it exactly」（§21.3，對照向量 N37） | forge-stats 另寫 quota 專用解析器，照 §21.3 順序；mod 若無法判斷 `/tmp` 是否在 RAM，一律不採 `/tmp` 候選（契約：只在 `/tmp` 找到的不算 mirror），並在回報中註明 |
| 8 | 30／7 天先用常數（§9-4） | 契約常數 `{token_warn_days: {notice: 30, urgent: 7}, observation_max_age_hours: 6}`，提議檔 `scripts/lib/quota-contract.json`，「the display never hard-codes them」；該檔屬 autopilot P3，**尚未存在** | 常數集中在 `QUOTA_CONTRACT` 區塊（含 `MAX_AGE_S` 21600、`FUTURE_SKEW_S` 300）；檔案出現後補同步測試（BACKLOG） |
| 9 | 欄位名漂移（§9-5） | 單一拼法；`pct`／`resetsAt`／`five_hour`／`seven_day`／`*_ms` 一律拒收（§21.5、N38） | 拒收整筆，不改名 |
| 10 | 多 host 的觸發條件未定（§9-7、PM 裁定 6） | 每個 key 一個 view，`rows[]` 依 `writer_courier` 分列（§22.2） | 「來自多於一台 host」= 通過 namespace 守門與年齡過濾後，仍有 ≥ 2 個不同 `writer_courier`；pane 標題列列出這些列的 `host_role`（取記錄內的值，不取列上的副本） |
| 11 | `utilization` ∈ `[0, 1.5]` | 存下的值 ≤ 1，> 1 的記錄「rejected by every reader」；`clamped: true` 可選（§6） | 範圍改 `[0, 1]` |
| 12 | 單檔 ≤ 4 KiB | 契約沒給讀取端上限；一個 view 可有多列 | 單檔 ≤ 32 KiB，最多 64 個 provider 目錄、256 個 view |
| 13 | L0 = `windows: []` | 從未觀察的別名是 placeholder：`visibility L0`、`windows []`、記錄層 `source: "registry"`，不受 6 h 規則淘汰（§21.6） | 分兩種：placeholder 畫「從未觀察」；有過 window 但全被丟掉的畫「過期或已重置，等新觀察」；只有 token 狀態、從沒有過 window 的（§21.6 shape c）畫中性的「沒有用量資料」（合併結果的 `hadWindow`／`had_window` 區分；band 兩者都併入 `無資料 N`）（B45） |
| 14 | 等級取記錄的 `visibility` | `visibility` 是每列各自的；合併後的等級契約沒定義 | 顯示端從合併結果推：還有 window 且都有 `resets_at` → L3；有 `resets_at: null` → L2；沒有 window 但有未過期 refusal → L1；其餘 L0 |

不變的部分：30 分鐘年齡顯示門檻、緊 ≥ 0.90 只用字樣、`估`、過渡規則（mirror 出現前維持 G36 快照晶片）、沒有走勢圖（契約 §21.7 也禁止從 mirror 讀數自行生成歷史）、`rpm`／`tpm` 只在 pane。

**本輪範圍**：forge-stats（band 晶片與 用量 分頁），用測試資料開發；真實 mirror 樣本與 Claude 的 `starts_window` 等操作員帳號卡 d-2a19fdb5。Rust 端（statusline、`codeforge usage accounts`）下一輪，共用同一個 `QUOTA_CONTRACT`。
