# Plan — install／bootstrap 對共用 settings.json 安全（v0.0.7）

> Status: frozen（hetero plan review g1+g2 完成，2026-11-02）· Owner: cookys · Branch: `feature/install-stable-paths` · Size: L
> BACKLOG: B35 · Project: [`doc/projects/2026-11-01-install-stable-paths/`](../projects/2026-11-01-install-stable-paths/README.md)

## 0. Context

- 7840hs 已用 `self-update` 升到 v0.0.6。它的 `~/.claude/settings.json` 是 symlink，指到 dotfiles 裡進版控、全部機器共用的檔案。owner 要求修好以下問題、發 v0.0.7 後才跑 `codeforge bootstrap`（fleet 訊息 `01M6BQ8BEDZKQM1TFARTSNYK0M`，2026-11-01）。
- owner 2026-11-01 指示：列進 BACKLOG（B35）、寫計畫、過 hetero plan review。
- 前作：`2026-06-15-ship-online`（global hooks、`cleanupPeriodDays`、sweep 規則）與 `2026-06-23-codeforge-bootstrap`。兩者已封存，不是草稿，沒有重疊的進行中計畫。
- 與 B31（codeforge 成為 autopilot extension、hooks 改由 autopilot 登記）的關係：B31 之後由 autopilot 登記 hooks 時，同樣需要一個不帶本機路徑與版號的入口。本計畫做的 `codeforge hook <name>` 正是那個入口，B31 直接沿用，不會白做。

## 1. Problem

`codeforge install --hooks|--all`（`bootstrap` 第 1 步就是它）寫進 settings.json 的內容綁死在「這台機器、這個版本」。對單機使用者沒事；但 settings.json 一旦是多台共用的檔案，就會出現下列問題。前三項是 7840hs 回報的，後三項是讀 code 時找到的：

| # | 問題 | 位置 | 後果 |
|---|---|---|---|
| P-a | hook 指令是帶版號的絕對路徑：`node <data_local_dir>/codeforge/hooks/<ver>/emit-session.js …` | `install.rs:115-131`、`448-461`、`520-583` | 每升一版、每換一台（含 macOS 的 `~/Library/Application Support`）共用檔都要改；別台機器上的路徑根本不存在 |
| P-b | `cleanupPeriodDays: 3650` 一律寫入（只要使用者沒設） | `install.rs:133-148`、`685-707` | 透過共用檔擴散到每台機器 |
| P-c | 不偵測 settings.json 是不是共用檔 | — | 使用者不知道寫入會擴散 |
| P-d | `statusLine.command` 是 `current_exe()` 的絕對路徑 | `install.rs:83-104` | 同 P-a（家目錄或安裝位置不同的機器上失效） |
| P-e | marker 是 `codeforge@<CARGO_PKG_VERSION>`，寫進每個 entry | `install.rs:351-353` | 升版後重跑 install，即使內容沒變，共用檔每個 entry 都改一行 |
| P-f | `write_settings_atomic` 先寫 `settings.json.tmp` 再 `rename` 到原路徑 | `install.rs:326-331` | `rename(2)` 會把 symlink 本身換成一般檔案：寫入後 `~/.claude/settings.json` 跟 dotfiles 脫鉤，dotfiles 完全沒更新。這是 7840hs 真跑 bootstrap 時最嚴重的問題 |

## 2. OKR / KRs

**O**：在共用 settings.json 上跑 `codeforge install`／`bootstrap`，寫出的內容在每台機器、每個版本都一樣，而且寫進共用檔本身。

