# CLAUDE.md — CodeForge

## Goal
- End goal: operator 的 CLI dashboard：Claude Code 的 statusline 和 mod，不用切視窗就看得到用量、配額、fleet、記憶狀態。長期願景用 repo 原話：「把 MUD 搬上來。先打鐵，再上場。」
- Current milestone: G40 配額顯示：用真的本機鏡像資料顯示正確。這項卡在新帳號註冊 d-2a19fdb5。
  - 追蹤：backlog B45；fleet 單：尚未開（卡 d-2a19fdb5 解開、拿到真實鏡像樣本時開單）；上線日：卡解後 1 天內跑 `scripts/g40-real-data/check.sh` 並 pin 上 live（PM 01M7VAG6 補欄位）。
- Not goals now: MUD 後段、自己產生配額資料、hosted CI。
- Source: operator 已確認。；操作員 d-fb9989dc 定案（2026-11-05）

## This repo is yours

這個 repo 是你的，你是它的長期維護者。看到壞掉、過時、重複或缺測試的地方，在授權範圍內直接修，不用等人指派；修不了的登進 backlog，寫明影響。不要把問題留給「之後的人」。

## Within your grant, stop only at acceptance

授權範圍內的工作做到驗收才停：已合併、live 已 pin 到那個 sha、真實使用者在真實 build 上走過核心路徑，並附一行證據（截圖、指令或 URL）。測試綠、審查通過、推送都只是中間步驟。守門類的改動（check-all 的閘門、pre-push guard、private-leaks 這類），還要在 live 上做一次反向探針：故意違規，確認真的會被擋下。

只有三種情況可以中途停下：需要 tier-2 破壞性操作、超出授權、要改 Goal 或里程碑。停下時一次說清楚，附上選項和建議。

## How we work

- 工作狀態只記在 fleet task。開工、卡住、改期、交件都寫回單上，不用聊天代替。
- 能開工的全部同時開工。實作交給便宜的引擎席（codex 優先；機械性的單交給更便宜的席），每條各用一個 worktree。本席只做判斷、審查和整合，不把工作排成單線。每條做完就立刻審，不等全部做完。
- 隨時至少保留兩件可以直接開工的工作（`doc/BACKLOG.md` 的 ready 列）。手上這件卡住或做完時，下一件的預檢已經通過。
- 要 go 的時候，同一時間把下一件準備好，不空等。內部換版的 go 跟 PM 要，先跑 canary，再全面換。
- KR 和觀察期只量測，不擋上線。計畫要寫上線日和 fleet 單號，不能一放好幾週。
- 卡住時只回報一次，附上建議，回報對象是 PM。在等別人時，寫上帶型別的 waiting_on（等哪張單、哪張卡、哪個人的哪則訊息）。
- 同一件事失敗兩次，就換做法或換引擎。

## Worktree and branch hygiene

- **先確認分支再動手**：每次開始編輯前，以及恢復 session 或切換情境之後，先跑 `git status` 和 `git branch --show-current`，確認自己在對的分支和對的 worktree。
- **一件工作一條分支一個 worktree**：分支命名用 `feat/<slug>`、`fix/<slug>`、`docs/<slug>`、`chore/<slug>`，從最新的 develop 開出來：`git worktree add .claude/worktrees/<slug> -b <branch> develop`（`.claude/worktrees/` 在 `.gitignore` 裡）。主 clone 永遠停在 develop，只用來整合（理由見 Conventions）。
- **平行 worktree 不共用相依目錄**：每個 worktree 各有自己的 `target/`、node_modules 和 build 輸出，不用 symlink。
- **commit 要帶 trailer**：每個 commit 都加上 `Fleet-Task: <26 碼單號>`。不在 develop 或 main 上直接 commit 功能改動。
- **合併方式**：用 `git merge --no-ff` 合進 develop，合併前在獨立的一次呼叫裡跑 `./scripts/check-all.sh` 並看結束碼；合併訊息寫審查結論和 tip 測試綠燈。main 只在 PM 給 go 後合併。共用分支一律禁止 force-push 和改寫歷史。
- **origin 是公開 repo**：只推 develop（自己的 lane 可直接推）和 main（要 PM 本人的 go）；pre-push guard 會擋其他 ref。功能分支不推 origin，所以「刪遠端分支」不是例行步驟；遠端已有的非 develop/main 分支要刪，先問 PM。
- **合併後當場清乾淨**（屬於 tier 0）：`git branch -d <branch>`、`git worktree remove .claude/worktrees/<slug>`，最後 `git worktree prune`。
- **不留孤兒**：
  - 超過 3 天沒有 commit、又沒有對應活單的分支或 worktree，要嘛開單接著做，要嘛刪掉。
  - 沒合併過的工作在刪除前，先打 tag `archive/<branch>`，再 `git bundle create` 到本機備份目錄並 `git bundle verify`，這樣才算 tier 1。archive tag 不推公開 origin。
