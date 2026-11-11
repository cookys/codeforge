# src/cli/ — commands and their runtime wiring (detail split from the root CLAUDE.md)

Loaded by Claude Code when a file under `src/cli/` is touched. Also read `src/CLAUDE.md`
(source layout, design decisions, LLM backend). Working on `ship.rs` or `mnemos_cli.rs`:
read `src/mnemos/CLAUDE.md` first — it does not load from here.

## CodePower ↔ CodeForge Interaction

CodeForge and CodePower are used together. Understanding the relationship prevents confusion:

### Statusline (global)

`codeforge statusline` is invoked by **all Claude Code sessions** across all projects — not just the CodeForge repo. The statusline hook in `~/.claude/settings.json` (CodePower) calls `codeforge statusline` to display the 5-line pet panel (+ right-column pet art). Changes to the statusline affect every project's session header.

### Dream (session-end, all projects)

`codeforge dream --quiet` runs at session end in **every** project, via the **global** SessionEnd hook (`~/.claude/settings.json`, installed by `codeforge install --hooks`/`--all`). The hook runs with CWD = the project root, so dream distills that project's own `.codeforge` (per-cwd memory). This means memory and pet XP update from activity in CodePower, or any other project — not just the CodeForge codebase. `--quiet` is mandatory for hook use (suppresses all stdout). `codeforge ship --no-hook` chains right after dream in the same SessionEnd group (see Ship in [`src/mnemos/CLAUDE.md`](../mnemos/CLAUDE.md)); ship self-gates on Mnemos opt-in, so a codeforge-only user with no Mnemos still distills via dream while ship is a clean no-op.

### Recall (session-start) — READ side, symmetric to the WRITE side

The memory loop is **absorb → distill → store → recall**. READ mirrors WRITE with the same local-always / central-opt-in split:

| | Local (always, no Mnemos) | Central (opt-in, needs Mnemos) |
|---|---|---|
| **WRITE** | `dream` (L0→L1) | `ship` (→ Mnemos) |
| **READ** | `codeforge memory context --hook` (global SessionStart) | `mnemos-cli context` (cross-source) |

`codeforge memory context` ranks active L1 by a unified recall score (importance × recency × citation — strength weighted by a recency half-life and log-damped citation count) → budgets to a **lean index** (~1.5K tokens, never a dump — context-pollution lesson from claude-mem) → emits `hookSpecificOutput.additionalContext` at SessionStart. No-op when the project has no active L1. Each line cites its `topic`; detail is pulled on demand via `codeforge memory search <topic>` (progressive disclosure). Installed in **global settings.json**, not a plugin (plugin `hooks.json` additionalContext is unreliable — Claude Code issue #16538). Shared-state + atom schema contract: [`doc/specs/codeforge-memory-contract.md`](../../doc/specs/codeforge-memory-contract.md).

### Learn (any project → codeforge memory)

`codeforge learn "..."` can be run from any directory. Where the signal lands depends on `CODEFORGE_DIR`:
- **`CODEFORGE_DIR` not set**: L0 signal written to `$CWD/.codeforge/signals/` — each project has its own memory store. Learnings from a CodePower session land in CodePower's `.codeforge/`, not the CodeForge repo.
- **`CODEFORGE_DIR` set** (e.g. `~/.codeforge/global`): all projects share a single memory store at that path, regardless of CWD. This is the "global memory" pattern documented in `.env.example`.

### Hook Path Note

Project `.claude/settings.json` carries **codeforge-clone-only DEV** hooks only: `check-improvements.js` (SessionStart), `check-dev-flow.js` (PreToolUse). The script paths use `${CLAUDE_PROJECT_DIR}/.claude/scripts/...` (Claude Code env var, expanded at hook spawn) — no per-clone hand-editing needed.

The **product-wide** hooks — `codeforge hook emit-session`, `codeforge hook session-digest` (the scripts are embedded in the binary, `src/cli/hook.rs`; no machine path or version in settings.json, marker `codeforge@1`), the SessionStart local-recall injector `codeforge memory context --hook`, and the `codeforge dream --quiet` → `codeforge ship --no-hook` memory-pipeline SessionEnd chain (plus a top-level `cleanupPeriodDays = 3650` on a local settings.json; a shared one — symlink or git-tracked — only with `--cleanup-period-days`) — live in the **global** install (`codeforge install --hooks`/`--all` → `~/.claude/settings.json`). dream/ship were moved here from `--project-hooks` so they run across **all** projects, not just the codeforge clone (`--project-hooks` is `ensure_in_codeforge_repo`-gated to the clone, so it could never cover other projects). This also avoids dual-fire when working in the codeforge clone. Contributors after fresh clone run `codeforge install --all` once for the global hooks; `--project-hooks` adds the clone-only dev hooks.

The two former V2.2 install bugs are **fixed** (this repo): `patch_hooks` now sweeps *all* codeforge-owned hook groups across every hook_type before re-adding the current set (and collapses emptied arrays), so entries relocate between hook_types / scopes without orphaning or duplicating. `is_codeforge_hook_command` also recognizes pre-marker node hook-script commands (by codeforge scripts path + known basename), so upgrading from an un-markered install sweeps the old versioned copy instead of stacking a duplicate.

Each hook entry is recognized by **marker or by command** (`is_codeforge_hook_command` matches the `codeforge <sub>` commands install writes — `hook …`, `dream`, `ship`, `memory context`, `mnemos-cli context`, `usage collect` — on a token boundary), so a Claude Code settings rewrite that drops the `_installed_by` marker does not cause duplicates on re-install or leave orphans on `uninstall`.
