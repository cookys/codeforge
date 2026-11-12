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
| 匯出 | `codeforge usage export` | 印出驗證過、拿掉 email 和 name 的 summary（同 push 上傳的內容），給 hangar 用 ssh 收 |
| 每小時匯出 | `codeforge usage export --hourly` | 同一趟收集後，改印 UTC 每小時、每帳號、每模型的 Claude token 列（`codeforge.usage-hourly/1`），給 capacity 在重置與換帳號時切開用量 |
| 面板分頁 | forge-stats pane 的「用量」分頁（`/forge-stats usage` 直接開到這頁） | 只讀 `summary.json`：帳號 5h／7d 與倒數、今天（UTC）各 model 的 token 與 API 等值；摘要超過 1 小時就提示執行 `codeforge usage`，不自己呼叫 codeforge |

`<data_dir>` = `CODEFORGE_DATA_DIR`，預設 `~/.local/share/codeforge`；`<live_base>` 見
`src/live.rs`。G36 只寫自己的 `usage/` 目錄：檔案 0600、目錄 0700，發現較寬的權限就收緊並在
stderr 提一次。

## 配額快照只增不減

同一個帳號的每個 session 都寫同一個快照檔，各自帶著自己最後一次 API 呼叫的 `rate_limits`，閒置
session 手上是舊值（2026-11-06 實測：同一組重置時間下，四組數字每秒輪流出現，5h 2–16 %、
7d 48–52 %）。所以寫入時先和檔案裡的值合併（`quota::merge_snapshot`），每個窗各自判斷：

- 重置時間相同（相差不到 600 秒）：取百分比較大的；
- 重置時間不同（兩個不同的窗）：重置時間已過的窗不取代還在跑的窗；其他情況取擷取時間（`seen_at_ms`）較新的
  那份（都沒有擷取時間時取重置時間較晚的；兩個都已重置時取重置時間較晚的，所以停在舊窗的閒置 session 不會刷新檔案）。
  以前是「較晚的窗取代、較早的丟掉」：2026-11-09 aimax395-team 的一個 session 在 06:13 TW 換登入前，把前一個帳號
  的週窗（週一 04:00 重置，7d 11%）記到了 acct-337c98de 下，之後這個帳號真正的讀數（週四 12:00 重置，63%）都因為
  「較早的窗」被丟掉，畫面一直停在 11%；
- 這次沒帶這個窗：保留檔案裡的。

本 session 自己的 5h／7d 長條顯示合併後的值。每個窗另記 `seen_at_ms`、`seen_by`（這個值是哪個
session、何時看到的；`seen_by` 是清理過的 session id）。同樣的值 30 秒內不重寫，所以 `seen_at_ms`
最多落後 30 秒。讀和寫不是同一個原子步驟：兩個 session 在同一瞬間寫入時，較小的值仍可能留下，
下一次較大值的寫入會再改回來。`summary.json` 的契約不變（只輸出 `pct`、`resets_at`）。

快照的 `written_at_ms` 是**最後一次有讀數被採用**的時間：這次帶來的窗在合併時全部落敗（已重置的窗、較舊的讀數、
或同一窗較小的值），檔案不重寫，`written_at_ms` 不更新。否則閒置 session 不斷送舊值時，檔案的時間會被刷新，
舊值永遠不會因超過 6 小時（`MAX_AGE_MS`）而被 statusline 和 `summary.json` 濾掉（2026-11-09 實測：
一個 12 小時前的 7d 11% 一直被當成目前值，其他主機同帳號已是 57%）。

## 帳號別名

- 身分規則（statusline 與收集共用）：(1) `AUTOPILOT_ACCOUNT` 符合 `^[a-z0-9-]{1,24}$` 就用它（autopilot G37）；
  (2) 否則由登入檔取身分：`acct-` + sha256(`organizationUuid` + "\n" + `emailAddress`) 前 8 碼；
  (3) 登入檔不存在、讀不了、超過 32 MiB 或沒有這兩個欄位：**沒有帳號（未歸屬）**。statusline 不寫配額快照；
  收集到的用量列 `account` 存空字串，`summary.json` 的 `days[].account` 寫 `null`，自成一行，不併進任何帳號；
  `usage --by account` 印「未歸屬（未登入）」，`--json` 的 `key` 是 `null`。fleet 的 rollup 用同一條規則（PM 01M7052H79）。
  以前退回 `acct-` + sha256(canonical config dir) 前 8 碼，2026-11-09 實測：`acct-ae88ee5d`（`/home/cookys/.claude`）
  在每台主機都出現、卻沒有任何窗，`acct-81916b7b`（`/home/team/.claude`）被當成 team 的舊帳號。讀舊資料時一樣當未歸屬：
  `acct-<自己 root 的 hash>` 的配額快照不讀；db 裡 `acct-<任一已知 root 的 hash>` 的用量列讀成未歸屬。
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
- **顯示名（`src/usage/names.rs`）**：`acct-…` 與 `codex-<8 碼小寫 hex>` 別名只是內部的合併鍵（自選的 `ci-20261105` 不算鍵），**永遠不上畫面**，也不能設成名字。statusline、`usage fleet` 文字表、
  forge-stats「全部帳號」分頁與 `fleet-quota.json` 的 `name` 用同一個顯示名，依序取：
  (1) operator 設的名字（`~/.config/codeforge/accounts.toml`，見下）；(2) org 的第一個字轉小寫（`personal`、`stranity`，最多 16 字元）；
  (3) email 的 `@` 前段；(4) 鍵給的名字：codex 帳號的鍵 `codex-<8 碼小寫 hex>`（例如 `codex-4cc1b5c8`）取 provider 名 `codex`，其他不是 `acct-` 開頭的鍵（`AUTOPILOT_ACCOUNT` 選的名字）用鍵本身；(5) 都沒有 → 沒有名字，
  **這個帳號不顯示**（修正身分前留下的合併快照，例如沒有 org、沒有 email 的 `acct-ae88ee5d`）。`usage fleet` 把它計入
  `skipped["no_identity"]`。預設名撞名時加「·email 前段」，還撞就依鍵的順序加 `-2`、`-3`；operator 設的名字不改。
  文字表頭與分頁標籤是「名字 · email」（email 與名字相同或未知時只有名字；名字欄最多 24 欄，分頁最多 40 欄、窄面板縮短）。
  `--json` 每筆同時帶 `alias`（鍵）與 `name`。forge-stats 對沒有 `name` 的舊檔：依序退回 email、org、別名給的名字（同 (4)）；舊檔的 `name` 若是 codex 鍵，也改顯示 `codex`，
  `acct-` 且三者皆無的列不顯示。
- **名字設定檔**：`~/.config/codeforge/accounts.toml`（`CODEFORGE_ACCOUNTS_FILE` 可改，其次 `$XDG_CONFIG_HOME/codeforge/`），
  只在本機、不進 repo；`[names]` 表以別名（= sha8(org uuid + email)，即 org uuid＋email 的鍵；codex 等非 `acct-` 鍵也可）對應名字。
  檔案 0600、資料夾 0700、原子寫入。名字最寬 24 欄（中文字 2 欄）、無 `@`、無控制字元、前後無空白、不以 `acct-` 開頭；
  不合規的條目畫面上略過。`alias set` 只改一條、檔內其他內容原樣保留；檔案讀不到或不是合法 TOML 時報錯、不覆寫
  （畫面端照樣當作沒有名字）。錯誤訊息不回顯 email。預設名的各段依欄寬截短（`org·前段` 各最多 10 欄），撞名編號取還沒被用的最小 `-N`。
  `codeforge usage alias list` 列出本機快照與 `fleet-quota.json` 認得的帳號（名字、是否自訂、org、email，只在本機畫面）；
  `codeforge usage alias set <email 或目前顯示名> <新名字>` 設定，拒絕和別的帳號同名。