- **不把 stash 當倉庫**：stash 堆疊是所有 worktree 共用的，不用裸的 `git stash`。一輪結束時還有未提交的工作，就在分支上提交一個 WIP commit。
- **live 只跑 pin 住的 sha 或 tag**，不跑分支的最新 commit。換版走既有的換版流程：每台主機先備份並記下 sha16，寫好回滾指令。
- **測試不碰正式環境**：worktree 和測試一律用沙箱 HOME 或暫存目錄。手動餵 stdin 跑真的 `codeforge statusline` 會寫 live 配額檔，要設沙箱的 `AUTOPILOT_LIVE_DIR` 和 `CODEFORGE_DATA_DIR`。

Claude Code 工作指南。

## Project Overview

CodeForge 是 **CodePower 生態系裡的私人鍛造間**（personal smithy）— 給 Claude Code power-user 用的 Rust CLI 工具集。

**三支柱**：
- **Memory pipeline** — L0 raw signals → L1 compiled knowledge（透過 Anthropic Haiku），本機累積
- **MUD 引擎** — codebase 是世界地圖、技術債是怪物、pet 是玩家角色
- **Mnemos source role** — `codeforge ship` 把 L1 + git log digest 成 L2 daily ledger、POST 到 Mnemos（中央 brain）；**production critical path**。詳見 `src/mnemos/CLAUDE.md` 與 [`doc/specs/codeforge-ship.md`](doc/specs/codeforge-ship.md)。

**生態系定位**：
- **CodePower**（鬥技場）— 公開、聯邦、競技；Nation 團戰、scoring 排行、federated 賽事
- **CodeForge**（鍛造間）— 私人、寧靜、累積；知識淬煉、寵物養成、loot 鍛造、本機 codebase 探索

**「Power 提供能量，Forge 塑形」**。三種用法：(1) Solo Smith — 純單機跑 MUD；(2) Connected Smith — 連到一個 CodePower nation 帶 forged self 參戰；(3) Multi-Nation Pilgrim（Phase 5）— 透過 Nation P2P 同時連多個 nation。

## Tech Stack

- **Language**: Rust (2021 edition)
- **CLI**: clap 4 (derive)
- **Storage**: rusqlite (bundled SQLite), WAL mode
- **Terminal**: crossterm + termcolor
- **I18n**: rust-i18n v3 (compile-time), `locales/` YAML
- **LLM**: dream/ship via `claude -p` → `agy` → `codex exec` → Haiku API → rule-based (5-layer); AI commentary via Haiku API → rule-based (2-layer, no `claude -p`)
- **HTTP**: reqwest (async, TLS)
- **Async**: tokio (for daemon, Phase 2+)

## Development Commands

```bash
cargo build                    # debug build
cargo build --release          # release build → target/release/codeforge
cargo check                    # fast type-check (no binary)
cargo test                     # run tests
cargo clippy                   # lint
./scripts/check-all.sh         # THE merge gate (no hosted CI since G38): fmt, clippy --all-targets, test (flake guard), cjk-safe, doc-drift, cargo deny, shellcheck, forge-stats plugin tests, g40 real-data scans, trinity-import test, checkout guard (main clone stays on develop), public push guard (pre-push refuses any ref but develop/main to the public origin; override `CODEFORGE_PUBLIC_PUSH_OVERRIDE="<id>: <reason>"`, audit line in the git dir), release archive hermetic test (B43), MSRV release smoke
./scripts/package.sh <target>  # release tarball + .sha256 named as install.sh downloads them (dist/); --cross, --max-glibc; reproducible
./scripts/repro-check.sh <target> # build HEAD twice at two paths; archives must match
./scripts/release-local.sh     # local release: linux x86_64/aarch64 via cross (glibc <= 2.35), install.sh verified from a local server, dist/RELEASE.txt; --publish = DRAFT only (operator publishes)
./scripts/test-flake-guard.sh  # cargo test; a failure is rerun once, a flake is recorded in target/flaky-tests.log (never hidden)
./scripts/g40-real-data/check.sh [<live_dir>]  # G40: statusline + panel/1 against a quota mirror at 5 widths (sandboxed); no arg = fixture, --negative = must fail

# Formatting — ALWAYS via the pinned wrapper, NEVER bare `cargo fmt`
./scripts/fmt.sh               # format in place (pinned rustfmt, self-installs)
./scripts/fmt.sh --check       # verify (exit 1 on drift; part of check-all)

# Install locally
cargo install --path .

# Run a command
cargo run -- learn "some note"
cargo run -- dream
cargo run -- statusline
cargo run -- pet
cargo run -- bootstrap --dry-run   # one-command per-machine setup (B14); see README
```

