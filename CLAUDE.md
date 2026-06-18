# CLAUDE.md — CodeForge

Claude Code 工作指南。

## Project Overview

CodeForge 是 **CodePower 生態系裡的私人鍛造間**（personal smithy）— 給 Claude Code power-user 用的 Rust CLI 工具集。

**三支柱**：
- **Memory pipeline** — L0 raw signals → L1 compiled knowledge（透過 Anthropic Haiku），本機累積
- **MUD 引擎** — codebase 是世界地圖、技術債是怪物、pet 是玩家角色
- **Mnemos source role** — `codeforge ship` 把 L1 + git log 再 digest 成 L2 daily ledger、POST 到 Mnemos（中央 brain）。（讀 session jsonl 為設計目標、code 未實作，見 ship spec 修正 (c)；`codeforge mnemos-cli cite-detect` 可回填用過的 atom，但目前是手動子命令、未接入 SessionEnd hook。）**Production critical path**（不是 nice-to-have）— SessionEnd hook 觸發、失敗有 retry policy、結果影響 Mnemos 資料完整性。詳見 [`doc/specs/codeforge-ship.md`](doc/specs/codeforge-ship.md)。

**生態系定位**：
- **CodePower**（鬥技場）— 公開、聯邦、競技；Nation 團戰、scoring 排行、federated 賽事
- **CodeForge**（鍛造間）— 私人、寧靜、累積；知識淬煉、寵物養成、loot 鍛造、本機 codebase 探索

**「Power 提供能量，Forge 塑形」**。三種用法：(1) Solo Smith — 純單機跑 MUD；(2) Connected Smith — 連到一個 CodePower nation 帶 forged self 參戰；(3) Multi-Nation Pilgrim（Phase 5）— 透過 Nation P2P 同時連多個 nation。

**Vision**: 把 MUD 搬上來。先打鐵，再上場。

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

# Install locally
cargo install --path .

# Run a command
cargo run -- learn "some note"
cargo run -- dream
cargo run -- statusline
cargo run -- pet
```

## Architecture

### Source Layout (`src/`)

```
src/
  main.rs           ← clap CLI dispatch (locale: CODEFORGE_LOCALE → system locale → "en")
  llm.rs            ← LLM backend chain: claude -p headless → ANTHROPIC_API_KEY (Haiku) → rule-based
  cli/              ← command handlers (one file per command)
    init.rs         ← codeforge init
    learn.rs        ← codeforge learn
    dream.rs        ← codeforge dream (--quiet / --only <op>)
    search.rs       ← codeforge memory search
    ingest.rs       ← codeforge ingest
    pet.rs          ← codeforge pet
    statusline.rs   ← codeforge statusline (ANSI 5-line panel + right-column pet art)
    install.rs      ← codeforge install (--hooks / --all / --project-hooks) + uninstall
    ship.rs         ← codeforge ship (Mnemos L2 ledger)
    mnemos_cli.rs   ← codeforge mnemos-cli (cite / cite-detect / context)
  memory/
    l0.rs           ← raw signals (JSONL file-based)
    l1.rs           ← compiled knowledge (SQLite FTS5)
    fts.rs          ← FTS5 search
  dream/
    compile.rs      ← L0 → L1 via the LLM backend chain (src/llm.rs)
    absorb.rs       ← aggregate recent signals
    ingest_digests.rs ← pull per-repo session digests into L0
  brain/
    episode.rs      ← session episode records
  pet/
    state.rs        ← PetState: level, HP, XP, stats
    xp.rs           ← XP award logic
    badges.rs       ← badge system (skeleton, not yet wired)
    live_state.rs   ← live-overlay read path (pet_snapshot + unseen event_inbox)
    village.rs      ← 5 hardcoded villages (rust/python/typescript/go/javascript)
    ability.rs      ← ability unlock catalog (display-only; combat effects not yet wired)
  daemon/           ← Phase 2 MUD engine: tick loop, ECS, event_inbox, combat, mob_scanner, loot, strategy, mood, lifecycle
  combat (in daemon/) ← auto-combat resolution
  mnemos/           ← ship transport/digest/config/state/cite/evidence (Mnemos source role)
  clan/             ← CodePower clan content provider skeleton (not yet wired to village.rs)
  craft/            ← loot crafting + active items (Phase 3e)
  tui/              ← alt-screen TUI + Local Map (Phase 2c)
  world/            ← World Map + Zone unlock (Phase 3a)
  commentary/       ← AI commentary (Haiku, ≤1/hour, Phase 3c)
  snapshot/         ← codeforge snapshot ASCII monthly card (Phase 3f)
  db/
    mod.rs          ← Context, Connection, PRAGMA setup
    migrations.rs   ← schema migrations (inline SQL)
  import/           ← claude.ai export parser
  power/            ← CharacterStats (5 fields: ATK/DEF/SUP/VER + HP/Activity)
