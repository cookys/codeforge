#!/usr/bin/env bash
# Negative controls for scan-statusline.py: a clean capture passes; a too-wide line, an acct- alias and money
# each fail on their own. Run by scripts/check-all.sh.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

scan() { python3 "$HERE/scan-statusline.py" "$T" 10 > /dev/null; }

# a clean line: 10 cells, with ANSI colour around a CJK pair (2 cells each)
printf '\033[38;2;1;2;3m記憶\033[39m 5h 6%%\n' > "$T/statusline-W10.ansi"
scan || { echo "scan-statusline.test: a clean capture failed" >&2; exit 1; }

fails() {
  printf '%s\n' "$1" > "$T/statusline-W10.ansi"
  if scan; then
    echo "scan-statusline.test: $2 passed the scan" >&2
    exit 1
  fi
}
fails '記憶記 5h 6%' 'a 12-cell line at width 10'
fails 'acct-a 62%' 'an acct- alias'
# shellcheck disable=SC2016 # a literal dollar sign is the point
fails 'cost $1.5' 'money'
printf '\n' > "$T/statusline-W10.ansi"
scan && { echo "scan-statusline.test: an empty capture passed the scan" >&2; exit 1; }
echo "scan-statusline.test: ok"
