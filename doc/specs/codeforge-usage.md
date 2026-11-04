# codeforge usage — 用量收集（G36）

> 設計與審查紀錄：[`doc/plans/2026-11-03-usage-collector.md`](../plans/2026-11-03-usage-collector.md)
> （hetero plan review g1+g2，收據已過）。本文件記錄「現在怎麼運作」與檔案契約。

## 做什麼

回答「用了多少、哪個帳號、離上限多遠」，全部在本機完成：不登入、不讀憑證檔、不匯出 OTel，
提示詞與路徑不離開這台機器。

| 部分 | 何時跑 | 產出 |
|---|---|---|
| 配額快照 | 每次 statusline（有 `rate_limits` 時） | `<live_base>/codeforge/usage/quota/<alias>.json` |
| 用量列 | 每次 statusline | 本 session 的 5h／7d／ctx 長條，後接其他帳號 `別名 NN% 倒數` |
| 收集 | `codeforge usage`、`usage collect`、statusline 背景觸發（每 15 分鐘至多一次）、SessionEnd hook | `<data_dir>/usage/usage.db` |
| 摘要 | 每次收集之後 | `<live_base>/codeforge/usage/summary.json` |
| 推送 | `codeforge usage push` | `fleet upload`，印出 blob sha |
| 面板分頁 | forge-stats pane 的「用量」分頁（`/forge-stats usage` 直接開到這頁） | 只讀 `summary.json`：帳號 5h／7d 與倒數、今天（UTC）各 model 的 token 與 API 等值；摘要超過 1 小時就提示執行 `codeforge usage`，不自己呼叫 codeforge |

`<data_dir>` = `CODEFORGE_DATA_DIR`，預設 `~/.local/share/codeforge`；`<live_base>` 見
`src/live.rs`。G36 只寫自己的 `usage/` 目錄：檔案 0600、目錄 0700，發現較寬的權限就收緊並在
stderr 提一次。

## 帳號別名

- statusline：`AUTOPILOT_ACCOUNT` 符合 `^[a-z0-9-]{1,24}$` 就用它（autopilot G37 設定），否則
  `acct-` + sha256(canonical config dir) 前 8 碼。快照另帶 `root` = 該 config dir 的 8 碼 hash。
- 收集：不讀 `AUTOPILOT_ACCOUNT`。一個 Claude root 的別名取「`root` 等於它 hash 的最新快照」，
  對應存進 db；沒有就是 `acct-<hash>`。Codex root 是 `codex-<hash>`。
- 限制：轉錄檔不記錄是哪個 OAuth token。同一個 config dir 跑多個帳號時，轉錄用量歸給那個 dir
  的別名（配額快照仍準確）。G37 採「每個帳號一個 config dir」，解除這個限制。

## 讀什麼、存什麼

| 來源 | glob | 存下 |
|---|---|---|
| Claude | `<cfg>/projects/*/*.jsonl`、`<cfg>/projects/*/*/subagents/*.jsonl` | request id、UTC 日、model id、token 數、speed、cwd 最後一段 |
| Codex | `<codex>/sessions/YYYY/MM/DD/rollout-*.jsonl` | session id（檔名）、UTC 日、model id、token 數、cwd 最後一段 |

- 不進 symlink 目錄、不拿 symlink 或特殊檔；`O_NOFOLLOW | O_NONBLOCK` 開檔，必須在 root 的
  canonical path 底下。glob 以外（`settings.json`、`.credentials.json`、`auth.json`）一律不開。
- 來源以 sha256(canonical path) 記錄，不存路徑；cwd 是家目錄時存 `~`；model id 不符
  `^[A-Za-z0-9._-]{1,64}$` 存 `unknown`。
- 增量：只讀換行結尾的行，每個檔一個 transaction；head = 前 min(4096, offset) bytes 的 sha256，
  改寫、縮短、換 inode 就從頭讀（去重保證不重算）。
