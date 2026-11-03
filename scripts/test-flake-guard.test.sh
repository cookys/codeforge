#!/usr/bin/env bash
# Tests for scripts/test-flake-guard.sh with a fake cargo. Run: ./scripts/test-flake-guard.test.sh
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
guard="$here/test-flake-guard.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fails=0

# fake cargo: MODE clean | flake | real | build. A rerun is the call with `--exact`.
cat >"$tmp/cargo" <<'EOF'
#!/usr/bin/env bash
rerun=0; for a in "$@"; do [ "$a" = "--exact" ] && rerun=1; done
echo "$*" >>"$FAKE_DIR/calls"
fail() { printf 'failures:\n\n---- a::tests::%s stdout ----\nboom\n\nfailures:\n    a::tests::%s\n\ntest result: FAILED. 9 passed; 1 failed\n' "$1" "$1"; exit 101; }
case "$MODE" in
  clean) echo "test result: ok. 10 passed; 0 failed"; exit 0 ;;
  flake) [ "$rerun" = 1 ] && { echo "test result: ok. 1 passed; 0 failed"; exit 0; }; fail flaky_one ;;
  real) fail broken_one ;;
  build) echo "error[E0308]: mismatched types"; exit 101 ;;
esac
EOF
chmod +x "$tmp/cargo"

run() { # mode → sets out, rc
  rm -f "$tmp/calls" "$tmp/flaky.log" "$tmp"/test-r*.log
  out="$(MODE="$1" FAKE_DIR="$tmp" FLAKE_LOG="$tmp/flaky.log" "$guard" "$tmp/cargo" 2>&1)"
  rc=$?
}
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi; }

run clean
check "clean: exit 0, one call, nothing recorded, no log left" \
  '[ $rc -eq 0 ] && [ "$(wc -l <"$tmp/calls")" -eq 1 ] && [ ! -e "$tmp/flaky.log" ] && [ -z "$(ls "$tmp"/test-r*.log 2>/dev/null)" ]'

run flake
check "flake: exit 0 after one rerun of exactly the failed test" \
  '[ $rc -eq 0 ] && grep -qx "test -- --exact a::tests::flaky_one" "$tmp/calls"'
check "flake: recorded in the log and on stderr, first-run log kept" \
  'grep -q "FLAKY a::tests::flaky_one log=" "$tmp/flaky.log" && [[ "$out" == *"FLAKY: a::tests::flaky_one"* ]] && [ -n "$(ls "$tmp"/test-run-*.log)" ]'

run real
check "real: exit 1, both logs kept, nothing recorded as flaky" \
  '[ $rc -eq 1 ] && [ ! -e "$tmp/flaky.log" ] && [ -n "$(ls "$tmp"/test-rerun-*.log)" ] && [[ "$out" == *"real failure"* ]]'

run build
check "build error: its exit code, no rerun" \
  '[ $rc -eq 101 ] && [ "$(wc -l <"$tmp/calls")" -eq 1 ] && [[ "$out" == *"without a failed test name"* ]]'

[ "$fails" -eq 0 ] && echo "test-flake-guard: all green" || { echo "test-flake-guard: $fails failed"; exit 1; }