- **operator 備註（只在本機）**：同一個 `accounts.toml` 的選填 `[notes]` 表，鍵是 provider 標籤（`kimi`、`codex`、`claude`…）
  或帳號鍵，值是短文字。規則同名字但最寬 40 欄：無控制字元、無 `@`、前後無空白（不檢查 `acct-` 前綴）；不合規的條目略過。
  `usage fleet`（文字表、`--json`、`fleet-quota.json`）把備註掛成選填 `note`，帳號鍵的備註優先於 provider 的。
  文字表與 forge-stats 分頁把它淡色放在列尾，依剩餘寬度截短（中文字 2 欄、加 `…`；剩不到 8 欄就不畫）。
  備註**永遠不進** `summary.json` 與 `usage push`（summary 契約沒有這個鍵）。
  `codeforge usage note set <provider|名字> <文字>`、`usage note clear <provider|名字>` 設定／清除（provider 標籤原樣當鍵，
  否則照 `alias set` 的規則認帳號；只改一條、其餘原樣，0600、原子寫入）；`usage alias list` 一併列出備註。不寫死任何事實。
  **沒有任何資料的帳號**（例如只存在於沒人能動的正式主機上的 Claude 帳號）：`usage note set --new <名字> <文字>`，
  以名字本身當鍵。名字須過 `valid_name`（不能是 email、`acct-`／`codex-` 鍵），且不能是 provider 標籤、已有的備註鍵、
  已知帳號的鍵或顯示名（錯誤訊息請使用者去掉 `--new`）；不帶 `--new` 的 `note set`／`note clear <名字>` 認得這種條目。
  `usage fleet` 對「鍵不對到任何列、不是 provider 標籤、且是合法名字」的備註多加一列：alias = name = 鍵、provider `claude`、
  `no_source`、無視窗、無 `source_note`、`hosts` 空、`note` 為備註文字；別的帳號已顯示同名時帳號優先、這列丟棄。
  文字表與分頁照一般 no_source 列畫（附備註）；`usage alias list` 以「（無資料）」列出。這列不算帳號：
  `alias`／`note` 解析名字時不把它當已知帳號。
- **訂閱方案（`[plans]`）**：同一個 `accounts.toml` 的選填 `[plans]` 表，鍵同 `[notes]`（provider 標籤、帳號鍵或 `--new` 名字；
  帳號鍵優先於 provider），值是封閉集合的 id：`pro`、`max5x`、`max20x`、`team`、`enterprise`、`api`；顯示為
  Pro、Max 5x、Max 20x、Team、Enterprise、API。`codeforge usage plan set <target> <方案>` 寫入（target 的解析同 `usage note set`；
  輸入別名 `5x`→`max5x`、`20x`→`max20x`，大小寫不拘，存的一律是 id）、`usage plan clear <target>` 清除；寫入時不在集合內報繁體中文錯誤並列出可用值，
  檔案不動。載入時不在集合內的條目（或非字串）略過，視為未知。檔案其餘內容原樣保留，0600、原子寫入。
  **只存在本機**：方案不進 `summary.json`、不進 `usage push`（同備註）。`usage fleet` 對有方案的列加選填鍵 `plan`（id 字串）；
  未知時不帶此鍵。文字表在名字與 email 後面顯示 `方案 Max 5x`／`plan Max 5x`，未知顯示 `方案 未知`／`plan unknown`；
  `usage alias list` 每個帳號列多一欄方案（未知同上）。
  **無效值的處理（Rust 與 forge-stats 一致）**：不在集合內的 `plan` 不是錯誤，一律當未知；forge-stats 解析器不因此把整份檔判為 bad，該列照畫並顯示「方案 未知」。
- **operator 重置規則（`[resets]`，低可信度）**：同一個 `accounts.toml` 的選填 `[resets]` 表，鍵同 `[notes]`
  （provider 標籤、帳號鍵或 `--new` 名字），每個鍵一條規則，三種形式（時間 24 小時制，**必須寫明 UTC 偏移**）：
  - 每週 `<dow>@<HH:MM><±HH:MM>`：`dow` 為小寫 `mon tue wed thu fri sat sun`，例如 `fri@17:00+08:00` = 每週五 17:00（UTC+8）；
  - 每月 `<日>@<HH:MM><±HH:MM>`：日為 `1`..`31`（不補 0），例如 `17@00:00+08:00` = 每月 17 日 00:00（UTC+8）；
    該月沒有這一天（例如 2 月的 30 日）就在該月最後一天重置；「哪一天」照規則的 UTC 偏移算；
  - 一次性 RFC 3339 `YYYY-MM-DDTHH:MM:SS<±HH:MM|Z>`（大寫 `T`、不帶小數秒），例如 `2026-11-21T00:00:00+08:00`；
    時間過了就沒有下一次（`note_resets_at` 與 `note_reset_rule` 都不帶），規則留在檔裡直到 operator 清掉。
  不合規的條目載入時略過（同備註）。`usage note set` 用 `--reset-weekly`／`--reset-monthly`／`--reset-at`（三選一，各只收自己的形式）；
  `usage note reset` 收任一形式。
  `usage fleet` 對符合規則的列（帳號鍵優先於 provider，同備註）加選填 `note_reset_rule`（規則字串）與 `note_resets_at`
  （下一次發生時間，unix 秒，**嚴格晚於現在**；剛好在該時刻算下一期）。兩個鍵同進同出。這是 operator 手填的事實，
  **低可信度**：有真實配額視窗的列照舊用真實的 `resets_at`，規則只是額外資訊，排程端不該拿它蓋過真實視窗。
  文字表與 forge-stats 分頁在備註後面淡色加 `◷ 2d3h`（與其他倒數同格式）；寬度不夠時先截短備註（加 `…`），倒數保留；
  沒有備註時只畫倒數。`codeforge usage note set [--new] <target> <文字> [--reset-weekly <規則>]` 同時存兩者（一次原子寫入）；
  `usage note reset <target> <規則>` 只設規則（target 須已有備註，或是已知帳號／provider）；`usage note clear <target>`
  同時清備註與規則；`usage alias list` 的備註區列出規則。規則格式不合時報繁體中文錯誤並附範例。檔案其餘內容原樣保留，0600、原子寫入。
- **statusline 其他帳號段**：只看還沒重置的週視窗。週 ≥ 95% 或「會不夠（估）」→「⚠ 名字 週 97% 3d1h 後重置」；
  「會浪費」→「名字 週 NN% … 後重置 會浪費」；其餘帳號不出現，沒有名字的帳號不出現。放不下就只留前面的段。
  （c4140 曾顯示 97%，原因是 statusline 讀到修正身分前的舊合併快照；gpu-lead 實測 /login 後新舊 session 的數字相同，
  「舊 session 拿舊 token」的推論不成立。）
