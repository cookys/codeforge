# Getting Started with CodeForge

5 minutes from clone to working Claude Code statusline + pet.

## What you'll have at the end

- `codeforge` on your `$PATH`
- A statusline panel in Claude Code showing your model, dir, git branch,
  token-usage bars, and a pet that levels up as you code
- A per-project memory store (`.codeforge/`) that captures learnings
  during sessions

## Prerequisites

- **Rust stable** — `rustup` from <https://rustup.rs> (only needed for the
  `cargo install` route). Pre-built binaries are also available via the
  `curl | sh` installer and `cargo binstall codeforge` — see the
  [README Install section](../README.md#install) for all three options.
- **Claude Code** — <https://claude.com/claude-code> if you don't.
- **(optional) Node 20+** — needed by the JS hook scripts (not the
  statusline): global Layer-2 hooks `emit-session.js` (SessionStart +
  SessionEnd) + `session-digest.js` (SessionEnd + PreCompact), and the
  two clone-only dev hooks `check-improvements.js` (SessionStart) +
  `check-dev-flow.js` (PreToolUse). Skip if you only want the statusline.
- **(optional) `ANTHROPIC_API_KEY`** — NOT required. `codeforge dream`
  (memory compilation, L0 → L1) uses a fallback chain: `claude -p` (the
  Claude Code CLI, no key) → this key (Haiku API) → rule-based. Set it only
  as a fallback when the `claude` CLI is unavailable. See
  [`concepts.md`](concepts.md).

## Step 1 — Build and install the binary

```bash
git clone https://github.com/cookys/codeforge ~/projects/codeforge
cd ~/projects/codeforge
cargo install --path .
```

Output ends with `Installed package codeforge v0.0.5` (or the current crate version). The binary lands
at `~/.cargo/bin/codeforge`.

Quick check:

```bash
codeforge --version
```

If the command isn't found, `~/.cargo/bin` isn't on your shell's
`$PATH`. Add it via:

```bash
. ~/.cargo/env   # adds ~/.cargo/bin for the current shell
```

…or permanently in your shell rc file. The next step works around this
automatically, so don't worry too much.

## Step 2 — Wire CodeForge into Claude Code

```bash
codeforge install
```

This patches `~/.claude/settings.json` to set `codeforge statusline` as
the Claude Code statusLine hook — using the **absolute path** of the
binary, so it works even when `~/.cargo/bin` isn't on PATH for the
shells Claude Code spawns (a common rustup quirk).

Re-running is safe — it prints `已是最新（無變動）` if nothing changed.

Other keys in `settings.json` (your theme, permission settings, etc.)
are preserved.

## Step 3 — Initialize your project store

Pick a project you work on. From inside that project:

```bash
cd ~/projects/<your-repo>
codeforge init
```

This creates `.codeforge/` (per-project memory + pet state). Output:

```
✓ CodeForge 初始化完成
  專案記憶：/home/you/projects/<your-repo>/.codeforge
  個人 brain：/home/you/.codeforge/brain
  狀態 DB：/home/you/.local/share/codeforge/state.db
```

**Want one shared memory across all projects instead?** Set
`CODEFORGE_DIR=~/.codeforge/global` in your shell rc, then `codeforge
init` in that dir once. All projects will read/write the same store.
See [`.env.example`](../.env.example) for the global pattern.

## Step 4 — Adopt your pet

```bash
codeforge adopt
```

Interactive prompt — pick a village (language affinity). Each village
gives a different starter pet:

- **Scriptorium Vast** — Python
- **Border Garrison** — TypeScript
- **The Forge-Ruins** — Rust
- **(more)** — see `codeforge adopt` output for the full list

Pick one and confirm. This bootstraps the pet record that the
statusline + `codeforge pet` commands read from.

## Step 5 — Verify in Claude Code

Open Claude Code in the project you just initialized:

```bash
cd ~/projects/<your-repo>
claude
```

The statusline panel should now show:

- Your model name + workspace + git branch (line 1)
- Token usage bars: 5h / 7d / context (line 2)
- Your pet's location, level, HP, XP, stats (lines 3-4)
- Memory status + codeforge version (line 5)
- ASCII pet portrait on the right

If you see only the minimal no-pet statusline (a 2-line panel: identity
strip with model/cwd/branch/version, then usage bars with a `→ codeforge
adopt` hint), your pet record isn't loaded — re-run `codeforge adopt`
and confirm `codeforge pet` shows stats.

If you see nothing at all, Claude Code didn't pick up the settings.json
change. Try `/clear` or restart Claude Code.

## What's next

- `codeforge learn "tokio::select! preserves cancellation across branches"`
  — log a learning into the L0 raw-signal store. Also accepts `--paste`
  (clipboard), `--file <FILE>`, or piped stdin instead of positional text.
- `codeforge ingest <path> [--source claude|chatgpt|markdown|auto]` — import
  an external chat/history export into L0 (default `--source auto`).
- `codeforge dream` — compile L0 signals into structured L1 knowledge
  (via `claude -p`; no API key needed).
- `codeforge memory search "tokio cancellation"` — search the compiled
  knowledge base.
- `codeforge pet` — full pet status panel (longer than statusline).
- `codeforge snapshot` — generate a shareable ASCII monthly report
  card.
- `codeforge world` — render the world map (codebase as zones).

For the SessionStart / SessionEnd / PreCompact hooks (session tracking,
local recall, dream-on-close memory pipeline), run `codeforge install --all`
(statusline + global hooks). See the
[README "First-time Claude Code hook setup"](../README.md#first-time-claude-code-hook-setup)
section and [`doc/specs/codeforge-install-subcommand.md`](specs/codeforge-install-subcommand.md).
These flags are shipped, not roadmap.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `codeforge: command not found` after `cargo install` | `~/.cargo/bin` not on PATH | `. ~/.cargo/env` for this shell; add to `~/.zshrc` / `~/.bashrc` permanently. **Or** just run `codeforge install` — it writes the absolute path so Claude Code finds it. |
| Statusline shows the minimal 2-line no-pet panel (identity + usage bars + `→ codeforge adopt` hint) | No pet adopted | `codeforge adopt` then `codeforge pet` to verify |
| Statusline shows nothing | Claude Code didn't reload settings.json | `/clear` or restart Claude Code |
| `codeforge install` rejected with `settings.json 根節點不是 object` | Existing settings.json is a JSON array or non-object | Open `~/.claude/settings.json` and wrap content in `{}` or back it up and start fresh |
| `codeforge dream` produces low-quality / rule-based output | `claude` CLI not on PATH and no `ANTHROPIC_API_KEY` set (fell through to rule-based pass) | Ensure the `claude` CLI is installed/on PATH, or set `ANTHROPIC_API_KEY=sk-ant-...` from <https://console.anthropic.com/> as a fallback |

For deeper issues, file at <https://github.com/cookys/codeforge/issues>.
