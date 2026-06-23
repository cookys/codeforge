# CodeForge Brain Connection Indicators

> Status: **DESIGN (converged)** · Owner: cookys · Created: 2026-06-23
> 3 輪 adversarial review 收斂（前提更正 → 假綠/spawn 機制 → 措辭/狀態機閉合）。
> 把 statusline bottom border 寫死的 `memory ● active` 升級成兩顆真實狀態燈：本地腦（local）+ 央腦 Mnemos（central）。

---

## 0. 前提（已驗證）

- **Mnemos Gen-2 server 已蓋、`/health` 已存在**：`cookys/mnemos:crates/mnemos/src/api.rs:33,41`（route `/health` → handler 回 `200 "ok"` 純文字，有 `health_ok()` 測試 :420，commit `1f1d241`/`39c0cf7`）。
- probe 一律打**既有 `GET /health`**（**非** `/v1/health` —— health 與 API 版本正交，全 mnemos repo 慣例無 `/v1` 前綴；打錯會 404 → 永遠假 offline）。
- `cookys/mnemos:docs/ARCHITECTURE.md:10` 的「🟡 Sprint 0 in design」是 **stale doc-drift**（與 api.rs 實況矛盾）。本 spec 不依賴它。
- **無 mnemos 端必做工作**。選配（非阻塞、落 mnemos repo）：`/health` 升 JSON `{status,version}`、修 stale 標籤。probe 端設計成容忍純文字與 JSON 兩者。

## 1. 紅線 / 不變式

- `codeforge statusline` 跑在**每個專案 × 每個 session × 每 ~5s**、one-shot、無常駐 → **熱路徑禁任何同步網路 / 阻塞 I/O**。
- CJK 字串截斷一律 `.chars().take(N).collect::<String>()`，禁 `&s[..N]`。
- `anyhow::Result`、user-facing 訊息正體中文。fmt 走 `./scripts/fmt.sh`。加 dep 前過 `cargo deny check licenses`。
- **零新 dep 目標**：本 feature 全部用 std 達成（detach / lock / boot-age）。若不得不引 dep，明示並過 cargo-deny。
- 不 import CodePower/Mnemos code 進 codeforge（runtime-level 互動）。

## 2. 健康模型：雙軸（不是 OR 單軸）

probe 與 ship 是兩個**不同事實**，不可 OR 成一軸（否則「/health 200 但 ingest 500」「probe 死但 ship 三天前綠」都洗成假綠）：

- **probe = liveness**：央腦 process 此刻活著嗎。唯一 user 能直接行動的訊號（去開 server）。
- **ship = readiness**：央腦真收得了 ledger 嗎（真實 ingest 成敗）。
- **ship 新鮮度視窗** `SHIP_FRESH_WINDOW`（24h）：ship 訊號超過此窗即**不參與**燈色合成，退回純 probe 判定（防陳值假綠）。

### 2.1 central 燈色狀態表（opt-in 時）

| probe | ship（新鮮度） | 燈 | word |
|---|---|---|---|
| ok | 無 / ok | 綠 `●` | `ok` |
| ok | fail（<24h 新鮮） | 黃 `◐` | `degraded` |
| ok | fail（>24h 陳，不參與） | 綠 `●` | `ok` |
| ok | — + queue 深度 ≥ 閾值 | 黃 `◐` | `degraded` |
| unreachable | 任意 | 灰 `○` | `offline` ← 中性「沒跑」，**非告警** |
| never（<7d） | — | 灰 `◌` | `pending` |
| never（>7d） | — | 不渲染 | — |
| **未知**（讀快取失敗） | — | 視同 `offline` 灰 `○`（或維持上次渲染，實作擇一並註明） | — |
| 未 opt-in | — | 不渲染 | — |

- 關鍵：server 沒跑 = **中性灰 offline**（不是 v2 的黃告警 → 修 always-on 麻木）。黃 degraded 只給「server 活著但 ingest 壞 / queue 積壓」這種真退化。
- offline `○` 與 pending `◌` **用不同 glyph**（否則純-glyph 降級級不可區分）。
- 黃/灰態的「要不要動手」由 `→ doctor` hint + `codeforge doctor` 承載（§5），statusline 燈本身保持單一健康軸。

### 2.2 never 退場（可逆）

- opt-in 但 `outcome=never` 持續 > `NEVER_RETIRE_DAYS`（7d）→ 不渲染（視為這台機器其實沒 central）。
- **可逆**：退場後 probe 仍每 `≤10m`（backoff 上限）嘗試一次；一旦某次 `ok` → `last_outcome` 轉 ok、age 歸零 → 下次 render 自動重新常駐。

## 3. local 燈

