#!/usr/bin/env bash
# G40 real-data check: run the statusline and forge-stats' panel/1 against one autopilot live dir,
# at the five band widths, and refuse a line that is too wide, shows an `acct-` alias, or (statusline,
# panel side) shows money. Built so the day real mirror data arrives (operator card d-2a19fdb5) it is one command.
#
#   scripts/g40-real-data/check.sh [<live_dir>]   a live dir holding quota/ (and codeforge/usage/summary.json)
#   scripts/g40-real-data/check.sh                 a synthetic fixture (make-fixture.py), times relative to now
#   scripts/g40-real-data/check.sh --negative      the fixture with an acct- alias planted: must exit non-zero
#
# The live dir is copied into a scratch sandbox first; HOME and codeforge's data dirs point into the sandbox, so
# nothing is written to the real live dir or the real home. Captures go to target/g40-real-data/ (gitignored).
# Needs: cargo, python3, claude.
set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT=$(pwd)
HERE="$ROOT/scripts/g40-real-data"
WIDTHS=(80 120 160 200 240)

NEGATIVE=0
SRC=""
for a in "$@"; do
  case "$a" in
    --negative) NEGATIVE=1 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) SRC="$a" ;;
  esac
done
if [ "$NEGATIVE" = 1 ] && [ -n "$SRC" ]; then
  echo "g40-real-data: --negative plants into the fixture; it takes no live dir" >&2
  exit 2
fi

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
OUT="$ROOT/target/g40-real-data"
rm -rf "$OUT"
mkdir -p "$OUT" "$T/home" "$T/data" "$T/proj"

# ---- the live dir, copied into the sandbox ----
if [ -n "$SRC" ]; then
  if [ ! -d "$SRC/quota" ]; then
    echo "g40-real-data: $SRC has no quota/ directory" >&2
    exit 2
  fi
  mkdir -p "$T/live/codeforge/usage"
  cp -a "$SRC/quota" "$T/live/quota"
  [ -f "$SRC/codeforge/usage/summary.json" ] && cp -a "$SRC/codeforge/usage/summary.json" "$T/live/codeforge/usage/"
  [ -f "$SRC/codeforge/pet.json" ] && cp -a "$SRC/codeforge/pet.json" "$T/live/codeforge/"
  echo "g40-real-data: live dir $SRC (copied into a sandbox)"
else
  if [ "$NEGATIVE" = 1 ]; then
    python3 "$HERE/make-fixture.py" "$T/live" --negative
    echo "g40-real-data: synthetic fixture, NEGATIVE control (an acct- alias planted)"
  else
    python3 "$HERE/make-fixture.py" "$T/live"
    echo "g40-real-data: synthetic fixture"
  fi
fi

FAIL=0

# ---- statusline at the five widths ----
cargo build -q
BIN="$ROOT/target/debug/codeforge"
for w in "${WIDTHS[@]}"; do
  env -i PATH="$PATH" HOME="$T/home" AUTOPILOT_LIVE_DIR="$T/live" CODEFORGE_DATA_DIR="$T/data" \
    CODEFORGE_DIR="$T/proj/.codeforge" COLUMNS="$w" \
    "$BIN" statusline < "$ROOT/tests/fixtures/statusline-p0.json" > "$OUT/statusline-W$w.ansi"
done
if ! python3 "$HERE/scan-statusline.py" "$OUT" "${WIDTHS[@]}"; then
  FAIL=1
fi

# ---- forge-stats panel/1, from a scratch copy of the plugin ----
cp -a "$ROOT/plugins/forge-stats" "$T/plugin"
find "$T/plugin/tests" \( -name '*.test.ts' -o -name '*.test.tsx' \) -delete
cp "$HERE/real-data.test.tsx" "$T/plugin/tests/"
python3 "$HERE/input-module.py" "$T/live" > "$T/plugin/tests/real-data.input.ts"
set +e
claude plugin test "$T/plugin" > "$OUT/forge-stats.log" 2>&1
TS=$?
set -e
if [ "$TS" -ne 0 ]; then
  echo "g40-real-data: forge-stats panel/1 FAILED (log: target/g40-real-data/forge-stats.log)"
  grep -E '\(fail\)|Expected|Received|too wide|acct-|money' "$OUT/forge-stats.log" | head -20 || true
  FAIL=1
else
  echo "g40-real-data: forge-stats panel/1 ok"
fi

if [ "$FAIL" -ne 0 ]; then
  echo "g40-real-data: FAILED"
  exit 1
fi
echo "g40-real-data: all checks green (captures in target/g40-real-data/)"
