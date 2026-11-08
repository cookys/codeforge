# codeforge usage — 用量收集（G36）

> 設計與審查紀錄：[`doc/plans/2026-11-03-usage-collector.md`](../plans/2026-11-03-usage-collector.md)
> （hetero plan review g1+g2，收據已過）。本文件記錄「現在怎麼運作」與檔案契約。

## 做什麼

回答「用了多少、哪個帳號、離上限多遠」，全部在本機完成：不登入、不讀憑證檔、不匯出 OTel，
提示詞與路徑不離開這台機器。

| 部分 | 何時跑 | 產出 |
|---|---|---|
| 配額快照 | 每次 statusline（有 `rate_limits` 時） | `<live_base>/codeforge/usage/quota/<alias>.json` |
| 用量列 | 每次 statusline | 本 session 的 5h／7d／ctx 長條；其他帳號只在週配額會不夠（或 ≥ 95%）／會浪費時才出現，用顯示名（見「顯示名」） |
| 收集 | `codeforge usage`、`usage collect`、statusline 背景觸發（每 15 分鐘至多一次）、SessionEnd hook | `<data_dir>/usage/usage.db` |
| 摘要 | 每次收集之後 | `<live_base>/codeforge/usage/summary.json` |
| 推送 | `codeforge usage push` | `fleet upload`，印出 blob sha |
| 面板分頁 | forge-stats pane 的「用量」分頁（`/forge-stats usage` 直接開到這頁） | 只讀 `summary.json`：帳號 5h／7d 與倒數、今天（UTC）各 model 的 token 與 API 等值；摘要超過 1 小時就提示執行 `codeforge usage`，不自己呼叫 codeforge |

`<data_dir>` = `CODEFORGE_DATA_DIR`，預設 `~/.local/share/codeforge`；`<live_base>` 見
`src/live.rs`。G36 只寫自己的 `usage/` 目錄：檔案 0600、目錄 0700，發現較寬的權限就收緊並在
stderr 提一次。

## 配額快照只增不減

同一個帳號的每個 session 都寫同一個快照檔，各自帶著自己最後一次 API 呼叫的 `rate_limits`，閒置
session 手上是舊值（2026-11-06 實測：同一組重置時間下，四組數字每秒輪流出現，5h 2–16 %、
7d 48–52 %）。所以寫入時先和檔案裡的值合併（`quota::merge_snapshot`），每個窗各自判斷：

- 重置時間相同（相差不到 600 秒）：取百分比較大的；
- 較晚的重置時間：新的窗，直接取代；
- 較早的重置時間：舊的窗，丟掉；
- 這次沒帶這個窗：保留檔案裡的。

本 session 自己的 5h／7d 長條顯示合併後的值。每個窗另記 `seen_at_ms`、`seen_by`（這個值是哪個
session、何時看到的；`seen_by` 是清理過的 session id）。同樣的值 30 秒內不重寫，所以 `seen_at_ms`
最多落後 30 秒。讀和寫不是同一個原子步驟：兩個 session 在同一瞬間寫入時，較小的值仍可能留下，
下一次較大值的寫入會再改回來。`summary.json` 的契約不變（只輸出 `pct`、`resets_at`）。

## 帳號別名

- 身分規則（statusline 與收集共用）：(1) `AUTOPILOT_ACCOUNT` 符合 `^[a-z0-9-]{1,24}$` 就用它（autopilot G37）；
  (2) 否則由登入檔取身分：`acct-` + sha256(`organizationUuid` + "\n" + `emailAddress`) 前 8 碼；
  (3) 登入檔不存在、讀不了、超過 32 MiB 或沒有這兩個欄位，退回 `acct-` + sha256(canonical config dir) 前 8 碼。
  為什麼不用路徑：每台主機的 config dir 都是 `/home/cookys/.claude`，路徑 hash 讓 7 台不同配額的主機撞同一個別名，
  而且換帳號（同 email、個人訂閱 vs 組織訂閱）別名不會變。