- 綠 `●`：active L1 concept > 0。
- 灰 `◌`：曾有 `store/` 歷史但 active=0（「記憶怎麼不見了」是有用訊號，不靜默）。
- 不渲染：`signals/` 也空、從未 dream 的全新專案。
- **拿掉數字**（語意薄弱、跟鄰近 version/stat/bar 數字打架）。L1 count 留 `codeforge doctor`。
- 來源：掃 `.codeforge/store/concepts/` 的 frontmatter（`l1::count_active`）— per-render parse、未快取；概念數通常 <100，成本可忽略（大量概念時見 BACKLOG B24）。

## 4. 渲染（彩色/無色雙路徑 + 窄窗降級）

### 4.1 句法對稱
central 沿用 local 的 `<label> <glyph> <word>`：
- 彩色：`memory ● active   mnemos ● ok   v0.0.5`（glyph 帶色、word 完整不縮）。
- **NO_COLOR**：用 `owo-colors` 的 `if_supports_color`（`Cargo.toml` 已開 `supports-colors` feature，**非**手刻 `std::env::var("NO_COLOR")` —— 手刻會漏 FORCE_COLOR / pipe-not-tty）→ 退 `memory:active   mnemos:ok`（**完整詞**，glyph 可留可去）。
- word 全寫 `active/ok/degraded/offline/pending`，**不**用 `deg/pend` 三碼（無色下縮寫 = fallback 失效）。
- ⚠️ NO_COLOR 重構會外溢到 brain 燈以外的既有色塊（`statusline.rs:205-214` 裸 `.truecolor()` 全改 `if_supports_color`）—— 這是一次 statusline 色彩層整體小重構，scope 明示。

### 4.2 bottom_border 改「先量測再印」
現況 `bottom_border`（`statusline.rs:1050-1069`）無腦 `format!` 全印 + `fill = panel_w.saturating_sub(fixed)`；加第二燈後 fixed 可能 > panel_w → saturating_sub 回 0、format 仍全印 → 破窄窗版。改成仿 `statusline.rs:620-627` hint 的「量測 avail → 選印哪些段」模式。

### 4.3 降級階梯（寬→窄逐級砍）
1. 全寬：`memory ● active   mnemos ● ok → doctor   v0.0.5`
2. 砍 `→ doctor` hint（hint 只在黃/灰態出現，沿用 `:617` `→ codeforge adopt` 慣例）
3. central word → 短碼（短碼**只**在此級登場）
4. 砍 version chip（row0 已有；保 `⬆` update banner）
5. central 降純 glyph 無詞（offline `○` / pending `◌` 仍可分）
6. 極窄：只留 local 一顆燈（回退今日行為）

- **斷點 col 數值於實作時量測決定**（用 `bottom_border` 的 fixed 公式逐級減項推）；此處只釘**降級順序**，不過度承諾數值。
- **窄 × 無色交集**：無色時禁用短碼（第 3 級跳過），直接走第 4/5 級。

## 5. `codeforge doctor`（漸進揭露）

- 人話列全維度：local L1 count、central opt-in、上次 probe 時間/結果/latency、上次 ship 時間/成敗、queue 深度與最舊一筆、base_url。
- **主動跑一次前景 probe**（複用 `mnemos-cli probe --verbose` 邏輯，~2s timeout，標「即時量測」vs 快取值）—— user 跑 doctor 就是想知道此刻通不通。
- 黃/灰態**附 next-step 建議**（名實相符：`brew/flutter doctor` = 診斷 + 指引）。命名選 `doctor`（生態系肌肉記憶 + 承接修復指引），非 `status`。

## 6. 快取（per-machine，雙檔）

### 6.1 位置
- `$XDG_RUNTIME_DIR/codeforge/`（Linux tmpfs / per-boot）。macOS 無 XDG → fallback `$TMPDIR`（注意：macOS `$TMPDIR` **跨 boot 持久**、非 tmpfs）。
- **不可放 home dotfile**：央腦連通是 **per-machine 事實**（A 機跑 server、B 機 localhost 是空的），放會被 dotfile sync 同步的路徑會跨機假綠。
- **macOS stale 處理**（零 dep）：不取 `sysctl kern.boottime`（需 libc，破零-dep）。改用**快取絕對年齡上限** `CACHE_MAX_AGE`（1h）：`last_probe_at` 早於 `now - CACHE_MAX_AGE` 即丟棄重 probe。Linux 端維持 `/proc/stat btime` 比對（純讀檔、零 dep）為精確 per-boot；macOS 用年齡上限退路（對 liveness 語意無傷）。

### 6.2 雙檔（兩獨立 writer，避 lost-update）
probe 與 ship 是兩個獨立 writer，拆兩檔、各自單一 writer、全檔 atomic rename（temp+rename，**temp 與目標同 dir**避 EXDEV，temp 名帶 `<pid>.<rand>` 避撞；tmpfs 上不需 fsync）：

