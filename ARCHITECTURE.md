# CodeForge ARCHITECTURE

Owner: the repo's lane lead. This file is the only source for "what does this repo have".
README says how to use it and links here; CLAUDE.md's Goal block links here and does not list feature status.
No host names: write host roles (the role id is a key of the hangar inventory). Work progress is not recorded here;
building rows carry a fleet task id and the overview page reads progress from fleet tasks.

## 總架構圖

```
 Claude Code session (every project, every host)                       ┌────────────────────────────┐
 ┌──────────────────────────────────────────────┐                      │ Mnemos (中央腦, 遠端)       │
 │ SessionStart ──► hook emit-session           │                      │  POST /ingest (L2 ledger)   │
 │                  memory context --hook ──────┼─► .codeforge/ L1 ─┐   │  GET  /context (atoms)      │
 │                  mnemos-cli context --hook ◄─┼────────────────────┼───┤  POST /cite                 │
 │ (工作中)   statusline ─► 一行字 (永遠 1 列)    │                   │   └──────────▲─────────────────┘
 │            subagent-statusline ─► tasks 檔    │                   │              │ ship / cite
 │ SessionEnd ──► hook emit-session             │   L0 raw signals  ▼              │
 │                hook session-digest ──► digests ─► dream (L0→L1 編譯, LLM 5 層) ──┤
 │                dream --quiet ──────────────────┘   compile/lint/dedup/absorb/    │
 │                ship --no-hook ───────────────────► decay/track     ~/.codeforge/ship-failed/ (重送佇列)
 │                usage collect --quiet ─► summary.json (只存數字)                   │
 │ PreCompact ──► hook session-digest           │                                   │
 └───────────────┬──────────────────────────────┘                                   │
                 │ 讀                                                                │
 ┌───────────────▼──────────────┐   寫 pet.json / quota 檔   ┌───────────────────────┴───────┐
 │ forge-stats mod              │◄──────────────────────────│ 即時目錄 (tmpfs, 每主機一份)     │
 │ plugins/forge-stats          │                           │ <live>/codeforge/pet.json      │
 │  /forge-stats command, Pane, │   讀 autopilot 的 quota    │ <live>/codeforge/...quota 檔   │
 │  PromptHint, AbovePrompt     │   mirror (只讀)            │ <live>/context/ (subagent 任務) │
 └──────────────────────────────┘◄──── autopilot.fleet 狀態 └───────────────────────────────┘

 CLI `codeforge` (clap, src/cli/*) 子命令群
   設定: init install uninstall bootstrap self-update hook emit
   記憶: learn lessons memory dream ingest doctor ship mnemos-cli
   用量: usage (collect/push/export/accounts/pools/fleet/alias/note/plan/trinity-import)
   寵物/MUD: adopt pet strategy commentary world snapshot inventory craft use daemon tui attach

 MUD 引擎: daemon (tick loop, 唯一寫 pet_snapshot 的人) ◄─ event_inbox (SQLite WAL, emit/CLI 只 append)
           └─► pet.json (即時目錄) ──► statusline / forge-stats / tui

 狀態存放
   <project>/.codeforge/         L0 signals、L1 知識、projections/AGENTS.md (每專案獨立)
   ~/.codeforge/                 SQLite (event_inbox, pet)、ship-failed/、usage store (CODEFORGE_DATA_DIR 可換)
   ~/.config/codeforge/          accounts.toml (alias / note / plan, 不上傳)
   <live>/codeforge/             tmpfs 即時檔 (AUTOPILOT_LIVE_DIR → XDG_RUNTIME_DIR → /dev/shm)
   Mnemos (遠端)                  L2 ledger 與 atoms (契約正本在 Mnemos repo 的 source-contract)
```

## 功能地圖

Six columns, all required for every live, building and planned row from day one (backfill rows tagged `backfill` get 7 days).