- **summary 帶 `name`**：summary 帳號**選填** `name`（顯示名，由寫 summary 的主機用同一套規則與它自己的名字設定檔算；沒有名字就不帶）。
  驗證同 `valid_name`，契約 `name_max_cells` = 24 與 pane 副本由測試比對。forge-stats「用量」分頁與側欄畫 `name`；舊 summary 沒有 `name`
  時依序退回 email、org、非 `acct-` 的別名，`acct-` 且三者皆無的帳號不畫。`usage push` 上傳前同時拿掉 `email` 與 `name`
  （預設名可能是 email 前段）。`codeforge usage --by account` 的文字表也印顯示名（無法命名的 `acct-` 印「未命名帳號」），`--json` 仍是鍵。
- 收集：不讀 `AUTOPILOT_ACCOUNT`，也不讀 `CODEFORGE_ACCOUNT_ALIAS`（後者只由 statusline 釘 session，見下）。一個 Claude root 的別名取「`root` 等於它 hash 的最新快照」（`root` 為 null 的覆寫快照不算），其次是該 root 自己的
  登入檔身分（預設 root `~/.claude` 對應 `~/.claude.json`，其他 root 對應 `<root>/.claude.json`），
  再其次是 db 裡存的，最後未歸屬（不再是 `acct-<hash>`；root 一律記進 db，讓它的舊 `acct-<hash>` 列認得出來）。
  Codex root 是 `codex-<hash>`。
- 重新登入後的配額與請求歸屬見「重新登入的歸屬」。
- 限制：轉錄檔不記錄是哪個 OAuth token。沒有 `CODEFORGE_ACCOUNT_ALIAS` 的 session，同一個 config dir 的用量仍依登入時間軸歸給那個 dir 的別名（配額快照仍準確）。setup-token lane 由 wrapper 設覆寫，並以 session 釘選分開歸屬（PM 01M76F3F）。G37 的「每個帳號一個 config dir」仍是沒有覆寫時的做法。

## 重新登入的歸屬（Re-login attribution）

同一個 Claude config dir 換帳號（帳號 A 登出、帳號 B 登入）之後，沒有行程覆寫的配額與請求依該 root 的登入時間軸歸屬。setup-token lane 的覆寫見下方。

時間軸檔是 `<quota 目錄的上一層>/logins-<root 的 hash8>.json`（也就是 `<live_base>/codeforge/usage/`），內容為
`{"entries":[{"since_ms":<毫秒>,"alias":"acct-xxxxxxxx"}]}`，新的在最後，最多 64 筆（丟掉最舊的）。
statusline 每次渲染，以及 `usage collect` 對每個解析出登入的 Claude root，都呼叫 `observe`：只有最後一筆的別名不同（或檔案是空的）才追加 `(現在, 目前別名)`。只存合法別名（`valid_alias`），不存 email、不存 org。寫入同其他 live JSON：暫存檔加 rename、0600。讀取不跟隨最後一層的 symlink。壞掉或外來的檔案（含任何 `@`）當成空的重寫。`alias_at(t)` 取 `since_ms <= t` 裡最大的那筆；`t` 早於第一筆則沒有。

配額閘門的檔是同一層的 `sessions-<root 的 hash8>.json`，內容為
`{"sessions":{"<session>":{"alias":"<別名>","fp":"<四個 rate_limits 原值接成的字串>","at_ms":<毫秒>}}}`。
`fp` 是這次 stdin 的 5h 百分比、5h `resets_at`、7d 百分比、7d `resets_at`。session 已有紀錄、別名不是目前帳號、而且 `fp` 相同：這是換帳號前的回應還留在畫面上，**不合併、不寫入**快照，長條改畫磁碟上目前帳號的快照（沒有這份檔就當成這次沒有 `rate_limits`）。該筆紀錄不改。
沒有紀錄、別名相同、或 `fp` 變了（換帳號之後的新回應）：照舊合併，並把紀錄設成目前別名、新的 `fp`、現在。別名與 `fp` 都沒變就不寫檔。
寫檔時刪掉 `at_ms` 早於 `quota::MAX_AGE_MS` 的紀錄，最多留 512 筆（丟掉 `at_ms` 最舊的）。session 是空字串就不設閘門，照舊合併。讀寫失敗就退回今天的合併，statusline 不因此失敗、也不印 stderr。

行程覆寫：lane wrapper 可設 `CODEFORGE_ACCOUNT_ALIAS=acct-<8 個小寫 hex>`。這是給 setup-token（環境變數登入）的 lane 用的（PM 01M76F3F）：那種 lane **不改寫**登入檔，若仍從 `.claude.json` 推別名，會把該 lane 的請求和 statusline 的配額讀數記到舊帳號。statusline 只在去掉前後空白後、同時通過 `valid_alias` 且符合 `acct-[0-9a-f]{8}` 時採用；大寫 hex、長度不對、`acct-` 後面像 email、空字串，都當成沒有，不印錯誤。採用時：

- 這個行程的帳號就是該別名，org 與 email 都是空的，**不讀**登入檔。
- **不**呼叫 `observe`。root 的登入時間軸仍只記登入檔上的帳號。
- 配額快照寫在覆寫別名下，`root` 為 null，因此 `root_alias` 不會把這個別名當成該 root 的帳號。
- 呼叫 `pin_session`，把這個 session 釘在覆寫別名上。檔案是同一層的 `pins-<root 的 hash8>.json`，內容為 `{"<session>":{"alias":"<別名>","at_ms":<毫秒>}}`。寫入慣例與 `sessions-` 相同：`state_file`、`load_bytes` 上限、0600、暫存檔加 rename、寫入時依 `quota::MAX_AGE_MS` 修剪、最多 512 筆、不能用的 session id 略過、內容沒變不寫。缺檔、壞掉或外來（含任何 `@`）讀成空的。
- `session_gate` 仍用覆寫別名跑。

請求：`collect` 插入**新的** `requests` 列時，session id 是轉錄檔名的 stem；`<session>/subagents/` 底下的子代理轉錄算父 session。同一個 root 每次收集只讀一次 pins。帳號的優先順序是：該 session 的釘選別名，沒有才取 `timeline.alias_at(該列的 seen_ms)`，再沒有才用 `root_alias`。已經在庫裡的列不改帳號（修正前入庫的列也一樣，不回填：`requests` 沒有 session 欄，無法事後分辨是哪一個 session）；同一列後來增加的 token 仍加進原來的 bucket。`buckets` 的帳號跟該列相同。每小時匯出讀 `requests.account`，因此不必另外改。

已知限制：換帳號要到下一次 statusline 渲染或 collect 才被看見；這段空檔裡的請求歸給前一個別名。時間軸出現之前就收集的轉錄檔，仍用舊規則（歸給收集當下的帳號）。釘選也要等該 lane 的 statusline 寫過 pin 之後，之後的**新**列才歸到覆寫別名。

aimax395 的 team，從 2026-11-12 06:30（+0800）起，到各 lane 帶著這個覆寫重啟為止，記成 `acct-65d2bffc` 的 Claude 請求與配額百分比實際上屬於 `acct-9832dff1`。PM 裁定不回填（PM 01M76FE9），所以這段期間的逐帳號合計是不準的。

## 讀什麼、存什麼

每次收集的 Claude config dirs 是以下來源的聯集：非空的 `$CLAUDE_CONFIG_DIR`、`~/.claude`、
`accounts.toml` 的 `[collect] claude_dirs`、usage.db 記住的目錄，以及本次所有 `--claude-dir`。
以 canonical path 去重，同一目錄用尾斜線或 `..` 等不同寫法只掃一次；`[collect] claude_dirs`、記住的目錄與 `--claude-dir`
不接受 symlink 目錄（含路徑中間的 symlink）；`$CLAUDE_CONFIG_DIR` 與 `~/.claude` 照舊，家目錄在 symlink 底下（如
`/home -> /usr/home`）仍會收集。被略過的明確來源（`[collect] claude_dirs` 或 `--claude-dir`）會向 stderr 各印一行警告
（`--quiet` 時不印）；記住的目錄靜默略過。
Codex 仍只用 `$CODEX_HOME`（非空時），否則 `~/.codex`，不記入 Claude 目錄表。