| KR | 門檻 | 驗證方式 |
|---|---|---|
| KR1 | global install 寫出的 emit-session／session-digest entry 是 `codeforge hook …`；所有 global hook `command` 都不含絕對路徑、`$HOME`、`~`、`/codeforge/hooks/` 或版號字串（`2>/dev/null` 這類重導向不算路徑，既有 dream／ship／memory／mnemos-cli 字串原樣保留） | 單元測試：對 `patch_hooks(global)` 的輸出逐條斷言（不做 `contains('/')`）；e2e：temp settings dry-run 後 grep |
| KR2 | 版本 A 與版本 B 的 install 對同一份檔案寫出 byte 相同的內容 | 單元測試：marker 不含 `CARGO_PKG_VERSION`；連跑兩次 install 輸出相同 |
| KR3 | settings.json 是 symlink 時，install 後它仍是 symlink，新內容寫在 target 上 | 單元測試（tempdir symlink；修之前要先紅） |
| KR4 | 共用檔（symlink 或在 git 版控中）預設不寫 `cleanupPeriodDays`；單機檔維持現況；`--cleanup-period-days <N>` 一律寫入 | 單元測試三種情況 |
| KR5 | 共用檔時，install／bootstrap（含 dry-run）印出警告，指出 symlink target 與所屬 git repo | 單元測試（純函式）+ e2e dry-run 輸出 |
| KR6 | `codeforge hook <name> [args…]` 把 stdin 轉給腳本、回傳腳本的 exit code；名稱不在清單內時 exit 1 並列出可用名稱 | 整合測試（需要 `node`，沒有時 skip 並印出原因） |
| KR7 | 品質閘全綠 | `./scripts/fmt.sh --check`、`cargo clippy -- -D warnings`、`cargo test`、`python3 scripts/check-doc-drift.py`、`bash scripts/check-cjk-safe.sh`；merge 後 main CI 綠 |
| KR8 | v0.0.7 發成 full release，`self-update --check` 看得到；回覆 7840hs | `gh release view v0.0.7`；fleet reply 的 message id |

## 2.5 Global Constraints（原文照抄進每個 dispatch）

- 寫進 global settings.json 的 hook `command` 不得含絕對路徑、`$HOME`、`~` 或版號；一律是 `codeforge <subcommand> …`，與既有 `codeforge dream --quiet 2>/dev/null || true` 同樣依賴 PATH。
- global install（hooks、statusLine、subagentStatusLine）的 marker 值固定為 `codeforge@1`（install 契約版本，不是套件版本）；`--project-hooks` 仍寫 `codeforge@<CARGO_PKG_VERSION>`。判斷是否為 codeforge 的規則維持 `starts_with("codeforge@")`，舊的 `codeforge@0.0.x` entry 照樣會被 sweep。
- `--project-hooks` 的行為與輸出完全不變（command 已經用 `${CLAUDE_PROJECT_DIR}`，marker 維持帶版號）。
- 不新增 crate 依賴。
- 不在作者的開發機上安裝 codeforge hooks；e2e 一律用 `CODEFORGE_CLAUDE_SETTINGS` 指向 scratch 的 temp 檔。
- 使用者看得到的訊息用正體中文；CJK 截斷用 `.chars().take(N)`。
- 格式化只用 `./scripts/fmt.sh`。

## 2.6 Change-policy decisions

- **Compatibility impact**: `published-compatible`，但共用檔有升級順序。新 install 寫出不同的 command 字串與 marker，舊 entry（`codeforge@0.0.x` marker、`hooks/<ver>/` node 路徑）都會被既有 sweep 收掉再重寫，不會重複觸發。單機檔：沒重跑 install 的機器繼續用舊的 versioned 腳本，照常運作。共用檔：一旦任一台用 0.0.7 改寫，所有讀這份檔的機器都會執行 `codeforge hook …`；PATH 上還是 0.0.6 的機器會得到 clap 的未知子命令（exit 2，在 PreCompact 上會擋下 compaction）。所以順序是：每台先 `self-update` 到 0.0.7，再讓改寫後的 settings.json 進共用來源。不雙寫舊 node 路徑（會重複觸發）。CHANGELOG 與 P4 第 5 步寫明這個順序。`cleanupPeriodDays` 的預設對單機使用者不變。
- **Dependency decision**: `platform/stdlib` + `existing`。symlink 用 `std::fs::symlink_metadata`／`canonicalize`；git 版控偵測呼叫 `git` 執行檔（找不到 git 就略過這項偵測）；子行程用 `std::process::Command`。

## 3. File-structure map