```
mnemos-liveness.json  (probe 寫)
  { last_probe_at: i64,            // wall-clock Unix 秒，probe 完成時刻
    last_outcome: "never"|"unreachable"|"http_error"|"ok",
    consecutive_failures: u32,     // backoff 狀態：ok→0、非ok→+1
    latency_ms: Option<u32>,
    http_status: Option<u16> }

mnemos-ship.json      (ship 寫)
  { last_ship_at: i64, last_ship_ok: bool }
```

- **queue_depth 不快取** → statusline 直接 `read_dir` 數 `ship-failed/`（stat 級即時，繞過 v2 黏黃）。⚠️ 實作（`queue_degraded()`）對每個 JSON entry 均讀 mtime（迭代中累積 oldest_age），`queue_degraded_from` 在 count==0 時短路回 false；queue 正常為空故迭代次數為 0，實際成本可忽略，但並無「count==0 先短路再迭代」的最佳化。閾值「≥3 或最舊 > 24h」在 `queue_degraded_from` 的純邏輯層判定。
- 讀失敗 → 該軸視為未知（§2.1 表，不觸發 spawn）。
- 時鐘：全 wall-clock Unix 秒。`last_probe_at` = probe 完成時刻；lock mtime = spawn 時刻（probe 執行中不刷新）。未來戳（clock skew）→ 視為立即過期但配合 backoff 不放大 herd。

## 7. probe 執行契約（熱路徑安全）

statusline 立即讀快取 render（永不阻塞）。刷新決策：
```
should_spawn =
   opted_in()                                       // 每 render 即查（純 stat，不吃快取）
   && (now - last_probe_at) > ttl(consecutive_failures)
   && try_acquire_lock()                             // ↓ statusline 自己、spawn 前搶
```

### 7.1 狀態相依 TTL
- `ok → 30s`（早發現掛）。
- `unreachable / http_error → min(30s · 2^(n-1), 10m)` 指數 backoff（`n = consecutive_failures`，別狂戳掛掉的 server）。
- `never → 30s`。
- recovery：probe 一旦 ok → `consecutive_failures = 0` → TTL 立刻回 30s。

### 7.2 搶鎖（防 spawn herd，純 std）
- lock = `$XDG_RUNTIME_DIR/codeforge/mnemos-liveness.lock`，由**決定 spawn 的 statusline 自己、在 spawn 之前**搶（**非**委派給還沒誕生的子進程）。
- 無 stale 時：`OpenOptions::new().write(true).create_new(true)`（O_EXCL 原子建檔）；成功者 spawn、`AlreadyExists` 放棄本輪。
- **stale 回收用 rename-steal（修 unlink+create_new 的 TOCTOU 窗）**：lock mtime > `INFLIGHT_GRACE`（10s，> 2s connect timeout + 網路/排程餘裕、遠短於 TTL 30s）視為 stale 時，`rename(stale_lock → owned_tmp)` **原子認領**（rename 對「來源存在」原子，只一個 statusline 成功、其餘 ENOENT），認領成功者才 `create_new` canonical lock 並 spawn。**不用 unlink 重搶**。
- probe 結束 / crash 殘留：probe 結束 unlink lock；crash 殘留靠上述 mtime stale 回收。

### 7.3 detach（純 std、零 libc）
- `std::os::unix::process::CommandExt::process_group(0)`（std **1.64 穩定**、MSRV 1.88 涵蓋、底層 child `setpgid`、零 libc）把 probe 放新 process group。
  - **作用範圍（精確）**：隔離 CC 對其 process-group 的 SIGTERM/SIGKILL 樹狀 cleanup（CC 殺 statusline child 的真實機制）。**不**脫離 session、不處理 terminal-hangup 型 SIGHUP —— 但 probe 是 ≤2s 瞬時 one-shot，存活窗短於任何互動 hangup，故不需 setsid 級脫離。（不為此引 libc。）
- `current_exe()` 取自身路徑（抄 `daemon.rs:193`）；**失敗 → 本輪不 spawn、不報錯、不 fallback PATH**。
- `.stdin(null).stdout(null).stderr(null)`（否則繼承 fd 污染 CC 讀的 statusline pipe）。
- spawn 點保證在 statusline 的同步路徑（非 tokio multi-thread context）。

