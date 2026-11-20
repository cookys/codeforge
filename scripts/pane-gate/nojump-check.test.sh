#!/usr/bin/env bash
# nojump-check.test.sh — controls for scripts/pane-gate/nojump-check.py.
# Builds fixture frame dirs in a mktemp dir and checks exit codes / output.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
check="$repo/scripts/pane-gate/nojump-check.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
controls=0

fail() { echo "nojump-check.test.sh: $*" >&2; exit 1; }

# frame <out> <strip-row> <prompt-row> <header-row|''> — write one 8-line frame.
frame() {
  local out=$1 strip_row=$2 prompt_row=$3 header_row=${4:-} i
  : >"$out"
  for i in 0 1 2 3 4 5 6 7; do
    if [[ $i == "$strip_row" ]]; then
      printf '1 現況  2 待決  3 艦隊  4 額度\n' >>"$out"
    elif [[ $i == "$prompt_row" ]]; then
      printf '> ready\n' >>"$out"
    elif [[ -n $header_row && $i == "$header_row" ]]; then
      printf '│ 待決 · 3 張\n' >>"$out"
    else
      printf 'chat %d\n' "$i" >>"$out"
    fi
  done
}

# expect <want-rc> <dir> [substring] — run check on dir, assert rc and optional output substring.
expect() {
  local want=$1 dir=$2 sub=${3:-} rc=0 out
  out="$(python3 -I "$check" "$dir")" || rc=$?
  [[ $rc == "$want" ]] || fail "expected exit $want, got $rc: $out"
  [[ -z $sub ]] || grep -qF "$sub" <<<"$out" || fail "output missing '$sub': $out"
}

# 1: three identical-layout frames pass; summary starts with frames=3.
d="$tmp/c1"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 5 7 3; frame "$d/03" 5 7 3
expect 0 "$d" "frames=3"
out="$(python3 -I "$check" "$d")"
[[ $out == frames=3* ]] || fail "summary does not start with frames=3: $out"
controls=$((controls + 1))

# 2: strip row moves in one frame -> exit 1, FAIL strip row moves.
d="$tmp/c2"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 4 7 3
expect 1 "$d" "FAIL strip row moves"
controls=$((controls + 1))

# 3: one frame has no strip -> exit 1, strip missing in: 02.
d="$tmp/c3"; mkdir "$d"
frame "$d/01" 5 7 3
for i in 0 1 2 3 4 5 6 7; do printf 'chat %d\n' "$i"; done >"$d/02"
printf '> ready\n' >>"$d/02"
expect 1 "$d" "strip missing in: 02"
controls=$((controls + 1))

# 4: prompt row moves -> exit 1, FAIL prompt row moves.
d="$tmp/c4"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 5 6 3
expect 1 "$d" "FAIL prompt row moves"
controls=$((controls + 1))

# 5: header collapsed in some frames but same row where present -> exit 0.
d="$tmp/c5"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 5 7 3; frame "$d/03" 5 7 ""
expect 0 "$d"
controls=$((controls + 1))

# 6: header row moves -> exit 1, FAIL header row moves.
d="$tmp/c6"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 5 7 2
expect 1 "$d" "FAIL header row moves"
controls=$((controls + 1))

# 7: empty dir -> exit 2.
d="$tmp/c7"; mkdir "$d"
expect 2 "$d"
controls=$((controls + 1))

# 8: no argument -> exit 2.
rc=0
python3 -I "$check" >/dev/null 2>&1 || rc=$?
[[ $rc == 2 ]] || fail "no-argument run: expected exit 2, got $rc"
controls=$((controls + 1))

# 9: dot-files are ignored -> .hidden with a moved strip does not fail the run.
d="$tmp/c9"; mkdir "$d"
frame "$d/01" 5 7 3; frame "$d/02" 5 7 3; frame "$d/03" 5 7 3
frame "$d/.hidden" 2 7 3
expect 0 "$d" "frames=3"
controls=$((controls + 1))

echo "nojump-check.test.sh: ok ($controls controls)"
