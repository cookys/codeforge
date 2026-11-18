#!/usr/bin/env bash
# check-all runs as the pre-push hook, where git exports GIT_DIR for the real repo. Its tests run
# `git init`/`git add` in temp dirs; without check-all's env cleanup those hit the real repo
# (core.bare=true, staged files). This runs one such test (check-private-leaks.test.sh) under a
# hook-like GIT_DIR pointing at a sentinel repo, with and without check-all's own cleanup line.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# shellcheck disable=SC2016  # the literal line, not an expansion
cleanup=$(grep -x 'unset $(git rev-parse --local-env-vars)' "$here/check-all.sh") \
  || fail "check-all.sh lost its git env cleanup line"

mk() {
  git init -q "$1"
  git -C "$1" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m init
}
state() { printf '%s %s\n' "$(git -C "$1" config core.bare)" "$(git -C "$1" ls-files --stage | wc -l)"; }

mk "$T/real"
before=$(state "$T/real")
GIT_DIR="$T/real/.git" bash -c "$cleanup; \"$here/check-private-leaks.test.sh\"" >/dev/null 2>&1 \
  || fail "check-private-leaks.test.sh failed under hook env with cleanup"
[ "$(state "$T/real")" = "$before" ] || fail "test touched the hook's repo despite the cleanup"

# Negative control: without the cleanup the same run damages the sentinel repo.
mk "$T/ctl"
before=$(state "$T/ctl")
GIT_DIR="$T/ctl/.git" "$here/check-private-leaks.test.sh" >/dev/null 2>&1 || true
[ "$(state "$T/ctl")" != "$before" ] || fail "negative control: no damage without cleanup (test is blind)"
echo "hook-env.test.sh: ok"