| 檔案 | 改動 |
|---|---|
| `src/cli/install.rs` | P-a/P-d/P-e：hook 與 statusLine 的 command、marker；P-f：寫入穿透 symlink；P-b/P-c：共用檔偵測、`cleanupPeriodDays` 規則與警告；`InstallOpts` 加 `cleanup_period_days: Option<u64>` |
| `src/cli/hook.rs`（新） | `codeforge hook <name> [args…]`：從 binary 內嵌的腳本（沿用 `HOOK_SCRIPTS`）落地到私有快取、以繼承的 stdio 執行 `node`、回傳 exit code |
| `src/cli/mod.rs`、`src/main.rs` | 註冊 `hook` 子命令；`install`／`bootstrap` 加 `--cleanup-period-days <N>` |
| `src/cli/bootstrap.rs` | 把 `cleanup_period_days` 傳給 install；dry-run 時共用檔警告也要出現 |
| `README.md` | hooks 段：新的 command 形式、`codeforge hook`、共用 settings.json 的行為、`--cleanup-period-days` |
| `doc/specs/codeforge-install-subcommand.md` | 取代「腳本目錄帶版號、marker 綁 binary 版本」兩段的理由（§Hook target directory、§Idempotency） |
| `doc/windows.md`、`CLAUDE.md`（Hook Path Note）、`.claude/knowledge/environment.md` | 更新 `cleanupPeriodDays = 3650` 一律寫入的說法 |
| `CHANGELOG.md` | `[Unreleased]` → `[0.0.7]` |
| `Cargo.toml`、`Cargo.lock` | 0.0.7 |

## 4. Phases

### P1（S）— 寫入穿透 symlink + 共用檔偵測

1. **先寫紅測試** `write_through_symlink_keeps_link`：tempdir 建 `dot/settings.json` 與指向它的 `home/settings.json`，呼叫 `write_settings_atomic(home/settings.json, {"a":1})`，斷言 `home/settings.json` 仍是 symlink（`symlink_metadata().file_type().is_symlink()`），且 `dot/settings.json` 內容含 `"a": 1`。現行程式這個測試會紅。
2. dangling symlink 在讀取階段就擋下：`run()` 在 `load_settings` 前、`run_uninstall()` 在 `exists()` 判斷前呼叫 `check_settings_path(path)`（dangling 時 `exists()` 為 false，否則 uninstall 會走「不存在、沒事可做」），`symlink_metadata` 是 symlink 且 `canonicalize` 失敗時回錯（訊息含連結路徑與 `read_link` 目標）。dry-run 與實寫走同一個檢查，`bootstrap --dry-run` 不會把斷掉的連結當成空檔、印出假成功。
2b. 改 `write_settings_atomic`：若 `symlink_metadata(path)` 是 symlink，`target = fs::canonicalize(path)`（可解相對連結與連結鏈，寫到最終實體）；在 `target` 同目錄寫 `<target>.tmp.<pid>`，`sync_all` 後 rename 到 `target`；不是 symlink 時行為不變（但同樣補 `sync_all`）。保留 target 原本的權限（`fs::metadata(target).permissions()` 套到 tmp）。錯誤處理：
   - dangling symlink（`canonicalize` 失敗）：回錯並印出連結路徑與 `read_link` 的目標，**絕不**退回對連結本身做 rename。
   - target 目錄不可寫（例如 home-manager 的 `/nix/store`，EACCES／EROFS）：錯誤訊息指出 target 路徑。
   - 寫入或 rename 失敗時刪掉 tmp，不留 `*.tmp.<pid>`。
   - 加測試：dangling symlink 在 install 與 uninstall 都回錯且連結保持原樣；相對路徑 symlink 寫到正確 target。
3. 新純函式 `fn shared_settings(path: &Path) -> SharedSettings { symlink_target: Option<PathBuf>, git_repo: Option<PathBuf> }`：
   - `symlink_target`：`symlink_metadata` 是 symlink 時填 `canonicalize` 的結果。
   - `git_repo`：對 canonical 路徑跑 `git -C <parent> ls-files --error-unmatch -- <basename>`，exit 0 時再跑 `git -C <parent> rev-parse --show-toplevel` 取 repo 根目錄；git 不存在或任一指令失敗時為 `None`。兩個 `Command` 都要 `env_remove("GIT_DIR")`、`GIT_WORK_TREE`、`GIT_INDEX_FILE`，避免繼承的環境讓 `-C` 指到別的 repo。
   - 兩個 git `Command` 的 stdin、stderr 都設 `Stdio::null()`；`ls-files` 只看 exit status（stdout 也 null）；`rev-parse` 用 `output()` 只解析 captured stdout。spawn 失敗（含 NotFound）與任何非 0 都當 `None`，不用 `?` 往上傳英文 os error。測試：對非 git 目錄的檔案呼叫時，測試行程看不到 git 的 `fatal:` 輸出（以 `CARGO_BIN_EXE_codeforge install --dry-run` 的 stderr 斷言）。
   - 已知假陰性（只能印不到警告、不影響寫入正確性）：chezmoi 複製模式、Syncthing 同步、PATH 上沒有 git。README 註明這些情況請加 `--cleanup-period-days` 或自行檢查。
   - `is_shared() = symlink_target.is_some() || git_repo.is_some()`。
