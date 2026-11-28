#!/usr/bin/env bash
# toolchain.test.sh — controls for the pane capture + contrast toolchain (B78).
# Sandbox only: no tmux pane is launched here (that is capture.py's job, driven by hand);
# these controls pin the measurement math and the env probe so two rounds of numbers stay comparable.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
gate="$repo/scripts/pane-gate"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
controls=0

fail() { echo "toolchain.test.sh: $*" >&2; exit 1; }

# 1) env probe: tmux present, harness + patch resolvable from the repo (no scratchpad paths).
out="$(python3 -I "$gate/capture.py" --check-env)" || fail "capture.py --check-env exited non-zero (tmux or harness missing)"
grep -q '^harness .*/v74-keys True$' <<<"$out" || fail "harness not resolved from the repo: $out"
grep -q '^patch .*/patch_awaiting.py True$' <<<"$out" || fail "patch_awaiting.py not resolved from the repo: $out"
grep -q '^TERM ' <<<"$out" || fail "env probe lost the TERM line"
grep -q '^size 209x49$' <<<"$out" || fail "check-env lost the default launch size (B83): $out"
sz="$(SZ=120x40 python3 -I "$gate/capture.py" --check-env)"
grep -q '^size 120x40$' <<<"$sz" || fail "SZ=120x40 not honored by the size probe (B83): $sz"
if SZ=notasize python3 -I "$gate/capture.py" --check-env >/dev/null 2>&1; then
  fail "SZ=notasize must exit non-zero"
fi
controls=$((controls + 1))

# 2) contrast256: fixed 256-colour fixture, both backgrounds, pinned ratios.
#    fg 231 (white), fg 16 (black), bg 232 (near-black), plus one truecolor white.
printf 'x\033[38;5;231mw\033[38;5;16mk\033[48;5;232mb\033[38;2;255;255;255mt\033[0m\n' >"$tmp/fixture.ans"
dark="$(python3 -I "$gate/contrast256.py" "$tmp/fixture.ans" dark)"
grep -q '抽出 3 色' <<<"$dark" || fail "dark parse lost colours: $dark"
grep -q '#ffffff (code rgb) = 16.67' <<<"$dark" || fail "white on the v7.4 dark base is no longer 16.67: $dark"
grep -q '#000000 (code 16) = 1.26' <<<"$dark" || fail "black on dark base is no longer 1.26: $dark"
light="$(python3 -I "$gate/contrast256.py" "$tmp/fixture.ans" light)"
grep -q '#000000 (code 16) = 21.00' <<<"$light" || fail "black on white is no longer 21.00: $light"
grep -q '低於 3.0' <<<"$dark" || fail "dark report lost the sub-3.0 list"
controls=$((controls + 1))

# 3) cjk-width: an over-wide CJK frame row must be flagged, a short row must not.
#    The width comes from the capture name, so B80's real shot sizes (209x49, 80x24) are scanned too.
mkdir -p "$tmp/120x40"
long=""
for _ in $(seq 1 62); do long="${long}訓"; done   # 62*2 = 124 display columns > 120
printf '│ %s ║\n│ 短列 ‣ OK ║\n' "$long" >"$tmp/120x40/KR-120x40.txt"
w="$(python3 -I "$gate/cjk-width.py" "$tmp")"
grep -q '超寬列=\[(1, 128)\]' <<<"$w" || fail "over-wide row 1 (128 cols incl. frame) not flagged: $w"
grep -q '120x40/KR-120x40.txt' <<<"$w" || fail "fixture not scanned: $w"
mkdir -p "$tmp/209x49" "$tmp/80x24"
wide=""
for _ in $(seq 1 110); do wide="${wide}訓"; done   # 110*2 + 4 single-width frame/space = 224 columns > 209
printf '│ %s ║\n' "$wide" >"$tmp/209x49/A-209x49.txt"
printf '│ 短列 ‣ OK ║\n' >"$tmp/80x24/A-80x24.txt"
w2="$(python3 -I "$gate/cjk-width.py" "$tmp")"
grep -q '209x49/A-209x49.txt  pane=209' <<<"$w2" || fail "209x49 shot size not scanned (B82 regression): $w2"
grep -q '超寬列=\[(1, 224)\]' <<<"$w2" || fail "224-col row at width 209 not flagged: $w2"
grep -q '80x24/A-80x24.txt  pane=80.*超寬列=無' <<<"$w2" || fail "short row at 80x24 falsely flagged: $w2"
# Flat layout (the real capture out-dir writes files at the top level, not per-size subdirs).
printf '│ %s ║\n' "$long" >"$tmp/F-120x40.txt"
w3="$(python3 -I "$gate/cjk-width.py" "$tmp")"
grep -q 'F-120x40.txt  pane=120.*超寬列=\[(1, 128)\]' <<<"$w3" || fail "flat-layout over-wide row not scanned (B84): $w3"
controls=$((controls + 1))

# 4) xterm 256 palette pins (the quantization table the contrast numbers depend on).
printf '\033[38;5;196mr\033[38;5;231mw\033[0m\n' >"$tmp/palette.ans"
p196="$(python3 -I "$gate/contrast256.py" "$tmp/palette.ans" dark)"
grep -q '#ff0000 (code 196)' <<<"$p196" || fail "code 196 no longer maps to pure red: $p196"
grep -q '#ffffff (code 231)' <<<"$p196" || fail "code 231 no longer maps to white: $p196"
controls=$((controls + 1))

# 5) post-shoot report (B84): --report aggregates cjk widths + per-shot contrast into report.txt.
mkdir -p "$tmp/rep"
printf '│ 短列 ‣ OK ║\n' >"$tmp/rep/A-120x40.txt"
cp "$tmp/fixture.ans" "$tmp/rep/A-120x40.ans"
rp="$(python3 -I "$gate/capture.py" --report "$tmp/rep" dark)" || fail "--report exited non-zero: $rp"
grep -q '^report ok$' <<<"$rp" || fail "--report did not report ok: $rp"
test -f "$tmp/rep/report.txt" || fail "--report wrote no report.txt"
r="$(cat "$tmp/rep/report.txt")"
grep -q '^== cjk-width ==' <<<"$r" || fail "report missing the cjk section: $r"
grep -q 'A-120x40.txt  pane=120.*超寬列=無' <<<"$r" || fail "report missing the width row: $r"
grep -q '^== contrast A-120x40.ans \[dark\] ==$' <<<"$r" || fail "report missing the contrast section: $r"
grep -q '#ffffff (code rgb) = 16.67' <<<"$r" || fail "report lost the pinned ratio: $r"
if python3 -I "$gate/capture.py" --report "$tmp/rep" bogus >/dev/null 2>&1; then
  fail "--report must reject a theme other than light|dark"
fi
controls=$((controls + 1))

echo "toolchain.test.sh: ok ($controls controls)"
