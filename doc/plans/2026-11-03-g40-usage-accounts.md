# G40 設計：配額檢視（codeforge 只負責顯示）

> 狀態：設計稿 v2（PM G40 重新定界，size S，只有文件，沒有程式碼）。2026-11-04。v1 是 `98c7a8b`，那一版把「收集」放在 codeforge，已被取代（見 §1）。
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
| `usage.db` 取樣表與 7d 走勢 | 取樣只為收集服務；契約沒有歷史欄位 | 無（走勢 v1 不做，見 §9 待解 10） |
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
| `host_role` | 字串 | v1 顯示端忽略（見 §9 待解 13） |

### 2.2 provider 清單

契約列出的 10 個營運者自有帳號（契約 §20 開頭；能力等級取自 capability matrix）：

| provider（契約用字） | matrix 觀察到的等級 | 預期的 window 組 |
|---|---|---|
| `claude`（claude code） | L3 | `rolling_5h`、`weekly` |
| `codex` | L3（僅 weekly 觀察到） | `weekly`；另有 credits 餘額，但契約沒有只帶餘額的欄位（§9 待解 3） |
| `minimax` | L3 | `rolling_5h`、`weekly` |
| `glm` | L2 | kind 對照是推論（未驗證），所以要標 `estimated`／推論 |
| `qoder` | L2（vendor 文件，未實測） | `credits`，重置時間未知 |
| `kimi` | L1 | 只有 429；文件說有 `weekly`／`rolling_5h`／`monthly` |
| `opencode` | L1（opencode-go） | 只有 limit 錯誤文字 |
| `qoder-cli-cn` | L0 | 無 |
| `agy` | L0 | 無 |
| `grok` | L0 | 無 |

**顯示端不寫死這張表**：契約 §20.8 Q12 明說清單只是範例、不是 registry，`qoder-cli-cn`、`opencode`、`agy` 到底是 provider 還是 harness 由 matrix 逐列決定。顯示端對任何符合 `^[a-z0-9-]{1,24}$` 的 `provider` 一視同仁，畫它的字面名稱；window 組完全由記錄的 `windows[]` 決定，不由 provider 推測。

## 3. 各 window 怎麼畫

標籤（固定欄寬 6 格，CJK 字佔 2 格）：

| `kind` | pane 標籤 | band 標籤 | 畫法 |
|---|---|---|---|
| `rolling_5h` | `5h` | `5h` | 量表 `▰▱`（10 格）＋ `%` ＋ `◷` 倒數 |
| `weekly` | `週` | `週` | 同上。契約的 `weekly` 不保證恰好 7 天（codex 觀察到 `window_minutes=10080`），所以不叫 `7d` |
| `daily` | `日` | `日` | 同上 |
| `monthly` | `月` | `月` | 同上 |
| `credits` | `額度` | `額度` | 有 `used`／`limit`：量表＋「剩 N unit」＋已用 %；沒有：量表＋「已用 %」。**量表永遠畫已用比例**，和其他 window 同方向；「剩」是另外一個數字 |
| `rpm`／`tpm` | `rpm`／`tpm` | 不畫 | 只在 pane 的次要行畫（短時節流，契約不列入 ok／tight 判定）；沒有量表，只寫 `1.00` 這類數值或 `100%` 加 `◷` |

倒數用 `◷`（時間，§3.2 已登記）＋1 空格＋精簡時間；**不用** `↻`／`↺`（`↺` 是審查輪次）。`resets_at` 已過的 window 不畫，視為「已重置，等新觀察」（契約 §5.2 規則 1）。

## 4. 可見度、估算、過期、429：每一種怎麼畫

這一節是顯示端最重要的規則。總原則：**沒有量到的東西不畫量表**；量表只代表一個有來源的 `utilization`。