4. `run()`（global 路徑，含 dry-run）在印出「目標 settings.json」後，若 `is_shared()`，印：
   - `⚠ settings.json 是 symlink → <target>`（有 target 時）
   - `⚠ <path> 在 git 版控中（repo：<root>）：寫入的內容會隨 commit 擴散到其他機器`（有 repo 時）
   - `  寫入會直接改 target 檔案；hook 指令不含本機路徑，可以安全共用`
5. 測試：`shared_settings` 對一般檔／symlink／git 追蹤檔（tempdir `git init` + `git add`；沒有 git 時 skip）三種情況；`write_settings_atomic` 對一般檔的既有行為不變。

**Acceptance**：KR3、KR5 的單元測試綠；第 1 步的測試在修改前確認是紅的（記錄輸出）。

### P2（M）— `codeforge hook` 轉發器 + 不帶路徑與版號的 command + 固定 marker

1. 新 `src/cli/hook.rs`：
   - `pub fn run(name: &str, args: &[String]) -> Result<i32>`。
   - 名稱對照 `HOOK_SCRIPTS`（`emit-session` → `emit-session.js`、`session-digest` → `session-digest.js`；去掉 `.js` 的名稱也接受帶 `.js`）。不在清單內：stderr 印 `未知的 hook：<name>（可用：emit-session, session-digest）`，回傳 1。
   - 落地：`<data_local_dir>/codeforge/hooks/<CARGO_PKG_VERSION>/<file>`（私有快取，不會寫進 settings.json）。內容與內嵌相同就不寫；檔案不存在或內容不同時，寫 `<file>.tmp.<pid>` 再 rename（多個 hook 同時觸發時不會讀到半個檔案）。rename 失敗時（Windows 上覆寫既有檔可能失敗）重新讀目的檔，內容相同就當成功，否則回錯。函式 `pub(crate) fn materialize(name) -> Result<PathBuf>` 與 `materialize_all()`。
   - 資料目錄覆寫：`default_data_local_dir()` 先讀 `CODEFORGE_DATA_LOCAL_DIR`（與既有 `CODEFORGE_CLAUDE_SETTINGS` 同一種覆寫），macOS 的 `dirs::data_local_dir()` 不讀 `XDG_DATA_HOME`。單元測試不設 env（避免 `set_var` 在 cargo test 內互相干擾），改呼叫 `materialize_in(base: &Path, name)`，傳 tempdir。
   - 執行：`fn exec_script(path, args, stdin: Stdio) -> Result<i32>`，`Command::new("node").arg(path).args(args)`，stdout／stderr 繼承；生產路徑傳 `Stdio::inherit()`，轉發器自身絕不讀 stdin；回傳 child 的 exit code（被 signal 中止時回傳 1）。`node` 找不到：stderr 印 `找不到 node，codeforge hook <name> 未執行`，回傳 1。轉發器自身絕不回傳 2（Claude Code 把 exit 2 當成 blocking）。
   - `HOOK_SCRIPTS` 從 `install.rs` 移到 `hook.rs`（`pub(crate)`）；`extract_hook_scripts` 改成呼叫 `hook::materialize_all()`：install（非 dry-run）仍先把快取暖好，`data_local_dir` 不可寫時在 install 當下就報錯，不會拖到 hook 執行時才失敗。
2. `main.rs`：`Hook { name: String, #[arg(trailing_var_arg = true, allow_hyphen_values = true)] args: Vec<String> }`。結果對應：`Ok(code)` → `exit(code)`；`Err(e)`（落地、spawn、wait 失敗）→ stderr 印正體中文訊息後 `exit(1)`。clap 的用法錯誤也不得回 2：`main` 改用 `Cli::try_parse()`，解析失敗且 argv[1] 是 `hook` 時印正體中文用法說明後 `exit(1)`；其他子命令維持 `e.exit()`。整合測試（`CARGO_BIN_EXE_codeforge`）：漏 name → 1、未知 name → 1。
3. `patch_hooks(global)` 的 command 改為：
   - `codeforge hook emit-session session_start`、`codeforge hook emit-session session_end`
   - `codeforge hook session-digest`（SessionEnd、PreCompact）
   - 其他 entry（`memory context`、`mnemos-cli context`、`dream`、`ship`）不變。
   - `patch_hooks` 的 global 分支不再需要 `scripts_dir` 參數（project 分支仍要）。
