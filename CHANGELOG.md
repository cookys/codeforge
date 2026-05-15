# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.2] - 2026-05-15

### Added

- `codeforge install` subcommand wires `~/.claude/settings.json` automatically with the binary's absolute path (statusLine MVP). Solves the rustup `--no-modify-path` + dotfiles-without-cargo-block silent-failure case. See `doc/specs/codeforge-install-subcommand.md` for the `--hooks` follow-up plan.
- `codeforge install --hooks` installs global-safe hooks; `--all` installs both statusLine + hooks.
- `doc/getting-started.md` — 5-step linear quickstart (clone → install → wire → init → adopt → verify) with a troubleshooting matrix.
- Distribution scaffolding: `[package.metadata.binstall]` for `cargo-binstall`, release profile (strip + LTO + codegen-units=1), declared MSRV `rust-version = "1.85"` in Cargo.toml, `CHANGELOG.md`, `release.toml` for `cargo-release`. See `doc/specs/codeforge-release-pipeline.md` for the full release pipeline plan.
- GitHub Actions workflows: `release.yml` (4-target matrix build on tag push; cross-compile aarch64-linux; macOS Intel + M-series), `release-smoke.yml` (PR-gate tarball assembly).

## [0.0.1] - 2026-05-15

Initial development version. Phases 1 through 3f shipped; see `README.md` Phase Roadmap.
