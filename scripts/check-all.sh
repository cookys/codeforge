#!/usr/bin/env bash
# Local quality gate — the replacement for GitHub CI (which burns Actions quota).
#
# Runs, in order: pinned fmt check → clippy -D warnings → cargo test (through the flake guard:
# a failed test is rerun once; one that then passes is recorded in target/flaky-tests.log and
# named on stderr, never hidden) → cjk-safe → doc-drift → cargo deny (licenses) → shellcheck
# install.sh → release smoke (the x86_64-unknown-linux-gnu tarball built with the MSRV toolchain
# through scripts/package.sh, unpacked and run). Together: every job of .github/workflows/ci.yml
# and release-smoke.yml.
# On Windows (Git Bash) it then repeats clippy + test on Linux through WSL, so the Unix code
# paths are still exercised. macOS cannot be covered locally.
#
# Usage:
#   ./scripts/check-all.sh                 # native pass (+ WSL Linux pass on Windows)
#   ./scripts/check-all.sh --no-wsl        # native pass only
#   ./scripts/check-all.sh --install-hook  # run this before every `git push` (core.hooksPath=.githooks)
#
# Env:
#   CODEFORGE_TOOLCHAIN   cargo +toolchain override for the native pass
#                         (Windows default: stable-x86_64-pc-windows-msvc — the GNU host has no gcc)
#   CODEFORGE_WSL_DISTRO  WSL distro for the Linux pass (default: the WSL default distro)
set -euo pipefail

cd "$(dirname "$0")/.."

WITH_WSL=1
for arg in "$@"; do
  case "$arg" in
    --no-wsl) WITH_WSL=0 ;;
    --install-hook)
      git config core.hooksPath .githooks
      echo "check-all: pre-push hook enabled (git config core.hooksPath .githooks)."
      echo "           bypass once with: git push --no-verify"
      exit 0
      ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "check-all: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) IS_WINDOWS=1 ;;
  *) IS_WINDOWS=0 ;;
esac

TC="${CODEFORGE_TOOLCHAIN:-}"
if [ -z "$TC" ] && [ "$IS_WINDOWS" = 1 ]; then
  TC="stable-x86_64-pc-windows-msvc"
fi
CARGO=(cargo)
[ -n "$TC" ] && CARGO=(cargo "+$TC")

step() { printf '\n\033[1m── %s ──\033[0m\n' "$*"; }

step "fmt (pinned)"
./scripts/fmt.sh --check

step "clippy -D warnings (${TC:-default toolchain})"
"${CARGO[@]}" clippy --all-targets -- -D warnings

step "cargo test (${TC:-default toolchain}, flake guard)"
./scripts/test-flake-guard.sh "${CARGO[@]}"

step "cjk-safe"
./scripts/check-cjk-safe.sh

step "doc-drift"
python3 scripts/check-doc-drift.py 2>/dev/null || python scripts/check-doc-drift.py

step "cargo deny (licenses)"
cargo deny check licenses

step "shellcheck install.sh"
shellcheck install.sh

# The MSRV, from Cargo.toml's rust-version: the release build must compile with it.
MSRV=$(grep -m1 '^rust-version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
if [ "$IS_WINDOWS" = 0 ]; then
  step "release smoke (x86_64-unknown-linux-gnu, toolchain $MSRV)"
  SMOKE_OUT=$(mktemp -d)
  trap 'rm -rf "$SMOKE_OUT"' EXIT
  ./scripts/package.sh x86_64-unknown-linux-gnu --toolchain "$MSRV" --out "$SMOKE_OUT" >/dev/null
  rm -rf "$SMOKE_OUT"
  trap - EXIT
fi

if [ "$IS_WINDOWS" = 1 ] && [ "$WITH_WSL" = 1 ]; then
  if command -v wsl.exe >/dev/null 2>&1 && wsl.exe -e true >/dev/null 2>&1; then
    step "Linux pass via WSL: clippy + test"
    WSL=(wsl.exe)
    [ -n "${CODEFORGE_WSL_DISTRO:-}" ] && WSL=(wsl.exe -d "$CODEFORGE_WSL_DISTRO")
    # Separate target dir on the Linux filesystem: /mnt/c is slow and must not mix with the
    # Windows target/. The path is passed as an argument so no quoting survives two shells.
    "${WSL[@]}" -e bash -lc '
      set -euo pipefail
      cd "$(wslpath -u "$1")"
      export CARGO_TARGET_DIR="$HOME/.cache/codeforge-check-target"
      cargo clippy --all-targets -- -D warnings
      ./scripts/test-flake-guard.sh cargo
    ' _ "$(pwd -W)"
  else
    echo
    echo "check-all: WSL not available — Linux pass SKIPPED (Unix code paths not exercised)." >&2
  fi
fi

printf '\n\033[1;32mcheck-all: all gates green\033[0m\n'
