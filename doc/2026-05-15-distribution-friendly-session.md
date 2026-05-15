# 2026-05-15 — Distribution-friendly session handoff

Pickup note for whoever continues this work.

## TL;DR

Five-step distribution-friendly retrofit landed in 4 commits today:

| # | Commit | What |
|---|--------|------|
| 1 | `ed66e7e` | `codeforge install` MVP (statusLine only) |
| 2 | `3592257` | `doc/getting-started.md` 5-step linear quickstart |
| 3 | `8ce36ab` | Release pipeline scaffolding (CI matrix, install.sh, binstall metadata) |
| 4 | `40d44d0` | `codeforge install --hooks` for global-safe hook scripts |

Local state: `~/.cargo/bin/codeforge` rebuilt with all new code.
`~/.claude/settings.json` has statusLine pointing to abs path. Hooks
NOT yet installed to user's real `~/.claude/settings.json` — opt-in via
`codeforge install --all` if you want them now.

## What's shipped

### `codeforge install [--hooks] [--all]`

- Default (no flags): patches `~/.claude/settings.json` with a
  `statusLine` block using `std::env::current_exe()` as absolute path.
  Idempotent. Preserves all other keys.
- `--hooks`: installs global-safe hooks. Embeds two scripts via
  `include_str!` at compile time:
  - `emit-session.js` → `SessionStart` + `SessionEnd` (shells
    `codeforge emit` with cwd; project-agnostic)
  - `session-digest.js` → `PreCompact` + `SessionEnd` (writes to
    `~/.claude/session-digests/`; project-agnostic)
  Scripts extract to `~/.local/share/codeforge/hooks/<version>/`.
  Hook entries carry `_installed_by: "codeforge@<version>"` marker
  for idempotency. Re-running replaces our entries in place; user-owned
  hooks (no marker) are preserved.
- `--all`: both.

9 unit tests + 3-scenario E2E smoke verified.

### Release pipeline (`8ce36ab`, not cut yet)

When someone tags `v0.0.2`, `.github/workflows/release.yml` builds a
4-target matrix (linux + macOS × x86_64 + aarch64), produces
`codeforge-<ver>-<target>.tar.gz` + `.sha256`, attaches to a **draft**
GH Release. Manual flip to published after smoke validation. Full
sequencing in [`doc/specs/codeforge-release-pipeline.md`](specs/codeforge-release-pipeline.md).

`install.sh` (POSIX, shellchecked) consumes those releases:

```
curl -sSL https://raw.githubusercontent.com/cookys/codeforge/main/install.sh | sh
```

Plus `[package.metadata.binstall]` for `cargo binstall codeforge` once
released.

### `doc/getting-started.md`

5-step linear quickstart with a troubleshooting matrix derived from the
real failure modes hit during today's installation.

## Tasks deferred

### A V2.2 — install/uninstall feature completion

Spec: [`doc/specs/codeforge-install-subcommand.md`](specs/codeforge-install-subcommand.md).
Outstanding:

- `codeforge uninstall [--statusline] [--hooks]` — marker-driven removal
- `--dry-run` — print resulting JSON diff, no writes
- `--force` — overwrite non-codeforge entries (with `--yes` to skip
  confirmation)
- `--project-hooks` — install all 4 scripts (incl. the codeforge-repo-
  specific check-improvements + check-dev-flow) into a target repo's
  `<repo>/.claude/settings.json`. Requires patching scripts to read
  `CODEFORGE_PROJECT_ROOT` env var at extract time.
- Node probe (warn, not error) after `--hooks` install
- Cross-platform path handling (Windows currently uncovered;
  `#[cfg(windows)] compile_error!` recommended as v1 hard-stop)

The architect's full plan in the spec doc has line-level pointers and
test coverage matrix.

### C — Cut v0.0.2 release

Scaffolding is in place. To actually ship:

1. Bump `version = "0.0.2"` in `Cargo.toml`
2. Add `## [0.0.2] - <date>` section to CHANGELOG.md
3. `cargo release patch --execute` (or manual `git tag v0.0.2 && git push --tags`)
4. Watch the release.yml workflow build all 4 targets
5. Validate per [`doc/specs/codeforge-release-pipeline.md`](specs/codeforge-release-pipeline.md) §6
6. Flip the draft release to published

### Repo-side `.claude/settings.json` cleanup

The repo's own `.claude/settings.json` still has hardcoded
`/home/codepower/projects/codeforge/.claude/scripts/...` paths. These
fire only when Claude Code is opened in the codeforge clone — they're
project-scoped, not global. For contributors who clone elsewhere, the
paths are wrong. Options:

1. Leave as-is and document in README that contributors must update.
2. Replace with `${CODEFORGE_REPO}/...` if Claude Code ever supports env
   var expansion in hook commands.
3. Add `codeforge install --project-hooks` (V2.2 above) that
   `__filename`-derives or re-templates the paths to the current clone.

(3) is the most fix-once-helps-everyone path. Tracked in V2.2 spec.

## Local state at end of session

- `~/.cargo/bin/codeforge` v0.0.1 with `install [--hooks/--all]`
- `~/.claude/settings.json` has statusLine wired to abs path
- `~/.local/share/codeforge/hooks/0.0.1/` has `emit-session.js` and
  `session-digest.js` extracted (from earlier smoke test)
- 4 new knowledge entries recorded under `.claude/knowledge/`
  (environment.md + rust-patterns.md + INDEX.md updated)

## Sibling project state

`~/projects/llm-playground/` — the swe-personal bench work from earlier
in this session is committed there (3 commits ending in `df8229a` —
bench audit found 4% of tasks have sound verifiers). Separate handoff
at `~/projects/llm-playground/notes/eval/2026-05-15-session-handoff.md`.