4. marker 分流：新增常數 `GLOBAL_MARKER = "codeforge@1"`；`current_marker()` 保留原樣（`codeforge@<CARGO_PKG_VERSION>`），只給 `--project-hooks` 用。`patch_hooks` 依 `project` 參數選 marker；`patch_statusline`／`patch_subagent_statusline` 只在 global 路徑被呼叫，改用 `GLOBAL_MARKER`。測試：project 模式的輸出（command 與 marker）與修改前逐字相同。
5. statusLine／subagentStatusLine 的 command 由純函式 `fn statusline_command(exe: &Path, path_hit: Option<&Path>, shared: bool, sub: &str) -> String` 決定（`sub` 為 `statusline` 或 `subagent-statusline`），三種情況各一個單元測試；`run()` 只負責查 PATH 並呼叫它：
   - 共用檔（`is_shared()`）：一律寫 `codeforge statusline`（`subagent-statusline` 同理），讓每台機器內容相同（KR2）。若 PATH 上的 `codeforge`（逐一查 `PATH` 目錄，取第一個存在的 `codeforge` 檔案）`canonicalize` 後不等於 `current_exe()` 的 `canonicalize`，加印 `⚠ PATH 上的 codeforge 不是目前這個 binary（<exe>）；statusLine 會執行 PATH 上的那一個`。
   - 單機檔：PATH 解析到同一個 binary 時寫 `codeforge statusline`；否則維持絕對路徑（單機不擴散，現行行為）。
6. `is_legacy_codeforge_command` 不用改（舊的 node 路徑 entry 都有 marker，靠 marker 就會被 sweep）；加測試證明：一份含 `codeforge@0.0.6` marker 與 `hooks/0.0.6/…` node 路徑的 settings，install 後只剩新形式、沒有重複。
7. 更新既有測試中預期 `hooks/0.0.x/` 路徑的斷言（`patch_hooks_creates_block`、`global_install_sweeps_unmarkered_legacy_node_hooks` 等）。
8. 新測試：KR1（逐條斷言，規則同 KR1 表格，不做 `contains('/')`）、KR2（同一份輸入連跑兩次 `patch_hooks` + `patch_statusline` 輸出相同；marker 等於 `codeforge@1`）、KR6（`hook::run` 對未知名稱回 1；`exec_script` 以 tempdir 裡的小 js 檔測試，stdin 傳 `Stdio::from(File::open(<含 JSON 的 temp 檔>))`：js 把收到的 stdin 與 argv 寫進輸出檔，斷言逐字相同；exit code 7 與 2 都原樣回傳；沒有 node 時 skip 並印出原因）。
9. 兩支腳本的相依（已查證）：`emit-session.js` 讀 stdin 的 `cwd`、`argv[2]` 是事件名、透過 PATH 呼叫 `codeforge emit`；`session-digest.js` 讀 stdin 的 `session_id`／`transcript_path`／`cwd`，唯一依賴自身位置的是 `__dirname/session-digest.log`（寫在快取目錄旁，寫入失敗已被 try/catch 包住）。兩者都不讀 env。所以從快取目錄執行與現在從 `hooks/<ver>/` 執行行為相同。

**Acceptance**：KR1、KR2、KR6 綠；e2e：`CODEFORGE_CLAUDE_SETTINGS=<scratch>/s.json codeforge install --hooks --dry-run` 輸出中，所有 hook `command` 都是 `codeforge …` 開頭；`echo '{}' | codeforge hook session-digest` 在 scratch HOME 下 exit 0。

### P3（S）— `cleanupPeriodDays` 依共用與否決定

