# Plan — hooks 改由 autopilot bridge 呼叫：`codeforge hook on <event>`（v0.0.8）

> Status: draft · Owner: cookys · Branch: `feature/autopilot-hook-bridge` · Size: L
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
| KR2 | `hook on session-start` 的 stdout 是空的或恰好一個 JSON 物件，`hookEventName == "SessionStart"`，additionalContext 是兩條 context 文字的合併，長度 ≤ 9500 chars | 單元測試（合併純函式：0/1/2 段、超長、非 JSON 輸入）+ binary 測試 |
| KR3 | `hook on session-end` 與 `hook on pre-compact` 在 2 秒內返回、stdout 為空；detach 出去的子行程在父行程結束後仍跑完（寫出可觀察的完成標記） | binary 測試用假的子指令鏈（測試 seam）量時間、等標記 |
| KR4 | 同一 session_id + event 在 TTL 內第二次呼叫是 no-op（不重跑、session-start 不再輸出） | 單元測試（stamp 純函式）+ binary 測試 |
| KR5 | `hook on` 任何情況 exit 0（含 stdin 不是 JSON、子指令失敗、資料目錄不可寫） | binary 測試 |
| KR6 | 單機 settings.json 偵測到 autopilot bridge（四項條件），或帶 `--hooks-via autopilot`（任何檔）時：不新增 hook，清掉所有 codeforge 擁有的 hook 項目（marker `codeforge@*` 與舊的 node 形式），非 codeforge 的 hook 原樣保留；statusLine、cleanupPeriodDays 規則不變；印出一行說明。共用檔偵測到 bridge 但沒帶旗標時：hooks 照 0.0.7 處理並印警告，不清任何項目 | 單元測試（偵測純函式：四項條件各缺一項 → 不成立；單機／共用／旗標三種 patch 結果）+ e2e dry-run（單機、symlink 兩種） |
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

- **Compatibility impact**: `published-compatible`，有升級順序。`hook on` 是新增子指令；`hook <name>` 照舊。偵測不到 bridge 時 install 不變。偵測到時會從 settings.json 清掉 codeforge hooks；若這份是共用檔，所有讀它的機器都失去直接註冊的 hooks，必須已經裝好帶 bridge 的 autopilot。順序：(1) 每台 `self-update` 到 0.0.8；(2) 每台更新到帶 bridge 的 autopilot；(3) 才在任一台跑 bootstrap。CHANGELOG、README 與 autopilot release notes 都寫明。混用期間的重複觸發由 KR4 的冪等擋下。
- **Dependency decision**: `platform/stdlib` + `existing`（`serde_json`、`std::process`、既有 `platform.rs` 的 process_group 用法）。

## 3. 設計

### 3.1 `codeforge hook on <event>`

- clap：`Hook` 現在是 `{ name, args }`，`on` 會被當成 name。改法：在 `hook::run` 開頭把 `name == "on"` 轉給新的 `hook::on::run(args)`；`codeforge hook on --help` 由這條路自行處理（印用法、exit 0），因為 `args` 是 trailing_var_arg、`--help` 會原樣進來。`hook --help` 的用法說明加一行 `on`。
- 讀 stdin 一次（上限 1 MiB，讀不到就當空字串），解析出 `session_id`（缺就用空字串，冪等跳過）。
- 每個子指令都是 `current_exe()` 加參數的子行程，stdin 餵同一份 bytes，cwd 繼承。

### 3.2 session-start

- 平行跑三個子行程：`hook emit-session session_start`、`memory context --hook`、`mnemos-cli context --hook --max 5 --with-themes --max-sensitivity work`。
- 整體截止 10s（bridge 給 15s，留 5s 給探測、啟動與轉送）：截止時還沒結束的子行程一律 kill 並 wait 回收，輸出丟棄；已完成的那段照常合併，不因另一段失敗或逾時而丟掉。
- 合併：每段 stdout trim 後解析成 JSON，取 `hookSpecificOutput.additionalContext`；非 JSON 或缺欄位就丟棄那段（stderr 記一行）。依固定順序（本地 memory 在前、mnemos 在後）以空白行接起來，`.chars().take(9500)`。全部為空就不輸出。

### 3.3 session-end、pre-compact