- 登入檔：`$CLAUDE_CONFIG_DIR/.claude.json`（有設且非空），否則 `$HOME/.claude.json`（在 `~/.claude/` 之外）。
  只讀 `oauthAccount` 底下三個欄位：`organizationUuid`、`organizationName`、`emailAddress`（operator 授權的範圍），
  用只命名這三欄的型別化 serde 結構解析，其餘欄位（token、accountUuid…）由 serde 忽略，不留、不記錄、不複製；
  從不寫這個檔。org uuid 不離開本機：快照、summary、`fleet-quota.json` 只有別名（hash）、org 顯示名與登入 email。
  email 只出現在本機 0600 的快照、身分快取與 summary／`fleet-quota.json` 檔，**不進 repo、不進日誌、不進 fleet 訊息與 blob（`usage push` 上傳前拿掉 email）、
  不進錯誤訊息**（`Identity` 的 `Debug` 也不印它；測試與 fixture 只用 `@example.com`）。
- 快取（statusline 每次刷新都跑，登入檔可能數 MB）：`<live_base>/codeforge/usage/identity-<config dir 8 碼>.json`
  （0600），以登入檔的 (mtime_ns, size) 為鍵；沒變就只做一次 stat 加一次小檔讀取，變了才重新解析。解析出「沒有登入」
  也會快取。任何錯誤都退回路徑 hash，不讓 statusline 失敗。快取也存 `email`（舊快取沒有這欄，讀成未知，
  登入檔一變就重新解析）。
- `org`：`organizationName` 以 `'s Organization`（ASCII 或 ’）結尾、或含 `@`，視為個人訂閱，記為 `personal`；
  否則為清掉控制字元後最多 40 字元（`.chars().take(40)`）的名稱。`org` 放在本機快照、summary 帳號（**選填**，
  不存在 = 未知，舊 summary 仍合格；1..=40 字元、無控制字元、無 `@`）與 `fleet-quota.json`（字串或 null）。
  文字表頭在別名後顯示（`personal` 顯示為「個人」／personal），順序：別名、org、provider、主機。
  主機太多放不下時，保留放得下的標籤並以「等 N 台」／「+N more」收尾，不會整段拿掉。
- `email`：`emailAddress` 去頭尾空白後 3..=254 字元、恰一個 `@` 且兩邊都有字、無空白與控制字元，否則視為未知（None，
  不是錯誤、不拒收快照）。有 `AUTOPILOT_ACCOUNT` 時別名仍用它，但 email 與 org 照樣取自登入檔。`email` 放在本機快照
  （壞值丟掉、數字照留）、summary 帳號（**選填**，驗證規則同上，壞值整份 summary 不合格，訊息不含該值；契約的
  `email` 正規表達式 `^(?=.{3,254}$)[^@\s\p{Cc}\uFEFF]+@[^@\s\p{Cc}\uFEFF]+$` 與 pane 副本由測試比對）與
  `fleet-quota.json`（字串或 null，取最新且帶 email 的那筆觀察，同 org）。
- **視窗擷取時間 `seen_at_ms`**：summary 帳號的 `five_hour`／`seven_day` 視窗**選填** `seen_at_ms`（整數毫秒，0..=2^53−1，
  不得晚於該 summary 的 `generated_at_ms` 超過 10 分鐘），表示這筆 rate-limit 資料是何時被擷取的，取自快照視窗的
  `seen.at_ms`；舊快照沒有就省略（舊 summary 仍合格，合併時以帳號 `written_at_ms` 代替）。超出範圍的值在產生時直接不帶，
  不丟掉整個帳號。契約 `window_optional` 與 pane 副本由測試比對。