1. `InstallOpts`／`BootstrapOpts` 加 `cleanup_period_days: Option<u64>`；CLI 旗標 `--cleanup-period-days <N>`（`install`、`bootstrap` 都有）。
2. 規則（取代 `install.rs:133-148` 的無條件呼叫）：
   - 有旗標：寫入 N（使用者已設值時，仍需 `--force` 才覆蓋——與現行 `--force` 語意一致）。
   - 沒旗標、單機檔（`!is_shared()`）：維持現行（未設時寫 3650）。
   - 沒旗標、共用檔：不寫也不刪，`--force` 也不動它。檔案裡沒有值時印 `ℹ cleanupPeriodDays 未寫入（共用檔會套用到每台機器）；要保留 transcript 給 dream/ship，可加 --cleanup-period-days 3650`；已經有值時（例如舊版 install 寫進去的 3650）印 `ℹ 共用檔裡已有 cleanupPeriodDays = <值>，保留不動；它會套用到每台機器`。
3. `patch_cleanup_period` 改簽名接 `days: u64`；測試涵蓋：有旗標、單機無旗標、共用無旗標（無值／有值）、共用無旗標 + `--force` 不動（KR4）。

**Acceptance**：KR4 綠；dry-run 對 symlink temp 檔不出現 `cleanupPeriodDays`。

### P4（S）— 文件、版本、發布

1. 文件依 §3 更新；`python3 scripts/check-doc-drift.py` 綠（新子命令要讓 cli-surface 檢查通過）。
2. CHANGELOG `[0.0.7]`：列 P-a～P-f 的修正、升級後重跑 install 的說明、`--cleanup-period-days`、`codeforge hook`。
3. 版號：`Cargo.toml` 0.0.7、`cargo update -p codeforge --offline`。
4. merge 進 main（`QC-Verdict` trailer）→ push → 看 CI → 打 tag `v0.0.7` → 看 release workflow → `gh release edit v0.0.7 --draft=false --latest --notes-file <CHANGELOG 0.0.7 段>`。發布是對外動作，在 owner 核可本計畫後才做。
5. 回覆 7840hs（fleet reply）：版本、升級步驟 `codeforge self-update` → `codeforge bootstrap --dry-run`（確認警告、command 形式、沒有 `cleanupPeriodDays`）→ `codeforge bootstrap`；並提醒 dotfiles 裡舊的 `hooks/0.0.x` entry 會被這次 install 換掉，以及升級順序：共用 dotfiles 的每台機器都先 `self-update` 到 0.0.7，最後才 commit／同步改寫後的 settings.json。

**Acceptance**：KR7、KR8。

依賴：P1 → P2（P2 的 statusLine 警告用到 `is_shared()`）→ P3（用到 `is_shared()`）→ P4。

## 5. Test / validation

- script-gated：§2 KR1–KR7 的單元／整合測試、品質閘、CI。
- human-gated：v0.0.7 發布前 owner 核可；7840hs 由它的 owner 決定何時真跑 bootstrap。
- e2e 只在 scratch 目錄、以 `CODEFORGE_CLAUDE_SETTINGS` 與 `CODEFORGE_DATA_LOCAL_DIR` 指向 temp 路徑執行，不碰本機 `~/.claude/settings.json` 與資料目錄。

## 6. Risks + inversion

| 什麼會保證失敗 | 對策 |
|---|---|
| Claude Code 跑 hook 時 PATH 裡沒有 `codeforge` | 既有的 `dream`／`ship`／`memory context` entry 已經依賴 PATH 且在 7840hs、本機運作；README 寫明前提。statusLine 只有在 PATH 解析到同一個 binary 時才用短形式 |
| 轉發器吞掉 stdin 或改變 exit code，hook 腳本收不到 JSON | KR6 測 stdin 原樣轉發與非零 exit code 傳遞；轉發器不讀 stdin |
| 兩個 hook 同時落地同一個快取檔，讀到半個檔案 | tmp + rename；同版本內容相同，競爭的兩方寫出相同內容 |
| 穿透 symlink 寫入時 target 在唯讀位置或跨檔案系統 | tmp 放在 target 同目錄，rename 不跨檔案系統；失敗時錯誤訊息指出 target 路徑 |
| git 偵測在大 repo 很慢或 git 不存在 | 只對單一檔案跑 `ls-files --error-unmatch`；git 不存在就略過 |
| 舊版 entry 沒被 sweep，新舊 hook 同時觸發 | P2 第 6 步的遷移測試 |
| 共用檔上 `cleanupPeriodDays` 不再預設寫入，transcript 30 天後被清 | 印出提示與旗標；單機使用者不受影響 |
| `is_shared()` 假陰性（chezmoi 複製、Syncthing、無 git） | 只影響警告與 `cleanupPeriodDays` 預設；hook／statusLine 在單機檔也已不帶路徑與版號（statusLine 只有 PATH 對不上時例外）；README 註明 |
| 共用檔被 0.0.7 改寫時，別台還是 0.0.6 | 升級順序寫進 CHANGELOG 與給 7840hs 的回覆（§2.6） |
| `/nix/store` 等唯讀 target | 不嘗試繞過；錯誤訊息指出 target，請使用者改 dotfiles 來源 |