```

### Data Layout (`.codeforge/`)

```
.codeforge/
  store/
    concepts/       ← L1 knowledge .md files (by topic)
    connections/    ← L1 link files
    qa/             ← Q&A pairs
  signals/
    YYYY-MM-DD.jsonl ← L0 raw signals
  projections/
    AGENTS.md       ← auto-projected context for sub-agents
  codeforge.db     ← SQLite: pet state, FTS index, episodes
```

### Key Design Decisions

- **L0**: JSONL files (human-readable, git-friendly, no lock needed for append)
- **L1**: SQLite FTS5 (full-text search, structured)
- **Daemon-free** (Phase 1): statusline is a one-shot command, not a daemon
- **Phase 2**: daemon owns the authoritative `pet_snapshot` (CLI never writes it). The CLI still appends XP events (`xp_events` + `event_inbox`) and computes a level-up cascade in-memory on read (live-overlay) — so "daemon-owned" means `pet_snapshot` specifically, not all game rows
- **CJK strings**: ALWAYS use `.chars().take(N).collect::<String>()`, NEVER `&s[..N]`

## Conventions

- **Error handling**: `anyhow::Result` throughout, user-facing messages in 繁體中文
- **Truncation**: `.chars().take(N).collect::<String>()` for all user-generated content (CJK safe)
- **SQLite PRAGMA**: WAL + foreign_keys + busy_timeout=5000 (set in `db::Context::open_db`)
- **I18n**: UI strings in `locales/en.yaml` + `locales/zh-TW.yaml`, game content runtime (Phase 2+)
- **Commit style**: Conventional Commits (`feat/fix/docs/chore/build`)
- **Branch**: `feature/{name}` for L-size, direct to main for S-size

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
| `doc/specs/codeforge-ship.md` | Mnemos source role — L2 ledger producer + cite client (stub; Sprint 1 expand) |
| `.claude/rpg-engine-spec.md` | Phase 1 architectural decisions (daemon model, write ownership) |
| `.claude/i18n-spec.md` | i18n two-layer design (compile-time UI + runtime content) |

> **Cross-project sync rule**: All CodeForge design decisions — including those discussed in CodePower sessions — land in `doc/specs/*.md`. Spec is the single source of truth; conversation logs are not carried across sessions.

## Phase Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1 | Memory CLI + Common Pet | ✅ done |
| 2a | Daemon framework + IPC socket + tick loop | ✅ done |
| 2b | MOB generation + auto-combat + loot | ✅ done |
| 2c | TUI rendering + Local Map | ✅ done |
| 3a | World Map + Zone unlock | ✅ done |
| 3b | Strategy Mode | ✅ done |
| 3c | AI Commentary (Haiku, 1/hour max, opt-in) | ✅ done |
| 3d | Stickiness: welcome-back, mood decay, zone mastery, milestones | ✅ done |
| 3e | Loot crafting + active item use | ✅ done |
| 3f | `codeforge snapshot` (shareable ASCII monthly report) | ✅ done |
| 4 | Zoa 3D pet animation | planned |
| 5a | Nation Plugin + credential verify (ed25519) | planned |
| 5b | Organizer cross-Nation events | planned |
| 5c | Nation Statusline Theme (tiered unlock) | planned |

## Mnemos Integration Roadmap

Orthogonal to MUD Phase Roadmap above; aligned with Mnemos sprint cadence (`cookys/mnemos:docs/specs/20-sprint-0-2.md`).

| Sprint | Deliverable | Status |
|--------|-------------|--------|
| Mnemos Sprint 0 | Mnemos Rust foundation (no codeforge work) | blocked on Mnemos |
| Mnemos Sprint 1 | `codeforge ship` + `codeforge mnemos-cli cite` end-to-end; SessionEnd hook chain `dream → ship` | ✅ shipped (v0.0.4) — see `doc/specs/codeforge-ship.md` |
| Mnemos Sprint 2 | `mnemos-cli context` for SessionStart hook (cross-source atom recall) | ✅ shipped (v0.0.4, `mnemos-cli context`) |
| Mnemos Sprint 5+ | Replace fulltext_match cite heuristic with Haiku-based detection | backlog |

## CodeForge ↔ Mnemos Interaction

Mnemos (`cookys/mnemos`) is the central brain that ingests from multiple sources (Slack, LINE, Email, Docs, ...). CodeForge is one of those sources — the **coding** source.

### Ship (session-end, daily L2 digest)

`codeforge ship` runs at session end (chained after `dream --quiet` in the SessionEnd hook). It reads:
- `.codeforge/store/concepts/*.md` (L1 compiled knowledge)
- `git log` for the day's commits
- `~/.claude/projects/<slug>/<uuid>.jsonl` for today's session transcripts
- `.codeforge/codeforge.db` for metrics

Then digests via Haiku and POSTs the L2 ledger payload to Mnemos at `http://127.0.0.1:8845/v1/ingest/ledger`. CodeForge spec: [`doc/specs/codeforge-ship.md`](doc/specs/codeforge-ship.md). Mnemos endpoint contract: `cookys/mnemos:docs/specs/10-source-contract.md` §5.1.

### Cite (natural citation, client-side write-back)

⚠️ **NOT YET IMPLEMENTED (auto-cite-on-ship)** — `ship` does NOT call cite; the SessionEnd hook chain does not run cite-detect. Citation write-back currently only happens via the **manual** `codeforge mnemos-cli cite-detect <transcript>` subcommand. *Designed* behavior (BACKLOG B18): when a session references a Mnemos atom (Sprint 1: fulltext_match; Sprint 5+: Haiku), cite-back via `mnemos-cli cite <atom_id>` per atom → Mnemos increments `citation_count` / `last_cited_at` / ranking signal. Mnemos contract: `cookys/mnemos:docs/specs/10-source-contract.md` §11.

### Retry policy

Per Mnemos source-contract §9.1: 1s → 5s → 30s exponential backoff, 4 attempts. Failure writes `~/.codeforge/ship-failed/<ship_id>.json` for next-ship retry. `--no-hook` mode is single-attempt (never blocks SessionEnd).

**Opt-in gate** (`MnemosConfig::opted_in`): `ship --no-hook` only POSTs when Mnemos is explicitly opted-in — `~/.config/mnemos.env` exists OR `MNEMOS_INGEST_URL` is set. With no opt-in it returns early (no POST, no `ship-failed/` write), so codeforge-only users who run the global SessionEnd chain keep distilling via dream without accumulating dead-letter junk. Interactive `codeforge ship` (no `--no-hook`) ignores the gate — it's a deliberate user action. The dream→ship chain lives in the **global** `~/.claude/settings.json` SessionEnd (installed by `codeforge install --hooks`/`--all`), so it runs in every project.

### Critical path note

`codeforge ship` is **production critical path**, not a nice-to-have toy. It runs on every SessionEnd, has explicit retry policy, and its success affects Mnemos data completeness. Treat schema changes / API changes here with the same rigor as core memory pipeline changes.

### Cross-project sync rule (Mnemos edge)

- CodeForge-side code + spec lives here (this repo) — `src/cli/ship.rs`, `src/cli/mnemos_cli.rs`, `doc/specs/codeforge-ship.md`
- Mnemos-side ingest contract + atom schema lives in `cookys/mnemos:docs/specs/*.md` — never duplicated here
- When a design decision spans both (e.g., evidence_refs naming), the **Mnemos repo's `docs/specs/10-source-contract.md` is the single source of truth**; this repo's spec quotes / links it

## CodePower ↔ CodeForge Interaction

CodeForge and CodePower are used together. Understanding the relationship prevents confusion:

### Statusline (global)

`codeforge statusline` is invoked by **all Claude Code sessions** across all projects — not just the CodeForge repo. The statusline hook in `~/.claude/settings.json` (CodePower) calls `codeforge statusline` to display the 5-line pet panel (+ right-column pet art). Changes to the statusline affect every project's session header.

### Dream (session-end, all projects)

`codeforge dream --quiet` runs at session end in **every** project, via the **global** SessionEnd hook (`~/.claude/settings.json`, installed by `codeforge install --hooks`/`--all`). The hook runs with CWD = the project root, so dream distills that project's own `.codeforge` (per-cwd memory). This means memory and pet XP update from activity in CodePower, or any other project — not just the CodeForge codebase. `--quiet` is mandatory for hook use (suppresses all stdout). `codeforge ship --no-hook` chains right after dream in the same SessionEnd group (see Ship above); ship self-gates on Mnemos opt-in, so a codeforge-only user with no Mnemos still distills via dream while ship is a clean no-op.

### Recall (session-start) — READ side, symmetric to the WRITE side

The memory loop is **absorb → distill → store → recall**. READ mirrors WRITE with the same local-always / central-opt-in split:

| | Local (always, no Mnemos) | Central (opt-in, needs Mnemos) |
|---|---|---|
| **WRITE** | `dream` (L0→L1) | `ship` (→ Mnemos) |
| **READ** | `codeforge memory context --hook` (global SessionStart) | `mnemos-cli context` (cross-source) |

`codeforge memory context` ranks active L1 by a unified recall score (importance × recency × citation — strength weighted by a recency half-life and log-damped citation count) → budgets to a **lean index** (~1.5K tokens, never a dump — context-pollution lesson from claude-mem) → emits `hookSpecificOutput.additionalContext` at SessionStart. No-op when the project has no active L1. Each line cites its `topic`; detail is pulled on demand via `codeforge memory search <topic>` (progressive disclosure). Installed in **global settings.json**, not a plugin (plugin `hooks.json` additionalContext is unreliable — Claude Code issue #16538). Shared-state + atom schema contract: [`doc/specs/codeforge-memory-contract.md`](doc/specs/codeforge-memory-contract.md).

### Learn (any project → codeforge memory)

`codeforge learn "..."` can be run from any directory. Where the signal lands depends on `CODEFORGE_DIR`:
- **`CODEFORGE_DIR` not set**: L0 signal written to `$CWD/.codeforge/signals/` — each project has its own memory store. Learnings from a CodePower session land in CodePower's `.codeforge/`, not the CodeForge repo.
- **`CODEFORGE_DIR` set** (e.g. `~/.codeforge/global`): all projects share a single memory store at that path, regardless of CWD. This is the "global memory" pattern documented in `.env.example`.

### CodePower as Phase 2 Test Zone

The CodePower repo is the primary test environment for Phase 2 features:
- The MUD daemon will first be piloted in CodePower sessions
- Phase 2 TUI will render in the CodePower terminal context
- This allows testing with real workload before generalizing

### LLM Backend (Shared, key-optional)

**dream/ship** resolve their LLM via a 3-layer **fallback chain**, not a single key:
`claude -p` headless (Claude Code CLI, no key, Opus default, highest quality) → `ANTHROPIC_API_KEY` (direct Haiku API, per-token billing) → rule-based passthrough (no LLM). See `src/llm.rs` + `src/dream/compile.rs` + `src/cli/ship.rs`. **AI commentary is NOT on this chain** — it's 2-layer only (Haiku API if `ANTHROPIC_API_KEY` set, else rule-based; never `claude -p` — `src/commentary/` doesn't import `crate::llm`). See `doc/concepts.md` for the user-facing explanation.

`ANTHROPIC_API_KEY` is therefore **optional** — only the second fallback. When set, it's read from the `~/.claude/` environment or shell profile (shared with CodePower), not a per-project `.env` (though `dotenvy` will also pick up a local `.env`). `.env.example` lists it under Optional.

### Dev Rule: Keep projects independent

Do NOT import CodePower's codebase into CodeForge tests or vice versa. CodeForge must be self-contained. The interaction is at the **runtime level** (hooks + CLI invocation), not at the code level.

### Hook Path Note

Project `.claude/settings.json` carries **codeforge-clone-only DEV** hooks only: `check-improvements.js` (SessionStart), `check-dev-flow.js` (PreToolUse). The script paths use `${CLAUDE_PROJECT_DIR}/.claude/scripts/...` (Claude Code env var, expanded at hook spawn) — no per-clone hand-editing needed.

The **product-wide** hooks — `emit-session.js`, `session-digest.js`, the SessionStart local-recall injector `codeforge memory context --hook`, and the `codeforge dream --quiet` → `codeforge ship --no-hook` memory-pipeline SessionEnd chain (plus a top-level `cleanupPeriodDays = 3650`) — live in the **global** install (`codeforge install --hooks`/`--all` → `~/.claude/settings.json`). dream/ship were moved here from `--project-hooks` so they run across **all** projects, not just the codeforge clone (`--project-hooks` is `ensure_in_codeforge_repo`-gated to the clone, so it could never cover other projects). This also avoids dual-fire when working in the codeforge clone. Contributors after fresh clone run `codeforge install --all` once for the global hooks; `--project-hooks` adds the clone-only dev hooks.

The two former V2.2 install bugs are **fixed** (this repo): `patch_hooks` now sweeps *all* codeforge-owned hook groups across every hook_type before re-adding the current set (and collapses emptied arrays), so entries relocate between hook_types / scopes without orphaning or duplicating. `is_legacy_codeforge_command` also recognizes pre-marker node hook-script commands (by codeforge scripts path + known basename), so upgrading from an un-markered install sweeps the old versioned copy instead of stacking a duplicate.

## Knowledge Management

`.claude/knowledge/` — record non-obvious issues and solutions.

**Auto-record when:**
- Command fails due to path/config, then fixed
- Rust compile error took 2+ retries
- Architecture decision iterated multiple times