| 狀況 | 判定（契約依據） | pane（完整檢視） | band 晶片 |
|---|---|---|---|
| **L0** | `visibility: L0`、沒有記錄、或 `windows: []`（§20.4） | 一行 `無資料`，下一行小字說明（沒有任何可見來源）。**不畫量表、不畫 0%** | 不畫個別晶片；併入尾端的 `無資料 N` 計數晶片（可捨棄） |
| **L1** | `windows` 只有 `source:"429"` 的項目（§20.4） | `429 冷卻`；有 `resets_at` 時接 `◷ 倒數後重試`，沒有時寫「重試時間未知」。**不畫量表** | `provider:alias 429 冷卻`，有 `resets_at` 時加 `◷ 倒數` |
| **L2** | 有 `utilization`，部分 `resets_at: null`（§20.4） | 有值的 window 畫量表；`resets_at` 是 `null` 的 window 寫「重置時間未知」 | 同 L3，沒有倒數就省略 `◷ …` |
| **L3** | 全部 window 有 `utilization` 與 `resets_at` | 量表＋倒數 | 最緊 window 的 `%`＋倒數 |
| **`source:"429"` 的 window** | 契約規定 `utilization` 固定 1.0，是合成值（§20.2） | 不論 record 等級，這個 window 一律畫 `429 冷卻`，**不畫成 100% 的量表**（因為 1.0 是合成的，不是量測；§9 待解 7） | 同 L1 的 band 寫法 |
| **`estimated`** | `source:"estimated"`，本機計數對方案上限（§20.2：只能讓別名「看起來更緊」，不是量測） | 量表照畫，百分比前面加字樣 `估`（`估 41%`），pane 底下一行註明「估 = 本機計數，不是供應商回報」 | `估 41%`；倒數照常 |
| **過期（年齡）** | window 的 `observed_at` 距今 > 顯示門檻 | 該行尾端加年齡（`· 2h10m 前`） | 晶片尾端加年齡（`2h10m 前`） |
| **過期（硬上限）** | 距今 > **6 小時**（契約 §5.2 規則 2，與 `MAX_AGE_MS` 同）或超前 > 5 分鐘（`quota_invalid`） | 當作無資料：`—  過期（最後觀察 7h 前，未計入）`，**不畫量表** | 不畫個別晶片，併入 `無資料 N` |
| **credits 類型** | `kind:"credits"`（§3 表） | `額度` 行：量表＋「剩 N unit」（有 `used`／`limit` 時）＋已用 %；重置時間常未知 | `額度 剩 3,100`；沒有 `used`／`limit` 時 `額度 已用 38%` |
| **緊**（≥ 0.90，與 autopilot `TIGHT_THRESHOLD` 一致） | 該 window 的 `utilization` ≥ 0.90 | `%` 後接字樣 `緊`（`91% 緊`）；**不改顏色**（§6.3 與 §9 待解 6） | 同左 |

要點：

- **「估」不是新符號。** `≈` 沒有登記在 §3.1／3.2，§3.3 規則 1 要求「新意思先改那張表」，所以本文只用中文字 `估`，不新增 glyph；若之後想用 `≈`，要先登記並過 glyph-check（§9 待解 9）。
- **過期的顯示門檻契約沒有給。** 契約只有 6 小時的硬上限（超過就丟）。「幾分鐘開始顯示年齡」是顯示端自己的決定：暫定 **30 分鐘**（§9 待解 1）。理由：reporter 不送 heartbeat、不偽造新鮮度（契約 §18.2），所以閒置別名的資料本來就會變舊，這是正確行為，要讓使用者看見年齡而不是隱藏。
- 同一個 record 內各 window 的 `observed_at` 可能不同，年齡**按 window 算**，不按 record 算。
- 一個 record 內全部 window 都因 `resets_at` 已過或超過 6 小時而被丟掉時，整個別名當 L0 畫（`無資料`），說明寫「已重置或過期，等新觀察」。

## 5. token 到期

契約的 record 自帶 `token_issued_at`／`token_expires_at`（epoch 秒，未知時為 `null`；§20.2、§20.3 `token_lifetime`）。顯示端因此**不需要**自己存到期日（v1 的 `accounts/<alias>.json` 作廢）。

