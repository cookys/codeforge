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

Requires Rust stable (MSRV 1.88). The built binary lands in
`~/.cargo/bin/codeforge`.

**Windows:** build from source (Options 1–2 have no Windows binary yet). Needs the MSVC toolchain
and VS Build Tools — full walkthrough in [`doc/windows.md`](doc/windows.md).

**Updating** — once installed (by any option above), update in place from the
latest GitHub release without re-running the installer or juggling binary paths:

```bash
codeforge self-update          # pull the latest release, replace this binary in place
codeforge self-update --check  # just report whether a newer version exists
```

It rewrites whichever binary you invoked (`~/.local/bin`, `~/.cargo/bin`, …), so
it sidesteps the "which copy is on PATH" problem. (`cargo install` users can also
just re-run `cargo install`.)

**Then wire it into Claude Code:**

```bash
codeforge install
```

This patches `~/.claude/settings.json` to use `codeforge statusline` as
Claude Code's statusLine hook. When `codeforge` on your PATH is this same
binary the command is just `codeforge statusline`; otherwise it uses the
binary's absolute path, so it still works when `~/.cargo/bin` isn't on the
spawned shell's PATH (common when `rustup` was installed with
`--no-modify-path`). Other keys in settings.json are preserved.

**Shared settings.json (dotfiles).** If `~/.claude/settings.json` is a symlink
(e.g. into a dotfiles repo) or the file is tracked by git, install says so and
writes *through* the link into the real file, so the link stays a link. What it
writes is the same on every machine and every version: hook commands are
`codeforge hook <name>` / `codeforge <subcommand>` with no paths or version
numbers, the statusLine is always `codeforge statusline` (so `codeforge` must be
on PATH there), and the entries carry the fixed marker `codeforge@1`.
`cleanupPeriodDays` is not written into a shared file unless you pass
`--cleanup-period-days <DAYS>`. Detection covers symlinks and git-tracked files;
a copy-based setup (chezmoi copy mode, Syncthing) is not detected, so pass the
flag or check the result yourself there. **Upgrade order for a shared file:**
update every machine (`codeforge self-update`) to the same version first, then
run `codeforge bootstrap` on one machine and commit/sync the rewritten
settings.json — a machine still on an older codeforge does not have the
`codeforge hook` subcommand.

`codeforge install` has more granular flags too (all shipped) — see
[`doc/specs/codeforge-install-subcommand.md`](doc/specs/codeforge-install-subcommand.md)
and the "First-time Claude Code hook setup" section below:

- `codeforge install --hooks` — global product-wide hooks only (SessionStart recall + SessionEnd memory pipeline + PreCompact digest)
- `codeforge install --all` — statusline + global hooks
- `codeforge install --project-hooks` — codeforge-clone-only dev hooks (SessionStart + PreToolUse)
- `--dry-run` previews the settings.json changes without writing.
- `--cleanup-period-days <DAYS>` sets `cleanupPeriodDays` (also on `bootstrap`); needed for a shared settings.json, see above.
- `--replace-statusline` takes over a `statusLine` / `subagentStatusLine` that another tool set, and touches nothing else. (`--force` also clears every hook entry, codeforge's or not; don't use it just to replace the statusLine.)

**Setting up a new machine?** Use the one-command bootstrap instead of running
the steps by hand:

```bash
codeforge bootstrap            # idempotent; --dry-run to preview, --quiet for hooks
```

It runs `install --all`, makes sure the pinned rustfmt toolchain is ready (only
when run inside a codeforge clone — see *Formatting* below), and reports your
Mnemos opt-in status. It's best-effort: a step that can't proceed (e.g. a
statusLine already set by something else) is reported and skipped, not fatal —
the rest still run. Re-run it any time; on each machine's clone do
`git pull && codeforge bootstrap`.

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

# log a learning (L0 raw signal) — also: --paste (clipboard), --file <FILE>, or stdin
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

