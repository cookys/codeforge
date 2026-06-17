# `codeforge install` — Full Feature Spec (post-MVP)

**Status:** MVP merged (statusline-only). This spec covers the
`--hooks` + uninstall path. Audience: the next implementer.

## 0. Module layout

The MVP introduced `src/cli/install.rs` as a single file. Expand to a
module dir:

```
src/cli/install.rs        ← dispatch + flag parsing
src/cli/install/
  settings.rs             ← read/merge/write ~/.claude/settings.json
  paths.rs                ← platform-aware claude_settings_path(),
                            hooks_install_dir()
  statusline.rs           ← (already exists from MVP) statusLine patcher
  hooks.rs                ← hook block construction, idempotency markers
  scripts.rs              ← embedded JS extraction (include_str!)
  uninstall.rs            ← reverse of all of the above
```

Add `Install(InstallArgs)` / `Uninstall(UninstallArgs)` variants to
`src/cli/mod.rs`. Follow the `Daemon { action }` flag-bag pattern.

## 1. `--hooks` design decisions

### Scope insight: not all 4 scripts are globally safe

Audit of the 4 scripts under `.claude/scripts/`:

| Script | Scope | Reason |
|---|---|---|
| `emit-session.js` | **global** | Just shells `codeforge emit` with cwd from stdin. Project-agnostic. |
| `session-digest.js` | **global** | Installed for all sessions, but writes **per-repo** to `<repoRoot>/.codeforge/digests/` (walks up cwd to nearest `.codeforge`; skips if none — A′ 2026-06-17). The hook itself is project-agnostic; landing is repo-scoped. |
| `check-improvements.js` | **codeforge-repo-only** | Reads `<PROJECT_ROOT>/.claude/knowledge/INDEX.md`. Only meaningful inside the codeforge clone. |
| `check-dev-flow.js` | **codeforge-repo-only** | Hardcodes codeforge's `src/` + Rust file patterns. |

`codeforge install --hooks` (writing to `~/.claude/settings.json` = ALL
sessions in ALL projects) should install only the **global** scripts.
The codeforge-repo-only scripts belong in `<codeforge>/.claude/settings.json`
(committed as part of the codeforge repo, only fires when Claude Code is
opened in the codeforge dir).

V2 split:
- `codeforge install --hooks` (default `--global`): writes emit-session
  + session-digest to `~/.claude/settings.json`.
- `codeforge install --project-hooks` (cwd-scoped): writes all 4 to
  `$CWD/.claude/settings.json`. Only makes sense from the codeforge
  clone — `bail!` if `Cargo.toml` doesn't name the package `codeforge`.

### Script delivery: **embed via `include_str!`, extract on install**

Total script size is ~1188 lines / ~40 KB. Embedding wins:

- `codeforge` binary is self-contained — `cargo install --git` works
  with zero repo clone.
- Script version locked to binary version → idempotency marker check
  (below) can guarantee correctness.
- No network at install time.

Rejected: GH raw URL (offline-hostile, supply-chain surface);
cloned-repo path (defeats the purpose of `install`).

Build mechanism: `build.rs` already exists. Validate each
`.claude/scripts/*.js` exists at compile time, then in `scripts.rs` do
`include_str!("../../.claude/scripts/check-improvements.js")` etc. for
each of the four files.

**Wrinkle**: `check-improvements.js` derives `PROJECT_ROOT` from
`__filename` going three `dirname`s up, expecting
`<repo>/.claude/scripts/foo.js` layout. When extracted to
`~/.local/share/codeforge/hooks/`, that derivation yields
`~/.local/share/` and breaks `.claude/knowledge/INDEX.md` lookups.

Fix: at extract time, patch the scripts to read a
`CODEFORGE_PROJECT_ROOT` env var (set by the hook command we write into
settings.json). Deterministic string replace in `scripts.rs`. Document
in the script header.

### Hook target directory: `~/.local/share/codeforge/hooks/<version>/`

Use `dirs::data_local_dir()`. Versioned subdirs let multiple installed
versions coexist briefly during upgrade and make uninstall a simple
`rm -rf` of one directory.