**額外來源設定**：沿用名字、`[plans]` 的同一份 `accounts.toml` 與 TOML parser；路徑選擇同上
（`CODEFORGE_ACCOUNTS_FILE` → `$XDG_CONFIG_HOME/codeforge/accounts.toml` → `~/.config/codeforge/accounts.toml`）。

```toml
[collect]
claude_dirs = ["/srv/claude/seats/seat-a", "/srv/claude/dispatches/run-b"]
```

陣列每一項必須是絕對路徑字串；相對路徑會讓 collect 報繁體中文錯誤，指出該項與值，不靜默忽略。
也可用 `codeforge usage collect --claude-dir /srv/claude/seats/seat-a`，旗標可重複；相對路徑同樣拒絕。
設定檔不存在或沒有 `[collect]` 時沒有額外設定來源；讀取失敗、TOML 或欄位型別錯誤則報錯。

**記住目錄**：usage.db 透過既有開檔 migration 建立 `claude_dirs` 表：`path TEXT PRIMARY KEY`（canonical config dir）、
`first_seen_ms INTEGER NOT NULL`、`last_seen_ms INTEGER NOT NULL`（首次／最近掃描的 unix 毫秒）。每個實際掃描的 Claude root
都寫入，包含環境變數與只給過一次的旗標來源；重掃只更新 `last_seen_ms`。既有 `roots` 的 hash／別名與每個 root 的登入身分規則不變。
記住的目錄不存在、變成一般檔案或 symlink 時，本次略過、不報錯，也不刪除表內紀錄或更新時間；之後恢復就繼續收集。
目錄路徑只在本機 usage.db，**不進 summary.json 或 usage push**；移除設定或旗標不會忘掉已掃過的目錄。
舊版 `roots` 只有 hash，無法還原路徑；升級後先由上述來源發現目錄，再記住它。

新增目錄第一次掃描會自動從頭回補最多 `HORIZON_DAYS`（目前 **35 天**）的用量，不需要額外 backfill 指令；
跨目錄同一 requestId 仍由 `requests.request_id PRIMARY KEY` 去重，每個 token 欄位取最大值，請求只計一次。

| 來源 | glob | 存下 |
|---|---|---|
| Claude | `<cfg>/projects/*/*.jsonl`、`<cfg>/projects/*/*/subagents/*.jsonl` | request id、UTC 日、model id、token 數、speed、cwd 最後一段 |
| Codex | `<codex>/sessions/YYYY/MM/DD/rollout-*.jsonl` | session id（檔名）、UTC 日、model id、token 數、cwd 最後一段；`rate_limits` 的窗口數字（見下） |

- **Codex 窗口（rollout `rate_limits`）**：`payload.type == "token_count"` 行的 `payload.rate_limits.primary`／`secondary`
  各是 `{used_percent, window_minutes, resets_at}` 或 `null`。只讀這三個數字：`window_minutes` 300 = 5 小時窗、10080 = 週窗，
  其他長度與 `null` 都是「沒有這個窗」；`used_percent / 100` 存成 0..1.5 的小數。觀察時間取該行 `timestamp`，沒有就用檔案 mtime。
  每個 codex 帳號（`codex-<hash>`）只留**最新**的一筆（db 表 `codex_limits`：帳號、觀察時間、兩窗的 pct 與 `resets_at`），
  比已存的舊的不覆蓋；collector 是增量的，表存在 db 裡，所以下一輪沒讀到新行也不會掉。credits、plan_type、limit_id 等一律不讀不存。
  summary 建立時，至少還有一個窗沒重置的觀察變成該 codex 別名的帳號項（形狀同 Claude 帳號：`alias`、`written_at_ms` = 觀察時間、
  `five_hour`／`seven_day` 帶 `seen_at_ms`），不套 6 小時新鮮度篩選（舊觀察在全帳號表上照樣顯示「過期」，不會變成誤導的「沒有窗口資料」）。

- 不進 symlink 目錄、不拿 symlink 或特殊檔；`O_NOFOLLOW | O_NONBLOCK` 開檔，必須在 root 的
  canonical path 底下。glob 以外（`settings.json`、`.credentials.json`、`auth.json`）一律不開。
- 轉錄檔來源以 sha256(canonical path) 記錄，不存轉錄檔路徑；Claude config dir 路徑另存於上述 `claude_dirs` 表。
  cwd 是家目錄時存 `~`；model id 不符
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
  意外的欄位名稱，也不轉發 fleet 的 stderr。設定／CLI 輸入驗證例外：相對 `claude_dirs`／`--claude-dir` 會指出使用者輸入的值。

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
`days[].account` 是別名，或 `null`（只限 `source` 為 `claude`：沒有登入的 root，未歸屬）。讀這份檔的舊版程式
（codeforge 與 forge-stats）不收 `null`，所以讀的一方要先升級。
所有整數（計數、時間、`resets_at`）都必須在 0..=2^53−1（`UINT_MAX`，JS number 能精確表示的
範圍）；飽和後的加總寫出時壓在這個上限。
某份本機配額快照的值超出契約（`written_at_ms`／`resets_at` 不在 0..=2^53−1、`pct` 不在 0..=1.5）時，
只丟掉那一份，其他帳號與全部 `days` 照常寫出，不讓整份摘要驗證失敗。丟了多少看頂層選用欄位
`dropped_accounts`：`{"<原因>": <計數>}`，原因只能是 `written_at_ms_out_of_range`、
`resets_at_out_of_range`、`pct_out_of_range`，計數是 uint；沒有丟任何東西時這個 key 不存在
（形狀與舊版相同），絕不帶別名或自由文字。`codeforge usage collect` 非 quiet 時在 stderr 提示丟了幾份。
forge-stats 的契約副本多了 `top_optional`／`drop_reasons`，解析時接受並驗證這個欄位（`codex_sessions` 同）。
`codex_sessions`（頂層選用，U6）：每個有 `token_count` 累計值的 Codex rollout 一筆，依 `rollout_id` 遞增排序
（最多 2000 筆，超過取最近有成長的）；只列最近 7 天（同 `days` 的視窗）內有成長的 rollout。
沒有任何 rollout、或 rollout 沒有 `token_count` 時這個 key **不存在**（不是空陣列，形狀與舊版相同），絕不出現零列。
```json
"codex_sessions":[{"rollout_id":"0199aaaa-bbbb-7ccc-8ddd-eeeeffff0001","account":"codex-4cc1b5c8",
  "input":240,"output":50,"cache_read":160,"cache_write":null,"reasoning":20}]
```
欄位對應（與 `days` 的 Codex 列同一套規則）：`rollout_id` = 檔名結尾的 uuid（小寫）；`account` = `root_alias` 的
`codex-<sha8>`；數字是該 rollout 最後的累計值（`total_token_usage` 各欄取到過的最大值，不把每回合與累計相加）。
`cache_read` = `cached_input_tokens`；`input` = `input_tokens − cached_input_tokens`（Codex 的 `input_tokens` 已含快取，
下限 0）；`output` = `output_tokens`（Codex 的輸出已含推理，**不再加** `reasoning`）；`reasoning` =
`reasoning_output_tokens`（`output` 的子集，單獨列出）；`cache_write` **永遠是 `null`**：Codex 沒有快取寫入計數，不捏造 0。
欄位只有 id 與數字，不含 email、路徑、cwd、model 或任何 rollout 文字。舊讀取端（不認識此 key 的驗證器）會把整份當壞；
forge-stats 與 `validate` 已接受。升級前就存在、之後沒再成長的 rollout 沒有帳號與推理計數，不列出，直到它再成長。
`days` 是最近 7 天，專案已加總掉。`usage push` 讀回、驗證、把驗過的 bytes 以固定檔名
`usage-summary.json` 交給 `fleet upload`，不送本機路徑。

