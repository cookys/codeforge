#!/usr/bin/env bash
# Tests for .githooks/post-checkout and scripts/install-checkout-guard.sh, in a throwaway repo
# isolated from the machine's global and system git config.
# HOOK=<file> tests another hook file instead (the red run: a no-op hook must fail these tests).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd -P)
hook=${HOOK:-$root/.githooks/post-checkout}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/checkout-guard.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid

fails=0
ok() { echo "  ok   $1"; }
bad() { echo "  FAIL $1"; fails=$((fails + 1)); }
warns() { grep -q '主 clone 離開了 develop' "$1"; }
expect() { if "$@"; then ok "$name"; else bad "$name"; fi; }
loud() { name=$2; expect warns "$1"; }
quiet() { name=$2; if warns "$1"; then bad "$name"; else ok "$name"; fi; }

repo="$tmp/main"
git init -q -b develop "$repo"
cd "$repo"
git config core.hooksPath "$repo/.git/hooks" # absolute: every worktree runs the common hooks
mkdir -p .githooks scripts
cp "$hook" .githooks/post-checkout
cp "$root/scripts/install-checkout-guard.sh" scripts/
echo a > a.txt
git add -A && git commit -qm init
git branch feature

out=$(bash scripts/install-checkout-guard.sh)
name="installer copies the hook"; expect test -x .git/hooks/post-checkout
out=$(bash scripts/install-checkout-guard.sh)
name="installer is idempotent"; expect grep -q "already installed" <<<"$out"

chmod 644 .git/hooks/post-checkout
bash scripts/install-checkout-guard.sh >/dev/null
name="installer restores a copy that lost its executable bit"; expect test -x .git/hooks/post-checkout

git switch -q feature 2>"$tmp/e1"
loud "$tmp/e1" "main clone: switching to a feature branch warns"
name="the warning names the branch"; expect grep -q '現在在 feature' "$tmp/e1"

git switch -q develop 2>"$tmp/e2"
quiet "$tmp/e2" "main clone: back on develop is quiet"

git tag develop
git switch -q feature 2>/dev/null
git switch -q develop 2>"$tmp/e2t"
quiet "$tmp/e2t" "main clone: back on develop is quiet with a tag also named develop"
git tag -d develop >/dev/null

echo b > a.txt
git checkout -q -- a.txt 2>"$tmp/e3"
quiet "$tmp/e3" "a file checkout is quiet"

git checkout -q --detach HEAD 2>"$tmp/e4"
loud "$tmp/e4" "main clone: a detached HEAD warns"
git switch -q develop 2>/dev/null

git worktree add -q "$tmp/wt" -b work develop 2>"$tmp/e5"
quiet "$tmp/e5" "a new linked worktree on a feature branch is quiet"
git -C "$tmp/wt" switch -q feature 2>"$tmp/e6"
quiet "$tmp/e6" "switching inside a linked worktree is quiet"

printf '#!/bin/sh\nexit 0\n' > .git/hooks/post-checkout
if bash scripts/install-checkout-guard.sh 2>"$tmp/e7"; then bad "installer refuses a foreign hook"; else ok "installer refuses a foreign hook"; fi

if [ "$fails" -gt 0 ]; then echo "checkout-guard: $fails failed"; exit 1; fi
echo "checkout-guard: all passed"
