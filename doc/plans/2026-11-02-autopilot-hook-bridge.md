# Plan — hooks 改由 autopilot bridge 呼叫：`codeforge hook on <event>`（v0.0.8）

> Status: contract（owner 卡 d-0ce83828 選 C：先凍結成與 autopilot 的介面契約，實作等 autopilot alpha.7 之後）· Owner: cookys · Branch: `feature/autopilot-hook-bridge` · Size: L
> BACKLOG: B31（hooks 段提前） · Project: [`doc/projects/2026-11-02-autopilot-hook-bridge/`](../projects/2026-11-02-autopilot-hook-bridge/README.md)

## 0. Context

- v0.0.7（B35）讓 global install 寫出的指令不帶本機路徑與版號，但 settings.json 仍是跟著 dotfiles 走、全部機器共用的一份，各機 binary 版本不同步：寫進 `codeforge hook …` 後，PATH 上還是 0.0.6 的機器 exit 2 擋住 PreCompact，沒裝 codeforge 的機器每次 session 結束都報錯。
- 7840hs 驗過 0.0.7 的 dry-run 後，那邊的 owner 決定不跑 bootstrap，改請 codeforge 的 hooks 由 autopilot plugin 註冊（fleet 訊息 `01M6CDDP51EF708E14PQ4W2V4N`）。
- owner 2026-11-02 在卡 d-d9be662c 選 A：B31 的 hooks 段提前做，不等 autopilot alpha.7。B31 其餘段（band、status 片段、設定鍵、binary 需求）照原排程。
- 介面已與 autopilot 對齊（fleet `01M6CDGZTHGHDFK7EMAARCH11V` → `01M6CDHXR3N1SSVW1VJCEN6HQW` → `01M6CDKJK52NB0AX9BM7G4TVJE` → `01M6CDKQQT6KZS1NGQWAXKW5PM`）。autopilot 端的 bridge 由 autopilot 自己實作，不在本計畫範圍；bridge 預設開或關由 autopilot 的卡 d-6812e1c5 決定。

## 1. Problem

| # | 問題 | 位置 | 後果 |
|---|---|---|---|
| P-a | 8 條 global hook 寫在共用 settings.json，每條直接是 codeforge 子指令 | `src/cli/install.rs` 約 L680-735 | 任一台用新版改寫後，舊版或沒裝 codeforge 的機器在每個事件報錯或擋下 compaction |
| P-b | 事件對應哪些指令寫死在 settings.json | 同上 | 每次增減指令都要改共用檔、重跑 install |
| P-c | SessionStart 有兩條指令各自輸出 additionalContext | 同上 | 外部 bridge 若依序跑兩條再轉 stdout，會是兩段 JSON 接在一起（無效） |
| P-d | SessionEnd 的 session-digest／dream／ship 各 30s 逾時、同步執行 | 同上 | bridge 必須給很長的逾時；session 結束被卡住 |
| P-e | install 不知道 autopilot 已經呼叫 codeforge | — | 兩邊都註冊時同一事件跑兩次（記憶注入兩次、dream 跑兩次） |

## 2. OKR / KRs

**O**：autopilot bridge 呼叫一條 `codeforge hook on <event>` 就跑完該事件的全部 codeforge 工作；install 偵測到 bridge 時不再寫 hooks，並清掉舊項目。

