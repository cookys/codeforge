# CodeForge

> Memory + gamified pet CLI for Claude Code power-users (Rust).

CodeForge gives Claude Code sessions persistent cross-session memory and a gamified ASCII pet that levels up as you code. The codebase becomes a world map; the pet becomes your character; technical debt becomes monsters.

## Status

Active development. Phases 1, 2 (a/b/c) and 3 (a/b/c/d/e/f) shipped. Phase 4 (Zoa 3D animation) and Phase 5 (Nation P2P) on the roadmap.

Full architecture in [`doc/specs/codeforge-mud-engine.md`](doc/specs/codeforge-mud-engine.md).

## Lore — Why CodeForge

CodeForge is the personal smithy of the CodePower ecosystem.

**CodePower** is the arena — public, federated, competitive; where Nations battle and code becomes score.
**CodeForge** is your forge — private, contemplative, accumulative; where raw signals become knowledge, where pets are raised, where your codebase becomes an explorable map.

Power generates, Forge shapes. Smith alone, or carry your forged self into the arena.

## Install

```bash
git clone https://github.com/cookys/codeforge
cd codeforge
cargo install --path .
```

Requires Rust stable. The built binary lands in `~/.cargo/bin/codeforge`.

## Quickstart

```bash
# initialize a project store
cd ~/projects/<your-repo>
codeforge init

# log a learning (L0 raw signal)
codeforge learn "tokio::select! preserves cancellation across branches"

# compile L0 → L1 (uses Anthropic Haiku)
export ANTHROPIC_API_KEY=sk-ant-...
codeforge dream

# search compiled knowledge
codeforge memory search "tokio cancellation"

# pet status
codeforge pet

# 6-line statusline panel (designed for Claude Code statusline hook)
codeforge statusline
```

For the global memory store pattern (one shared store across projects), see [`.env.example`](.env.example) — set `CODEFORGE_DIR=~/.codeforge/global`.

## First-time Claude Code hook setup

CodeForge ships a `.claude/settings.json` that wires SessionStart / PreToolUse / SessionEnd hooks. The hook commands use absolute paths (Claude Code does not yet support relative paths in hook configuration). After cloning, edit `.claude/settings.json` and replace **every occurrence** of the example prefix `/home/codepower/projects/codeforge/` with the absolute path to your local clone.

The two `.claude/scripts/check-*.js` files derive `PROJECT_ROOT` from `__filename` and work without modification.

A first-run setup script that rewrites `settings.json` paths is tracked in BACKLOG.

## Phase Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1 | Memory CLI + Common Pet | shipped |
| 2a | Daemon framework + IPC + tick loop | shipped |
| 2b | MOB + auto-combat + loot | shipped |
| 2c | TUI + Local Map | shipped |
| 3a | World Map + Zone unlock | shipped |
| 3b | Strategy Mode (Aggressive / Defensive / Explorer / Scholar) | shipped |
| 3c | AI Commentary (Haiku, ≤1/hour, opt-in) | shipped |
| 3d | Stickiness mechanics (welcome-back / mood decay / next unlock) | shipped |
| 3e | Loot crafting + active items | shipped |
| 3f | `codeforge snapshot` ASCII monthly card | shipped |
| 4  | Zoa 3D pet animation | planned |
| 5  | Nation P2P (cross-Nation events, ed25519 credentials) | planned |

Design specs for upcoming phases live in [`doc/specs/codeforge-mud-engine.md`](doc/specs/codeforge-mud-engine.md) and [`doc/specs/nation-p2p-design.md`](doc/specs/nation-p2p-design.md).

## Data & Privacy

- All data stays on your machine in `.codeforge/` (per-project) or `$CODEFORGE_DIR` (global).
- When `ANTHROPIC_API_KEY` is set, your raw signals (the text you pass to `codeforge learn`) are sent to Anthropic's `claude-haiku-4-5` model for L0 → L1 compilation. This is subject to [Anthropic's privacy policy](https://www.anthropic.com/privacy).
- The project author collects no telemetry and runs no servers.
- `.codeforge/codeforge.db` (SQLite) and `.codeforge/signals/*.jsonl` contain your raw inputs and compiled knowledge — both are gitignored by default.

## Tech Stack

Rust 2021, clap 4, rusqlite (bundled SQLite, WAL), tokio, hecs ECS, crossterm TUI, rust-i18n, reqwest. Anthropic API for `dream compile` and AI commentary.

## Disclaimer

CodeForge is an independent open-source project and is not affiliated with or endorsed by Anthropic, PBC. "Claude" and "Claude Code" are trademarks of Anthropic, used here in nominative fair use to describe the integration target.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Project conventions documented in [CLAUDE.md](CLAUDE.md). Bug reports and feature ideas welcome via GitHub Issues.

## License

[Apache License 2.0](LICENSE) © 2026 CodeForge contributors.