### 7.4 probe 子進程本體（`codeforge mnemos-cli probe`）
- target = `MnemosConfig::load().base_url` + `/health`（與 ship/context 同源，不另立 URL 解析）。
- `tokio::runtime::Builder::new_current_thread`（單執行緒最小冷啟）+ 專用 **~2s connect-timeout** client（**非** ship 的 20s `http_client()`）。
- HTTP 200 → `ok`（不解 body，容忍純文字 `"ok"` 與未來 JSON）；connection-refused / timeout → `unreachable`；4xx/5xx → `http_error`（證據 = 快取 `http_status` 欄，**不另開 log 檔**避免 tmpfs 無界 log）。
- 永不向 stdout 寫人類訊息（只寫快取）。
- `--verbose`：前景模式，印 stderr、不寫快取，供手動 debug。

## 8. ship 順風車（central 第二訊號源）

- ship 寫 `mnemos-ship.json` **只在真有網路往返的分支**：
  - fresh-digest POST：`ship.rs:117` 後的 `Ok`(:119) / `Exhausted`(:129) / `BadRequest`(:142)。
  - **flush 成功**（`flush_failed_queue`，`ship.rs:100`/`:113` 呼叫）也回寫 `last_ship_ok=true`（同證 readiness）。
- **早 return 路徑不寫**（無 :117 往返）：`no-ship opt-out`(:37)、`未 opt-in no-hook`(:47)、`--resend`(:56)、`--dry-run`(:86)、`already_shipped` 純早退(:94)、`empty lessons`(:106)。
- **寫快取受 `opted_in()` gate 約束**（與**渲染** gate 同源，**非** POST 是否發生）—— `record_ship_health` 內部先查 `opted_in()`：**opted-in 時才寫快取**（互動 ship 亦然；opted-in 使用者的手動 ship 是真實 readiness 訊號，寫快取正確）。未 opt-in 時不寫（避免「pre-opt-in 假綠」，opt-in gate 已達成此保護）。
- `--no-hook` 失敗分支全回 `Ok(())`，不拖垮 SessionEnd。

## 9. 模組邊界

- **聚合層在 statusline**（caller）：組 `BrainHealth { local, central }`。
- `local` 健康：statusline / memory 側即算（不讓 mnemos 反向依賴 memory/L1）。
- `central` 健康：`src/mnemos/health.rs` 只管央腦這半 —— 讀雙快取 + 提供 probe 子命令邏輯，回已算好的 `CentralHealth`。

## 10. 命名

| 物件 | 名 |
|---|---|
| central 健康模組 | `src/mnemos/health.rs` |
| probe 子命令 | `codeforge mnemos-cli probe`（+ `--verbose`） |
| 診斷命令 | `codeforge doctor` |
| liveness 快取 / 鎖 | `$XDG_RUNTIME_DIR/codeforge/mnemos-liveness.json` / `.lock` |
| ship 健康快取 | `$XDG_RUNTIME_DIR/codeforge/mnemos-ship.json` |
| health endpoint | 既有 `GET /health`（不新增） |

## 11. 可調常數（集中一處）

| 常數 | 建議值 | 用途 |
|---|---|---|
| `PROBE_TTL_OK` | 30s | ok 後刷新間隔 |
| `PROBE_TTL_MAX` | 10m | backoff 上限 |
| `SHIP_FRESH_WINDOW` | 24h | ship 訊號參與合成的新鮮度 |
| `NEVER_RETIRE_DAYS` | 7d | never 態退場 |
| `CACHE_MAX_AGE` | 1h | macOS 快取絕對年齡上限（per-boot 退路） |
| `INFLIGHT_GRACE` | 10s | lock stale 判定 |
| `PROBE_CONNECT_TIMEOUT` | 2s | probe client |
| `QUEUE_WARN_THRESHOLD` | ≥3 或最舊 > 24h | queue 判黃 |
| bottom-border col 斷點 | 實作時量測 | 降級階梯 |

## 12. 範圍 / 測試

codeforge-only L-size。實作前 invoke `autopilot:dev-flow`。
- 單元測試：燈色狀態表（§2.1 每格）、TTL backoff（含 recovery reset）、ship 寫快取分支決策表、雙快取 atomic write/讀失敗 fail-soft、渲染降級階梯（含 NO_COLOR、窄窗、CJK 寬度）、never 退場可逆狀態機、rename-steal 鎖回收。
- 不可在熱路徑驗證的（detach SIGHUP 行為、真實 herd）以契約 + 手動 smoke 驗。

## 13. 評審紀錄

3 輪 adversarial review（5 + 3 + 2 agents）：
- R1：翻盤前提錯誤（誤信 stale doc 標籤以為 server 未蓋）+ 3 架構 BLOCKER（detach/stdio/herd）+ UX 重導。
- R2：probe∪ship OR 假綠、ship 寫快取時機、libc 編譯不過、claim-race、per-machine 快取、NO_COLOR、窄窗破版。
- R3（收斂）：process_group 措辭、rename-steal 鎖回收、macOS boot-age 零-dep 退路、狀態表補白、ship 分支清單完整性。判定「能進實作、不需第四輪」。