| KR | 門檻 | 驗證方式 |
|---|---|---|
| KR1 | `codeforge hook on --help` exit 0；`hook on <未知事件>` exit 0 且無 stdout、stderr 列出三個事件名 | binary 測試 |
| KR2 | `hook on session-start` 在 12 秒內返回（含子行程忽略 SIGTERM），stdout 是空的或恰好一個 JSON 物件，`hookEventName == "SessionStart"`，additionalContext 是兩條 context 文字的合併，長度 ≤ 9500 chars | 單元測試（合併純函式：0/1/2 段、超長、非 JSON、欄位不是字串）+ binary 測試 |
| KR3 | `hook on session-end` 在 3 秒內、`hook on pre-compact` 在 2 秒內返回，stdout 為空（含同步子指令忽略 SIGTERM 的情況）；stdin 未完整讀到 EOF 時不啟動任何子指令；detach 出去的 worker 在父行程結束、且父行程的 process group 收到 SIGHUP／SIGTERM 後仍跑完（寫出完成標記）；逾時會收掉該步的整個 process group（含孫行程）；同一 session 兩次重疊呼叫時，第二條鏈在第一條的 ship 回收前不開始 | binary 測試用假的子指令鏈（測試 seam）量時間、等標記、查孫行程 pid |
| KR4 | 同一 session_id + event 在 TTL 內第二次呼叫是 no-op（不重跑、session-start 不再輸出） | 單元測試（stamp 純函式）+ binary 測試 |
| KR5 | `hook on` 任何情況 exit 0（含 stdin 不是 JSON、stdin 非 UTF-8 或超過 1 MiB、子指令 exit 2、資料目錄不可寫、未知事件、未知旗標）；每個子指令收到的 stdin 與 hook 收到的逐 byte 相同 | binary 測試 |
| KR6 | 單機 settings.json 偵測到 autopilot bridge（§3.5 五項條件），或帶 `--hooks-via autopilot`（任何檔）時：不新增 hook，清掉所有 codeforge 擁有的 hook 項目（marker `codeforge@*` 與舊的 node 形式），非 codeforge 的 hook 原樣保留；statusLine、cleanupPeriodDays 規則不變；印出一行說明。共用檔偵測到 bridge 但沒帶旗標時：hooks 照 0.0.7 處理並印警告，不清任何項目 | 單元測試（偵測純函式：五項條件各缺一項、schema 不是 1、config 形狀不合 → 不成立；單機／共用／旗標三種 patch 結果）+ e2e dry-run（單機、symlink 兩種） |
| KR7 | 偵測不到 bridge 時，install 輸出與 0.0.7 byte 相同 | 單元測試（既有 global install 測試不改期望值） |
| KR8 | 品質閘全綠；v0.0.8 發 full release；回覆 autopilot 與 7840hs | `./scripts/fmt.sh --check`、`cargo clippy -- -D warnings`、`cargo test`、`python3 scripts/check-doc-drift.py`、`bash scripts/check-cjk-safe.sh`；main CI 綠；`gh release view v0.0.8` |

## 2.5 Global Constraints（原文照抄進每個 dispatch）

- `hook on` 一律 exit 0，自己的錯誤只寫 stderr；絕不 exit 2。
- session-start 的 stdout 只能是空的，或恰好一個 `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":…}}`；session-end、pre-compact 的 stdout 一律為空。
- 事件名固定為 `session-start`、`session-end`、`pre-compact`（已告知 autopilot）。
- `hook on` 執行時的 cwd 就是 hook 的 cwd，不得 chdir；子指令沿用同一個 cwd（dream 讀 per-cwd `.codeforge`）。
- SessionEnd 內 dream 必須在 ship 之前完成（ship 讀 dream 剛產出的 L1）。
- 偵測不到 bridge 時，install 的行為與輸出完全不變。
- 不新增 crate 依賴。
- 不在作者的開發機上安裝 codeforge hooks；e2e 一律用 `CODEFORGE_CLAUDE_SETTINGS`、`CODEFORGE_DATA_DIR` 與新增的 plugins 路徑覆寫指向 scratch。
- 使用者看得到的訊息用正體中文；CJK 截斷用 `.chars().take(N)`。
- 格式化只用 `./scripts/fmt.sh`。

## 2.6 Change-policy decisions

- **Compatibility impact**: `published-compatible`，有升級順序。`hook on` 是新增子指令；`hook <name>` 照舊。偵測不到 bridge 時 install 不變。偵測到時會從 settings.json 清掉 codeforge hooks；若這份是共用檔，所有讀它的機器都失去直接註冊的 hooks，必須已經裝好帶 bridge 的 autopilot。順序：(1) 每台 `self-update` 到 0.0.8；(2) 每台更新到帶 bridge 的 autopilot；(3) 才在任一台跑 bootstrap。CHANGELOG、README 與 autopilot release notes 都寫明。混用期間（bridge 與舊 settings 項目並存）會重複觸發；KR4 的 stamp 擋不到舊項目，根本解是 sweep，所以升級順序要盡快走完。
- **Dependency decision**: `platform/stdlib` + `existing`（`serde_json`、`std::process`、既有 `platform.rs` 的 process_group 用法）。

## 3. 設計

### 3.1 `codeforge hook on <event>`

