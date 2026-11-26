#!/usr/bin/env bash
# Behavioral test for scripts/backlog-view.sh: doc/BACKLOG.md is a view of the hippo store.
#  1. the checked-in file equals the store rendering (a hand edit or a store change without a re-render turns this red);
#  2. in a throwaway copy, a hand edit is refused with "never edit the file by hand", and re-rendering repairs it.
# Needs python3 and a hippo binary (HIPPO_BIN, else PATH, else ~/.local/bin/hippo); skips with a notice when there is none.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${HIPPO_BIN:-}" ]; then
  if command -v hippo > /dev/null 2>&1; then HIPPO_BIN=hippo; elif [ -x "$HOME/.local/bin/hippo" ]; then HIPPO_BIN=$HOME/.local/bin/hippo; fi
fi
if [ -z "${HIPPO_BIN:-}" ]; then echo "backlog-view.test: SKIP (no hippo binary)"; exit 0; fi
export HIPPO_BIN
fail() { echo "FAIL: $1" >&2; exit 1; }

sh scripts/backlog-view.sh --check || fail "doc/BACKLOG.md differs from the store"

t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT
git init -q "$t/c"
mkdir -p "$t/c/doc" "$t/c/scripts"
cp -R .hippo "$t/c/"
cp scripts/backlog-view.sh scripts/backlog-restore.py "$t/c/scripts/"
cp doc/BACKLOG.md "$t/c/doc/BACKLOG.md"
printf '%s\n' '- **Needs**: hand edit' >> "$t/c/doc/BACKLOG.md"
if sh "$t/c/scripts/backlog-view.sh" --check 2> "$t/err"; then fail "a hand-edited doc/BACKLOG.md passed"; fi
grep -q 'never edit the file by hand' "$t/err" || fail "no hand-edit message: $(cat "$t/err")"
sh "$t/c/scripts/backlog-view.sh"
sh "$t/c/scripts/backlog-view.sh" --check || fail "re-render did not repair the file"
echo "backlog-view.test: ok"