- 計數：Claude 同一 request id 逐欄取最大值；Codex 只加 session 累計值超過已計的部分。
- 時間：35 天以前的紀錄不計，35 天沒寫過的檔不讀；去重資料保留 40 天。Codex 在 35 天以前的
  累計值不計入，但仍當作 session 的基準；基準被修剪（session 靜默超過 40 天）後檔案又長了，就
  從頭重讀那個檔，舊累計值再次成為基準。
- 上限：單行最多 16 MiB（`MAX_LINE`，實際帶用量的行最長約 8 MB），更長的整行讀過丟棄、不計；
  單一計數最多 2^40（`MAX_COUNT`），超過的那筆略過；bucket 用 checked add，溢位就整個檔案
  rollback、這次略過。報表端加總一律 saturating。
- 儲存區：目錄、`usage.db`、`-wal`、`-shm` 都以 `O_NOFOLLOW | O_NONBLOCK` 開 handle 檢查，
  是 symlink 或不是目錄／一般檔案就拒絕，權限只在 handle 上收緊（不會順著 link 改）；SQLite
  以 `SQLITE_OPEN_NOFOLLOW` 開檔。這些檢查都在 SQLite 開檔**之前**做；連線開著的期間不再對
  db、`-wal`、`-shm` 開或關任何 fd（關掉同一檔案的任一 fd 會釋放本程序在其上的所有 POSIX
  鎖，包括連線在 `-shm` 上的 DMS 鎖）。之後新建的 `-wal`／`-shm` 由 SQLite 沿用 db 的 0600。
- 錯誤訊息只寫檔名（`usage.db`、`summary.json`）、欄位位置與錯誤類別，不帶本機路徑，不回顯
  意外的欄位名稱，也不轉發 fleet 的 stderr。

## 成本估算

`src/usage/price.rs`，`PRICES_AS_OF = 2026-10-06`，每個 model 一列費率（輸入、輸出、快取寫
5 分／1 小時、快取讀、fast 倍率）。標成「API 等值」：訂閱方案不按 token 計費。沒有定價的
model（含所有 Codex model）顯示 `—`，合計只加有定價的列並註明幾列沒定價。

## summary.json（唯一會離開本機的東西）

```json
{"schema":"codeforge.usage-summary/1","generated_at_ms":0,"host":"host-1a2b3c4d","prices_as_of":"2026-10-06",
 "accounts":[{"alias":"acct-a","written_at_ms":0,"five_hour":{"pct":0.3,"resets_at":0},"seven_day":null}],
 "days":[{"day":"2026-11-03","source":"claude","account":"acct-a","model":"claude-opus-5-5","speed":"standard",
          "input":0,"output":0,"cache_write_5m":0,"cache_write_1h":0,"cache_read":0,"requests":0,"cost_usd":null}]}
```

每一層的 key、型別、值都驗證（別名格式、`host-` + 8 hex、model id 格式、日期、列舉、非負數）。
所有整數（計數、時間、`resets_at`）都必須在 0..=2^53−1（`UINT_MAX`，JS number 能精確表示的
範圍）；飽和後的加總寫出時壓在這個上限。
`days` 是最近 7 天，專案已加總掉。`usage push` 讀回、驗證、把驗過的 bytes 以固定檔名
`usage-summary.json` 交給 `fleet upload`，不送本機路徑。

## 配額檢視（G40）

`codeforge usage accounts` 與 forge-stats（用量分頁、band 晶片）、statusline 都讀 autopilot 的配額
**mirror**：`<live_dir>/quota/<provider>/<alias>.json`，schema `fleet.quota-view/1`，每台寫入者一列，
讀取端依 autopilot G40 契約 §5.2 按年齡合併。codeforge 只讀、絕不寫這個目錄，不連 hub。設計與每種
狀態的畫法：[`doc/plans/2026-11-03-g40-usage-accounts.md`](../plans/2026-11-03-g40-usage-accounts.md)（§10 為準）。