More commands: `codeforge memory search|status|context`, `tui` / `attach [--size PCT]` (full TUI + Local Map; `--size` sets companion-pane width %), `daemon start|stop|status|install` (background MUD engine; `install` writes a systemd user unit), `strategy` (combat mode), `world [--refresh]` (zone map; `--refresh` rescans L1), `ingest <path> [--source claude|chatgpt|markdown|auto]` (import external history), `craft` / `inventory` / `use` (loot), `snapshot [--days N]` (ASCII activity card; `--days` sets the lookback window), `commentary on|off|list [-n N]|test [kind]` (AI pet commentary), `dream --only <op>` (single dream op), `emit <event> [-f KEY=VALUE]... [--json <JSON>]` (push an event into the daemon inbox; used by hooks), `ship [--date YYYY-MM-DD] [--dry-run] [--no-hook] [--resend]` / `mnemos-cli cite [--matched-text]|cite-detect <transcript> [--topic|--max]|context [--topic|--max|--max-sensitivity|--with-themes]|probe [--verbose]` (Mnemos integration), `doctor` (brain health diagnostics). Run `codeforge --help` for the full tree.

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

- `codeforge hook emit-session` (SessionStart + SessionEnd) — session boundary signals
- `codeforge hook session-digest` (PreCompact + SessionEnd) — captures errors/corrections to per-repo `.codeforge/digests/`

