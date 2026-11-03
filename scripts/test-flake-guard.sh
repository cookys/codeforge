#!/usr/bin/env bash
# cargo test with a flake guard: a failure is rerun once, and never hidden.
#
#   ./scripts/test-flake-guard.sh [cargo command...]     # default: cargo
#   ./scripts/test-flake-guard.sh cargo +stable          # e.g. a toolchain override
#
# Runs `<cargo command> test`. If tests fail, only the failed tests are rerun once
# (`test -- --exact <names>`):
#   - they fail again   → a real failure: both logs are printed and kept, exit 1.
#   - they now pass     → a flake: one line per test is appended to the flake log, the
#                         names go to stderr, the first run's full log is kept, exit 0.
#   - no test name in the output (a build error, a crashed binary) → exit with its code.
# A clean first run leaves no log behind.
#
# Env:
#   FLAKE_LOG   where flakes are recorded (default: target/flaky-tests.log)
set -uo pipefail

cd "$(dirname "$0")/.."

cmd=("$@")
[ "${#cmd[@]}" -eq 0 ] && cmd=(cargo)
log="${FLAKE_LOG:-target/flaky-tests.log}"
dir="$(dirname "$log")"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
head_sha="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
first="$dir/test-run-$stamp.log"
second="$dir/test-rerun-$stamp.log"
mkdir -p "$dir"

"${cmd[@]}" test >"$first" 2>&1
rc=$?
grep -E '^test result' "$first" || true
if [ "$rc" -eq 0 ]; then
  rm -f "$first"
  exit 0
fi

# the failed tests, as libtest names them: `---- <name> stdout ----`
mapfile -t failed < <(sed -n 's/^---- \(.*\) stdout ----$/\1/p' "$first" | sort -u)
if [ "${#failed[@]}" -eq 0 ]; then
  echo "test-flake-guard: cargo test failed (exit $rc) without a failed test name; log: $first" >&2
  tail -n 40 "$first" >&2
  exit "$rc"
fi

echo "test-flake-guard: ${#failed[@]} failed, rerunning once: ${failed[*]}" >&2
"${cmd[@]}" test -- --exact "${failed[@]}" >"$second" 2>&1
rc2=$?
if [ "$rc2" -ne 0 ]; then
  echo "test-flake-guard: failed again — a real failure. Logs: $first $second" >&2
  sed -n '/^failures:$/,$p' "$second" >&2
  exit 1
fi

rm -f "$second"
for name in "${failed[@]}"; do
  printf '%s %s FLAKY %s log=%s\n' "$stamp" "$head_sha" "$name" "$first" >>"$log"
  echo "FLAKY: $name failed once and passed on rerun (recorded in $log; first run: $first)" >&2
done
exit 0
