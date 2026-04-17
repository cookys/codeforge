# CLAUDE.md — CodeForge

Claude Code 工作指南。

## Project Overview

CodeForge 是 Claude Code power-user 的 CLI 工具集：跨 session 記憶管理 + 遊戲化寵物陪伴系統（MUD 引擎）。目標是讓每個 session 都能從上次中斷的地方繼續，並在程式工作中找到遊樂場感。

**Vision**: 把 MUD 搬上來。codebase 是世界地圖，技術債是怪物，pet 是玩家角色。

## Tech Stack

- **Language**: Rust (2021 edition)
- **CLI**: clap 4 (derive)
- **Storage**: rusqlite (bundled SQLite), WAL mode
- **Terminal**: crossterm + termcolor
- **I18n**: rust-i18n v3 (compile-time), `locales/` YAML
- **LLM**: Anthropic API (claude-haiku-4-5 for dream compile + AI commentary)
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
  main.rs           ← clap CLI dispatch
  cli/              ← command handlers (one file per command)
    init.rs         ← codeforge init
    learn.rs        ← codeforge learn
    dream.rs        ← codeforge dream
    search.rs       ← codeforge memory search
    ingest.rs       ← codeforge ingest
    pet.rs          ← codeforge pet
    statusline.rs   ← codeforge statusline (ANSI 6-line panel)
  memory/
    l0.rs           ← raw signals (JSONL file-based)
    l1.rs           ← compiled knowledge (SQLite FTS5)
    fts.rs          ← FTS5 search
  dream/
    compile.rs      ← L0 → L1 via Haiku API
    absorb.rs       ← aggregate recent signals
  brain/
    episode.rs      ← session episode records
  pet/
    state.rs        ← PetState: level, HP, XP, stats
    xp.rs           ← XP award logic
    badges.rs       ← badge system (Phase 2+)
  db/
    mod.rs          ← Context, Connection, PRAGMA setup
    migrations.rs   ← schema migrations (inline SQL)
  import/           ← claude.ai export parser
  power/            ← CharacterStats (ATK/DEF/SUP/VER)
  projection/       ← memory projection to AGENTS.md
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
- **Phase 2**: daemon owns all game state writes; CLI is read-only from `pet_snapshot` table
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
| `.claude/rpg-engine-spec.md` | Phase 1 architectural decisions (daemon model, write ownership) |
| `.claude/i18n-spec.md` | i18n two-layer design (compile-time UI + runtime content) |

> **Cross-project sync rule**: All CodeForge design decisions — including those discussed in CodePower sessions — land in `doc/specs/*.md`. Spec is the single source of truth; conversation logs are not carried across sessions.

## Phase Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1 | Memory CLI + Common Pet | ✅ done |
| 2a | Daemon framework + IPC socket + tick loop | planned |
| 2b | MOB generation + auto-combat + loot | planned |
| 2c | TUI rendering + Local Map | planned |
| 3a | World Map + Zone unlock | planned |
| 3b | Strategy Mode | planned |
| 3c | AI Commentary (Haiku, 1/hour max, opt-in) | planned |
| 3d | Stickiness: welcome-back, mood decay, zone mastery, milestones | planned |
| 3e | Loot crafting + active item use | planned |
| 3f | `codeforge snapshot` (shareable ASCII monthly report) | planned |
| 4 | Zoa 3D pet animation | planned |
| 5a | Nation Plugin + credential verify (ed25519) | planned |
| 5b | Organizer cross-Nation events | planned |
| 5c | Nation Statusline Theme (tiered unlock) | planned |

## CodePower ↔ CodeForge Interaction

CodeForge and CodePower are used together. Understanding the relationship prevents confusion:

### Statusline (global)

`codeforge statusline` is invoked by **all Claude Code sessions** across all projects — not just the CodeForge repo. The statusline hook in `~/.claude/settings.json` (CodePower) calls `codeforge statusline` to display the 6-line pet panel. Changes to the statusline affect every project's session header.

### Dream (session-end, all projects)

`codeforge dream --quiet` runs at session end in **any** project that has the SessionEnd hook configured. This means memory and pet XP are updated from activity in CodePower, or any other project — not just the CodeForge codebase. The `--quiet` flag is mandatory for hook use (suppresses all stdout).

### Learn (any project → codeforge memory)

`codeforge learn "..."` can be run from any directory. Where the signal lands depends on `CODEFORGE_DIR`:
- **`CODEFORGE_DIR` not set**: L0 signal written to `$CWD/.codeforge/signals/` — each project has its own memory store. Learnings from a CodePower session land in CodePower's `.codeforge/`, not the CodeForge repo.
- **`CODEFORGE_DIR` set** (e.g. `~/.codeforge/global`): all projects share a single memory store at that path, regardless of CWD. This is the "global memory" pattern documented in `.env.example`.

### CodePower as Phase 2 Test Zone

The CodePower repo is the primary test environment for Phase 2 features:
- The MUD daemon will first be piloted in CodePower sessions
- Phase 2 TUI will render in the CodePower terminal context
- This allows testing with real workload before generalizing

### Shared API Key

Both projects use the same `ANTHROPIC_API_KEY`. The key is set in `~/.claude/` environment or shell profile — not per-project `.env`. The `.env.example` in CodeForge documents this pattern.

### Dev Rule: Keep projects independent

Do NOT import CodePower's codebase into CodeForge tests or vice versa. CodeForge must be self-contained. The interaction is at the **runtime level** (hooks + CLI invocation), not at the code level.

### Hook Path Note

`.claude/settings.json` contains absolute paths to the session scripts (e.g. `/home/codepower/projects/codeforge/.claude/scripts/check-improvements.js`). These must be updated if the repo is moved or cloned to a different machine. Claude Code hooks do not support relative paths.

## Knowledge Management

`.claude/knowledge/` — record non-obvious issues and solutions.

**Auto-record when:**
- Command fails due to path/config, then fixed
- Rust compile error took 2+ retries
- Architecture decision iterated multiple times