| 名稱 [aliases: 別名1, 別名2] | 狀態 | 在哪裡 | 依據 | 互動介面 | 驗收方式 |
|---|---|---|---|---|---|
| Statusline 單行 [aliases: statusline, codeforge statusline, band 單行] | live | `src/cli/statusline.rs`; `src/live.rs`; role: every-dev-host | doc/plans/2026-10-31-statusline-band.md@f39bca1 | TUI | `cargo test --test statusline_one_line` |
| Subagent statusline [aliases: subagent-statusline, codeforge subagent-statusline] | live | `src/cli/subagent_statusline.rs` | doc/specs/codeforge-agent-coupling.md@00c76dc | 無 | `cargo test subagent_statusline` |
| Init 專案目錄 [aliases: init, codeforge init] | live | `src/cli/init.rs` | doc/getting-started.md@3c01049 | CLI | `cargo test cli::init` |
| Install / hooks 寫入 [aliases: install, codeforge install, uninstall, codeforge uninstall] | live | `src/cli/install.rs`; `install.sh`; role: every-dev-host | doc/specs/codeforge-install-subcommand.md@24bc7d9 | CLI | `cargo test --test install_shared` |
| Bootstrap 一鍵多機 [aliases: bootstrap, codeforge bootstrap] | live | `src/cli/bootstrap.rs` | doc/plans/2026-06-23-codeforge-bootstrap.md@c5a7d05 | CLI | `cargo run -q -- bootstrap --dry-run` |
| 待辦由 hippo 紀錄庫產生 [aliases: backlog-view, BACKLOG.md view, backlog 守門] | live | `scripts/backlog-view.sh`; `scripts/backlog-restore.py`; `scripts/backlog-view.test.sh`; `.hippo/**` | CLAUDE.md@c01041b | 無 | `./scripts/backlog-view.test.sh` |
| Self-update [aliases: self-update, codeforge self-update] | live | `src/cli/selfupdate.rs` | doc/specs/codeforge-release-pipeline.md@24bc7d9 | CLI | `cargo test selfupdate` |
| Hook runner [aliases: hook, codeforge hook, hook emit-session, hook session-digest] | live | `src/cli/hook.rs`; `src/memory/l0.rs`; `.claude/scripts/emit-session.js`; `.claude/scripts/session-digest.js` | doc/specs/codeforge-memory-contract.md@75d5f07 | 無 | `cargo test cli::hook` |
| SessionStart hooks [aliases: SessionStart, memory context --hook, mnemos-cli context --hook] | live | `src/cli/install.rs`; `src/memory/recall.rs`; `src/mnemos/context.rs` | doc/plans/2026-06-16-local-recall-phase-a.md@15e5f3a | 無 | `cargo test --test install_shared` |
| SessionEnd hook chain [aliases: SessionEnd, emit-session session_end, dream --quiet, ship --no-hook, usage collect --quiet] | live | `src/cli/install.rs`; `src/dream/**`; `src/mnemos/**`; `src/usage/**` | doc/plans/2026-11-01-install-stable-paths.md@33b96b6 | 無 | `cargo test --test install_shared` |
| PreCompact hook [aliases: PreCompact] | live | `src/cli/install.rs`; `.claude/scripts/session-digest.js` | doc/plans/2026-11-02-autopilot-hook-bridge.md@33b96b6 | 無 | `cargo test --test install_shared` |
| Clone-only dev hooks [aliases: project-hooks, check-improvements.js, check-dev-flow.js] | live | `.claude/scripts/check-improvements.js`; `.claude/scripts/check-dev-flow.js`; `src/cli/install.rs` | doc/specs/codeforge-install-subcommand.md@24bc7d9 | 無 | `node .claude/scripts/session-digest.test.js` |
| Emit 事件 [aliases: emit, codeforge emit, event_inbox] | live | `src/cli/emit.rs`; `src/daemon/inbox.rs` | doc/plans/2026-04-17-phase2a-daemon.md@8d1ecc1 | CLI | `cargo test daemon::inbox` |
| Learn 手動知識 [aliases: learn, codeforge learn] | live | `src/cli/learn.rs`; `src/memory/l0.rs` | doc/specs/codeforge-memory-contract.md@75d5f07 | CLI | `cargo test memory::l0` |
| 候選教訓擷取 [aliases: lessons, codeforge lessons, lessons candidates, codeforge lessons candidates] | building | `src/lessons/`; `src/cli/lessons.rs` | doc/plans/2026-11-23-knowledge-pipeline.md@b3eae59 | CLI | `cargo test lessons` |
| Memory 搜尋與狀態 [aliases: memory, memory search, memory status, codeforge memory] | live | `src/cli/search.rs`; `src/memory/fts.rs` | doc/specs/codeforge-memory-contract.md@75d5f07 | CLI | `cargo test memory::` |
| Memory recall 注入 [aliases: memory context, local recall] | live | `src/cli/recall.rs`; `src/memory/recall.rs` | doc/plans/2026-06-16-local-recall-phase-a.md@15e5f3a | CLI | `cargo test memory::recall` |
| Memory project 投影 [aliases: memory project, AGENTS.md projection] | live | `src/memory/projection.rs` | doc/specs/codeforge-memory-contract.md@75d5f07 | CLI | `cargo test memory::projection` |
| Dream L0 到 L1 [aliases: dream, codeforge dream] | live | `src/cli/dream.rs`; `src/dream/**`; `src/llm.rs` | doc/specs/codeforge-memory-contract.md@75d5f07 | CLI | `cargo test dream::` |
| Ingest 匯入 [aliases: ingest, codeforge ingest] | live | `src/cli/ingest.rs`; `src/import/**` | doc/plans/2026-06-25-extraction-filter.md@6684c46 | CLI | `cargo run -q -- ingest --help` |
| Ship 到 Mnemos [aliases: ship, codeforge ship, L2 ledger] | live | `src/cli/ship.rs`; `src/mnemos/**`; role: every-dev-host | doc/specs/codeforge-ship.md@24bc7d9 | CLI | `./scripts/e2e-mnemos-chain.sh` |
| Mnemos client [aliases: mnemos-cli, codeforge mnemos-cli, mnemos-cli cite, mnemos-cli cite-detect, mnemos-cli probe, mnemos-cli context] | live | `src/cli/mnemos_cli.rs`; `src/mnemos/**` | doc/specs/codeforge-ship.md@24bc7d9 | CLI | `cargo test mnemos::` |
| Doctor 腦部診斷 [aliases: doctor, codeforge doctor, brain indicators] | live | `src/cli/doctor.rs`; `src/brain/**` | doc/plans/2026-06-23-brain-indicators.md@526b45b | CLI | `cargo test cli::doctor` |
| Usage 用量表與收集 [aliases: usage, codeforge usage, usage collect] | live | `src/cli/usage.rs`; `src/usage/collect.rs`; `src/usage/summary.rs` | doc/plans/2026-11-03-usage-collector.md@d520a54 | CLI | `cargo test usage::collect` |
| Usage 匯出與上傳 [aliases: usage push, usage export, usage fleet] | live | `src/usage/sessions_export.rs`; `src/usage/fleet_quota.rs` | doc/specs/codeforge-usage.md@33b96b6 | CLI | `cargo test usage::fleet_quota` |
| Usage 帳號設定 [aliases: usage alias, usage note, usage plan, usage accounts, usage pools, usage trinity-import] | live | `src/usage/alias.rs`; `src/usage/quota_view.rs`; `src/usage/trinity.rs`; `src/usage/usage_v1.rs` | doc/specs/codeforge-usage.md@33b96b6 | CLI | `./scripts/trinity-import/trinity-import.test.sh` |
| G40 配額顯示 [aliases: G40, 配額顯示, quota display] | building | `src/usage/quota_mirror*.rs`; `scripts/g40-real-data/**`; `plugins/forge-stats/hooks/quota*.ts` | task 01M7VAKYT6KFQQ2001X8Q7HHC5 | TUI | `./scripts/g40-real-data/check.sh` |
| 領養寵物 [aliases: adopt, codeforge adopt] | live | `src/cli/adopt.rs`; `src/pet/village.rs` | doc/specs/codeforge-mud-engine.md@0141414 | CLI | `cargo test pet::` |
| 查看寵物 [aliases: pet, codeforge pet] | live | `src/cli/pet.rs`; `src/pet/view.rs`; `src/pet/sprite.rs` | doc/plans/2026-10-31-pixel-pet.md@43a89cb | CLI | `cargo test pet::view` |
| 戰鬥策略 [aliases: strategy, codeforge strategy] | live | `src/cli/strategy.rs`; `src/daemon/strategy.rs` | doc/plans/2026-04-18-phase3b-strategy.md@0560a16 | CLI | `cargo test daemon::strategy` |
| AI 評論 [aliases: commentary, commentary on, commentary off, commentary list, commentary test] | live | `src/cli/commentary.rs`; `src/commentary/**` | doc/plans/2026-04-18-phase3c-ai-commentary.md@c76a5bd | CLI | `cargo test commentary::` |
| 世界地圖 [aliases: world, codeforge world] | live | `src/cli/world.rs`; `src/world/**` | doc/plans/2026-04-18-phase3a-world-map.md@81a0049 | CLI | `cargo test world::` |
| 月報卡 [aliases: snapshot, codeforge snapshot] | live | `src/cli/snapshot.rs`; `src/snapshot/**` | doc/plans/2026-04-18-phase3f-snapshot.md@d9ce091 | CLI | `cargo test snapshot::` |
| 道具與合成 [aliases: inventory, craft, use, codeforge inventory, codeforge craft, codeforge use] | live | `src/cli/inventory.rs`; `src/cli/craft.rs`; `src/cli/use_item.rs`; `src/craft/**` | doc/plans/2026-04-18-phase3e-crafting.md@a4a6634 | CLI | `cargo test craft::` |
| Daemon [aliases: daemon, daemon start, daemon stop, daemon status, daemon install] | live | `src/cli/daemon.rs`; `src/daemon/**` | doc/plans/2026-04-17-phase2a-daemon.md@8d1ecc1 | CLI | `cargo test daemon::` |
| MUD TUI [aliases: tui, attach, codeforge tui, codeforge attach] | live | `src/cli/tui.rs`; `src/cli/attach.rs`; `src/tui/**` | doc/plans/2026-04-17-phase2c-tui.md@dc322f6 | TUI | `cargo test tui::` |
| 黏著度機制 [aliases: stickiness, welcome-back, mood] | live | `src/daemon/mood.rs`; `src/daemon/catchup.rs` | doc/plans/2026-04-17-phase3d-stickiness.md@46afbe3 | 無 | `cargo test daemon::mood` |
| 戰鬥與戰利品 [aliases: combat, loot, mob] | live | `src/daemon/combat.rs`; `src/daemon/loot.rs`; `src/daemon/mob.rs` | doc/plans/2026-04-17-phase2b-combat.md@b87c520 | 無 | `cargo test daemon::combat` |
| forge-stats 指令與 Pane [aliases: forge-stats, /forge-stats, Pane, forge-stats mod] | live | `plugins/forge-stats/hooks/register.tsx`; `plugins/forge-stats/hooks/pane-view.ts`; `plugins/forge-stats/hooks/panel.ts` | doc/specs/codeforge-mod-ui.md@3792062 | TUI | `claude plugin test plugins/forge-stats` |
| forge-stats band [aliases: PromptHint, AbovePrompt, band] | live | `plugins/forge-stats/hooks/register.tsx`; `plugins/forge-stats/hooks/side.ts` | doc/specs/panel-band-contract.md@c526fbc | TUI | `claude plugin test plugins/forge-stats` |
| forge-stats 艦隊與寵物頁 [aliases: fleet tab, pet tab, pet-icon] | live | `plugins/forge-stats/hooks/fleet-view.ts`; `plugins/forge-stats/hooks/pet-icon.ts` | doc/plans/2026-10-31-forge-tabs.md@f39bca1 | TUI | `claude plugin test plugins/forge-stats` |
| v7.4 autopilot-live pane 改版 [aliases: v7.4 pane, live-pane, pane rework] | building | `scripts/pane-gate/v74-keys/**`（本 repo 只放驗收；實作在 autopilot repo 的 `mods/live/v74/**`） | task 01M7N307HWS1C2BE9W1V7Q7K6J | TUI | `scripts/pane-gate/v74-keys/run.py` |
| v7.4 pane 右側預覽區 [aliases: 下一張卡預覽, pane 右欄] | requested | PM 01M7XBE2KG8MNA1J7GZYSRTTVJ | 操作員排版規則 (3)，經 PM 轉述；這一輪先留白 |  |  |
| 共用 tux 詞彙 [aliases: tux, tux-sync, tux-lock, tux-cross, tux-lint] | live | `plugins/forge-stats/vendor/tux/**`; `scripts/tux-*.sh`; `scripts/tux-lint.py` | doc/specs/tui-guideline.md@3792062 | 無 | `./scripts/tux-lock.sh` |
| Merge gate [aliases: check-all, 合併閘門, check-all.sh, fmt.sh, push-ref-guard, check-private-leaks, check-doc-drift] | live | `scripts/check-all.sh`; `scripts/fmt.sh`; `scripts/check-*.sh`; `scripts/*.test.sh`; `.githooks/**` | doc/specs/codeforge-release-pipeline.md@24bc7d9 | CLI | `./scripts/check-all.sh` |
| Release 打包 [aliases: package.sh, release-local.sh, repro-check.sh, release] | live | `scripts/package.sh`; `scripts/release-local.sh`; `scripts/repro-check.sh`; `scripts/lib/**`; role: operator-console | doc/specs/codeforge-release-pipeline.md@24bc7d9 | CLI | `./scripts/test-archive-hermetic.sh` |
| curl 安裝器 [aliases: install.sh] | live | `install.sh`; role: every-dev-host | doc/specs/codeforge-release-pipeline.md@24bc7d9 | CLI | `shellcheck install.sh` |
| G40 真實資料檢查 [aliases: g40 check, g40-real-data] | live | `scripts/g40-real-data/**` | doc/plans/2026-11-03-g40-usage-accounts.md@2ff6f80 | CLI | `./scripts/g40-real-data/check.sh` |
| Pane 閘門腳本 [aliases: pane-gate, ux-review, nojump-check] | live | `scripts/pane-gate/**` | doc/specs/forge-stats-ux-gate.md@89747b4 | CLI | `./scripts/pane-gate/v74-keys/parse.test.sh` |
| Trinity 配額匯入 [aliases: trinity-import] | live | `scripts/trinity-import/**`; role: operator-console | doc/specs/codeforge-usage.md@33b96b6 | CLI | `./scripts/trinity-import/trinity-import.test.sh` |
| Zoa 3D 寵物動畫 [aliases: phase 4, Zoa] | planned | `src/pet/**` | doc/CLAUDE.md@0141414 | TUI | `tests/interaction/zoa-pet.sh` |
| Nation 外掛與憑證 [aliases: nation, phase 5a, nation plugin] | planned | `src/clan/**`; `src/power/**` | doc/specs/nation-p2p-design.md@7ddc66a | CLI | `tests/interaction/nation-verify.sh` |
| Nation 主辦跨國事件 [aliases: organizer, phase 5b] | planned | `src/power/**` | doc/specs/nation-p2p-design.md@7ddc66a | CLI | `tests/interaction/nation-organizer.sh` |
| Nation statusline 主題 [aliases: nation theme, phase 5c] | planned | `src/cli/statusline.rs`; `src/power/**` | doc/specs/codeforge-mud-engine.md@0141414 | TUI | `tests/interaction/nation-theme.sh` |
| pane 寵物分頁正式上線 [aliases: B57, pet tab live] | planned | `plugins/forge-stats/hooks/pet-icon.ts`; `plugins/forge-stats/hooks/pane-view.ts` | doc/BACKLOG.md@d87e5c3 | TUI | `tests/interaction/pet-tab.sh` |
| FTS5 中日韓斷詞 [aliases: B2, cjk fts] | planned | `src/memory/fts.rs` | doc/BACKLOG.md@d87e5c3 | CLI | `cargo test memory::` |
| ChatGPT 匯出解析 [aliases: B5, chatgpt ingest] | planned | `src/import/chatgpt.rs` | doc/BACKLOG.md@d87e5c3 | CLI | `tests/interaction/chatgpt-ingest.sh` |
| 徽章授予事件 [aliases: B3, badges] | planned | `src/pet/badges.rs` | doc/BACKLOG.md@d87e5c3 | CLI | `cargo test pet::` |
| codeforge 成為 autopilot extension [aliases: B31, extension] | planned | `src/**` | doc/BACKLOG.md@d87e5c3 | CLI | `cargo test --test statusline_one_line` |

