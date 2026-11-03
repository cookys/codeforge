# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- **Linux-only releases** — `scripts/release-local.sh` builds and publishes Linux x86_64 and aarch64
  only (`--allow-missing` is gone). On macOS and other platforms, `install.sh` exits with the
  `cargo install --git https://github.com/cookys/codeforge --locked` instruction instead of a 404, and
  `codeforge self-update` says the same before contacting GitHub.

## [0.0.9] - 2026-11-03

Usage tracking (G36: per-account quota, transcript token counts, cost estimate, forge-stats 用量 tab, statusline-triggered
background collect); forge-stats as a global plugin (G33); one TUI guideline (G34); and a local release pipeline
replacing hosted CI (G38) — this is the first release built and verified locally.

### Removed
- **macOS binaries** — from this release on, releases ship Linux x86_64 and aarch64 only. On macOS, build
  from source: `cargo install --git https://github.com/cookys/codeforge --locked` (add `--force` to update an
  existing install). A 0.0.8 macOS binary's `codeforge self-update` fails to find a darwin asset; this is why.

### Added
- **Local release pipeline** (PM G38) — `scripts/release-local.sh` builds the linux x86_64 and
  aarch64 tarballs with `cross` (podman/docker) and the MSRV toolchain, refuses a binary needing glibc
  newer than 2.35 (the cross builds need 2.18), verifies `install.sh` end to end from a local server,
  and only ever creates a draft release. `scripts/package.sh` is the one packaging path;
  `scripts/check-all.sh` now also runs `cargo deny`, `shellcheck install.sh` and the MSRV release smoke.
  `install.sh` gains `CODEFORGE_DOWNLOAD_BASE` (default: this repo's GitHub releases).
- **`codeforge usage`** (PM G36) — per-account quota on the statusline usage row (`alias NN% countdown`
  for every other account seen in 6 h; alias from `AUTOPILOT_ACCOUNT` or a config-dir hash), an
  incremental collector over Claude Code and Codex transcripts that keeps numbers only
  (`<data_dir>/usage/usage.db`, 0600), an API-equivalent cost estimate, `summary.json` with an exact,
  value-checked schema, and `codeforge usage push` (`fleet upload`). `install --hooks` adds
  `usage collect --quiet` to the SessionEnd chain. The forge-stats pane gains a 用量 / Usage tab
  (`/forge-stats usage`) that reads `summary.json` bounded and never spawns codeforge. Without any hook,
  the statusline starts a background `usage collect --quiet` at most every 15 minutes
  (`CODEFORGE_USAGE_AUTOCOLLECT=0` turns it off); a collect already running makes it skip. Spec:
  `doc/specs/codeforge-usage.md`.
- **Fleet summary in forge-stats** (PM G33) — the band's one-line summary leads with the fleet from autopilot's
  `autopilot.fleet` state: `fleet ▲1 ✗1 ⊘2 ●4 ⧗11 · Ferris Lv.1`, widest form that fits, dropping from the end
  (pet, then ⧗ ● ⊘ ✗, then the word) down to the most severe count. With autopilot, the summary is the band's second
  row under autopilot's tab bar (the heartbeat now says `summary_row: true`); with 7+/13+ rows it sits above the
  mini/full columns. No autopilot: the pet alone, as before.
- **`doc/specs/tui-guideline.md`** — one TUI guideline for the statusline, forge-stats, autopilot's live mod and the
  fleet views; `scripts/ansi2html.py` turns a tmux capture into a coloured HTML preview.

### Changed
- **forge-stats is a global plugin** — moved from `.claude/skills/forge-stats/` (codeforge clone only) to
  `plugins/forge-stats/`, listed by the repo's marketplace. Install: `claude plugin marketplace add <codeforge>` then
  `claude plugin install forge-stats@codeforge`.
- forge-stats draws the one-line summary at one row too (it passed one row on before).

## [0.0.8] - 2026-11-03

Model speed moves to autopilot's live mod (B32); duplicate code spawns Doppelgangers (B10); install can take over a
foreign statusLine without touching hooks (B28).

### Upgrading

- **Don't use `install --force` to take over a statusLine.** On 0.0.7 the "statusLine 已被其他程式設定" error points
  at `--force`, which also clears every hook entry, codeforge's or not. Use `--replace-statusline` (new here) instead;
  or, when only the hook commands need refreshing, `codeforge install --hooks`, which never touches the statusLine.
- forge-stats shows no speed any more. autopilot's live mod measures and draws it from autopilot develop `add397e9b`
  (BL-0359) on; reload or restart the session to pick both mods up.

### Removed

- **forge-stats 不再量測與顯示速度（B32）**：tok/s 改由 autopilot 的 live mod 自己量、自己畫（owner 卡 d-7b0f7a4a）。forge-stats 拿掉 `turn.step` 量測、band 的速度與最近請求兩欄、pane 的速度段與最近請求表，以及 1 列的分頁標籤（改為原樣傳給下層）；band 只剩 Ferris｜寵物｜記憶與大腦，第 3 欄的門檻從 170 欄降到 86 欄，一列摘要只顯示寵物名與等級。沒有 autopilot 時也不顯示速度。

### Added

- **Doppelganger 分身怪 🪞（B10）**：daemon scanner 新增跨檔重複程式碼區塊偵測（8 個保留行、略過註解／import／瑣碎行，每次最多 3 隻，名稱 `dup @ <a> ↔ <b>`）；分身怪被擊殺時依 proposal A/A/A 分裂（每次 1 隻、到第 2 代為止、stats × 0.7），Doppelganger Ward 生效期間不分裂。`mobs` 新增 `parent_id`、`generation` 欄位（自動升級）。`codeforge inventory` 拿掉 Ward「尚未實作」的警語。scanner 改為依檔名排序走訪，超過 1000 檔時收進來的檔案在每台機器都一樣。
- **`install --replace-statusline`** takes over a `statusLine` / `subagentStatusLine` that another tool set, without touching any hook. Before, the only way was `--force`, which also clears every hook entry, codeforge's or not; the conflict error and the bootstrap hint now point at the new flag (B28).

### Fixed

- **forge-stats 速度欄灌水**：模型的思考不顯示時（思考文字不串流），思考的 token 在第一塊可見內容之前就寫完了，舊算法只拿「第一塊 → 最後一塊」的時間去除全部輸出 token，曾顯示 3578 tok/s（實際約 83）。現在沒有思考文字串流時改從送出算起（含讀 prompt 的時間，是下限而非高估），並扣掉 API 自己跑工具（advisor）的時間；舊的計時樣本在升級時清掉。

## [0.0.7] - 2026-11-02

Install is safe on a shared `~/.claude/settings.json` — a symlink into a dotfiles repo, or a git-tracked file
(B35).

### Fixed

- **A symlinked settings.json stays a symlink.** install, bootstrap and uninstall used to replace the link with a
  regular file (tmp file renamed over the link), so the dotfiles copy never changed and the machine silently
  detached from it. Writes now go to the link's final target. A dangling link is an error (dry-run included), never
  overwritten or read as an empty file.
- **Hook commands carry no machine path or version.** They were `node <data dir>/codeforge/hooks/<version>/….js`,
  which broke on every other machine and changed on every upgrade.

### Added

- **`codeforge hook <name> [args…]`** runs a hook script embedded in the binary (`emit-session`,
  `session-digest`): copied to a private per-version cache, run with `node`, stdin and the script's exit code passed
  through. Its own failures exit 1, never 2 (blocking in Claude Code). Scripts now update with `self-update`.
- **`--cleanup-period-days <DAYS>`** on `install` and `bootstrap`.
- install warns when settings.json is a symlink or tracked by git.

### Changed

- Global entries use the hook commands above and the fixed marker `codeforge@1` (was `codeforge@<version>`), so
  re-running install after an upgrade changes nothing. `--project-hooks` output is unchanged.
- statusLine is `codeforge statusline` on a shared settings.json, and on a local one when PATH's `codeforge` is
  this binary; otherwise the absolute path as before.
- `cleanupPeriodDays` is no longer written into a shared settings.json unless you pass `--cleanup-period-days`
  (an existing value there is left alone, `--force` included). A local file keeps the default 3650.
- The hook script cache honours `CODEFORGE_DATA_DIR`.

### Upgrading

- Run `codeforge install --all` (or `codeforge bootstrap`) once to replace the versioned `node …/hooks/0.0.x/…`
  entries; preview with `--dry-run`.
- **Shared settings.json:** self-update **every** machine that reads the file to 0.0.7 first, then run bootstrap
  on one machine and commit/sync the result. A machine still on 0.0.6 has no `codeforge hook` subcommand; clap
  exits 2 there, which blocks compaction on PreCompact.

## [0.0.6] - 2026-11-01

### Changed

- **The forge tab label is structured** — a row of Boxes keyed `speed`, `trend`, `sep`, `pet`, each holding one plain
  Text, so autopilot colours each part by its key instead of splitting the string (a Text's key does not reach the
  plugin above; a Box's does). autopilot reads it since its `3ecd193af`.
- **forge-stats speed follows the last timed request** — a step too short to time (a lone tool call) no longer
  blanks the tab label to `ϟ — tok/s`; the label, the band's latest line and the sparklines use only requests that
  had a tok/s.
- **forge-stats speaks speed with ϟ and colour** — the tab label is `ϟ 93 tok/s ▁▂▃▅▇ · Ferris Lv.1` (pet, then
  sparkline dropped to fit; `ϟ` is U+03DF, one cell, never an emoji). The expanded band colours by meaning: speed in
  the theme's suggestion colour (autopilot's `running`, the colour of its own speed label), the pet's name and XP in its village colour, a light before memory and brain coloured by
  state.
- **forge-stats band columns follow the width** — 1 to 5 columns in priority order (Ferris, pet, speed, recent
  requests, memory and brain): from 26, 56, 92, 140 and 170 cells; the request table takes the spare width.
- **forge-stats no longer pins a `tok/s` status entry** under the prompt: autopilot's forge tab label already shows
  the latest speed, so the `⚠ forge-stats: 93 tok/s` line was a duplicate.

### Added

- **forge-stats band tiers** — the band draws by the rows it gets: 12+ full, 6–11 a 16×6 mini Ferris
  (`village.sprite_mini` in `pet.json`, 16×12 px), 2–5 or under 26 columns a one-line summary, one row the forge tab label (see Changed). autopilot asks
  only for 1 row and for 12+, so the mini and summary tiers show only without autopilot.
- **forge-stats band** — `/forge-stats layout band|pane` (kept in the mod's store, band by default): the band
  layout draws Ferris, pet, speed, recent requests, memory and brain above the prompt (columns by width, see
  Changed) instead of opening the pane, and writes the heartbeat the compact statusline reads.
- **Compact statusline while the forge-stats band runs** — `codeforge statusline` reads the forge-stats
  heartbeat (`<live_base>/codeforge/mod-alive/<session_id>.json`); when it is this session's, under 10 s old and says
  `layout: band`, the statusline prints a 2-line identity + usage strip instead of the pet panel: the pet lives in the
  band (autopilot's forge tab), the statusline keeps the slow data. Without it the output is
  byte-identical to before.
- **Session view** — every statusline run writes `<live_base>/codeforge/session/<session_id>.json`
  (`codeforge.session-view/1`: memory and brain lights for this session only).
- **forge-stats pane** — a Claude Code mod shipped in `.claude/skills/forge-stats/` (loaded by any
  `claude` started in this repo): model speed (tok/s, time to first token) and the pet, with Ferris drawn
  as an animated half-block pixel sprite in the terminal (ASCII elsewhere).
- **Pet pixel sprite** — `pet.json` now carries `village.sprite`: a 24×24, two-frame (idle /
  blink) pixel sprite for Ferris (Rust village) as a palette plus rows of palette characters, for
  UI readers that draw terminal half blocks; `null` for villages without one yet. Sprites are
  checked against the contract (`src/pet/sprite.rs`) before they are written.
- **Pet view snapshot** — `codeforge statusline` (every render) and `codeforge adopt` write
  `<live_base>/codeforge/pet.json` (schema `codeforge.pet-view/1`: pet state, village, ASCII art)
  next to the live context files, so UI readers (e.g. a Claude Code mod pane) can draw the pet
  from any project without spawning `codeforge`. See README → Live context files.

- **Windows support** (MSVC toolchain, verified on Windows 11) — the crate now builds, passes its
  test suite and runs end-to-end on Windows. All OS
  differences are isolated in the new `src/platform.rs`: cross-platform process
  alive/terminate/identity via `sysinfo` (replaces `kill`, `/proc` and Linux-only identity check),
  detached spawn, SIGTERM/Ctrl-Break stop signal, low-priority + timeout wrapper for `claude -p`
  (Unix keeps `nice`/`timeout`), and shell-safe path rendering for hook/statusLine commands
  (forward slashes + quoting, since Claude Code runs hooks through Git Bash). Home directory now
  comes from `dirs::home_dir()` instead of `$HOME`; `bootstrap` runs `fmt.sh` via Git for Windows'
  bash; `.gitattributes` forces LF for scripts/sources. New `scripts/check-all.sh` (+ opt-in
  `.githooks/pre-push`) runs every gate locally — Windows-native plus a Linux pass via WSL — in place of
  GitHub CI. See [`doc/windows.md`](doc/windows.md).

- **Live context files** (autopilot v2.36.1 P1) — `codeforge statusline` now also writes a small
  RAM-backed JSON snapshot (`<live-base>/context/<sid>.json`) after every render, and a new
  `codeforge subagent-statusline` command writes the subagent task list
  (`<live-base>/context/<sid>.tasks.json`) — one writer per file, no merge, atomic same-dir
  temp+rename at mode 0600. Base-dir resolution order: `$AUTOPILOT_LIVE_DIR` →
  `$XDG_RUNTIME_DIR/autopilot` → `/dev/shm/autopilot-<uid>` → `/tmp/autopilot-<uid>`, each verified
  tmpfs/ramfs via `findmnt` (falls back to `/proc/mounts` parsing when `findmnt` is absent); every
  candidate rejected falls back to `~/.autopilot` with one warning line on stderr. `codeforge
  install --subagent-statusline` (opt-in, only when `statusLine.command` already points at
  `codeforge statusline`) idempotently wires the `subagentStatusLine` hook. Never changes
  statusline rendering or exit code — a write failure is logged to stderr once per process. See
  README § "Live context files".
- **Brain connection indicators** — statusline bottom border 升級為兩顆真實狀態燈：
  - **`memory ●/◌`**（本地腦，永遠顯示）：`●` 綠＝active L1 > 0；`◌` 灰＝store 存在但 active = 0；不顯示＝全新專案。
  - **`mnemos ●/◐/○/◌`**（央腦，僅 opt-in 顯示）：`●` ok / `◐` degraded / `○` offline / `◌` pending。server 沒跑為中性灰 `offline`，不是告警。
  - liveness 由 detached 背景 probe（`process_group` 隔離，O_EXCL rename-steal 防 herd）寫 per-machine 快取；statusline 熱路徑只讀快取，零阻塞。readiness 由 `ship` 真實 POST 成敗順風車寫入。兩軸獨立防 OR 假綠。
  - NO_COLOR / 無色終端：退 `memory:active`、`mnemos:offline` 等完整詞，無縮寫。
  - 窄窗降級階梯：砍 `→ doctor` hint → short code → 砍 version → 降純 glyph → 極窄只留 local 燈。
- **`codeforge doctor`** — 全腦健康診斷命令，列出：本地 L1 active count + store 歷史、Mnemos opt-in 狀態、**即時** probe 結果（~2s 前景 `GET /health`）、上次快取 probe 時間/結果、上次 ship 時間/成敗、queue 深度與最舊 age、base_url 設定、黃/灰態 next-step 操作建議。
- **`codeforge mnemos-cli probe [--verbose]`** — 手動探 Mnemos `/health`，分類 `ok / unreachable / http_error`，寫 liveness 快取；`--verbose` 印 stderr 詳情、不寫快取，供 debug。
- **`ship` 順風車快取寫入** — `codeforge ship` 真實 POST ledger 成功（或 flush 成功）後寫 `mnemos-ship.json`，作為 central 燈的 readiness 訊號（dry-run / resend / already-shipped 等早 return 路徑不寫，防假綠）。

### Fixed

- **Village ASCII art indentation** — the art literals in `src/pet/village.rs` used Rust string
  line continuation, which strips each next line's leading spaces, so every pet printed
  misaligned in `pet`, `adopt` and the statusline art column. Now one literal per line.

## [0.0.5] - 2026-06-23

### Added

- **`codeforge self-update`** (B20) — 從最新 GitHub release 原地替換 `current_exe` 的二進位更新器（`self_update` crate,GitHub 後端,ureq+rustls）。`--check` 只報新版。不管 binary 在 `~/.local/bin` 或 `~/.cargo/bin` 都更新對,繞開「哪個在 PATH」問題。搭配既有 `install.sh`（首裝）。⚠️ 需 publish 為 full release（非 draft/prerelease）才會動。
- **`codeforge bootstrap`** (B14) — 一鍵多機部署:`install --all`（statusLine 衝突自動 fallback hooks-only,non-destructive）+ fmt pin toolchain 對齊（在 clone 內）+ Mnemos opt-in 狀態（report-only）。best-effort、idempotent、`--dry-run`。
- **CJK-safe 確定性 gate** (B9) — `scripts/check-cjk-safe.sh` + CI `cjk-safe` job,偵測 `src/**/*.rs` 對字串的 `&s[..N]` byte-slice（CJK panic 形）。把 HARD RULE 沉澱成自動檢查,精準優先於召回。
- **pinned rustfmt wrapper** (B19) — `scripts/fmt.sh`（單一版本來源,self-install pinned toolchain,只 pin fmt 不動 build）。CI `fmt` job + S/L/H quality gate 都走 `--check`。消除 rolling-stable rustfmt drift。
- **記憶 recall Phase B(T2.2 + T2.3)** — `doc/plans/2026-06-17-memory-recall-phase-b.md`:
  - **T2.3 統一 ranking** — `recall::rank` 從「strength 單因子排序」改為複合分數 `importance(strength) × recency × citation(refs)`。recency = `0.5^(age/90 天)`,age 對齊「active 集合裡最新的 effective date」(= `updated` 與 `last_ref` 取較新),純函式無 clock。`refs`/`last_ref` 本地引用訊號首次進入排序;近期被引用的舊筆記會復活到前排。
  - **T2.2 mem0 式對賬** — `dream::compile` 在把新事實寫成新 slug 前,先 FTS 找既有相關 active 條目;有的話交 Haiku 判 ADD/UPDATE/DELETE/NOOP(JSON-in-text)。UPDATE 併入既有條(保留 created/strength/refs、union sources/links、bump updated);DELETE 把舊條標 `superseded-by:<slug>`(同 `dedup` 慣例)再寫新條;NOOP 不寫。一律偏向 ADD:無 API key / FTS 無命中 / 解析失敗 / target 越界都退回 ADD,絕不遺失知識。`CompileResult` 新增 `l1_noop`,dream 輸出印「N 對賬略過」。
  - T2.1(async worker)依 2026-06-17 決策 defer 到 BACKLOG B16(觸發條件未成立、且擾動剛上線的 SessionEnd 鏈)。
- **本地 recall(無 mnemos 的 READ 路徑)** — `codeforge memory context [--max-tokens][--hook]`:讀本地 active L1 → 複合分數排序(importance × recency × citation,見 T2.3)→ budget 截斷成 **lean ranked index**(~1500 token,非 dump)→ 印 markdown 或(`--hook`)SessionStart `hookSpecificOutput.additionalContext` JSON;無 active L1 則 no-op。每條帶 citation `topic`,詳情走 `codeforge memory search`(progressive disclosure)。`codeforge install --hooks`/`--all` 把它接進 global SessionStart(非 plugin,因 CC issue #16538)。對稱於中央的 `mnemos-cli context`。完成記憶迴圈 absorb→distill→store→**recall** 的 READ 側。偷自 prior art(claude-mem lean-index/citation、SuperBrain「injection not storage」)。
- `doc/specs/codeforge-memory-contract.md` — 共享 state(L0/L1/L2/improvement-queue)的 producer/consumer 明文契約 + L1 frontmatter schema(含 reserved `nature` 欄位)+ READ-path 注入契約。

### Removed

- `src/projection/`(`project_to_claude_memory`)死碼 —— 機制錯(寫進 Claude 自有的 auto-memory 目錄,丟入檔不可靠載入),由 `codeforge memory context` 的 SessionStart additionalContext 注入取代。

### Changed

- **MSRV 1.85 → 1.88** — `self_update` 依賴鏈拉進 `time 0.3.51`（需 Rust 1.88）。誠實 bump `rust-version` + release workflows 的 toolchain pin（`@1.85`→`@1.88`）。連帶 `is_multiple_of` 因 MSRV 跨過 1.87 而從 `% n == 0` 改回慣用法（clippy `manual_is_multiple_of`）。
- **reqwest 改 rustls-tls**（drop native-tls/openssl-sys） — `default-features = false` + `json` + `rustls-tls`。移除 openssl-sys → aarch64-linux 交叉編譯不再卡在 openssl C build,且與 self_update 統一成單一 rustls/ring TLS stack。5 處 reqwest 用法只用 `.timeout()`/`.build()`,無行為破壞。
- **clippy rolling-stable drift 清理** — 修 `nonminimal_bool`（install.rs 布林提取 `neither`）、`trim_split_whitespace`（statusline 移除多餘 `.trim()`）。CI clippy job 轉綠。
- **dream compile 改為對賬式,不再盲目同 slug 覆蓋** — 新事實落在新 slug 時會先和既有相關條目對賬(T2.2);同 slug 仍 in-place 合併,且合併後一律回到 `active`(新訊號落在 superseded slug 視為「重新觀察」而復活,不會把新知識寫進看不到的死檔)。無行為破壞、無需重裝 hook;`memory context` 的浮現順序會因 T2.3 三因子排序而改變(近期被引用的條目排更前)。
- **記憶 pipeline 跨專案上線** — `codeforge dream --quiet` → `codeforge ship --no-hook` 的 SessionEnd 鏈從 codeforge-clone-only 的 `--project-hooks` 移到 global `--hooks`/`--all`,在每個專案 session 結束都 per-project 萃取（hook CWD = 專案 root → per-cwd `.codeforge`）。`--project-hooks` 因 `ensure_in_codeforge_repo` 只能在 clone 跑,無法覆蓋其他專案;唯有 global 路徑能跨專案。`--project-hooks` 現只保留 dev scripts（check-improvements / check-dev-flow）。
- `codeforge install --hooks`/`--all` 現會寫入 `cleanupPeriodDays: 3650`(只填未設值,`--force` 才覆蓋使用者選擇)。Claude Code 預設 30 天回收 session transcript,會在 dream/ship 萃取前刪掉;拉高留存讓 raw material 存活。
- **LLM backend 改為 fallback 鏈(新增 `src/llm.rs`)** — dream/ship/commentary 的 digest 不再「Haiku-or-passthrough」。新鏈:`claude -p` headless(Claude Code CLI,免 API key,預設 Opus,品質最高;`CODEFORGE_DIGEST_MODEL` 可改)→ `ANTHROPIC_API_KEY`(直連 Haiku API)→ rule-based passthrough。`ANTHROPIC_API_KEY` 因此為 optional,只是第二層 fallback。每層降級印 warning。

### Added

- **session-digest 落地重設計(A′)+ per-repo opt-out** — `session-digest.js` 不再倒進全域 `~/.claude/session-digests/`(明文、歸屬錯),改從 cwd 往上找 `.codeforge` 落 per-repo `<repo>/.codeforge/digests/`,找不到即 skip 不寫(非 codeforge 專案零明文落盤)。`dream ingest-digests` 改讀 per-repo dir + 吸完刪檔(明文不長存),過渡期同讀舊全域路徑。新增 `<repo>/.codeforge/no-ship` per-repo opt-out 檔(`ingest` skip + `ship` 硬 no-op)。

- `codeforge ship --no-hook` opt-in gate（`MnemosConfig::opted_in`):僅當 `~/.config/mnemos.env` 存在或 `MNEMOS_INGEST_URL` 設定才送。沒有 Mnemos 的 codeforge-only 使用者照常用 dream 萃取 L1,ship 變乾淨 no-op — 不再每次 session-end 往 `ship-failed/` 堆 dead-letter。互動式 `codeforge ship` 不受 gate(明示動作)。

- `codeforge mnemos-cli context --with-themes` flag — 送 `include_themes=true` 給 Mnemos,在 atoms 前注入一段 theme-summary 區塊。

### Changed

- **收嚴 ledger source** — ship 的 L2 ledger 來源斷開 `absorb`(跨專案吸收的二手知識)、改接 `session-digest`(本 repo 第一手),加上 origin-purity 過濾(只送 `origin != absorbed`)+ idempotency / confidence gating。確保送進 Mnemos 的是本 repo 第一手 coding 經驗。

### Fixed

- `patch_hooks` 改成「先全面 sweep 所有 hook_type 的 codeforge group → 加入當前 entries → collapse 空 array」,讓 hook entry 可跨 hook_type / scope 搬移而不留孤兒。一併修掉:(1) `--project-hooks` re-run 漏 model 非 script 的 dream SessionEnd 條目而 drop/duplicate;(2) pre-marker 安裝的 node hook scripts(`hooks/0.0.3/…` 等無 `_installed_by` marker)在升級 re-install 時與新版並存導致 dual-fire — `is_legacy_codeforge_command` 現以 codeforge scripts 路徑 + 已知 basename 辨識並 sweep。

- **signal_cursors 跨 repo key 碰撞**(上線 blocker) — 全域 `state.db` 的 `signal_cursors` 原以 filename 為 key,多專案共用 `CODEFORGE_DIR` 時不同 repo 的同名日期檔會互相覆蓋 cursor → 漏編譯或重複。改為以 repo-qualified key 區隔。

## [0.0.4] - 2026-06-10

### Added

- `codeforge ship` — CodeForge becomes Mnemos's first streaming source. Digests the day's L1 concepts + git log into an L2 ledger (Haiku `claude-haiku-4-5-20251001`, or rule-based passthrough when no `ANTHROPIC_API_KEY`) and POSTs it to `POST /v1/ingest/ledger`. ULID `ship_id` idempotency (retry never mints a new ULID), 1s/5s/30s backoff, 4xx never retried, failures queued to `~/.codeforge/ship-failed/<ship_id>.json`, `ship-state.json` prevents same-day re-ship. Flags: `--date`, `--dry-run`, `--no-hook` (single-attempt, never blocks SessionEnd), `--resend` (flush queue only). Spec: `doc/specs/codeforge-ship.md`; Mnemos contract `mnemos:docs/specs/10-source-contract.md` §5.1.
- `codeforge mnemos-cli context [--topic][--max][--max-sensitivity]` — fetches relevant atoms from `GET /v1/atoms/context` and formats them as markdown for SessionStart injection. Topic auto-derived from git branch + recent commit subjects when omitted. Degrades gracefully (warn + empty block, exit 0) when Mnemos is unreachable.
- `codeforge mnemos-cli cite <atom_id> [--matched-text]` — citation write-back to `POST /v1/atoms/:atom_id/cite` (§11.1 envelope: ULID `cite_id`, RFC3339 `cited_at`, `client=codeforge_ship/<version>`, `evidence.method=fulltext_match`).
- `codeforge mnemos-cli cite-detect <transcript> [--topic][--max]` — SessionEnd heuristic: full-text matches candidate atom titles against a transcript and cites each hit (high false-positive rate accepted for Sprint 1; Sprint 5+ moves to Haiku judgement).
- Endpoint/auth resolution reads `~/.config/mnemos.env` (`MNEMOS_INGEST_URL` / `MACHINE_ID` / `MNEMOS_TOKEN`), falling back to `http://127.0.0.1:8845`.

## [0.0.3] - 2026-05-15

### Added

- `codeforge install --project-hooks` + `codeforge uninstall` + `--dry-run`, `--force`, `--yes`, `--settings-path`, `--quiet` flags (full V2.2 surface per `doc/specs/codeforge-install-subcommand.md`). Marker-driven uninstall (`_installed_by: codeforge@<ver>`) preserves user-owned settings.
- `.claude/settings.json` rewritten to use `${CLAUDE_PROJECT_DIR}` (Claude Code's documented project-root env var) instead of hardcoded paths — checked-in file is now portable across every clone, no post-clone path-rewrite needed. Models the [Husky](https://github.com/typicode/husky) / [pre-commit](https://pre-commit.com/) "committed config + relative paths" pattern. `codeforge install --project-hooks` likewise writes `${CLAUDE_PROJECT_DIR}` paths, producing byte-identical commit-friendly output across machines.
- Statusline V2 — unified visual language across minimal & full modes. `▏▕` chip wrappers replaced by `──` dim separators (matches box-drawing language). Git ahead/behind indicator `⇡N⇣M` (powerlevel10k-style). Claude Code version + update banner in both modes. Onboarding hint `→ codeforge adopt` always visible in minimal mode (degrades to `→ adopt` on narrow terminals). Context-pressure dimming when ctx > 80% — Tier-3 elements (separators, box-drawing) shift dimmer so the chat regains visual priority.
- Statusline V2 (full mode): village name collapsed into top border (6→5 rows). ASCII art column auto-drops below 90 cols. `HP/XP` → `♥/✦` symbols. Colon-less stat labels (`ATK 18` not `ATK: 18`). Strategy as `▸ agg` mode indicator. `Memory: active` → k9s-style `memory ● active`. i18n (`en.yaml` + `zh-TW.yaml`) updated to match.

### Fixed

- Project `.claude/settings.json` no longer dual-fires emit-session / session-digest hooks when `codeforge install --all` is run (clone-only scripts stay in project layer; product-wide scripts live in `~/.claude/`). README "First-time Claude Code hook setup" §Layer 1 / Layer 2 documents the split.
- `codeforge dream --quiet` SessionEnd hook restored in project settings (V2.2 portability rewrite had silently dropped it because it's not a `.claude/scripts/*.js` entry).

## [0.0.2] - 2026-05-15

### Added

- `codeforge install` subcommand wires `~/.claude/settings.json` automatically with the binary's absolute path (statusLine MVP). Solves the rustup `--no-modify-path` + dotfiles-without-cargo-block silent-failure case. See `doc/specs/codeforge-install-subcommand.md` for the `--hooks` follow-up plan.
- `codeforge install --hooks` installs global-safe hooks; `--all` installs both statusLine + hooks.
- `doc/getting-started.md` — 5-step linear quickstart (clone → install → wire → init → adopt → verify) with a troubleshooting matrix.
- Distribution scaffolding: `[package.metadata.binstall]` for `cargo-binstall`, release profile (strip + LTO + codegen-units=1), declared MSRV `rust-version = "1.85"` in Cargo.toml, `CHANGELOG.md`, `release.toml` for `cargo-release`. See `doc/specs/codeforge-release-pipeline.md` for the full release pipeline plan.
- GitHub Actions workflows: `release.yml` (4-target matrix build on tag push; cross-compile aarch64-linux; macOS Intel + M-series), `release-smoke.yml` (PR-gate tarball assembly).

## [0.0.1] - 2026-05-15

Initial development version. Phases 1 through 3f shipped; see `README.md` Phase Roadmap.