`codeforge hook <name> [args…]` runs a hook script embedded in the binary (copied to a private per-version cache under the codeforge data dir, honouring `CODEFORGE_DATA_DIR`) with `node`, passing stdin and the script's exit code through. Its own failures exit 1, never 2 (Claude Code treats 2 as blocking). The scripts update with the binary, so `self-update` needs no reinstall.
- `codeforge memory context --hook` (SessionStart) — injects a lean ranked L1 index as additionalContext (local recall; no-op when the project has no active L1)
- `codeforge dream --quiet` → `codeforge ship --no-hook` (SessionEnd) — the memory pipeline: distill L0 → L1 in every project, then ship to Mnemos if opted in (clean no-op otherwise). See [`doc/concepts.md`](doc/concepts.md).
- `codeforge usage collect --quiet` (SessionEnd, after ship) — incremental usage collect: numbers only from the session transcripts, then `summary.json`. See [Usage](#usage).

Across SessionStart / SessionEnd / PreCompact (3 hook types). On a local (non-shared) settings.json, `--hooks`/`--all` also set the top-level `cleanupPeriodDays = 3650` (unless you already set a non-null value) so Claude Code doesn't reclaim session transcripts before dream/ship distil them; on a shared one only `--cleanup-period-days <DAYS>` writes it.

These fire in **every** Claude Code session (any project), not just inside the codeforge clone. Install once after first build:

```bash
codeforge install --all
```

Idempotent: re-running it writes byte-identical entries. Installs from v0.0.6 and earlier wrote versioned `node …/hooks/<version>/….js` commands; re-run `codeforge install --all` (or `bootstrap`) once after upgrading to replace them.

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
| Mnemos Sprint 1 | `ship` + `mnemos-cli cite` end-to-end | shipped (v0.0.4) |
| Mnemos Sprint 2 | `mnemos-cli context` for SessionStart hook | shipped (`mnemos-cli context`, v0.0.4) |
| Brain indicators | Statusline dual brain lights + `doctor` + `mnemos-cli probe` | shipped (unreleased) |
| Mnemos Sprint 5+ | Replace fulltext_match cite heuristic with Haiku detection | backlog |

### Brain Connection Indicators

The `codeforge statusline` bottom border carries two connection lights:

- **`memory ●/◌`** (local brain, always shown when the project has history):
  - `●` green — active L1 concepts > 0
  - `◌` grey — store directory exists but active count is 0 ("where did my memory go?" is a useful signal)
  - not shown — brand-new project, never dreamed

- **`mnemos ●/◐/○/◌`** (central brain, only shown when opted-in):
  - `●` green — liveness probe ok, ingest healthy
  - `◐` yellow `degraded` — server is up but last ingest failed or queue depth is high
  - `○` grey `offline` — server not reachable (neutral, not an alarm — just go start the server)
  - `◌` grey `pending` — opted-in but never connected yet

The liveness probe runs as a detached background process (`process_group` isolated), writing to a per-machine cache (`$XDG_RUNTIME_DIR/codeforge/`). The hot path (every ~5 s in statusline) only reads the cache — it never blocks on network I/O. The second signal (`mnemos-ship.json`) comes from real ingest outcomes piggybacked on `codeforge ship`.

**NO_COLOR / colour-blind terminals:** glyphs fall back to `memory:active`, `mnemos:ok`, etc. (full words, no abbreviated codes).

**Diagnostics:** `codeforge doctor` shows all dimensions — local L1 count, opt-in status, live probe result, last ship time/outcome, queue depth, and next-step recommendations. `codeforge mnemos-cli probe [--verbose]` probes `GET /health` directly.

## Live context files

`codeforge statusline` and the opt-in `codeforge subagent-statusline` each write a small JSON
snapshot to a RAM-backed directory after every render, so other tools can poll session state
without touching the transcript. One writer per file, no merge — no writer reads another's file.
The statusline also writes a pet view snapshot (below).

**Base-dir resolution** (fixed order, first accepted candidate wins):

1. `$AUTOPILOT_LIVE_DIR` (override knob)
2. `$XDG_RUNTIME_DIR/autopilot`
3. `/dev/shm/autopilot-<uid>`
4. `/tmp/autopilot-<uid>`

Every candidate — including the override — is accepted only if it resolves to a `tmpfs`/`ramfs`
mount (checked via `findmnt -T <dir> -o FSTYPE -n`; falls back to a longest-prefix match over
`/proc/mounts` when `findmnt` is absent). A rejected candidate is skipped, not fatal. If every
candidate is rejected, the base falls back to `~/.autopilot` (disk-backed) and prints exactly one
warning line to stderr per process.

**Files** (under `<base>/context/`, mode 0600, atomic same-dir temp+rename):

- `<sid>.json` — main status line record:
  ```json
  {
    "schema_version": 1,
    "session_id": "<raw session_id>",
    "written_at": "<RFC3339 UTC>",
    "cc_version": "<version>",
    "model": {"id": "...", "display_name": "..."},
    "context_window": {
      "context_window_size": 1000000,
      "used_percentage": 26,
      "total_input_tokens": 262707,
      "current_usage": {"input_tokens": 32, "output_tokens": 4, "...": "..."}
    }
  }
  ```
- `<sid>.tasks.json` — subagent status line record (written by `codeforge subagent-statusline`):
  ```json
  {
    "schema_version": 1,
    "session_id": "<raw session_id>",
    "written_at": "<RFC3339 UTC>",
    "tasks": [
      {"id": "...", "type": "...", "status": "...", "description": "...", "label": "...",
       "startTime": 0, "model": "...", "cwd": "...", "contextWindowSize": 0, "tokenCount": 0}
    ]
  }
  ```
  Task fields (`name` included, when present) are copied only when present in the input — never
  invented.

`<sid>` is the raw stdin `session_id`, sanitized: every Unicode scalar not in `[A-Za-z0-9_-]` is
replaced with `_`, then the result is truncated to the first 64 scalars; empty input becomes
`unknown`.

**Pet view** (under `<base>/codeforge/`, same mode and atomic write): `pet.json` is a
display-ready copy of the pet for UI readers such as a Claude Code mod pane, so they can draw it
without running `codeforge` or needing a `.codeforge/` in their cwd. `codeforge statusline` rewrites
it after every render (any project) and `codeforge adopt` writes it as soon as a pet is created.
Schema `codeforge.pet-view/1` (full shape in `src/pet/view.rs`):

```json
{
  "schema": "codeforge.pet-view/1",
  "written_at": "<RFC3339 UTC>",
  "pet": {"name": "Ferris", "level": 1, "xp": 0, "xp_to_next": 100,
          "hp": 16, "atk": 19, "def": 11, "sup": 18, "ver": 13,
          "mood": null, "strategy": "explorer", "message": null},
  "village": {"id": "rust", "display_name": "The Forge-Ruins", "language": "Rust",
              "tagline": "...", "rgb": [215, 95, 0],
              "art_small": ["..."], "art_full": ["..."],
              "sprite": {"width": 24, "height": 24, "transparent": ".",
                         "palette": {"o": [215, 95, 0], "...": "..."},
                         "frames": [["24 rows of 24 palette chars"], ["..."]],
                         "frame_ms": 500},
              "sprite_mini": {"width": 16, "height": 12, "...": "same fields"}}
}
```

`pet` and `village` are both `null` until a pet is adopted. `village.sprite` is always present
for an adopted pet: a pixel sprite for UI readers that draw terminal half blocks (one cell = two
vertical pixels, so `height` is even; colours stay inside the xterm-256 cube), or `null` when the
village has no sprite yet (only Rust/Ferris has one) — readers then fall back to the ASCII art.
`village.sprite_mini` is the same contract at a fixed 16×12 (16 columns × 6 rows), for a band with 6 to
11 rows; `null` likewise.

**Session view** (`<base>/codeforge/session/<session_id>.json`, written by every statusline run, same mode and atomic
write): the lights for this session only — `pet.json` is shared by every session, so per-project state stays out of it.
Schema `codeforge.session-view/1`: `{session_id, written_at, memory: {light: active|empty|hidden, active}, brain: {light:
ok|degraded|offline|pending|hidden}}`.

**Compact statusline**: forge-stats writes a heartbeat `<base>/codeforge/mod-alive/<session_id>.json`
(`codeforge.mod-alive/1`: `session_id`, `written_at_ms`, `layout`). When the heartbeat for the statusline's own
`session_id` is under 10 seconds old and says `layout: band`, `codeforge statusline` prints a 2-line strip (identity +
usage) instead of the pet panel: the pet lives in the band (autopilot's forge tab), the statusline keeps the slow data.
With no such heartbeat the output is unchanged. Set
`statusLine.refreshInterval` (e.g. 2) so the strip follows within seconds; without it Claude Code runs the statusline only
on UI events.

The repo ships one reader: **forge-stats**, a Claude Code plugin in `plugins/forge-stats/`, listed by the repo's own
marketplace (`.claude-plugin/marketplace.json`). Install it once and every session shows it, in any project:

```bash
claude plugin marketplace add /path/to/codeforge   # this clone
claude plugin install forge-stats@codeforge
# undo: claude plugin uninstall forge-stats@codeforge
```

It draws the pet — Ferris as an animated half-block pixel sprite in the terminal — with the memory and brain lights. Model
speed (tok/s) is not forge-stats': autopilot's live mod measures and draws it. It has two layouts, kept across sessions: `/forge-stats layout band` (the default) draws a band
right above the prompt and `/forge-stats layout pane` opens a side pane instead (`/forge-stats` alone opens the
pane in either layout). The band shows as many columns as the width allows, in this order: Ferris, pet (level, XP,
stats), memory and brain — 2 columns from 56 cells, 3 from 86 (Ferris alone below 56; under 26 cells the band is a
one-line summary). Its size follows the rows it is given: 12 or more draw the full band, 6 to 11 the 16×6 mini Ferris,
1 to 5 (or under 26 columns) a one-line summary. With autopilot's live mod in the session the summary leads with the fleet:
`fleet ▲1 ✗1 ⊘2 ●4 ⧗11 · Ferris Lv.1` (needs you, failed, stalled, running, waiting, read from autopilot's
`autopilot.fleet` state; without autopilot, the pet alone). autopilot keeps the band's first row for its tab bar and
hands the second row to forge-stats while no tab is open; the heartbeat's `summary_row: true` says this forge-stats
draws it. With 7 or more rows (mini) or 13 or more (full) the fleet line sits above the columns. When nothing fits, the
row is passed on, never left blank. Layout rules: [`doc/specs/tui-guideline.md`](doc/specs/tui-guideline.md). Only one plugin draws the band slot: one that sits above
forge-stats and draws its own band without passing the slot on hides it. The heartbeat does not see that, so the statusline
still compacts in band layout; autopilot's tab bar passes the slot on, so it is not affected. Test it with `claude plugin test plugins/forge-stats`.

A write failure never changes statusline rendering or exit code — it's logged to stderr once per
process and otherwise swallowed.

**Opt-in install:** `codeforge install --subagent-statusline` (only takes effect alongside
statusLine install, i.e. without `--hooks`/`--project-hooks`, or with `--all`) idempotently adds a
`subagentStatusLine` entry to `settings.json` pointing at `<binary> subagent-statusline`. Default
`install`/`install --all` behaviour is unchanged — this is opt-in only.

## Usage

`codeforge usage` answers "how much did I use, on which account, how close to the limit" on this machine
(spec: [`doc/specs/codeforge-usage.md`](doc/specs/codeforge-usage.md)):

```bash
codeforge usage                         # collect, then the last 7 days by model
codeforge usage --days 3 --by project   # --by model | project | account; --json
codeforge usage push                    # validate summary.json and `fleet upload` it; prints the blob sha
```

- The statusline usage row shows this session's 5h / 7d / ctx bars, then every other account seen in the
  last 6 h as `alias NN% countdown`. Each statusline run writes its account's quota snapshot; the alias is
  `AUTOPILOT_ACCOUNT` when set (autopilot's account-pinned dispatch), else a hash of the config dir.
- The collector reads only the numeric usage fields of `~/.claude/projects/**` (and `CLAUDE_CONFIG_DIR`) and
  Codex `~/.codex/sessions/**` transcripts, incrementally, into `<data_dir>/usage/usage.db` (0600). No
  prompt, path or email is stored; a project is its directory's last name.
- Cost is an **API-equivalent estimate** at list price (prices as of 2026-10-06); subscription plans are not
  billed this way, and models without a price row (all Codex models) show `—`.
- `summary.json` (per-day × model totals and per-account quota, no projects or paths) is the only thing
  `usage push` sends.

## Data & Privacy

- All data stays on your machine in `.codeforge/` (per-project) or `$CODEFORGE_DIR` (global).
- `dream`/`ship` compile your signals via an LLM. The backend is a fallback chain: `claude -p` (the Claude Code CLI, default, no key) → `ANTHROPIC_API_KEY` (direct Haiku API, only if set) → a local rule-based pass (no LLM). The first two routes send the text to Anthropic's models and are subject to [Anthropic's privacy policy](https://www.anthropic.com/privacy); the rule-based fallback sends nothing off-machine. See [`doc/concepts.md`](doc/concepts.md) for the full flow.
- When `codeforge ship` runs (Mnemos Sprint 1+), the L2 ledger is POSTed to your **local** Mnemos daemon at `127.0.0.1:8845` — same machine, no external upload. Mnemos itself stores data locally in `~/.mnemos/data/`.
- The project author collects no telemetry and runs no servers.
- `.codeforge/codeforge.db` (SQLite) and `.codeforge/signals/*.jsonl` contain your raw inputs and compiled knowledge — both are gitignored by default.

## Tech Stack

Rust 2021, clap 4, rusqlite (bundled SQLite, WAL), tokio, hecs ECS, crossterm TUI, rust-i18n, reqwest. `dream compile` / `ship` use the 3-layer LLM chain `claude -p` (Claude Code CLI, default) → Anthropic Haiku API (fallback) → rule-based. AI commentary uses a 2-layer path only — Anthropic Haiku API when `ANTHROPIC_API_KEY` is set, else rule-based (it never calls `claude -p`).

## Disclaimer

CodeForge is an independent open-source project and is not affiliated with or endorsed by Anthropic, PBC. "Claude" and "Claude Code" are trademarks of Anthropic, used here in nominative fair use to describe the integration target.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Project conventions documented in [CLAUDE.md](CLAUDE.md). Bug reports and feature ideas welcome via GitHub Issues.

## License

[Apache License 2.0](LICENSE) © 2026 CodeForge contributors.