- 分派：`hook on` 不走 `hook::run` 的既有契約（`Err` → exit 1、`Ok(code)` → 子行程結束碼）。`mod.rs` 在呼叫 `hook::run` 之前就把 `name == "on"` 轉給 `hook::on::run(args)`，回來後一律 `exit(0)`。`on::run` 自己解析 `args`（`--help`、`--detached <event> <payload>`、事件名、未知旗標），不交給 clap；所有錯誤、子行程結束碼只寫 stderr 與 log（正體中文），絕不影響結束碼。`hook --help` 的用法說明加一行 `on`。
- 牆鐘預算：從行程啟動開始計時，包含讀 stdin、子行程執行與回收。session-start 全程 ≤ 12s（bridge 15s）；session-end、pre-compact 的同步段全程 ≤ 3s（bridge 5s）。預算到點就停止等待、照規則輸出（或不輸出）並 exit 0。
- stdin：在背景執行緒讀到 EOF，保存原始 bytes，不設上限、不轉 String。**完整讀到 EOF 才啟動該事件的任何子指令**；作業系統讀取錯誤，或讀取超過預算的一半（session-start 6s、其他 1.5s）仍未到 EOF，就 stderr 記一行、整個事件跳過並 exit 0，絕不以空的或殘缺的 stdin 啟動子指令。完整讀到的資料即使不是 JSON 或 UTF-8，也逐 byte 原樣轉交。`session_id` 另外從這份 bytes 做 JSON 解析，解析失敗不改動要轉交的 bytes。
- 子指令：一律是 `current_exe()` 加參數的子行程，cwd 繼承，stdin 寫入同一份原始 bytes，每個子行程自成一個 process group（`process_group(0)`）。**沒有任何子行程繼承父行程的 stdout**：要用輸出的就 pipe 擷取，其餘導到 log。
- 逾時回收：對子行程的 process group 送 SIGTERM，等 300 ms，仍在就送 SIGKILL，再有上限地 wait（≤ 500 ms）。這樣 wrapper 底下的 node 等後代也一起收掉。Windows 沿用 `platform.rs` 現有的終止方式。wait 上限到了仍無法確認結束，稱為「回收未確認」：父行程（同步段）照樣停止等待並返回；worker 的處理見 §3.3。

### 3.2 session-start

- 平行跑三個子行程：`hook emit-session session_start`（stdout 導 log）、`memory context --hook`、`mnemos-cli context --hook --max 5 --with-themes --max-sensitivity work`（後兩者 stdout 擷取）。
- 子行程截止 10s，之後依 §3.1 回收，整體仍在 12s 預算內。已完成的那段照常合併，不因另一段失敗或逾時而丟掉。
- 合併：每段擷取到的 stdout trim 後用 serde_json 解析，只取 `hookSpecificOutput.additionalContext` 字串；非 JSON、缺欄位或不是字串就丟棄那段（stderr 記一行）。依固定順序（本地 memory 在前、mnemos 在後）以空白行接起來，`.chars().take(9500)`，再用 serde_json 重新輸出**恰好一個** `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":<合併字串>}}`。沒有任何一段成功就完全不寫 stdout。子行程的原始 stdout 絕不直接拼進父行程 stdout。

### 3.3 session-end、pre-compact

- session-end 父行程：同步跑 `hook emit-session session_end`（stdout/stderr 導 log，截止 2s，依 §3.1 回收），然後寫 payload 檔並 spawn detached worker `codeforge hook on --detached session-end <payload 檔>`，立刻返回。父行程不對 stdout 寫任何 byte。
- pre-compact 父行程：不跑同步子指令，直接寫 payload 檔並 spawn worker（`--detached pre-compact`）。
- payload 檔：每次被接受的呼叫各自一個，路徑 `<data dir>/hooks/pending/<session_id>-<event>-<pid>-<nanos>.json`，以 `O_CREAT|O_EXCL`、0600 建立，完整寫入並關閉後才 spawn worker；spawn 失敗由父行程刪除。worker 只讀、只刪自己那一個。
- worker（detached）：`process_group(0)`，stdin 為 null，stdout/stderr 導到 `<data dir>/logs/hook-on.log`（附加寫入、超過 1 MiB 時改名成 `.1`），不繼承父行程任何 pipe。新 process group 不是終端的前景 group，終端關閉時的 SIGHUP 不會送到它。
- 序列化：worker 先取 session lock `<data dir>/hooks/pending/<session_id>.lock`（`O_CREAT|O_EXCL`，內容寫 worker pid、目前子行程的 pgid 與時間，每換一步就改寫），從 session-digest 之前一直持有到最後一個子行程確認回收才刪除。取不到就每秒重試，最多 120s，仍取不到就記 log 並放棄（刪自己的 payload）。缺 session_id 時 lock 名用 `nosession`。
- 殘留 lock 的接管不看檔案年齡：只有在 lock 裡記的 worker pid 與子行程 pgid 都已不存在（`platform::pid_alive` 為否；pid 仍在但 `platform::cmdline_contains` 不含 `codeforge` 視為 pid 已被重用）時才可接管，接管用 rename 搶（沿用 brain-indicators 的 O_EXCL rename-steal 做法）。lock 內容無法解析時，超過 1 小時才接管並記 log。
- 回收未確認（§3.1）時，worker 不啟動後續任何步驟（特別是不跑 ship），也不刪 lock，記 log 後結束；同 session 的下一條鏈會在接管檢查時看到舊 pgid 仍在而等待或放棄，直到它真的結束。
- worker 步驟：session-end 依序 `hook session-digest`（stdin 餵 payload 檔）→ `dream --quiet` → `ship --no-hook`；pre-compact 只有 `hook session-digest`。各步截止 30s／120s／60s，逾時依 §3.1 回收整個 process group。前一步失敗、或逾時且已確認回收，都不擋後一步；前一步回收未確認就中止整條鏈（dream 確認結束後才跑 ship）。
- 可觀察：每一步的開始、結束碼或逾時各寫一行到 hook-on.log（含 session_id、event、耗時）。殘留超過 7 天的 payload、lock、stamp 由下一次 `hook on` 清掉。`codeforge doctor` 顯示這份 log 不在本計畫範圍。

