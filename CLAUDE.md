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
| `doc/specs/codeforge-mud-engine.md` | Phase 2 MUD engine design — daemon, combat, TUI |
| `.claude/rpg-engine-spec.md` | Phase 1 architectural decisions (daemon model, write ownership) |
| `.claude/i18n-spec.md` | i18n two-layer design (compile-time UI + runtime content) |

## Phase Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1 | Memory CLI + Common Pet | ✅ done |
| 2a | Daemon framework + IPC socket + tick loop | planned |
| 2b | MOB generation + auto-combat + loot | planned |
| 2c | TUI rendering + Local Map | planned |
| 3a | World Map + Zone unlock | planned |
| 3b | Strategy Mode | planned |
| 3c | AI Commentary (Haiku) | planned |
| 4 | Zoa 3D pet animation | planned |

## Knowledge Management

`.claude/knowledge/` — record non-obvious issues and solutions.

**Auto-record when:**
- Command fails due to path/config, then fixed
- Rust compile error took 2+ retries
- Architecture decision iterated multiple times