- `token_expires_at` 為 `null`：不畫任何到期字樣（契約：`days: null` 表示未知，並抑制到期警告）。
- 剩餘 > 30 天：pane 區塊標題列右側畫 `token 到期 214 天`；band 不畫。
- 剩餘 ≤ 30 天、> 7 天：同上，數字改成實際天數；band 仍不畫（要人處理的才進 band）。30 天對應契約 §5.6 的 `accounts.token_warn_days` 預設值。
- 剩餘 ≤ 7 天或已過期：`▲` 晶片（`▲ claude:home token 到期 6 天`、`▲ claude:home token 已過期`），這是 §3.1 的 `need_you`，唯一允許黃色（warning）的地方；排在所有用量晶片之前。
- 7／30 天門檻：契約把 30 天放在 autopilot 設定 `accounts.token_warn_days`，7 天只寫在 §5.6 文字；codeforge 先用常數 30／7，與契約同步方式見 §9 待解 5。
- **缺口**：契約 §5.6 說「記錄到的 401 蓋過日期，`status` 顯示 `invalid`」，但這個狀態只出現在 autopilot 的 `status`，**沒有進 `fleet.quota/1`**。顯示端因此無法畫「token 失效」（§9 待解 2）；v1 的 `▲ token 失效` 暫時拿掉。

## 6. 畫面

### 6.1 forge-stats band 晶片

依 §3.5：晶片內「符號＋1 空格＋值」，晶片之間 2 空格，次要資訊 `' · '`，band 的組與組 `' │ '`。別名與 provider 是文字不是符號，直接接值。

```
艦隊 ▲ 1  ● 4 · claude:work 5h 62% ◷ 2h05m  claude:home 週 91% 緊 ◷ 1d 2h  kimi:main 429 冷卻 ◷ 1h20m  無資料 3 · Ferris Lv.3
```

- **位置**：§2.1 第 4 級（進度、用量），在艦隊計數之後、寵物之前。有 autopilot 時是第 2 列，沒有時是第 1 列；沒有任何可畫晶片時原樣 `next(e)`，不畫空白列（§2.2）。
- **每個 `(provider, alias)` 一個晶片**，畫它「最緊的 window」：在 `rolling_5h`／`weekly`／`daily`／`monthly`／`credits` 中取 `utilization` 最大者（與契約 pick 的 `used` 定義一致，`rpm`／`tpm` 不參與）。晶片內寫 window 標籤，讓人知道是哪個。
- **標籤**：`provider:alias`。窄寬度時先拿掉 `provider:` 前綴（保留 alias），前提是畫面上別名不重名；重名時保留前綴（不同 provider 可以有同名別名）。
- **晶片排序（保留優先序，由高到低）**：① `▲` token 晶片 → ② 緊（≥0.90）→ ③ `429 冷卻` → ④ 其餘依 `utilization` 由高到低 → ⑤ `無資料 N` 計數晶片（L0、過期、已重置的別名合計）。
- **窄寬度捨棄順序**（從尾端拿，§2.1 規則）：寵物 → `無資料 N` → 所有年齡字樣（`2h10m 前`）→ 所有倒數（`◷ …`）→ 由排序尾端往前拿掉整個晶片，直到只剩一個（最緊的）→ 只剩 `alias 62%`（別名一定保留）→ 整段用量交回。`▲` token 晶片屬於要人處理，最後才拿掉；不拆開符號和數字（§3.5 規則 4）。
- **資料來源與成本**：只讀 mirror（§7）；render 不跑子行程、不碰 hub（§8-1）。

### 6.2 `codeforge usage accounts`（完整檢視，≤100 欄）

單次呼叫、只讀 mirror，不探測（沒有 `--probe`）。下面是示意（數字與別名皆為虛構，glm 那行是為了示範 `estimated`，真實的 glm kind 對照仍待 matrix 驗證）。