- 解析與合併：`src/usage/quota_mirror.rs`（Rust）與 `plugins/forge-stats/hooks/quota.ts`（TS）同一套
  規則；常數在 `QUOTA_CONTRACT`（`contract:begin quota`），cargo test 讀出 TS 區塊比對。
- `<live_dir>`：`$AUTOPILOT_LIVE_DIR` → `$XDG_RUNTIME_DIR/autopilot` → `/run/user/<uid>/autopilot`
  （僅在 xdg 未設且目錄存在）→ `/dev/shm/autopilot-<uid>` → `~/.autopilot`；不採 `/tmp` 候選。
- 讀檔：Rust 以 `O_NOFOLLOW` 開檔、對已開的檔 fstat 確認是一般檔與大小（≤ 32 KiB）；forge-stats 的
  `$.fs` 沒有 no-follow，改為讀後再驗大小與 stat。最多 64 個 provider 目錄、256 個 view，其餘只計數。
- 過渡：`<live_dir>/quota/` 不存在時，statusline 與 forge-stats 的輸出和 G36 完全相同（測試逐位元組比對）；
  存在後改畫 mirror 的晶片，G36 快照晶片不再出現。
- 只有 token 狀態、從沒有過 window 的記錄（契約 §21.6 shape c）畫「沒有用量資料」，不是「過期或已重置，等新觀察」（後者只給有過 window 但全被丟掉的）；`--json` 的 `state` 為 `no_usage`。
- `--json` 輸出解讀後的清單（已套用丟棄規則），schema `codeforge.usage-accounts/1`。

## 指令

```bash
codeforge usage                     # 收集 + 最近 7 天，依 model 分組
codeforge usage --days 3 --by project
codeforge usage --json
codeforge usage collect --quiet     # hook 用
codeforge usage push                # 印出 blob sha
codeforge usage accounts [--json]   # G40 配額檢視：讀 autopilot 的配額 mirror（只讀，不探測、不連網）
/forge-stats usage                  # Claude Code 裡：開 forge-stats pane 的用量分頁（/forge-stats pet 回寵物）
```

`summary.json` 的契約以 `src/usage/summary.rs` 的 `contract()` 為準；forge-stats 的
`USAGE_CONTRACT`（`plugins/forge-stats/hooks/model.ts`，`contract:begin`／`contract:end` 之間）
必須和它相同，cargo test 會讀出來比對，兩邊不會各自漂移。帳號列依寬度降級：先省略倒數，再省略
7d，欄位整個省略、不截半。

用量分頁讀檔有上限：必須是一般檔案、不是 symlink、不超過 256 KiB（`USAGE_MAX_BYTES`），內容不符
`codeforge.usage-summary/1` 契約（別名、model id、數字）就整份不顯示，只說讀不懂。

不需要任何 hook 的觸發：每次 `codeforge statusline` render 都對
`<live_base>/codeforge/usage/collect.stamp`（0600，`O_NOFOLLOW`）取非阻塞 flock、讀出上次觸發
時間；距今至少 15 分鐘（或時間在未來：時鐘倒退不會卡死）就寫入現在時間，並在背景 spawn
`codeforge usage collect --quiet`（自己的 process group、stdio 導到 null、不等待）。每次 render
只多一次開檔、一次 flock、一次讀取。`CODEFORGE_USAGE_AUTOCOLLECT=0` 關閉（目前只在 unix）。

collect 本身持有 `<data_dir>/usage/collect.lock`（flock）：`collect --quiet` 遇到另一個還在跑就
直接跳過，`codeforge usage`、`usage push` 與不帶 `--quiet` 的 collect 會等它跑完。

`codeforge install --hooks` 另外把 `usage collect --quiet` 接在 SessionEnd 的 `ship --no-hook`
之後；第一次收集若超過 hook 時限，已完成的檔案已各自 commit，下次接著做。