- Windows: `%LOCALAPPDATA%\codeforge\hooks\<version>\`
- macOS:   `~/Library/Application Support/codeforge/hooks/<version>/`

Rejected: `~/.codeforge/hooks/` (collides with the optional
`CODEFORGE_DIR` global memory store); `~/.claude/scripts/` (pollutes
Claude's namespace).

### Idempotency: marker inside each hook entry

Insert `"_installed_by": "codeforge@0.x.y"` as an extra key in each hook
object we write. Claude Code ignores unknown keys (verified — `timeout`
etc. coexist). On re-install: parse settings, scan for entries with
`_installed_by` matching `codeforge@*`, replace them in place. Entries
lacking the marker are left untouched.

Rejected: separate sidecar checksum file. Splits state, race-prone,
harder for users to inspect.

### Conflict resolution: **merge, never destroy**

For each of the four hook types (SessionStart / PreToolUse / PreCompact
/ SessionEnd), append our marker-tagged entry to the existing
`hooks[<type>]` array. If a previous codeforge-tagged entry of the same
script is found, overwrite that single entry. Non-codeforge entries are
preserved verbatim — including array position.

Flag override: `--force` clears all four hook types entirely before
writing (destructive). `--dry-run` prints the resulting JSON diff and
exits.

### Uninstall: marker-driven removal

`codeforge uninstall --hooks` walks `hooks/*[]` and drops every entry
whose `_installed_by` starts with `codeforge@`. If a hook-type array
becomes empty, remove the type key. If `hooks` becomes empty, remove
`hooks`. Then `rm -rf ~/.local/share/codeforge/hooks/`. No separate
state file — the marker IS the state.

### Node probe: **warn, don't error**

`Command::new("node").arg("--version")`. If absent or fails, print a
yellow warning naming install methods (nvm / brew / apt) but exit 0.
The hooks themselves error gracefully (Claude Code logs a hook failure
and continues). Hard-failing install would block users whose `node` is
only present in non-login shells (asdf, fnm, etc.).

## 2. Cross-platform

### settings.json location

Claude Code's settings.json lives at `~/.claude/settings.json` on **all
platforms** including macOS (the Claude desktop app uses
`~/Library/Application Support/Claude/`; the CLI tool does not). One
code path: `dirs::home_dir().join(".claude").join("settings.json")`.
Override with `--settings-path <PATH>`.

### Path separators in JSON values

`current_exe()` returns a `PathBuf`. For the JSON `command` string, use
`.display().to_string()` on Unix; `.to_string_lossy().to_string()` on
Windows. **Critical**: do not call `.canonicalize()` on Windows — it
returns `\\?\C:\...` extended-length paths that some shells choke on.
Raw `current_exe()` is already absolute and shell-safe.

For embedded JS script paths in hook commands, build via `PathBuf::join`
then convert to string. The JSON encoder will escape backslashes
correctly. Verify with a Windows CI smoke test.

## 3. CLI surface

```
codeforge install [OPTIONS]
  (no flags)         → install statusline only (MVP behavior, unchanged)
  --hooks            → install hooks only
  --all              → statusline + hooks
  --dry-run          → print resulting settings.json + extraction plan,
                        no writes
  --force            → overwrite non-codeforge entries (prompts unless
                        --yes)
  --yes              → skip confirmation prompts
  --settings-path P  → target settings.json other than the default
  --quiet            → exit-code-only, no stdout

codeforge uninstall [OPTIONS]
  (no flags)         → remove everything codeforge-tagged + extracted
                        scripts
  --statusline       → remove only statusLine (if it points to
                        current_exe)
  --hooks            → remove only hooks
  --settings-path P  → as above
  --quiet
```

Default install with no flags stays statusline-only to preserve MVP
semantics. `--all` is the recommended invocation for new users.

Output: verbose by default, one line per action. No JSON output mode in
v1.

## 4. Test plan

**Unit (in-module `#[cfg(test)]`)** — extend MVP's `tests` mod:
- `settings::merge_hooks` — preserves unrelated user hooks; replaces
  same-marker entry; appends new-marker entry.
- `settings::remove_codeforge_entries` — drops marker entries, collapses
  empty arrays, leaves user entries.
- `paths::claude_settings_path` — `#[cfg(target_os)]` matrix.
- `scripts::patch_project_root` — string replacement on the JS source.

**Integration (`tests/install.rs`)**:
- Run binary in `tempdir` with `HOME=tempdir`, no preexisting
  `~/.claude/`. `install --all`. Assert settings.json structure, assert
  four `.js` files in `hooks/<ver>/`.
- Seed a `~/.claude/settings.json` with a user-owned `PreToolUse` hook.
  Run install. Assert user hook still present, codeforge hook appended.
- `install --all` → `uninstall`. Diff resulting settings.json vs
  pre-install snapshot — byte-equal modulo JSON key order.
- Edge: settings.json missing → create; malformed JSON → error with
  line number, no writes; `statusLine` already set to user's own
  command → refuse without `--force` (safer than backup).

**Manual smoke**: `cargo install --path . && codeforge install --all &&
claude` — verify hooks fire in a fresh Claude Code session.

## 5. Release-day checklist

README §"First-time Claude Code hook setup" replaced with:

```
After installing the binary:
  codeforge install --all

This patches ~/.claude/settings.json (statusline + hooks) and extracts
hook scripts to ~/.local/share/codeforge/. Re-run after upgrading
codeforge to refresh the embedded scripts.
```

Delete the BACKLOG note about a first-run setup script. Add §Uninstall.

CLAUDE.md §"Hook Path Note" (line 193) updated: hardcoded-path warning
becomes "Run `codeforge install --hooks` after cloning or upgrading;
the install command extracts versioned scripts to a stable location."

Success message:

```
✓ statusline installed → ~/.claude/settings.json
✓ hooks installed (4 types) → ~/.local/share/codeforge/hooks/0.1.2/
✓ node detected: v20.11.0

Restart any open Claude Code sessions to pick up changes.
```

If node missing, third line becomes a yellow warning with install hints.

`.claude/settings.json` in the repo: keep it as-is (canonical template
that `build.rs` and `include_str!` reference). Add a header comment
noting it's source-of-truth for embedded scripts.

## 6. Explicitly NOT in scope

- **No post-install Claude Code reload.** No documented API. Hooks pick
  up on next session start; restart message is honest and trivial.
- **No `codeforge init` auto-trigger.** Install touches `~/.claude/`;
  init touches `$CWD/.codeforge/`. Different scopes — composing invites
  surprise.
- **No PATH validation/warning.** `current_exe()` gives an absolute path
  that works regardless of PATH.
- **No migration tool for users who hand-edited settings.json.**
  Population is small; the marker-based merge handles it (their edits
  lack the marker → survive). Document in CHANGELOG.
- **No interactive prompts beyond `--force` confirmation.** Install is
  a one-shot tool, not a wizard. Flags > TUI.
- **No Windows support in v1 if it's expensive.** Linux + macOS only.
  Add `#[cfg(windows)] compile_error!` with a friendly message until
  someone files an issue.
- **No hook removal granularity** (`--hooks=SessionStart` etc.). All
  four together or none.