- **顯示名（`src/usage/names.rs`）**：`acct-…` 與 `<provider>-<8 碼 hex>` 別名只是內部的合併鍵，**永遠不上畫面**，也不能設成名字。statusline、`usage fleet` 文字表、
  forge-stats「全部帳號」分頁與 `fleet-quota.json` 的 `name` 用同一個顯示名，依序取：
  (1) operator 設的名字（`~/.config/codeforge/accounts.toml`，見下）；(2) org 的第一個字轉小寫（`personal`、`stranity`，最多 16 字元）；
  (3) email 的 `@` 前段；(4) 鍵給的名字：`<provider>-<8 碼 hex>`（例如 codex 帳號 `codex-4cc1b5c8`）取 provider 名 `codex`，其他不是 `acct-` 開頭的鍵（`AUTOPILOT_ACCOUNT` 選的名字）用鍵本身；(5) 都沒有 → 沒有名字，
  **這個帳號不顯示**（修正身分前留下的合併快照，例如沒有 org、沒有 email 的 `acct-ae88ee5d`）。`usage fleet` 把它計入
  `skipped["no_identity"]`。預設名撞名時加「·email 前段」，還撞就依鍵的順序加 `-2`、`-3`；operator 設的名字不改。
  文字表頭與分頁標籤是「名字 · email」（email 與名字相同或未知時只有名字；名字欄最多 24 欄，分頁最多 40 欄、窄面板縮短）。
  `--json` 每筆同時帶 `alias`（鍵）與 `name`。forge-stats 對沒有 `name` 的舊檔：依序退回 email、org、別名給的名字（同 (4)），
  `acct-` 且三者皆無的列不顯示。
- **名字設定檔**：`~/.config/codeforge/accounts.toml`（`CODEFORGE_ACCOUNTS_FILE` 可改，其次 `$XDG_CONFIG_HOME/codeforge/`），
  只在本機、不進 repo；`[names]` 表以別名（= sha8(org uuid + email)，即 org uuid＋email 的鍵；codex 等非 `acct-` 鍵也可）對應名字。
  檔案 0600、資料夾 0700、原子寫入。名字最寬 24 欄（中文字 2 欄）、無 `@`、無控制字元、前後無空白、不以 `acct-` 開頭；
  不合規的條目畫面上略過。`alias set` 只改一條、檔內其他內容原樣保留；檔案讀不到或不是合法 TOML 時報錯、不覆寫
  （畫面端照樣當作沒有名字）。錯誤訊息不回顯 email。預設名的各段依欄寬截短（`org·前段` 各最多 10 欄），撞名編號取還沒被用的最小 `-N`。
  `codeforge usage alias list` 列出本機快照與 `fleet-quota.json` 認得的帳號（名字、是否自訂、org、email，只在本機畫面）；
  `codeforge usage alias set <email 或目前顯示名> <新名字>` 設定，拒絕和別的帳號同名。
- **statusline 其他帳號段**：只看還沒重置的週視窗。週 ≥ 95% 或「會不夠（估）」→「⚠ 名字 週 97% 3d1h 後重置」；
  「會浪費」→「名字 週 NN% … 後重置 會浪費」；其餘帳號不出現，沒有名字的帳號不出現。放不下就只留前面的段。
  （c4140 曾顯示 97%，原因是 statusline 讀到修正身分前的舊合併快照；gpu-lead 實測 /login 後新舊 session 的數字相同，
  「舊 session 拿舊 token」的推論不成立。）
- **summary 帶 `name`**：summary 帳號**選填** `name`（顯示名，由寫 summary 的主機用同一套規則與它自己的名字設定檔算；沒有名字就不帶）。
  驗證同 `valid_name`，契約 `name_max_cells` = 24 與 pane 副本由測試比對。forge-stats「用量」分頁與側欄畫 `name`；舊 summary 沒有 `name`
  時依序退回 email、org、非 `acct-` 的別名，`acct-` 且三者皆無的帳號不畫。`usage push` 上傳前同時拿掉 `email` 與 `name`
  （預設名可能是 email 前段）。`codeforge usage --by account` 的文字表也印顯示名（無法命名的 `acct-` 印「未命名帳號」），`--json` 仍是鍵。
- 收集：不讀 `AUTOPILOT_ACCOUNT`。一個 Claude root 的別名取「`root` 等於它 hash 的最新快照」，其次是該 root 自己的
  登入檔身分（預設 root `~/.claude` 對應 `~/.claude.json`，其他 root 對應 `<root>/.claude.json`），
  再其次是 db 裡存的，最後 `acct-<hash>`。Codex root 是 `codex-<hash>`。
- 已知限制：重新登入之前寫下的轉錄檔，會歸給目前登入的帳號。
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