## 每小時匯出（`usage export --hourly`）

`codeforge usage export --hourly` 先做與 `usage export` 相同的收集（寫 `summary.json`），再把下面這份
JSON 印到 stdout：一個文件、結尾一個換行。不印 summary。給 capacity 在每個配額池的重置時間、以及換帳號時
切開用量。沒有 `--hourly` 的 `usage export` 印出的 bytes 不變。

```json
{"schema":"codeforge.usage-hourly/1","generated_at_ms":0,"host":"host-1a2b3c4d",
 "rows":[{"hour":"2026-11-04T13","account":"acct-1a2b3c4d","model":"claude-opus-4-8","speed":"standard",
          "input":0,"cache_write_5m":0,"cache_write_1h":0,"cache_read":0,"output":0,"requests":0}]}
```

沒有登入的列，`account` 是 `null`（未歸屬），不是空字串。

- **schema**：`codeforge.usage-hourly/1`。每一層的 key 都是封閉集合；多一個或少一個欄位，整份不合格。
- **generated_at_ms**：產出當下的 unix 毫秒，整數，0..=2^53−1。
- **host**：`host-` 加 hostname sha256 的前 8 碼 hex（`summary::host_alias`）。絕不是 hostname。
- **rows**：`requests` 裡 `source = 'claude'`、而且 `seen_ms` 沒有比 `generated_at_ms` 早超過 35 天
  （`HORIZON_DAYS`，與收集的視野相同；未來的時間戳照計）的列，依 (hour, account, model, speed) 加總。
  最多 100_000 列。依 hour、account、model、speed 排序（未歸屬的空字串排在有別名的前面，印出時寫成 `null`）。
- **hour**：`seen_ms` 的 UTC 小時，`YYYY-MM-DDTHH`（`strftime('%Y-%m-%dT%H', seen_ms/1000, 'unixepoch')`）。
- **account**：未歸屬 sentinel（空字串，同 `summary::build`）寫 `null`；否則必須是合法別名（`^[a-z0-9-]{1,24}$`）。
- **model**：與 summary 同一條 model id 規則，`^[A-Za-z0-9._-]{1,64}$`。不合語法的記為 `unknown`
  （同一小時、同一帳號、同一速度裡多個不合語法的 id 併成一列）。
- **speed**：`standard` 或 `fast`。
- **計數**：`input`、`cache_write_5m`（欄 `cw5m`）、`cache_write_1h`（欄 `cw1h`）、`cache_read`（欄 `read`）、
  `output` 是加總，`requests` 是 `COUNT(*)`。每個整數壓在 `UINT_MAX`（2^53−1）。
- 印出前先驗證。驗證失敗是錯誤，stdout 什麼都不印。

**覆蓋**：收集只把 35 天以內的紀錄算進去，去重列在 `requests` 留到 40 天（`DEDUP_KEEP_DAYS`）才刪。
這份匯出再依同一個 35 天視野濾一次，所以是最近 35 天的 Claude 用量。

**只有 Claude**：Codex 的用量在 `codex_sessions`（每個 rollout 的累計），沒有逐筆 `requests` 列，所以這裡不出現。

**為什麼沒有 email、name、org、project、路徑**：這份 JSON 會離開這台機器（capacity 用 ssh 收，跟 `usage export`
同一條路）。email、顯示名、組織名留在本機快照與 summary。project 是 cwd 的最後一段，只留在本機 `requests`，
不進任何一列。host 只用 hash。

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

## agy 配額（`agy -p /usage` 的本機帳號）

`codeforge usage collect` 同時收這台主機的 `agy`（Antigravity CLI）帳號配額，進 `summary.json`、
隨之進 `usage fleet`：一個 pool 一個帳號，共兩個——Gemini pool 的鍵是 `agy-<hash8>`、
Claude/GPT pool 的鍵是 `agyclaude-<hash8>`（實作 `src/usage/agy.rs`）。

- **來源命令**：`agy -p /usage`（無 model call，約 3 秒回四行 tab 分隔：pool／窗口標籤／**剩餘** %／
  窗口重置的 RFC3339 時間）。這是別的程序輸出（信任邊界）：不是恰好四個欄位、pool 或窗口標籤不認識、
  % 不是 0..=100 的整數後面跟 `%`、或時間解析不出的行都略過；沒有有效視窗的 pool 不產生快照。
- **換算**：輸出是**剩餘**比例，`quota::Window.pct`（**已用**比例 0.0–1.0）= `1 − 剩餘/100`；
  `resets_at` = 重置時間的 unix 秒。
- **快取**：原始輸出存 `<live_base>/codeforge/usage/agy-usage.json`（`{"observed_ms", "text"}`、
  0600、原子寫入），至多每 10 分鐘重跑一次（期間不 spawn `agy`）；跑失敗時改用不超過 6 小時
  （同配額快照的 `MAX_AGE_MS`）的快取，再沒有就本次不收集。
- **逾時／關斷**：`agy` 與 headless CLI 同一個包裝跑（`platform::low_priority_command`：Unix
  `nice -n 19 timeout 20`、Windows 殺 pid 的看門狗），20 秒後被殺；exit 0 的 stdout 才算輸出。
  環境變數 `CODEFORGE_USAGE_AGY=0` 完全不 spawn `agy`。
- **鍵**：`hash8` = 這台 `$HOME`（canonicalized）的 `alias::path_hash8`；同一個 Google 帳號在兩台
  主機上是兩列，operator 用 `usage alias set` 設了顯示名之前不會合併。

## grok 配額（本機 `grok` CLI 的週配額）

`codeforge usage collect` 同時收這台主機的 `grok` CLI 帳號週配額，寫進 `summary.json`，再進
`usage fleet`（`codeforge.fleet-quota/1`）：grok 帳號的鍵是 `grok-<hash8>`，只有週視窗
（`five_hour` 為 null）。還沒設顯示名時，畫面顯示 `grok-cli`。實作 `src/usage/grok.rs`。

同一份 billing 回應的 `config.productUsage` 若有一列，其 `product` 轉成小寫並去掉每個非字母、非數字的字元後等於 `grokbot`（`GrokBot`、`Grok Bot`、`grok-bot` 都算），且 `usagePercent` 是 0..=150 的數字，就多一筆快照：鍵 `grokbot-<同一個 hash8>`，週視窗的 `pct` 是該百分比除以 100，`resets_at` 與 grok 列同一個 `currentPeriod.end`。百分比缺欄、不是數字或不在範圍內就略過該列，grok 快照不變。帳號的 billing 沒有這一列就沒有 grokbot 快照；trinity 若在另一個 xAI 帳號，這裡不會出現。還沒設顯示名時這一列顯示 `trinity`。