- session-end：同步跑 `hook emit-session session_end`（2s 截止），然後 detach 一個子行程 `codeforge hook on --detached session-end <payload 檔>`，立刻返回。
- detached 子行程：依序跑 `hook session-digest`（stdin 餵 payload 檔）→ `dream --quiet` → `ship --no-hook`，每條有自己的截止時間（30s／120s／60s），前一條失敗不擋後一條，但 dream 未結束前不跑 ship。結束時刪除 payload 檔。
- pre-compact：只 detach `hook session-digest`。
- detach 方式：`process_group(0)`（沿用 `platform.rs`），stdin 用 payload 檔，stdout/stderr 導到 `<data dir>/logs/hook-on.log`（附加寫入、超過 1 MiB 時改名成 `.1`），不繼承父行程的 pipe。新 process group 不是終端的前景 group，終端關閉時的 SIGHUP 不會送到它；P2 用 binary 測試實證：父行程所在的 process group 收到 SIGHUP／SIGTERM 後，detached 工作仍寫出完成標記。
- 序列化：同一個 session 的 detached 工作（PreCompact 的 digest 與 SessionEnd 的鏈）以 `<data dir>/hooks/pending/<session_id>.lock`（`O_CREAT|O_EXCL`，超過 10 分鐘視為殘留可搶）排隊，最多等 120s，等不到就記 log 放棄，不重疊執行。
- 每一步的結果（成功、失敗、逾時）寫一行到 hook-on.log，`codeforge doctor` 不在本計畫範圍。payload 檔在 detached 工作結束時刪除；啟動失敗時由父行程刪除；殘留超過 7 天的 payload 與 lock 由下一次 `hook on` 清掉。
- payload 檔放在 `<data dir>/hooks/pending/<session_id>-<event>.json`，0600。

### 3.4 冪等

- 只對 session-end 與 pre-compact 生效：stamp 檔 `<data dir>/hooks/stamps/<event>-<session_id>`，`O_CREAT|O_EXCL` 建立成功才執行；已存在且 mtime 在 60s 內就 no-op，超過 60s 就覆寫並執行（resume 後的同 id 合法事件照跑）。
- session-start 不做 stamp：`/clear`、resume 會以同一個或新的 session_id 合法地再觸發 SessionStart，壓掉會讓新 context 拿不到記憶。session-start 只輸出、不改狀態，重複的代價只是同一 session 多注入一次。
- stamp 只擋 bridge 自己被重送；舊 settings 項目直接呼叫子指令，不經過 stamp，所以重複觸發的根本解是 §3.5 的 sweep。
- 缺 session_id 時不做 stamp、照常執行。每次執行順手刪掉超過 7 天的 stamp。

### 3.5 install 偵測 autopilot bridge

- 純函式 `bridge_registered(installed_plugins: &Value, settings: &Value, read_hooks: impl Fn(&Path) -> Option<String>) -> Option<PathBuf>`：
  1. `installed_plugins.json`（version 2）`plugins` 裡 key 以 `autopilot@` 開頭的項目，取各 `installPath`；
  2. settings 的 `enabledPlugins[<同一個 key>] == true`；
  3. `<installPath>/hooks/hooks.json` 存在；
  4. 其中某條 hook 的 command 含 `codeforge hook on`。
  四項都成立才回傳 installPath。
- `installed_plugins.json` 路徑：`$CODEFORGE_CLAUDE_PLUGINS_FILE`（測試用）否則 `~/.claude/plugins/installed_plugins.json`。
- 新旗標 `install --hooks-via <settings|autopilot>`（預設自動偵測）；`bootstrap` 同樣接受。
- 偵測成立且 settings.json 是單機檔：`patch_hooks` 走既有的 sweep，但不加回任何 global hook；印 `ℹ hooks 由 autopilot 註冊（<installPath>），settings.json 不寫 codeforge hooks，已清掉 N 條舊項目`。
- 偵測成立但 settings.json 是共用檔（B35 的 `shared_settings()`：symlink 或 git 追蹤）：本機的偵測無法證明其他機器也有 bridge，所以**不自動清**。hooks 照 0.0.7 的方式處理，並印警告：說明清掉會影響所有共用這份檔的機器，確認每台都已更新 autopilot 後，用 `--hooks-via autopilot` 明確執行。這段期間本機會重複觸發（bridge＋settings 項目），session-end／pre-compact 的重複由 §3.4 擋不住（舊項目不經 stamp），所以警告裡寫明要盡快完成。
- `--hooks-via autopilot`：不檢查偵測結果，直接 sweep、不加回（共用檔也照做，dry-run 先列出會清掉的項目）。`--hooks-via settings`：不偵測，照 0.0.7。
- `--project-hooks` 不受影響。

## 4. Phases

| Phase | 內容 | 驗收 |
|---|---|---|
| P1 | `hook on` 骨架：分派、`--help`、stdin、未知事件、永遠 exit 0；session-start 平行跑與合併 | KR1、KR2、KR5 |
| P2 | session-end／pre-compact detach、payload 檔、log；冪等 stamp | KR3、KR4 |
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