### 3.4 冪等

- 只對 session-end 與 pre-compact 生效，作用僅是擋 bridge 自己在短時間內被重送；**不重疊靠 §3.3 的 lock，不靠 stamp**。stamp 檔 `<data dir>/hooks/stamps/<event>-<session_id>`：`O_CREAT|O_EXCL` 建立成功才繼續；已存在且 mtime 在 60s 內就 no-op（不寫 payload、不 spawn），超過 60s 就更新 mtime 並繼續（resume 後的同 id 合法事件照跑）。
- session-start 不做 stamp：`/clear`、resume 會以同一個或新的 session_id 合法地再觸發 SessionStart，壓掉會讓新 context 拿不到記憶。session-start 只輸出、不改狀態，重複的代價只是同一 session 多注入一次。
- stamp 擋不到舊 settings 項目（它們直接呼叫子指令），重複觸發的根本解是 §3.5 的 sweep。缺 session_id 時不做 stamp、照常執行。

### 3.5 install 偵測 autopilot bridge（契約：autopilot fleet 訊息 `01M6CEGXJAHY5NCTZGJ4N0GBQ4`，autopilot 卡 d-92a2231d 定案採通用提供者清單）

- autopilot 的 hooks.json 只註冊它自己的 bridge；bridge 讀提供者清單再呼叫 codeforge，hooks.json 裡不會出現 `codeforge` 字樣，所以 codeforge 檢查的是清單與停用設定。
- 純函式 `bridge_registered(installed_plugins: &Value, settings: &Value, autopilot_config: Option<&Value>, read_file: impl Fn(&Path) -> Option<String>) -> Option<PathBuf>`，五項都成立才回傳 installPath：
  1. `installed_plugins.json`（`version == 2`）`plugins` 裡 key 以 `autopilot@` 開頭的項目，取各 `installPath`；
  2. settings 的 `enabledPlugins[<同一個 key>] == true`；
  3. `<installPath>/hooks/hook-providers.json` 存在、是 JSON，且 `schema == 1`（其他值一律當作沒有 bridge）；
  4. 其 `providers` 陣列裡有一項 `id == "codeforge"`（其他欄位給 bridge 用，codeforge 不驗）；
  5. `~/.autopilot/config.json` 沒有把它關掉：檔案不存在、沒有 `hook_providers` 鍵、沒有 `codeforge` 子鍵、沒有 `enabled` 欄位，或 `enabled == true`，都算開啟；`enabled == false` 算關掉。`hook_providers` 或 `codeforge` 不是物件、`enabled` 有寫但不是布林值、或 config.json 不是合法 JSON，都當作沒有 bridge。
- 檔案路徑覆寫（測試用）：`$CODEFORGE_CLAUDE_PLUGINS_FILE`（預設 `~/.claude/plugins/installed_plugins.json`）、`$CODEFORGE_AUTOPILOT_CONFIG`（預設 `~/.autopilot/config.json`）。
- 新旗標 `install --hooks-via <settings|autopilot>`（預設自動偵測）；`bootstrap` 同樣接受。
- 偵測成立且 settings.json 是單機檔：`patch_hooks` 走既有的 sweep，但不加回任何 global hook；印 `ℹ hooks 由 autopilot 註冊（<installPath>），settings.json 不寫 codeforge hooks，已清掉 N 條舊項目`。
- 偵測成立但 settings.json 是共用檔（B35 的 `shared_settings()`：symlink 或 git 追蹤）：本機的偵測無法證明其他機器也有 bridge，所以**不自動清**。hooks 照 0.0.7 的方式處理，並印警告：說明清掉會影響所有共用這份檔的機器，確認每台都已更新 autopilot 後，用 `--hooks-via autopilot` 明確執行。這段期間本機會重複觸發（bridge＋settings 項目），警告裡寫明要盡快完成。
- `--hooks-via autopilot`：不檢查偵測結果，直接 sweep、不加回（共用檔也照做，dry-run 先列出會清掉的項目）。`--hooks-via settings`：不偵測，照 0.0.7。
- `--project-hooks` 不受影響。

