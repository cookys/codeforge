# CLAUDE.md — CodeForge

## Goal
- End goal: operator 的 CLI dashboard：Claude Code 的 statusline 和 mod，不用切視窗就看得到用量、配額、fleet、記憶狀態。長期願景用 repo 原話：「把 MUD 搬上來。先打鐵，再上場。」
- Current milestone: G40 配額顯示：用真的本機鏡像資料顯示正確。這項卡在新帳號註冊 d-2a19fdb5。
- Not goals now: MUD 後段、自己產生配額資料、hosted CI。
- Source: operator 已確認。；操作員 d-fb9989dc 定案（2026-11-05）

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
- **LLM**: dream/ship via `claude -p` → Haiku API → rule-based (3-layer); AI commentary via Haiku API → rule-based (2-layer, no `claude -p`)
- **HTTP**: reqwest (async, TLS)
- **Async**: tokio (for daemon, Phase 2+)

## Development Commands

```bash
cargo build                    # debug build
cargo build --release          # release build → target/release/codeforge
cargo check                    # fast type-check (no binary)
cargo test                     # run tests
cargo clippy                   # lint
./scripts/check-all.sh         # THE merge gate (no hosted CI since G38): fmt, clippy --all-targets, test (flake guard), cjk-safe, doc-drift, cargo deny, shellcheck, checkout guard (main clone stays on develop), MSRV release smoke
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
- **SessionEnd chain is global**: `codeforge dream --quiet` → `codeforge ship --no-hook` runs in every project; `--quiet` is mandatory (no stdout in a hook), and `ship --no-hook` never fails SessionEnd (single attempt, Mnemos opt-in gated — no POST and no `ship-failed/` write when not opted in).
- **ship is production critical path**: schema/API changes get core-pipeline rigor; the Mnemos contract's single source of truth is `cookys/mnemos:docs/specs/10-source-contract.md`, never duplicated here. Keep the retry policy and `~/.codeforge/ship-failed/` persistence + next-ship replay (a dropped payload leaves Mnemos incomplete).
- **Daemon owns `pet_snapshot`**: the CLI never writes it (it appends `xp_events` + `event_inbox` only).
- **Hook placement**: product-wide hooks live only in the global install (`codeforge install --hooks`/`--all`); never move dream/ship into `--project-hooks` (dual-fire in the clone). Project `.claude/settings.json` holds the clone-only dev hooks. The SessionStart recall injector stays in global settings.json, not a plugin `hooks.json` (additionalContext unreliable, CC #16538). Re-install must stay idempotent: `patch_hooks` sweeps every codeforge-owned group (and legacy pre-marker commands) across all hook types before re-adding. `cleanupPeriodDays` goes into a shared settings.json (symlink or git-tracked) only with `--cleanup-period-days`.
- **Keep projects independent**: never import CodePower code into CodeForge tests or vice versa; they meet at runtime (hooks + CLI) only.
- **Memory stays per project**: dream/learn write the CWD project's `.codeforge/`; one shared store only when `CODEFORGE_DIR` selects it.
- **LLM key is optional**: dream/ship use the chain `claude -p` → `ANTHROPIC_API_KEY` (Haiku) → rule-based; never make a key required.

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
| `.claude/rpg-engine-spec.md` | Phase 1 architectural decisions (daemon model, write ownership) |
| `.claude/i18n-spec.md` | i18n two-layer design (compile-time UI + runtime content) |

> **Cross-project sync rule**: every CodeForge design decision (also ones made in CodePower sessions) lands in `doc/specs/*.md`, the single source of truth; conversations are not carried over.

## forge-stats mod

The **forge-stats mod** lives in `plugins/forge-stats/` and is a global plugin, installed from the repo's directory marketplace (`.claude-plugin/marketplace.json`; `claude plugin marketplace add <codeforge>` + `claude plugin install forge-stats@codeforge`), so every project's session loads it (PM G33). It reads `pet.json` and autopilot's `autopilot.fleet` state and never spawns codeforge. Test: `claude plugin test plugins/forge-stats`. Specs: `doc/specs/codeforge-mod-ui.md`, `doc/specs/tui-guideline.md`.

## Knowledge Management

`.claude/knowledge/` — record non-obvious issues and solutions (path/config failure then fixed,
a compile error that took 2+ retries, a design that iterated); `autopilot:learn` has the flow.
