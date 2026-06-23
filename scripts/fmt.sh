#!/usr/bin/env bash
# Pinned, reproducible `cargo fmt` for CodeForge — the SINGLE SOURCE OF TRUTH
# for how this repo is formatted. Every actor (your machines, CI's fmt job, and
# AI agents in any session) MUST format through this script so output is
# byte-identical everywhere. Do NOT run a bare `cargo fmt` — a rolling `stable`
# rustfmt re-wraps long lines between toolchain versions (RFC 2437 explicitly
# refuses cross-version stability), which is what caused the 2026-06 drift.
#
# Why a +toolchain wrapper instead of rust-toolchain.toml: a `channel` pin in
# rust-toolchain.toml would drag `cargo build`/`test`/`clippy` onto the fixed
# version too — the exact thing this project avoids (MSRV stays declared via
# Cargo.toml `rust-version`, build stays on rolling stable). The `+toolchain`
# override (highest rustup precedence) pins ONLY formatting, nothing else.
#
# Self-bootstrapping: installs the pinned toolchain + rustfmt on demand, so a
# fresh checkout / CI runner / agent session just runs this one script and is
# automatically aligned — no "remember to install X" step.
#
# Usage:
#   ./scripts/fmt.sh            # format the whole workspace in place
#   ./scripts/fmt.sh --check    # verify formatting (CI gate; exit 1 on drift)
#
# Bump policy: change PIN deliberately in a dedicated commit that also re-runs
# `./scripts/fmt.sh` repo-wide (intentional upgrades, never silent rolling).
set -euo pipefail

# The one place the formatting toolchain version is defined.
PIN="1.94"

if ! command -v rustup >/dev/null 2>&1; then
  echo "fmt.sh: needs rustup to pin the formatting toolchain — install from https://rustup.rs" >&2
  exit 1
fi

# Ensure the pinned toolchain + rustfmt component exist (idempotent, quiet).
if ! rustup run "$PIN" rustfmt --version >/dev/null 2>&1; then
  echo "fmt.sh: installing pinned toolchain ${PIN} + rustfmt ..." >&2
  rustup toolchain install "$PIN" --component rustfmt --no-self-update >&2
fi

exec cargo "+$PIN" fmt --all "$@"