```
配額檢視                                       更新 12s 前（reporter · mirror 7 筆）
──────────────────────────────────────────────────────────────────────────────────
claude:work  L3 · token 到期 214 天
  5h    ▰▰▰▰▰▰▱▱▱▱  62%   ◷ 2h05m 後重置
  週    ▰▰▰▱▱▱▱▱▱▱  31%   ◷ 3d 4h 後重置

claude:home  L3 · ▲ token 到期 6 天
  5h    ▰▱▱▱▱▱▱▱▱▱  12%   ◷ 4h40m 後重置
  週    ▰▰▰▰▰▰▰▰▰▱  91% 緊   ◷ 1d 2h 後重置

codex:main  L3
  週    ▰▱▱▱▱▱▱▱▱▱  17%   ◷ 3d 後重置 · 2h10m 前

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

- 每個 `(provider, alias)` 一個區塊，標題列：`provider:alias`、`L<n>`（暗色）、`token 到期`；`▲` 與到期字樣之間照 §3.5 規則 1 隔 1 格。
- 區塊順序：先 `▲`、緊、429，其餘依 provider、alias 字母序；L0 與過期放最後。
- `rpm`／`tpm` 的 window 列在該區塊最下面，不畫量表。
- 每個 window 行：標籤（6 格）、量表、`%`（含 `估`／`緊` 字樣）、倒數、年齡；欄與欄之間 2 空格，倒數 `◷ ` 後 1 空格。
- 寬度 <80：先拿掉年齡，再拿掉「後重置」，再拿掉量表只留數字；每行依顯示寬度截（§6）。<60 時每個 `(provider, alias)` 縮成單行。
- `--json`：輸出顯示端解讀後的清單（已套用 §4 的過期與丟棄規則），不是 mirror 檔案的原樣轉存；欄位名跟契約一致。
- 讀不到、schema 不符的 mirror 檔：不顯示該列，區塊最底下寫 `N 筆無法讀取（schema 或內容不符）`。

### 6.3 顏色（§3.3 規則 4、5）

- 黃色（warning）**只**給 `▲` 的要人處理（token ≤7 天、已過期）。
- 緊（≥0.90）、`429 冷卻`、`估`、過期、無資料：不用黃色。緊用字樣 `緊` 標示，`無資料`／過期／`429 冷卻` 用 `inactive`，`估` 用預設文字色。
- 這與 v1 的「緊的配額用 warning 色」不同：配額緊算不算要人處理要規範持有人裁定（§9 待解 6）；裁定前不碰黃色。

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
   - 單一檔案不存在：該別名根本不出現（顯示端不知道它存在；要看「有哪些別名」得靠 autopilot 的 registry，v1 不讀，見 §9 待解 11）。
6. **過期行為**：套用契約 §5.2 規則 1、2（window 的 `resets_at` 已過就丟；record 的觀察年齡 > 6 小時就丟，超前 > 5 分鐘視為無效），再套 §4 的 30 分鐘年齡顯示。不做 per-window 合併（合併是 reader／reporter 的事，mirror 已是合併後的結果）。
7. **過渡期**：reporter 還沒上線時（契約 P2b 之前），`<live_dir>/quota/` 不存在。此時顯示端**維持現況**（G36 的 `codeforge.quota/1` 快照 chip，只有互動別名），不會把兩個來源疊在一起。mirror 一出現就只讀 mirror（契約 §18.1：「mirror 出現後沒有任何顯示端直接讀 `codeforge.quota/1`」）。這條過渡規則是否被接受見 §9 待解 12。
8. TS（forge-stats）與 Rust（statusline、`usage accounts`）各一份讀取器，契約區塊用 `QUOTA_CONTRACT` 常數同步，cargo test 比對 TS 與 Rust 的 schema 名、window 列舉、單位、上限值（同 G36 `USAGE_CONTRACT` 作法）。

## 8. 責任切分與要協調的事

| 誰 | 負責 |
|---|---|
| autopilot | 觀察（wrapper passive／probe）、manifest 與 adapter、reporter、mirror 的寫入、token 日期的紀錄與 401 狀態、pick 讀取 |
| fleet | hub 的 quota 表、key `quota/<provider>/<alias>`、跨 host 衝突規則 |
| codeforge | 讀 mirror、畫 band 晶片與 `usage accounts`、statusline 快照照舊寫出（一個來源）、`QUOTA_CONTRACT` 同步測試、不變式測試（§3.5 規則 3） |

向 autopilot、fleet 提的要求在 §9 標「協調」。

## 9. 待解問題

1. **（顯示端自定，需 PM 同意）** 年齡顯示門檻：契約只有 6 小時硬上限。暫定 30 分鐘開始顯示年齡；是否合適？閒置別名常態顯示年齡，會不會讓畫面長期很吵？
2. **（協調：autopilot）** token 失效／被撤銷的狀態沒有進 `fleet.quota/1`（契約 §5.6 的 `invalid` 只在 `status`）。顯示端無法畫「token 失效」。要不要加 `token_state`（`ok`／`invalid`／`unknown`）？
3. **（協調：autopilot）** `credits` 只能用「`used` 與 `limit` 成組」表示。codex 觀察到只有餘額（`balance`）、沒有分母（matrix codex 欄 5），無法用契約表達。顯示端的「剩 N」因此只在有分母時可畫；要不要允許只帶餘額的 window？
4. **（協調：autopilot／fleet）** `<live_dir>` 是 autopilot `resolveLiveDir()` 的值，codeforge 現有檔案在 `<live_base>/codeforge/...`。兩者是不是同一個根？顯示端要的是確定的 mirror 絕對路徑解析規則（環境變數或固定相對路徑）。
5. **（協調：autopilot）** token 門檻 30／7 天：30 天是 autopilot 設定值，7 天只在文字。顯示端用常數會與設定值脫鉤。要不要讓 record 帶「警告等級」或把兩個門檻放進契約？
6. **（需 PM／規範持有人裁定）** §3.3-5「黃色只給要人處理」與「配額緊要不要用警告色」。本文暫定緊只用 `緊` 字樣、不上色；若裁定緊算要人處理，則 band 需要決定每列最多一個黃色晶片，其餘維持預設色。
7. **（顯示端自定，需確認）** `source:"429"` 的 window 一律畫 `429 冷卻`、不畫 100% 量表；若同一 record 同時有真實的 L3 window，兩者並排顯示。是否同意？契約沒有規定顯示方式，只規定 `utilization` 固定 1.0。
8. **（顯示端自定）** `rpm`／`tpm` 只在 pane、不在 band；契約 §20.8 Q13 本來就在問其他 lane 顯不顯示。
9. **（需規範持有人裁定）** `估` 字樣是否改為登記 `≈` 為結構符號？目前 §3.1／3.2 沒有 `≈`，本文不新增 glyph，只用中文字。
10. **（歷史）** 走勢圖：契約沒有歷史欄位，收集也已移出，所以 v1 的 7d 走勢取消。要不要有走勢、由誰記取樣（hub、reporter，還是 codeforge 讀 mirror 後自己存），要先決定歷史的擁有者。
11. **（需協調）** 「哪些別名存在」：mirror 只有有過觀察的別名才有檔案，從沒觀察過的別名（真正的 L0）顯示端看不到。要不要由 autopilot registry 或 hub 提供別名清單，讓 L0 列出現在 pane？
12. **（需 PM 確認）** 過渡期規則（§7 規則 7）：mirror 不存在時維持現有 G36 chip，mirror 出現後完全改讀 mirror。這會讓只有互動別名的人在 reporter 上線瞬間看到畫面變化。是否接受？
13. **（協調：fleet）** 同一個 `(provider, alias)` 在多台 host 上觀察時，hub 一個 key 只存一筆，`host_role` 顯示端目前忽略。契約 §5.4 的 hub 問題（key 命名、衝突）解決後，是否要顯示是哪台 host 看到的？
14. **（協調：autopilot）** 契約欄位名與 codeforge 檔案名稱漂移（契約 Q11：`resets_at`／`utilization` vs `pct`）。顯示端鎖定 `fleet.quota/1`，遇到其他名字就整份不顯示；請在契約凍結版本號時通知 codeforge。
15. **（待 matrix）** glm 的 kind 對照、kimi 的 reset、qoder-cli-cn／agy／grok 的 L0 何時能升等：升等時 band／pane 不需要改程式，但範例畫面與 §2.2 表需要跟著更新。