## summary.json（唯一會離開本機的東西；帳號列帶登入 email，`usage push` 上傳的 blob 會先拿掉 email）

```json
{"schema":"codeforge.usage-summary/1","generated_at_ms":0,"host":"host-1a2b3c4d","prices_as_of":"2026-10-06",
 "accounts":[{"alias":"acct-a","org":"personal","email":"me@example.com","name":"personal","written_at_ms":0,"five_hour":{"pct":0.3,"resets_at":0},"seven_day":null}],
 "days":[{"day":"2026-11-03","source":"claude","account":"acct-a","model":"claude-opus-5-5","speed":"standard",
          "input":0,"output":0,"cache_write_5m":0,"cache_write_1h":0,"cache_read":0,"requests":0,"cost_usd":null}]}
```

每一層的 key、型別、值都驗證（別名格式、`host-` + 8 hex、model id 格式、日期、列舉、非負數）。
所有整數（計數、時間、`resets_at`）都必須在 0..=2^53−1（`UINT_MAX`，JS number 能精確表示的
範圍）；飽和後的加總寫出時壓在這個上限。
某份本機配額快照的值超出契約（`written_at_ms`／`resets_at` 不在 0..=2^53−1、`pct` 不在 0..=1.5）時，
只丟掉那一份，其他帳號與全部 `days` 照常寫出，不讓整份摘要驗證失敗。丟了多少看頂層選用欄位
`dropped_accounts`：`{"<原因>": <計數>}`，原因只能是 `written_at_ms_out_of_range`、
`resets_at_out_of_range`、`pct_out_of_range`，計數是 uint；沒有丟任何東西時這個 key 不存在
（形狀與舊版相同），絕不帶別名或自由文字。`codeforge usage collect` 非 quiet 時在 stderr 提示丟了幾份。
forge-stats 的契約副本多了 `top_optional`／`drop_reasons`，解析時接受並驗證這個欄位。
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

## 全帳號配額表（`usage fleet`）

`codeforge usage fleet --dir <D>`：把許多主機的 `codeforge.usage-summary/1` 合併成一張全帳號配額表，
頂端列出「會浪費／會不夠（估）」的帳號。只讀、不連網、不存任何東西；實作 `src/usage/fleet_quota.rs`。

- **資料夾**：每台主機一個 `*.json`，檔名（不含副檔名）就是顯示的主機標籤（由收集端依角色命名，如
  `gpu-a.json`）；summary 裡的 `host` hash 與任何路徑都不顯示。每個檔都過 `summary::validate`，不合格的略過並計數
  （文字顯示「N 份略過」，JSON 的 `skipped` 為 `{原因: 數量}`，原因：`invalid`、`not_json`、`unreadable`、
  `too_large`（> 4 MiB）、`too_many`（> 256 份）、`bad_name`；沒有顯示名的帳號列記為 `no_identity`），不會中斷整張表。
- **合併**：每個別名一列；`hosts` 是有這個別名的所有檔名。**5h 與 7d 視窗各自取「擷取時間最新」的那筆觀察**
  （擷取時間 = 視窗的 `seen_at_ms`，沒有就用該帳號的 `written_at_ms`；平手取 `written_at_ms` 較新者，再取標籤較小者），
  **不是取最大 pct**，也不是整個帳號只看 `written_at_ms`：一台主機的 statusline 久未刷新、檔案卻剛寫出，
  舊資料不會蓋過另一台較新的擷取。擷取時間較舊**且**內容不同（`resets_at` 或 `pct` 不同）的主機記為「舊」
  （任一視窗輸了就算）：JSON `old_hosts`、文字與分頁在標籤後加「舊」／old。列的 `observed_at_ms` 與 6 小時過期
  都看被選中視窗的擷取時間。被選中的視窗 `resets_at` ≤ now 就是「已重置」：JSON `reset: true`，畫成「週 已重置」／
  `week reset`（不顯示舊的 pct），不估算、不警示（`projected`、`warn`、`exhaust_at` 皆 `null`）。
  只出現在 `days`（source codex）的別名是「無來源」列（summary 不帶 codex 百分比）；grok、MiniMax、agy 固定列
  「無來源」（`NO_SOURCE_PROVIDERS`）。