## 4. Phases

| Phase | 內容 | 驗收 |
|---|---|---|
| P1 | `hook on` 骨架：分派、`--help`、stdin、未知事件、永遠 exit 0；session-start 平行跑與合併 | KR1、KR2、KR5 |
| P2 | session-end／pre-compact detach、獨立 payload 檔、session lock、process group 回收、log；冪等 stamp | KR3、KR4 |
| P3 | install 偵測、`--hooks-via`、sweep 不加回 | KR6、KR7 |
| P4 | README（安裝與升級順序）、CLAUDE.md Hook Path Note、CHANGELOG 0.0.8、版本號、e2e（scratch 內模擬 bridge 呼叫三個事件） | KR8 |

## 5. 不做

- autopilot 端的 bridge、hooks.json、bridge 預設開關（autopilot 自己做）。
- statusLine／subagentStatusLine 搬家（不是 hook）。
- B28（`--force` 範圍）：本計畫之後再做。
- B31 其他段。

## 6. U1 consult（gpt-6.1-sol low，run `author-1793565868-3381949`）與處置

| 問題 | consult 意見 | 處置 |
|---|---|---|
| detached 存活 | 證據不足，要有實測 | §3.3 加 binary 測試：父行程 group 收 SIGHUP／SIGTERM 後仍完成；不另加 `setsid`（需要 libc，違反不加依賴；新 group 已不在前景） |
| 合併輸出 | 每段獨立驗證、一段失敗不丟另一段；逾時子行程要 kill | §3.2 已改 |
| 冪等 | 同 id 的 SessionStart 可能是合法重觸發；stamp 擋不到舊項目 | §3.4：session-start 不 stamp，其他 60s TTL；根本解靠 sweep |
| 共用檔清 hooks | 本機偵測不能證明全機隊都有 bridge，警告不夠 | §3.5：共用檔不自動清，要 `--hooks-via autopilot` |
| 其他 | 12s 太貼 15s；detached 工作要可觀察、會重疊 | §3.2 改 10s；§3.3 加 log、lock 序列化、殘留清理 |

## 7. Review log

- g1（2026-11-02，gpt-6-astra low STOP、grok-4.7 high STOP）：8 條全採納（6 blocker、2 non-blocking），處置見 `2026-11-02-autopilot-hook-bridge.g1-dispositions.json`。改動：§3.1 結束碼一律 0 且不經 `hook::run`、stdin 原始 bytes 不設上限、單一牆鐘預算、TERM→KILL 回收整個 process group；§3.2 只用解析出的字串重新輸出一個物件；§3.3 子行程絕不繼承 stdout、每次呼叫獨立 payload 檔、lock 涵蓋 digest 到 ship、可搶門檻 15 分鐘；§3.4 stamp 只擋重送。
- g2（2026-11-02）：gpt-6-astra low 給 STOP，2 條 blocker（stdin 未讀完不得餵空或殘缺的 stdin；回收未確認時不得繼續 ship 或釋放 lock，殘留 lock 不能只看年齡接管）。grok-4.7 這輪傳輸失敗（exit 3、無輸出），備援 kimi 前置條件不符，rail 以 `required_seat_transport_exhausted` 結束，所以 astra 的兩條沒進 artifact，原文存在 `2026-11-02-autopilot-hook-bridge.g2-astra-raw.json`。兩條都成立，depth-0 裁定採納並併入 §3.1、§3.3、KR3。凍結檢查對送審快照 exit 0。**g2 只有一個模型家族實際審過**；實作開工時（autopilot alpha.7 之後）以這份契約為底，再跑一次兩席的 plan review。
- 同一輪（g1 後）併入 autopilot 定案的偵測契約（不是 review 意見）：§3.5 由「hooks.json 含 `codeforge hook on`」改成提供者清單 `hooks/hook-providers.json` 加 `~/.autopilot/config.json` 停用設定，共五項。