## 7. Out of scope

- 清掉 `<data_local_dir>/codeforge/hooks/` 底下舊版本的快取目錄（`uninstall` 仍整個刪除）。
- `--project-hooks` 的任何改動。
- B31：hooks 改由 autopilot 登記。
- 修改 7840hs 的 dotfiles 或在任何機器上實際跑 bootstrap／install。
- Windows 上 `codeforge hook` 以外的路徑處理（`shell_path` 不動）。

## 8. Open questions（只有 owner 能答）

- 無。範圍沿用決策卡 d-f0103e28 的推薦選項 A（三點都做並發 v0.0.7），owner 改以「寫計畫、過 hetero plan review」推進；v0.0.7 發布前仍需 owner 核可。

## Review log

- R0（2026-11-01）：作者 Claude（Opus 5.5）。
- Research U1（2026-11-01）：grok-4.6 xhigh consult（`dispatch-consult.sh`，ladder receipt 寫入）。採納：共用檔 statusLine 一律 PATH 形式；install 暖快取提前報錯；Windows rename 失敗回讀比對；dangling symlink 與唯讀 target 的錯誤處理、tmp 清理、`sync_all`；git 子行程清 `GIT_*` 環境；共用檔已有 `cleanupPeriodDays` 與 `--force` 的行為。未採納：無（consult 的旁註「腳本本文未附」已由讀碼查證，見 P2 第 9 步）。
- Ladder：consult 後重跑 `probe-unknown.js classify` 仍是 U1，唯一訊號是 S4（這些詞在 knowledge／memory 零命中），不是未解的設計問題；第二次 U1 與即將進行的 hetero plan review 重複，故不再花。finish 時以 `autopilot:learn` 記下這些詞，讓下次命中。
- Proposal：選項 A／B／C 已在決策卡 d-f0103e28 呈給 owner；owner 2026-11-01 改指示「寫計畫、過 hetero loop review」，本計畫依卡上推薦的 A 撰寫，卡已關閉。
- Plan review g1（2026-11-01）：gpt-6.1-sol CONDITIONAL、grok-4.6 STOP；3 個 blocker 全部採納（marker 分流保住 `--project-hooks` 輸出；KR1 改成不誤傷既有 `2>/dev/null` 字串），4 個非阻擋意見也採納（hook 結果對應 exit code、statusLine 選擇抽成純函式、dangling symlink 在讀取階段擋下）。處置檔：`doc/plans/2026-11-01-install-stable-paths.g1-dispositions.json`。
- Plan review g2（2026-11-02，terminal）：兩席 STOP；2 個 blocker（hook 子命令的 clap 錯誤也回 1、git 子行程不把英文輸出漏給使用者）與 4 個非阻擋（uninstall 的 dangling 判斷順序、`CODEFORGE_DATA_LOCAL_DIR` 覆寫、`exec_script` 的 stdin 參數化、共用檔升級順序）由 depth-0 直接裁決，全部採納並併入。處置檔：`doc/plans/2026-11-01-install-stable-paths.g2-dispositions.json`。
- 凍結：`check-phase-review-receipt.js --plan-artifact <g2> --dispositions <g2-disp>` 對 g2 送審時的版本 exit 0。審查產物：`*.g1-review.json`、`*.g2-review.json`。
- 實作偏離（2026-11-02）：資料目錄覆寫沿用既有的 `CODEFORGE_DATA_DIR`（`db::Context` 已用它，值為 `<data_local_dir>/codeforge`），不另造 `CODEFORGE_DATA_LOCAL_DIR`；快取路徑為 `$CODEFORGE_DATA_DIR/hooks/<ver>/`。行為與 P2 第 1 步相同。
- Plan review manifest：`doc/plans/2026-11-01-install-stable-paths.plan-review-manifest.json`。rubric、各 generation 產物與處置在下方補上。