- **端點**：`GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`。標頭
  `Authorization: Bearer <key>`、`User-Agent: grok/<version>`。`<version>` 取自 `grok --version`
  （印出例如 `grok 1.0.70 (f50904eaee6c)`，用 `1.0.70`）。逾時 10 秒。`grok` 不在 PATH 上，或讀不到
  版號，就不讀登入檔、不送請求。
- **憑證（操作員卡 d-8bbff214）**：只在這台主機、這個帳號、發出請求的當下讀 `~/.grok/auth.json`。
  金鑰只放在記憶體裡的區域變數，隨請求送出；不複製到任何檔案，不寫進日誌、stderr、錯誤訊息、快取、
  summary 或測試快照；除了這個 billing 端點，不送到別的地方。`auth.json` 是頂層恰好一個欄位的 JSON
  物件：忽略欄位名，取那個值。那個值只用 `"key"`（字串，Bearer）與 `"expires_at"`（字串，ISO 8601
  UTC、帶奈秒與 `Z`，例如 `2026-11-11T15:27:38.257878022Z`）。其餘欄位（`refresh_token`、`email`、
  `user_id` 等）不讀。`expires_at` 已經到期（≤ now），或 HTTP 401／403：停止，不重新登入、不換發
  token，本次沒有新資料，改用下面的快取（快取自己逾齡後變成過期）。
- **換算**：`config.creditUsagePercent`（數字，接受 0..=150，否則整筆不要）除以 100 得到
  `seven_day.pct`；`config.currentPeriod.end`（RFC3339）換成 unix 秒，寫進 `seven_day.resets_at`。
  `five_hour` 為 null。回應不可信：缺欄位或型別不對就沒有快照。
- **快取**：`<live_base>/codeforge/usage/grok-usage.json` 只存 `{"observed_ms","pct","resets_at"}`，
  有 Grok Bot 列時多一個數字 `bot_pct`（已除以 100；不含產品名稱或其他欄位）。舊快取沒有 `bot_pct`
  仍只回 grok 快照。0600、原子寫入，同其他 usage 快取。未滿 10 分鐘就用快取、不打端點（有 `bot_pct`
  時一併回 grokbot 快照）。否則打端點：解析成功就覆寫快取並回傳；Denied、失敗或解析失敗時，若快取未超過
  6 小時（`quota::MAX_AGE_MS`）就用快取，否則這次不收集。
- **關斷**：環境變數 `CODEFORGE_USAGE_GROK=0` 時什麼都不讀（包括快取，不輸出 grok 或 grokbot 列）、不打端點。
- **鍵**：`hash8` 是這台 `$HOME`（canonicalized）的 `alias::path_hash8`。

## kimi 配額（本機 `kimi` Code CLI 的 5 小時與週配額）

`codeforge usage collect` 同時收這台主機的 `kimi`（Kimi Code CLI）帳號的 5 小時與週配額，寫進
`summary.json`，再進 `usage fleet`（`codeforge.fleet-quota/1`）：一個帳號，鍵是 `kimi-<hash8>`。
還沒設顯示名時，畫面顯示 `kimi-code`。月配額與 booster 錢包不收集（fleet-quota/1 沒有月視窗）。
實作 `src/usage/kimi.rs`。不需要 `kimi` 執行檔在 PATH 上。

- **端點**：`GET <base>/usages`。`base` 是環境變數 `KIMI_CODE_BASE_URL`，但只在它以 `https://` 開頭且
  含主機時採用（`http://`、其他 scheme、以及只有 `https://` 都忽略，尾端的 `/` 會去掉），否則
  `https://api.kimi.com/coding/v1`。標頭 `Authorization: Bearer <access_token>`、
  `Accept: application/json`。逾時 8 秒。不走代理、不跟隨重新導向（權杖只送到這個端點）。
- **憑證（操作員卡 d-a080c798）**：只在這台主機、這個帳號、發出請求的當下讀
  `<kimi_home>/credentials/kimi-code.json`。`kimi_home` 是環境變數 `KIMI_CODE_HOME`（有設且非空），
  否則 `<home>/.kimi-code`。檔案不以 symlink 開啟。權杖只放在記憶體裡的區域變數，隨請求送出；
  不複製到任何檔案，不寫進日誌、stderr、錯誤訊息、快取、summary 或測試快照；除了這個 usages
  端點，不送到別的地方。檔案是 JSON 物件，只用 `"access_token"`（字串，Bearer）與 `"expires_at"`
  （數字，unix 秒）。其餘欄位（`refresh_token`、`scope` 等）不讀。`access_token` 是空字串（kimi
  用來標記已撤銷）、`expires_at` ≤ now、或 HTTP 401／403：停止，不重新整理、不換發權杖（kimi
  自己換發，不能跟它搶）、不打端點，本次沒有新資料，改用下面的快取（快取自己逾齡後變成過期）。
- **換算**：`usages.limit_5h` → `five_hour`，`usages.limit_7d` → `seven_day`。`used_ratio` 是數字
  或數字字串，已經是已用比例（0.42 = 42%），接受 0.0..=1.5，否則丟掉那個視窗。`reset_time` 是
  RFC3339，換成 unix 秒寫進 `resets_at`；缺了或解析不出就丟掉那個視窗。`limit_month_total`、
  `limit_month_code`、`boosterWallet` 忽略。回應不可信：沒有 `usages` 物件，或兩個視窗都無效，
  就沒有快照。
- **快取**：`<live_base>/codeforge/usage/kimi-usage.json` 只存 `observed_ms`，以及每個看得到的視窗
  的 `pct`／`resets_at`（0600、原子寫入，同其他 usage 快取）。未滿 10 分鐘就用快取、不打端點。
  否則打端點：解析成功就覆寫快取並回傳；Denied、失敗或解析失敗時，若快取未超過 6 小時
  （`quota::MAX_AGE_MS`）就用快取，否則這次不收集。
- **關斷**：環境變數 `CODEFORGE_USAGE_KIMI=0` 時什麼都不讀（包括憑證檔與快取）、不打端點。
- **鍵**：`hash8` 是這台 `$HOME`（canonicalized）的 `alias::path_hash8`。

## trinity 配額（自報）

操作員 d-e31e39c0（PM 01M76PF4）。fleet 成員 `cookys/grokbot/trinity`（Grok Bot，自己的 VM，沒有可讀的憑證）每次被叫醒時，把自己的 Grok 額度寫成一行，同一個 UTC 小時最多一次，送到 `cookys/aimax395-team/fleet-cli`（或由它那台的 courier 定時送出同一行）。只收配額：token 數與醒來次數不匯入（醒來次數是 hangar 每 6 小時從 hub 算的代理值，給 capacity，PM 01M76GXN）。codeforge 不連線、不讀憑證，只在 `codeforge usage trinity-import <FILE>`（`-` 表示標準輸入）時匯入。帳號鍵是 `trinity-<sha8>`：sha8 是成員位址字串的 sha256 前 8 碼 hex，各主機相同；沒設顯示名時顯示 `trinity (Grok Bot)`。實作 `src/usage/trinity.rs`。

```json
{"schema":"trinity-usage/1","observed_at":"2026-11-12T03:12:00Z","pct":0.34,"resets_at":"2026-11-18T00:00:00Z"}
```

