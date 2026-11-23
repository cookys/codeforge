#!/usr/bin/env bash
# in-dir.sh: runs in the dir, keeps the exit code, refuses a missing dir and a missing command.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
[ "$("$here/in-dir.sh" "$tmp" pwd -P)" = "$(cd "$tmp" && pwd -P)" ] || { echo "FAIL: cwd" >&2; exit 1; }
rc=0; "$here/in-dir.sh" "$tmp" sh -c 'exit 7' || rc=$?; [ "$rc" = 7 ] || { echo "FAIL: exit code $rc" >&2; exit 1; }
rc=0; "$here/in-dir.sh" "$tmp/none" true 2>/dev/null || rc=$?; [ "$rc" = 2 ] || { echo "FAIL: missing dir rc=$rc" >&2; exit 1; }
rc=0; "$here/in-dir.sh" "$tmp" 2>/dev/null || rc=$?; [ "$rc" = 2 ] || { echo "FAIL: missing cmd rc=$rc" >&2; exit 1; }
echo "in-dir.test: ok"
