#!/usr/bin/env bash
# The merge gate. There is no hosted CI (PM G38 retired .github/workflows after v0.0.9).
#
# Runs, in order: pinned fmt check → clippy -D warnings → cargo test (through the flake guard:
# a failed test is rerun once; one that then passes is recorded in target/flaky-tests.log and
# named on stderr, never hidden) → forge-stats plugin tests (when claude is on PATH)
# → cjk-safe → doc-drift → cargo deny (licenses) → shellcheck
# install.sh and the checkout guard → checkout-guard test → release smoke (the x86_64-unknown-linux-gnu tarball built with the MSRV toolchain
# through scripts/package.sh, unpacked and run) — every job the retired ci.yml and
# release-smoke.yml ran.
# On Windows (Git Bash) it then repeats clippy + test on Linux through WSL, so the Unix code
# paths are still exercised. macOS cannot be covered locally.
#
# Usage:
#   ./scripts/check-all.sh                 # native pass (+ WSL Linux pass on Windows)
#   ./scripts/check-all.sh --no-wsl        # native pass only
#   ./scripts/check-all.sh --install-hook  # run this before every `git push` (copied into the common hooks dir)
#
# Env:
#   CODEFORGE_TOOLCHAIN   cargo +toolchain override for the native pass
#                         (Windows default: stable-x86_64-pc-windows-msvc — the GNU host has no gcc)
#   CODEFORGE_WSL_DISTRO  WSL distro for the Linux pass (default: the WSL default distro)
set -euo pipefail

# As the pre-push hook this runs with GIT_DIR (and in a worktree GIT_INDEX_FILE) pointing at the
# real repo; the tests' `git init`/`git add` in temp dirs then hit it instead (it set core.bare=true
# on the main clone and staged 502 deletions in a worktree, 2026-11-19). Test: hook-env.test.sh.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)

cd "$(dirname "$0")/.."

WITH_WSL=1
for arg in "$@"; do
  case "$arg" in
    --no-wsl) WITH_WSL=0 ;;
    --install-hook)
      # Copy into the common hooks dir (shared by every worktree; in a worktree .git is a file).
      # Never set core.hooksPath: a repo-level value hides the global lab hook, which chains
      # to this dir itself (lab 01M7K094, PM 01M7K08N).
      hooks="$(git rev-parse --path-format=absolute --git-common-dir)/hooks"
      dst="$hooks/pre-push"
      if [ -e "$dst" ] && ! cmp -s .githooks/pre-push "$dst" \
        && ! grep -q 'check-all.sh --install-hook' "$dst"; then
        echo "check-all: $dst already exists and is not this hook; not replacing it" >&2
        exit 1
      fi
      mkdir -p "$hooks"
      cp .githooks/pre-push "$dst.tmp.$$"
      chmod 755 "$dst.tmp.$$"
      mv -f "$dst.tmp.$$" "$dst"
      if git config --local --get core.hooksPath >/dev/null; then
        git config --local --unset core.hooksPath
        echo "check-all: removed the local core.hooksPath (it hid the global hooks)."
      fi
      echo "check-all: pre-push hook installed at $dst."
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

step "forge-stats plugin tests"
if command -v claude >/dev/null 2>&1; then
  claude plugin test plugins/forge-stats
else
  echo "check-all: SKIP forge-stats plugin tests (claude is not on PATH)."
fi

step "tux vocabulary lint"
python3 scripts/tux-lint.py --test
python3 scripts/tux-lint.py

step "cjk-safe"
./scripts/check-cjk-safe.sh

step "private-leaks (hashed denylist + email allowlist, public tree)"
./scripts/check-private-leaks.test.sh
./scripts/check-private-leaks.sh

step "doc-drift"
python3 scripts/check-doc-drift.py 2>/dev/null || python scripts/check-doc-drift.py

step "cargo deny (licenses)"
cargo deny check licenses

step "shellcheck install.sh + checkout guard"
shellcheck install.sh .githooks/post-checkout scripts/install-checkout-guard.sh scripts/checkout-guard.test.sh \
  scripts/g40-real-data/check.sh scripts/g40-real-data/check.test.sh scripts/g40-real-data/scan-statusline.test.sh \
  scripts/trinity-import/trinity-import.sh scripts/trinity-import/trinity-import.test.sh \
  scripts/check-private-leaks.sh scripts/check-private-leaks.test.sh scripts/hook-env.test.sh \
  .githooks/pre-push scripts/push-ref-guard.sh scripts/push-ref-guard.test.sh \
  scripts/tux-sync.sh scripts/tux-lock.sh scripts/tux-lock.test.sh \
  scripts/tux-cross.sh scripts/tux-cross.test.sh scripts/tux-golden-data.test.sh

step "g40 real-data scan (negative controls)"
./scripts/g40-real-data/scan-statusline.test.sh
./scripts/g40-real-data/check.test.sh

step "trinity quota import (fake fleet, tmpfs live dir)"
./scripts/trinity-import/trinity-import.test.sh

step "checkout guard (the main clone stays on develop)"
./scripts/checkout-guard.test.sh
./scripts/hook-env.test.sh

step "tux-golden-data (bridge generator, isolated copy)"
./scripts/tux-golden-data.test.sh

step "tux-lock (vendor shas and file list)"
./scripts/tux-lock.test.sh
./scripts/tux-lock.sh

step "tux-cross (pinned tux vs autopilot; sibling absent = SKIP)"
./scripts/tux-cross.test.sh
TUX_CROSS_RESULT=$(./scripts/tux-cross.sh)
printf '%s\n' "$TUX_CROSS_RESULT"

step "public push guard (origin is public: only develop/main)"
./scripts/push-ref-guard.test.sh

step "release archive hermetic (B43)"
./scripts/test-archive-hermetic.sh

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

if [[ "$TUX_CROSS_RESULT" == *'tux-cross: skip:'* ]]; then
  printf '\ncheck-all: other gates green; SKIP tux-cross (sibling unavailable)\n'
else
  printf '\n\033[1;32mcheck-all: all gates green\033[0m\n'
fi