- **schema**：必須是 `trinity-usage/1`。
- **observed_at**：RFC3339；比現在晚超過 5 分鐘就略過該行。
- **pct**：已用比例，有限數字 0.0..=1.5（34% 寫 0.34）。
- **resets_at**：RFC3339，必須晚於 `observed_at`。
- 三個欄位都必填，不合格的行略過並計數，空白行不計；有不合格的行、卻一行合格的都沒有時是錯誤（結束碼非 0，收件端會通知）。其他欄位忽略，不寫進任何檔案。檔案超過 64 KiB 或 200 行，整份拒絕。

**寫入**：這次匯入裡 `observed_at` 最新的那一行，比 `<live_base>/codeforge/usage/trinity-quota.json` 裡的 `observed_ms` 新時才覆寫（0600、原子寫入）：`observed_ms`（unix 毫秒）、`pct`、`resets_at`（unix 秒）。不寫 `usage.db`，不進 `usage export --hourly`。`codeforge usage` 收集時讀這份檔，做成週視窗快照（沒有 5 小時窗）；`CODEFORGE_USAGE_TRINITY=0` 時不讀。快照比 `quota::MAX_AGE_MS`（6 小時）舊、或 `resets_at` 已過，就不進 summary。

**收件**：`scripts/trinity-import/trinity-import.sh`，由 systemd user timer 每 15 分鐘執行一次（`codeforge-trinity-import.timer`）。它讀這台 fleet-cli 的收件匣（`fleet inbox --peek --json`），只收 courier 證明的 `from` 在 `CODEFORGE_TRINITY_SENDERS`（預設 `cookys/grokbot/trinity` 與 `cookys/grokbot/fleet-cli`：trinity 那台用排程跑 shell `fleet send` 時，寄件者是那台的 fleet-cli；hub 只認證到 courier 這層，所以內容一律當不可信資料解析）裡、內容是 `trinity-usage/1` JSON 的訊息，每則訊息只匯入一次。匯入失敗，或成功匯入後超過 `CODEFORGE_TRINITY_STALE_H`（預設 24）小時沒有新資料，就通知 capacity 與 codeforge 一次（每則訊息一次，每天一次）。

## 全帳號配額表（`usage fleet`）

`codeforge usage fleet --dir <D>`：把許多主機的 `codeforge.usage-summary/1` 合併成一張全帳號配額表，
頂端列出「會浪費／會不夠（估）」的帳號。只讀、不連網、不存任何東西；實作 `src/usage/fleet_quota.rs`。

- **資料夾**：每台主機一個 `*.json`，檔名（不含副檔名）就是顯示的主機標籤（由收集端依角色命名，如
  `gpu-a.json`）；summary 裡的 `host` hash 與任何路徑都不顯示。每個檔都過 `summary::validate`，不合格的略過並計數
  （文字顯示「N 份略過」，JSON 的 `skipped` 為 `{原因: 數量}`，原因：`invalid`、`not_json`、`unreadable`、
  `too_large`（> 4 MiB）、`too_many`（> 256 份）、`bad_name`；沒有顯示名的帳號列記為 `no_identity`），不會中斷整張表。
資料夾由 hangar 每 15 分鐘對每台主機跑 `ssh <host> codeforge usage export > <dir>/<label>.json` 收集填入
（PM 01M74XA6Y5）：匯出的 bytes 就是 `usage push` 會上傳的那份（驗證過、拿掉 email 與 name），email 從不離開主機。
- **合併**：每個別名一列；`hosts` 是有這個別名的所有檔名。**5h 與 7d 視窗各自取「擷取時間最新」的那筆觀察**
  （擷取時間 = 視窗的 `seen_at_ms`，沒有就用該帳號的 `written_at_ms`；平手取 `written_at_ms` 較新者，再取標籤較小者），
  **不是取最大 pct**，也不是整個帳號只看 `written_at_ms`：一台主機的 statusline 久未刷新、檔案卻剛寫出，
  舊資料不會蓋過另一台較新的擷取。擷取時間較舊**且**內容不同（`resets_at` 或 `pct` 不同）的主機記為「舊」
  （任一視窗輸了就算）：JSON `old_hosts`、文字與分頁在標籤後加「舊」／old。列的 `observed_at_ms` 與 6 小時過期
  都看被選中視窗的擷取時間。被選中的視窗 `resets_at` ≤ now 就是「已重置」：JSON `reset: true`，畫成「週 已重置」／
  `week reset`（不顯示舊的 pct），不估算、不警示（`projected`、`warn`、`exhaust_at` 皆 `null`）。
  帳號的 provider 由鍵的形式推得（`codex-<8 碼 hex>` → `codex`，`agy-<8 碼 hex>`／`agyclaude-<8 碼 hex>` →
  `agy`／`agyclaude`（見「agy 配額」），`grok-<8 碼 hex>` → `grok`（見「grok 配額」，只有週視窗；預設顯示名
  `grok-cli`），`grokbot-<8 碼 hex>` → `grokbot`（見「grok 配額」，只有週視窗；預設顯示名 `trinity`），
  `kimi-<8 碼 hex>` → `kimi`（見「kimi 配額」，5 小時與週視窗；預設顯示名 `kimi-code`），
  `trinity-<8 碼 hex>` → `trinity`（見「trinity 配額（自報）」，只有週視窗；預設顯示名 `trinity (Grok Bot)`），
  其他 → `claude`）。有窗口的 codex 帳號照 Claude 帳號一樣合併；
  只出現在 `days`（source codex）的別名是「無來源」列。provider 列（`PROVIDERS`，小寫標籤）在沒有任何該 provider 的列時補一列
  「無來源」，並帶原因 `source_note`：qoder、qodercn、cursor、opencode → `web_only`（只有網頁）；
  glm、minimax → `needs_token`（要 token，待 autopilot 鏡像）；codex → `no_window`（帳號沒有窗口資料，有窗口時就不是無來源）。
  agy、grok、grokbot、kimi 與 trinity 有本機來源，不在 `PROVIDERS` 裡：沒有資料就沒有那一列，不會補一列「只有網頁」。
  文字表與分頁在「無來源」後淡色接「 · 原因」（i18n：`quota.fleet_src_*`、forge-stats `fleetSrc*`）。
- **畫法**：沿用 G40（`quota_view`）：觀察超過 30 分鐘顯示年齡、超過 6 小時算「過期」、`resets_at` 已過的視窗
  畫成「已重置」而不是 pct（兩個都過就是「已重置，等新觀察」，`state` 為 `stale`）、`pct ≥ 0.90` 標「緊」；寬度 ≤ 100 欄。
- **警示（只看週視窗、線性估算）**：W = 7 天，remaining = resets_at − now，elapsed = W − remaining；elapsed < 6 小時
  不估（「暖機」）。`projected = pct × W / elapsed`。**會浪費**：remaining < 48 小時且 pct < `--use-target`（預設 0.85）。
  **會不夠（估）**：projected ≥ 1.0 且 pct < 1.0，並估 `now + (1−pct)/(pct/elapsed)` 用完。兩者同時成立時以會不夠為準
  （照現速會用完，就不是浪費）。有警示的列排前面：會浪費、會不夠（各依重置時間由近到遠），其餘依別名。