Column rules:

1. 名稱 and aliases: the name the operator would say, plus every other name used for it. Put aliases in `[aliases: a, b]` (comma-separated); pm-intake matches on them.
2. 狀態: `live`, `building`, `planned` or `requested`.
3. 在哪裡: at least one repo path glob (the merge gate maps a diff to rows by it); host only as a role id.
4. 依據: `plan-path@sha` or a task id. Building rows: task id only.
5. 互動介面: 無, CLI, TUI, web, 語音 or 裝置 UI.
6. 驗收方式: one re-runnable command or script path. If the interface is not 無, it must be a script under `tests/interaction/` that finish-flow runs against the real build (rc 0). A screenshot is not acceptance.

`requested` rows fill only three columns: name, intake receipt id, and the ledger sha of the operator's words or constraint. The other cells stay empty. (Today there are none.)

Checked by `scripts/check-architecture-coverage.sh` (every CLI subcommand, written hook, mod component and operator-facing script maps to a row; six cells per row; no deny-list term).

## 缺口清單

- `tests/interaction/` does not exist at all: every row whose 互動介面 is not 無 uses the best existing command (cargo unit test, plugin test, or a scripted gate) in place of a real-build interaction script. Impact: finish-flow cannot run an end-to-end script for any CLI/TUI row.
- `tests/interaction/statusline.sh` is missing (real `codeforge statusline` through a tmux pane at several widths; today only the golden `tests/statusline_one_line.rs` and the G40 scan cover it).
- `tests/interaction/install-hooks.sh` is missing (install into a sandbox HOME, then run each written hook command; today `tests/install_shared.rs` checks the settings.json edit only).
- `tests/interaction/forge-stats.sh` is missing (open `/forge-stats` in a real `claude --plugin-dir` tmux session; only `scripts/pane-gate/v74-keys/run.py` does this, and it targets the autopilot pane, not this mod).
- `tests/interaction/tui.sh` is missing for `codeforge tui` / `attach` (alt-screen, needs tmux).
- `src/import/**` (`ingest`) has no test at all, and `tests/interaction/ingest.sh` and `tests/interaction/chatgpt-ingest.sh` are missing; the ingest row's acceptance is only a `--help` run.
- `tests/interaction/pet-tab.sh`, `zoa-pet.sh`, `nation-verify.sh`, `nation-organizer.sh`, `nation-theme.sh` are missing because their rows are planned (B57, Phase 4, Phase 5).
- G40 quota display is not accepted on real data: waits for a real mirror sample behind card d-2a19fdb5 (task 01M7VAKYT6KFQQ2001X8Q7HHC5, B45).
- Pane second-batch defects B52, B53, B56, B61, B62, B63 are open in `doc/BACKLOG.md` (band truncation, missing counts, hot-key leak at narrow width, no-project pages, spend page without numbers, tab row at 120x30).
- B64: a pushed binary is not reproducible (same commit, different sha16 per host), so rollout checks cannot compare sha across hosts.
- B65: `absorb` writes L0 rows without a source slug, so a row's origin project cannot be traced.
- B1: two concurrent `dream` runs race on the signal cursor.
- B13: no end-to-end check of a real production `ship` against the live Mnemos (`scripts/e2e-mnemos-chain.sh` uses test-tagged data only).

