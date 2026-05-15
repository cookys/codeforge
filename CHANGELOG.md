# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.3] - 2026-05-15

### Added

- `codeforge install --project-hooks` + `codeforge uninstall` + `--dry-run`, `--force`, `--yes`, `--settings-path`, `--quiet` flags (full V2.2 surface per `doc/specs/codeforge-install-subcommand.md`). Marker-driven uninstall (`_installed_by: codeforge@<ver>`) preserves user-owned settings.
- `.claude/settings.json` rewritten to use `${CLAUDE_PROJECT_DIR}` (Claude Code's documented project-root env var) instead of hardcoded paths — checked-in file is now portable across every clone, no post-clone path-rewrite needed. Models the [Husky](https://github.com/typicode/husky) / [pre-commit](https://pre-commit.com/) "committed config + relative paths" pattern. `codeforge install --project-hooks` likewise writes `${CLAUDE_PROJECT_DIR}` paths, producing byte-identical commit-friendly output across machines.
- Statusline V2 — unified visual language across minimal & full modes. `▏▕` chip wrappers replaced by `──` dim separators (matches box-drawing language). Git ahead/behind indicator `⇡N⇣M` (powerlevel10k-style). Claude Code version + update banner in both modes. Onboarding hint `→ codeforge adopt` always visible in minimal mode (degrades to `→ adopt` on narrow terminals). Context-pressure dimming when ctx > 80% — Tier-3 elements (separators, box-drawing) shift dimmer so the chat regains visual priority.
- Statusline V2 (full mode): village name collapsed into top border (6→5 rows). ASCII art column auto-drops below 90 cols. `HP/XP` → `♥/✦` symbols. Colon-less stat labels (`ATK 18` not `ATK: 18`). Strategy as `▸ agg` mode indicator. `Memory: active` → k9s-style `memory ● active`. i18n (`en.yaml` + `zh-TW.yaml`) updated to match.

### Fixed

- Project `.claude/settings.json` no longer dual-fires emit-session / session-digest hooks when `codeforge install --all` is run (clone-only scripts stay in project layer; product-wide scripts live in `~/.claude/`). README "First-time Claude Code hook setup" §Layer 1 / Layer 2 documents the split.
- `codeforge dream --quiet` SessionEnd hook restored in project settings (V2.2 portability rewrite had silently dropped it because it's not a `.claude/scripts/*.js` entry).

## [0.0.2] - 2026-05-15

### Added

- `codeforge install` subcommand wires `~/.claude/settings.json` automatically with the binary's absolute path (statusLine MVP). Solves the rustup `--no-modify-path` + dotfiles-without-cargo-block silent-failure case. See `doc/specs/codeforge-install-subcommand.md` for the `--hooks` follow-up plan.
- `codeforge install --hooks` installs global-safe hooks; `--all` installs both statusLine + hooks.
- `doc/getting-started.md` — 5-step linear quickstart (clone → install → wire → init → adopt → verify) with a troubleshooting matrix.
- Distribution scaffolding: `[package.metadata.binstall]` for `cargo-binstall`, release profile (strip + LTO + codegen-units=1), declared MSRV `rust-version = "1.85"` in Cargo.toml, `CHANGELOG.md`, `release.toml` for `cargo-release`. See `doc/specs/codeforge-release-pipeline.md` for the full release pipeline plan.
- GitHub Actions workflows: `release.yml` (4-target matrix build on tag push; cross-compile aarch64-linux; macOS Intel + M-series), `release-smoke.yml` (PR-gate tarball assembly).

## [0.0.1] - 2026-05-15

Initial development version. Phases 1 through 3f shipped; see `README.md` Phase Roadmap.