- **畫法**：沿用 G40（`quota_view`）：觀察超過 30 分鐘顯示年齡、超過 6 小時算「過期」、`resets_at` 已過的視窗
  畫成「已重置」而不是 pct（兩個都過就是「已重置，等新觀察」，`state` 為 `stale`）、`pct ≥ 0.90` 標「緊」；寬度 ≤ 100 欄。
- **警示（只看週視窗、線性估算）**：W = 7 天，remaining = resets_at − now，elapsed = W − remaining；elapsed < 6 小時
  不估（「暖機」）。`projected = pct × W / elapsed`。**會浪費**：remaining < 48 小時且 pct < `--use-target`（預設 0.85）。
  **會不夠（估）**：projected ≥ 1.0 且 pct < 1.0，並估 `now + (1−pct)/(pct/elapsed)` 用完。兩者同時成立時以會不夠為準
  （照現速會用完，就不是浪費）。有警示的列排前面：會浪費、會不夠（各依重置時間由近到遠），其餘依別名。
- **JSON**（`--json` 印出；`--out <file>` 以暫存檔＋rename 原子寫入，0600，可與文字表並用）：
  `{"schema":"codeforge.fleet-quota/1","generated_at_ms":…,"use_target":0.85,"skipped":{},"accounts":[…]}`，每筆
  `{alias, name, org|null, email|null, provider("claude"|"codex"|"grok"|…), hosts[], old_hosts[]（hosts 的子集，排序）, observed_at_ms|null, age_s|null, state, weekly, five_hour}`；
  `state`：`ok`（有視窗可畫）、`stale`（6 小時內有觀察但視窗全已重置）、`expired`（最後觀察超過 6 小時）、`no_source`；
  `weekly` = `{pct, resets_at, projected|null, warn("waste"|"short"|null), exhaust_at|null, reset}`（`resets_at`、`exhaust_at`
  為 unix 秒；`reset` 為 true 時視窗已重置，`pct` 不是現值），沒有可畫的週視窗時為 `null`；`five_hour` = `{pct, resets_at, reset}|null`。
  forge-stats 解析器對 `old_hosts`、`reset` 容忍缺席（舊檔：空、false），其餘鍵仍嚴格。`pct` 是小數（1.0 = 100%）。
  讀者（forge-stats 分頁、fleet.console/1 的配額區）直接讀，不重算。

**寫檔（`usage collect` 順手維護）**：環境變數 `CODEFORGE_FLEET_USAGE_DIR` 有設且非空時，`codeforge usage collect`
（每 15 分鐘由 statusline 的 autocollect 觸發，也可手動）寫完 `summary.json` 後，對該資料夾做同一套合併（直接呼叫
`fleet_quota` 的函式，不開子行程），把 `codeforge.fleet-quota/1` 寫到 `summary.json` 旁邊的 `fleet-quota.json`
（暫存檔＋rename，0600，同 summary.json 的 `write_live_json`）。沒設環境變數：什麼都不做，不寫檔，也不清掉以前留下的舊檔。
合併失敗（資料夾讀不了等）不讓 collect 失敗；`--quiet` 靜默，否則在 stderr 報一行。`codeforge usage fleet` 省略 `--dir`
時也用這個環境變數（兩者都沒有就報錯）。讀者是 forge-stats 的「全部帳號」分頁（`codeforge-mod-ui.md`），只在這個檔存在時才出現。

## 指令

```bash
codeforge usage                     # 收集 + 最近 7 天，依 model 分組
codeforge usage --days 3 --by project
codeforge usage --json
codeforge usage collect --quiet     # hook 用
codeforge usage push                # 印出 blob sha
codeforge usage accounts [--json]   # G40 配額檢視：讀 autopilot 的配額 mirror（只讀，不探測、不連網）
codeforge usage fleet [--dir <D>] [--json] [--out <file>] [--use-target 0.85]  # 全帳號配額表（多主機 summary 合併）
codeforge usage alias list          # 帳號與顯示名（本機）
codeforge usage alias set <email或顯示名> <新名字>
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
