# CodeForge

> Memory + gamified pet CLI for Claude Code power-users (Rust).

CodeForge gives Claude Code sessions persistent cross-session memory and a gamified ASCII pet that levels up as you code. The codebase becomes a world map; the pet becomes your character; technical debt becomes monsters.

## Status

Active development. Phases 1, 2 (a/b/c) and 3 (a/b/c/d/e/f) shipped. Phase 4 (Zoa 3D animation) and Phase 5 (Nation P2P) on the roadmap.

CodeForge is also the first **Mnemos source** — Sprint 1 of the [Mnemos](https://github.com/cookys/mnemos) Gen-2 active memory daemon adds `codeforge ship` (daily L2 ledger producer) and `codeforge mnemos-cli cite` (natural citation write-back). See [Mnemos Integration](#mnemos-integration) below.

Full architecture in [`doc/specs/codeforge-mud-engine.md`](doc/specs/codeforge-mud-engine.md).

## Lore — Why CodeForge

CodeForge is the personal smithy of the CodePower ecosystem.

**CodePower** is the arena — public, federated, competitive; where Nations battle and code becomes score.
**CodeForge** is your forge — private, contemplative, accumulative; where raw signals become knowledge, where pets are raised, where your codebase becomes an explorable map.

Power generates, Forge shapes. Smith alone, or carry your forged self into the arena.

## Install

For a step-by-step walkthrough, see [`doc/getting-started.md`](doc/getting-started.md).

**Option 1 — curl installer (no Rust required)** *(available from v0.0.2 onward; see [`doc/specs/codeforge-release-pipeline.md`](doc/specs/codeforge-release-pipeline.md))*

```bash
curl -sSL https://raw.githubusercontent.com/cookys/codeforge/main/install.sh | sh
```

Downloads the prebuilt binary for Linux/macOS (x86_64 or arm64) and
installs to `~/.cargo/bin` or `~/.local/bin`. Respects
`CODEFORGE_VERSION`, `CODEFORGE_INSTALL_DIR`, `CODEFORGE_FORCE` env vars.

**Option 2 — `cargo binstall` (Rust users, no compile)**

```bash
cargo binstall codeforge
```

**Option 3 — `cargo install` (developers / from source)**

```bash
cargo install --git https://github.com/cookys/codeforge
# or, for a local clone:
git clone https://github.com/cookys/codeforge && cd codeforge && cargo install --path .
```

Requires Rust stable (MSRV 1.85). The built binary lands in
`~/.cargo/bin/codeforge`.

**Then wire it into Claude Code:**

```bash
codeforge install
```

This patches `~/.claude/settings.json` to use `codeforge statusline` as
Claude Code's statusLine hook — with the binary's absolute path, so it
works even when `~/.cargo/bin` isn't on the spawned shell's PATH (common
when `rustup` was installed with `--no-modify-path`). Re-run after
upgrading codeforge to refresh the path. Other keys in settings.json are
preserved.

`codeforge install` has more granular flags too (all shipped) — see
[`doc/specs/codeforge-install-subcommand.md`](doc/specs/codeforge-install-subcommand.md)
and the "First-time Claude Code hook setup" section below:

- `codeforge install --hooks` — global product-wide hooks only (SessionStart recall + SessionEnd memory pipeline + PreCompact digest)
- `codeforge install --all` — statusline + global hooks
- `codeforge install --project-hooks` — codeforge-clone-only dev hooks (SessionStart + PreToolUse)
- `--dry-run` previews the settings.json changes without writing.

**macOS first-run note:** if Gatekeeper blocks the binary, run once:

```bash
xattr -d com.apple.quarantine "$(command -v codeforge)"
```

(Code-signing/notarization deferred until v1.0.)

## Uninstall

`codeforge uninstall` reverses the settings.json patches (both the
statusLine and the hooks blocks; `--statusline` / `--hooks` to scope it,
`--quiet` for hook use). Then remove the binary and data:

```bash
codeforge uninstall          # un-patches ~/.claude/settings.json
rm "$(command -v codeforge)"
rm -rf ~/.codeforge ~/.local/share/codeforge
```

## Quickstart

```bash
# initialize a project store
cd ~/projects/<your-repo>
codeforge init

# log a learning (L0 raw signal)
codeforge learn "tokio::select! preserves cancellation across branches"

# compile L0 → L1 (uses the LLM backend chain: claude -p → Haiku → rule-based)
# no API key needed if the `claude` CLI is on your PATH; see Data & Privacy
codeforge dream

# search compiled knowledge
codeforge memory search "tokio cancellation"

# pet status
codeforge pet

# 5-line statusline panel + right-column pet art (for the Claude Code statusline hook)
codeforge statusline
```

More commands: `codeforge memory search|status|context`, `tui` / `attach` (full TUI + Local Map), `daemon start|stop|status|install` (background MUD engine; `install` writes a systemd user unit), `strategy` (combat mode), `world` (zone map), `craft` / `inventory` / `use` (loot), `snapshot` (monthly card), `commentary on|off|list|test` (AI pet commentary), `dream --only <op>` (single dream op), `emit <event>` (push an event into the daemon inbox; used by hooks), `ship` / `mnemos-cli cite|cite-detect|context` (Mnemos integration). Run `codeforge --help` for the full tree.

For the global memory store pattern (one shared store across projects), see [`.env.example`](.env.example) — set `CODEFORGE_DIR=~/.codeforge/global`.

**New here?** [`doc/concepts.md`](doc/concepts.md) explains how CodeForge actually works — solo vs. brain-connected operation, the `dream`/`ship` memory pipeline, and the pet system — in plain terms.

## First-time Claude Code hook setup

CodeForge hooks come in two layers, each scoped to a different responsibility:

**Layer 1 — Codeforge-clone-only hooks** (committed in this repo at `.claude/settings.json`):

- `check-improvements.js` (SessionStart) — surfaces unprocessed digests + improvement queue
- `check-dev-flow.js` (PreToolUse) — enforces dev-flow before code touches

These use `${CLAUDE_PROJECT_DIR}` (Claude Code's documented env var for the project root, expanded before each hook spawn), so the committed file works in every clone without modification.

If you need to regenerate it (e.g., after a settings.json corruption), run:

```bash
codeforge install --project-hooks --force --yes
```

**Layer 2 — Product-wide hooks** (installed globally to `~/.claude/settings.json`):

- `emit-session.js` (SessionStart + SessionEnd) — session boundary signals
- `session-digest.js` (PreCompact + SessionEnd) — captures errors/corrections to per-repo `.codeforge/digests/`
- `codeforge memory context --hook` (SessionStart) — injects a lean ranked L1 index as additionalContext (local recall; no-op when the project has no active L1)
- `codeforge dream --quiet` → `codeforge ship --no-hook` (SessionEnd) — the memory pipeline: distill L0 → L1 in every project, then ship to Mnemos if opted in (clean no-op otherwise). See [`doc/concepts.md`](doc/concepts.md).

Across SessionStart / SessionEnd / PreCompact (3 hook types).

These fire in **every** Claude Code session (any project), not just inside the codeforge clone. Install once after first build:

```bash
codeforge install --all
```

Idempotent. Re-run after upgrading codeforge to refresh the path.

**Why split**: keeping Layer 2 globally avoids the dual-fire problem — if both project + global settings carry the same script entries, hooks run twice per event. Each layer owns a distinct responsibility, single-source-of-truth.

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

## Mnemos Integration

Beyond the local Memory pipeline + MUD engine, CodeForge is also a **Mnemos source** — it produces a daily L2 ledger from each coding session and ships it to [Mnemos](https://github.com/cookys/mnemos), the central active-memory daemon (Gen-2 successor to PKB).

Two subcommands handle the integration (Sprint 1 deliverable, see [`doc/specs/codeforge-ship.md`](doc/specs/codeforge-ship.md)):

- `codeforge ship` — at SessionEnd, digest the day's L1 + git log into an L2 ledger and POST to Mnemos. Retry policy + failure queue built in.
- `codeforge mnemos-cli cite <atom_id>` — when a session referenced a Mnemos atom, write back the citation so Mnemos can rank by usage.

Endpoint defaults to `http://127.0.0.1:8845/v1/ingest/ledger` (configurable via `~/.config/mnemos.env`). The ledger payload schema is the joint contract — defined in `cookys/mnemos:docs/specs/10-source-contract.md` §5.1.

This makes CodeForge the **coding source** for Mnemos's multi-source brain (alongside Slack, LINE, Email, Docs, Photos). It is **production critical path**, not a toy subcommand.

| Sprint | CodeForge deliverable | Status |
|--------|------------------------|--------|
| Mnemos Sprint 1 | `ship` + `mnemos-cli cite` end-to-end | spec stub; expand at sprint launch |
| Mnemos Sprint 2 | `mnemos-cli context` for SessionStart hook | shipped (`mnemos-cli context`, v0.0.4) |
| Mnemos Sprint 5+ | Replace fulltext_match cite heuristic with Haiku detection | backlog |

## Data & Privacy

- All data stays on your machine in `.codeforge/` (per-project) or `$CODEFORGE_DIR` (global).
- `dream`/`ship` compile your signals via an LLM. The backend is a fallback chain: `claude -p` (the Claude Code CLI, default, no key) → `ANTHROPIC_API_KEY` (direct Haiku API, only if set) → a local rule-based pass (no LLM). The first two routes send the text to Anthropic's models and are subject to [Anthropic's privacy policy](https://www.anthropic.com/privacy); the rule-based fallback sends nothing off-machine. See [`doc/concepts.md`](doc/concepts.md) for the full flow.
- When `codeforge ship` runs (Mnemos Sprint 1+), the L2 ledger is POSTed to your **local** Mnemos daemon at `127.0.0.1:8845` — same machine, no external upload. Mnemos itself stores data locally in `~/.mnemos/data/`.
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