## 重構候選

- Split `src/cli/install.rs` (hook tables, project hooks, statusline patching and settings.json merge in one file); expected benefit: each hook set gets its own test and this map's hook rows point at one file each.
- Merge the three overlapping pane/band specs (`doc/specs/panel-band-contract.md`, `doc/specs/codeforge-mod-ui.md`, `doc/specs/tui-guideline.md`, see B29) into one source; expected benefit: one place for the band row count and contract.
- Move `scripts/pane-gate/**` (a gate for autopilot's pane, not for this repo's code) toward the repo it gates, keeping only the forge-stats part here; expected benefit: this repo's merge gate stops carrying another repo's fixtures.
- Give `src/usage/` a sub-structure (collect, quota, fleet, accounts); it has 30 files in one directory, and `usage` is three separate user stories in this map.
- Fold `src/daemon/` and `src/pet/` ownership of `pet_snapshot` / `pet.json` behind one writer module; today the writer rule ("daemon owns pet_snapshot, CLI appends only") is a convention, not a type boundary.

## 如何更新

- A diff that touches a path in some row's 「在哪裡」 must change that row in the same diff.
- A new CLI subcommand, MCP tool, API route or systemd unit with no matching row needs a new row in the same diff.
- Re-run the row's 驗收方式 before changing 狀態 to `live`.
- Lint: `./scripts/check-architecture-coverage.sh` (runs in `check-all.sh`): no host names or IPs (`/etc/lab/deny.txt`), six columns filled, every subcommand/hook/mod component mapped.
- When a requested row is taken, change it to building and fill all six columns.