- **JSON**（`--json` 印出；`--out <file>` 以暫存檔＋rename 原子寫入，0600，可與文字表並用）：
  `{"schema":"codeforge.fleet-quota/1","generated_at_ms":…,"use_target":0.85,"skipped":{},"accounts":[…]}`，每筆
  `{alias, name, org|null, email|null, provider("claude"|"codex"|"kimi"|…), hosts[], old_hosts[]（hosts 的子集，排序）, observed_at_ms|null, age_s|null, state, weekly, five_hour}`，
  另有兩個選填鍵：`source_note`（只在 `no_source` 列，`"web_only"|"needs_token"|"no_window"`）、`note`（operator 備註，見上），以及成對的 `note_resets_at`／`note_reset_rule`（operator 重置規則：每週、每月或一次性，見上；低可信度）；另有選填 `plan`（`"pro"|"max5x"|"max20x"|"team"|"enterprise"|"api"`，operator 設定的訂閱方案，見上；未知時不帶）；
  `state`：`ok`（有視窗可畫）、`stale`（6 小時內有觀察但視窗全已重置）、`expired`（最後觀察超過 6 小時）、`no_source`；
  `weekly` = `{pct, resets_at, projected|null, warn("waste"|"short"|null), exhaust_at|null, reset, age_s, stale}`（`resets_at`、`exhaust_at`
  為 unix 秒；`reset` 為 true 時視窗已重置，`pct` 不是現值），沒有可畫的週視窗時為 `null`；`five_hour` = `{pct, resets_at, reset, age_s, stale}|null`。
  每個窗的 `age_s` 是被選中那筆讀數的擷取時間到 `generated_at_ms` 的秒數，`stale` 在超過 2 小時（`STALE_MS`）時為 true
  （各窗分開算：週窗可能剛讀到，5h 窗只剩一台的舊讀數）；summary.json 不加這兩個鍵（沒升級的主機會拒收未知鍵），
  它的窗已有 `seen_at_ms`，年齡用 `generated_at_ms − seen_at_ms` 算。
  `five_hour_state: "no_source"`（只在這種情況出現，且 `five_hour` 為 `null`）：5h 窗已重置，而週窗的讀數比那次重置還晚
  （帳號重置後還在用），卻沒有新的 5h 讀數。Claude Code（2.1.319）只在最近一次 API 回應的 header 帶 5h 時才把
  `rate_limits.five_hour` 交給 statusline，所以舊 pct 不再顯示，畫成「5h 無來源」（PM 01M70CZ6A0）；重置後沒再用的帳號仍是「已重置」。
  讀取方先升級：舊版 forge-stats 不認得這個鍵，會把整份檔判成不合規格。
  forge-stats 解析器對 `old_hosts`、`reset`、成對的 `age_s`／`stale`、`source_note`、`note`、`note_resets_at`／`note_reset_rule`、`plan`、`five_hour_state` 容忍缺席（`plan` 的無效值也當未知，不拒絕）（舊檔：空、false、無），其餘鍵仍嚴格。`pct` 是小數（1.0 = 100%）。
  讀者（forge-stats 分頁、fleet.console/1 的配額區）直接讀，不重算。

**寫檔（`usage collect` 順手維護）**：環境變數 `CODEFORGE_FLEET_USAGE_DIR` 有設且非空時，`codeforge usage collect`
（每 15 分鐘由 statusline 的 autocollect 觸發，也可手動）寫完 `summary.json` 後，對該資料夾做同一套合併（直接呼叫
`fleet_quota` 的函式，不開子行程），把 `codeforge.fleet-quota/1` 寫到 `summary.json` 旁邊的 `fleet-quota.json`
（暫存檔＋rename，0600，同 summary.json 的 `write_live_json`）。沒設環境變數：什麼都不做，不寫檔，也不清掉以前留下的舊檔。
合併失敗（資料夾讀不了等）不讓 collect 失敗；`--quiet` 靜默，否則在 stderr 報一行。`codeforge usage fleet` 省略 `--dir`
時也用這個環境變數（兩者都沒有就報錯）。讀者是 forge-stats 的「全部帳號」分頁（`codeforge-mod-ui.md`），只在這個檔存在時才出現。

## usage.v1 配額池（`usage pools`）

`codeforge usage pools [--json]` 讀 fleet 的唯讀端點 `GET /v1/usage`（schema `usage.v1`），預設關：
環境變數 `CODEFORGE_USAGE_V1` 未設就只印「關」的說明。值就是來源：檔案路徑或 `http(s)://` URL；
URL 時 bearer token 從 `CODEFORGE_USAGE_V1_TOKEN_FILE` 指的檔案讀（token 不進 argv、不印出）。

- 信任邊界只驗 `schema`、`pools` 是陣列、輸入大小（≤ 256 KiB）、逾時 5 秒；某個 window 缺欄位或型別錯，
  只有那個 window 是「未知」，其餘照畫；多出來的未知鍵接受。
- `reset_passed`、`stale` 在讀取時用注入的 `now_ms` 重新套用，hub 已設的旗標絕不清掉。
  已過重置時間的 window 不畫 pct（畫「已重置，等新讀數」）；超過 2 小時的讀數或 stale 旗標畫「舊讀數，N 前」，
  絕不畫得像新的。來源本身的年齡另印一行。
- `pct` 是 0..1 的已用比例（fleet 01M73XAJAM 確認，hub 照 digest 原樣轉出）。capacity 的 `available` 等欄位是 0..100 的 pct_points_7d，單位不同，client 目前不讀也不顯示。
- 程式：`src/usage/usage_v1.rs`；測試 `src/usage/usage_v1_tests.rs`，fixture 在 `tests/fixtures/usage-v1/`。

## 指令

```bash
codeforge usage                     # 收集 + 最近 7 天，依 model 分組
codeforge usage --days 3 --by project
codeforge usage --json
codeforge usage collect --quiet     # hook 用
codeforge usage collect --claude-dir /srv/claude/seat-a --claude-dir /srv/claude/seat-b  # 本次掃描並記住（絕對路徑）
codeforge usage push                # 印出 blob sha
codeforge usage export              # 印出 push 會上傳的 bytes（驗證後拿掉 email 與 name，不連線、不寫檔），給 hangar 用 ssh 收
codeforge usage export --hourly     # 同一趟收集後改印每小時（UTC）每帳號每模型的 Claude token 列（codeforge.usage-hourly/1）；沒有 email、name、org、project、路徑
codeforge usage trinity-import <FILE|->  # 匯入 trinity 自報配額（trinity-usage/1）；`-` 讀標準輸入。不連線、不讀憑證
codeforge usage accounts [--json]   # G40 配額檢視：讀 autopilot 的配額 mirror（只讀，不探測、不連網）
codeforge usage pools [--json]      # usage.v1 配額池（CODEFORGE_USAGE_V1 未設 = 關）
codeforge usage fleet [--dir <D>] [--json] [--out <file>] [--use-target 0.85]  # 全帳號配額表（多主機 summary 合併）
codeforge usage alias list          # 帳號與顯示名（本機）
codeforge usage alias set <email或顯示名> <新名字>
codeforge usage note set <provider|名字> <文字>   # 本機備註（最多 40 欄），顯示在全帳號表列尾
codeforge usage note set --new <名字> <文字>      # 沒有任何資料的帳號：以名字當鍵，`usage fleet` 多一列 no_source
codeforge usage note set <provider|名字> <文字> --reset-weekly fri@17:00+08:00  # 另存每週重置規則，`usage fleet` 算出 note_resets_at
codeforge usage note set <provider|名字> <文字> --reset-monthly 17@00:00+08:00  # 每月 17 日（沒有這天就月底）
codeforge usage note set <provider|名字> <文字> --reset-at 2026-11-21T00:00:00+08:00  # 一次性，過了就不顯示倒數
codeforge usage note reset <provider|名字> <規則>   # 只設規則（三種形式都收）
codeforge usage note clear <provider|名字>          # 備註與規則一起清
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