> **Formatting is pinned**: `scripts/fmt.sh` holds the one rustfmt version (rustfmt output drifts
> between toolchains); build/clippy/test stay on rolling stable. Details:
> `.claude/quality-gate-config.md` → Formatting.

## Where the detail lives

Domain detail is in subdirectory `CLAUDE.md` files. Claude Code loads one when a file in that
directory is touched; Codex and agy do not descend, so a dispatch brief must name the file
(each directory also has an `AGENTS.md` stub pointing to it).

| Working on | Read |
|---|---|
| anything under `src/` — source/data layout, key design decisions, LLM backend chain, CodePower test zone | `src/CLAUDE.md` |
| `src/cli/` — statusline, dream, memory recall, learn, install/hook wiring | `src/cli/CLAUDE.md` |
| `ship` / `mnemos-cli` (`src/mnemos/`, `src/cli/ship.rs`, `src/cli/mnemos_cli.rs`) — ship, cite, retry, opt-in gate | `src/mnemos/CLAUDE.md` |
| Phase Roadmap + Mnemos Integration Roadmap | `doc/CLAUDE.md` |

## Damage-class rules (always apply; detail in the files above)

- **CJK**: cut user-generated strings with `.chars().take(N).collect::<String>()`, never `&s[..N]` (`scripts/check-cjk-safe.sh` enforces).
- **Statusline is global**: `codeforge statusline` runs in every Claude Code session of every project; a change hits every session header.
- **Display rules come from the operator**: before changing what the statusline or band shows (which form draws, when the pet/RPG panel appears), the statusline is exactly one line in every state and never draws the pet (operator via PM 01M7MEY1, 2026-11-17; golden `tests/statusline_one_line.rs`); the pet lives in its own tab (PM 01M78CB6, 01M7MD5Z). A lane-added safeguard may not override them (B34 `band_seen` did, reverted in 48d2489).
- **SessionEnd chain is global**: emit-session → session-digest → `codeforge dream --quiet` → `codeforge ship --no-hook` → `codeforge usage collect --quiet` runs in every project (`src/cli/install.rs:832-859`; usage collect `src/cli/install.rs:849-858`); `--quiet` is mandatory (no stdout in a hook), and `ship --no-hook` never fails SessionEnd (single attempt, Mnemos opt-in gated — no POST and no `ship-failed/` write when not opted in).
- **ship is production critical path**: schema/API changes get core-pipeline rigor; the Mnemos contract's single source of truth is `cookys/mnemos:docs/specs/10-source-contract.md`, never duplicated here. Keep the retry policy and `~/.codeforge/ship-failed/` persistence + next-ship replay (a dropped payload leaves Mnemos incomplete).
- **Daemon owns `pet_snapshot`**: the CLI never writes it (it appends `xp_events` + `event_inbox` only).
- **Hook placement**: product-wide hooks live only in the global install (`codeforge install --hooks`/`--all`); never move dream/ship into `--project-hooks` (dual-fire in the clone). Project `.claude/settings.json` holds the clone-only dev hooks (`check-improvements.js` SessionStart, `check-dev-flow.js` PreToolUse; `src/cli/install.rs:783-801`). The global set today: SessionStart emit-session + the two recall injectors; SessionEnd emit-session → session-digest → `dream --quiet` → `ship --no-hook` → `usage collect --quiet`; PreCompact session-digest (`src/cli/install.rs:805-865`). The SessionStart recall injectors (local `codeforge memory context --hook` + central `codeforge mnemos-cli context --hook`; `src/cli/install.rs:806-829`) stay in global settings.json, not a plugin `hooks.json` (additionalContext unreliable, CC #16538). Re-install must stay idempotent: `patch_hooks` sweeps every codeforge-owned group (and legacy pre-marker commands) across all hook types before re-adding. `cleanupPeriodDays` goes into a shared settings.json (symlink or git-tracked) only with `--cleanup-period-days`.
- **Keep projects independent**: never import CodePower code into CodeForge tests or vice versa; they meet at runtime (hooks + CLI) only.
- **Memory stays per project**: dream/learn write the CWD project's `.codeforge/`; one shared store only when `CODEFORGE_DIR` selects it.
- **LLM key is optional**: dream/ship use the 5-layer chain `claude -p` → `agy` → `codex exec` → `ANTHROPIC_API_KEY` (Haiku) → rule-based (headless three: `src/llm.rs:313-339`; key and rule-based layers in the callers: `src/dream/compile.rs:471-490`, `src/cli/ship.rs:199-225`); never make a key required.

## Conventions

- **Error handling**: `anyhow::Result` throughout, user-facing messages in 繁體中文
- **Truncation**: `.chars().take(N).collect::<String>()` for all user-generated content (CJK safe)
- **SQLite PRAGMA**: WAL + foreign_keys + busy_timeout=5000 (set in `db::Context::open_db`)
- **I18n**: UI strings in `locales/en.yaml` + `locales/zh-TW.yaml`, game content runtime (Phase 2+)
- **Commit style**: Conventional Commits (`feat/fix/docs/chore/build`)
- **Branch**: 所有工作先走 feature branch 再進 develop，main 只接受 --no-ff 合併（d-6b183bd3）
- **主 clone 永遠停在 develop**（PM 2026-11-06）：forge-stats 外掛直接從主 clone 的 `plugins/forge-stats` 載入（`claude plugin update` 回報 "read from its folder"），主 clone 切到哪個分支、改了什麼還沒 commit 的檔案，各 session 在 `/reload-plugins` 或重開後就跑那一份。所以：feature 開發一律在 `git worktree add ../<目錄> -b <分支> develop` 做，不在主 clone 切分支，也不在主 clone 直接改檔。主 clone 要裝 post-checkout 警告：`./scripts/install-checkout-guard.sh`（切離 develop 時大聲警告；測試 `scripts/checkout-guard.test.sh`，在 check-all 裡）。警告擋不住的：主 clone 上沒 commit 的修改、`git reset --hard` 或 merge 讓 develop 本身指到別的內容、`git bisect`。

## Session Start (Mandatory)

**STOP. Before doing ANYTHING — invoke `autopilot:dev-flow` first. No exceptions.**

1. Invoke `autopilot:dev-flow`
2. Review `.claude/knowledge/INDEX.md` for relevant learnings
3. Check `doc/projects/INDEX.md` for active projects (orphan detection)
4. If unprocessed digests → invoke `autopilot:learn`

## Specs

| File | Content |
|------|---------|
| `doc/specs/codeforge-mud-engine.md` | Phase 2+ MUD engine — daemon, combat, TUI, §3 黏著度機制, §3.10 Nation Theme |
| `doc/specs/nation-p2p-design.md` | Phase 5 Nation P2P — credential schema, Organizer role, P2P integrity |
| `doc/specs/codeforge-ship.md` | Mnemos source role — L2 ledger producer + cite client (shipped v0.0.4) |
| `doc/specs/codeforge-mnemos-runtime.md` | **運維現況**（非設計）— 腦↔codeforge 目前怎麼跑：拓樸、WRITE/READ live vs 未接、hook chain、自檢指令、fleet 隧道。免每次翻 hook+code |
| `doc/specs/codeforge-agent-coupling.md` | **ADR** — codeforge↔coding-agent 耦合接縫（4 surface：transcript→signal / hook wiring / session 事件 / statusline）。決策 *A-with-seam*：先綁 CC、命名守住接縫、不過早抽象，留多-agent（B）路 |
| `doc/specs/codeforge-usage.md` | **用量（G36）** — 每帳號配額快照 + statusline 用量列、轉錄檔收集器（只存數字）、成本估算、`summary.json` 契約與 `usage push` |
| `doc/specs/codeforge-mod-ui.md` | **開發依據** — Claude Code mod UI 能力與限制（已覆核，CC 2.1.314）、owner 終端實測（mod `Image` 在 Windows 終端不可用 → 像素走 Raster 半格）、codeforge↔mod 分工（`pet.json` 契約） |
| `doc/specs/forge-stats-ux-gate.md` | **UX 閘門** — forge-stats 畫面改動上線前必過：carbon-high + astra 皆 ≥8/10 且無 Critical、最多 3 輪；評分項目（好用／fancy）、必交截圖 S1–S6 |
| `.claude/rpg-engine-spec.md` | Phase 1 architectural decisions (daemon model, write ownership) |
| `.claude/i18n-spec.md` | i18n two-layer design (compile-time UI + runtime content) |

> **Cross-project sync rule**: every CodeForge design decision (also ones made in CodePower sessions) lands in `doc/specs/*.md`, the single source of truth; conversations are not carried over.

## forge-stats mod

The **forge-stats mod** lives in `plugins/forge-stats/` and is a global plugin, installed from the repo's directory marketplace (`.claude-plugin/marketplace.json`; `claude plugin marketplace add <codeforge>` + `claude plugin install forge-stats@codeforge`), so every project's session loads it (PM G33). It reads `pet.json` and autopilot's `autopilot.fleet` state and never spawns codeforge. Test: `claude plugin test plugins/forge-stats`. Any change to what it draws passes the UX gate first (`doc/specs/forge-stats-ux-gate.md`). Specs: `doc/specs/codeforge-mod-ui.md`, `doc/specs/tui-guideline.md`.

## Knowledge Management

`.claude/knowledge/` — record non-obvious issues and solutions (path/config failure then fixed,
a compile error that took 2+ retries, a design that iterated); `autopilot:learn` has the flow.
