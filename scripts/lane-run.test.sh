#!/usr/bin/env bash
# All runners and outputs live in temporary directories; no real sessions are started.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
lane=$here/lane-run.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
export HOME="$T/home"
mkdir -p "$HOME"
unset CODEFORGE_LANE_SCRATCH CODEFORGE_LANE_REPO CODEFORGE_LANE_GATE_WORKTREE CODEFORGE_LANE_DISPATCH
checks=0
fail() { printf 'FAIL: %s\n%s\n' "$*" "${output:-}" >&2; exit 1; }
run() {
  local expected=$1 command=$2 rc=0
  shift 2
  output=$(bash "$lane" "$@" 2>&1) || rc=$?
  [[ "$rc" == "$expected" ]] || fail "exit $rc, expected $expected"
  [[ $(tail -n 1 <<<"$output") =~ ^lane-run:\ $command\ rc=$expected\ [0-9]+s$ ]] || fail 'missing final audit'
  [[ $(grep -c '^lane-run: .* rc=' <<<"$output") == 1 ]] || fail 'duplicate audit'
  checks=$((checks + 1))
}
contains() { grep -Fqx -- "$1" <<<"$output" || fail "missing: $1"; }

cat >"$T/result.json" <<'JSON'
{"status":"committed","branch":"chore/example","commit":"abc123","files_changed":2,"insertions":8,"deletions":1,"error":null,"ignored":"hidden"}
JSON
run 0 result result "$T/result.json"
contains 'status: "committed"'
contains 'branch: "chore/example"'
contains 'commit: "abc123"'
contains 'files_changed: 2'
contains 'insertions: 8'
contains 'deletions: 1'
contains 'error: null'
[[ $(wc -l <<<"$output") == 8 ]] || fail 'unexpected result fields'
run 2 result result
printf 'invalid\n' >"$T/bad.json"
run 1 result result "$T/bad.json"
run 1 result result "$T/missing.json"
printf '[]\n' >"$T/bad.json"
run 1 result result "$T/bad.json"
run 2 unknown unknown
run 0 --help --help

mkdir -p "$T/bin" "$T/scratch space/work tree/mods/live"
export PATH="$T/bin:$PATH"
export TEST_WORKTREE="$T/scratch space/work tree"
export TEST_CLAUDE="$T/bin/claude-real"
cat >"$TEST_CLAUDE" <<'SH'
#!/usr/bin/env bash
echo 'FAIL: claude must not be invoked' >&2
exit 99
SH
chmod +x "$TEST_CLAUDE"
ln -s "$TEST_CLAUDE" "$T/bin/claude"
cat >"$TEST_WORKTREE/mods/live/run-tests.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$PWD" == "$TEST_WORKTREE" ]]
[[ "$AUTOPILOT_MODS_REAL_CLAUDE" == "$TEST_CLAUDE" ]]
if read -r unused; then exit 98; fi
printf '(pass) first\n(pass) second\n'
if [[ ${TEST_MODS_RC:-0} != 0 ]]; then
  printf '(fail) speed tab sample\n(fail) other tab\n(fail) another tab\n'
fi
exit "${TEST_MODS_RC:-0}"
SH
run 2 mods-test mods-test "$TEST_WORKTREE"
run 2 mods-test mods-test
run 2 mods-test mods-test --scratch
run 0 mods-test --scratch "$T/scratch space" mods-test 'work tree'
contains 'pass: 2'
contains 'fail: 0'
contains 'non-speed-tab-fail: 0'
export CODEFORGE_LANE_SCRATCH="$T/scratch space"
TEST_MODS_RC=7 run 7 mods-test mods-test 'work tree' --scratch "$T/unused"
contains 'fail: 3'
contains 'non-speed-tab-fail: 2'
run 0 mods-test mods-test 'work tree'
contains 'fail: 0'
[[ $(grep -Ec '^\(pass\)' "$CODEFORGE_LANE_SCRATCH/lane-run/mods-test.log") == 2 ]] || fail 'log accumulated'
run 1 mods-test mods-test missing

export CODEFORGE_LANE_GATE_WORKTREE="$T/gate tree"
mkdir -p "$CODEFORGE_LANE_GATE_WORKTREE/scripts/pane-gate/v74-keys"
cat >"$CODEFORGE_LANE_GATE_WORKTREE/scripts/pane-gate/v74-keys/run.py" <<'PY'
import os
from pathlib import Path
import sys

checkout, version, flag, out = sys.argv[1:]
assert checkout == os.environ["TEST_WORKTREE"]
assert version == os.environ.get("TEST_VERSION", "--v76")
assert flag == "--out"
assert Path.cwd() == Path(os.environ["CODEFORGE_LANE_SCRATCH"])
assert Path(out).is_absolute()
assert not sys.stdin.read()
Path(out).mkdir(parents=True, exist_ok=True)
print("PASS hidden detail")
print("FAIL sample gate step")
print("RESULT RED" if os.environ.get("TEST_GATE_RC") else "RESULT GREEN")
sys.exit(int(os.environ.get("TEST_GATE_RC", "0")))
PY
run 0 gate gate 'work tree' 'gate output'
contains 'RESULT GREEN'
contains 'FAIL sample gate step'
[[ "$output" != *'PASS hidden detail'* ]] || fail 'gate output was not summarized'
TEST_VERSION=--v75 TEST_GATE_RC=9 run 9 gate gate 'work tree' 'gate output' --v75
contains 'RESULT RED'
run 0 gate gate 'work tree' 'gate output' --v76
[[ "$output" != *'RESULT RED'* ]] || fail 'stale gate result'
run 2 gate gate 'work tree' 'gate output' --invalid
run 2 gate gate 'work tree'
CODEFORGE_LANE_GATE_WORKTREE="$T/missing" run 2 gate gate 'work tree' 'gate output'
[[ "$output" == *'set CODEFORGE_LANE_GATE_WORKTREE'* ]] || fail 'missing gate script does not name the variable'

export CODEFORGE_LANE_REPO="$T/codeforge repo"
export CODEFORGE_LANE_DISPATCH="$T/bin/fake-dispatch"
mkdir -p "$CODEFORGE_LANE_REPO"
printf 'fixture brief\n' >"$CODEFORGE_LANE_SCRATCH/brief file"
cat >"$CODEFORGE_LANE_DISPATCH" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$PWD" == "$CODEFORGE_LANE_REPO" ]]
if read -r unused; then exit 98; fi
expected=(--no-campaign --no-verify-tip --runner codex --model gpt-6.1-sol --effort high
  --prompt-file "$CODEFORGE_LANE_SCRATCH/brief file" --branch chore/example --base develop
  --timeout "${TEST_TIMEOUT:-50m}")
[[ "$#" == "${#expected[@]}" ]]
for arg in "${expected[@]}"; do
  [[ "$1" == "$arg" ]]
  shift
done
echo 'runner diagnostics' >&2
if [[ ${TEST_BAD_JSON:-0} == 1 ]]; then echo invalid; exit "${TEST_DISPATCH_RC:-0}"; fi
printf '{"status":"%s","branch":"chore/example","commit":null,"files_changed":0,"insertions":0,"deletions":0,"error":null}\n' "${TEST_STATUS:-no_op}"
exit "${TEST_DISPATCH_RC:-0}"
SH
chmod +x "$CODEFORGE_LANE_DISPATCH"
run 0 dispatch dispatch 'brief file' --branch chore/example --base develop
contains 'status: "no_op"'
contains 'files_changed: 0'
[[ $(cat "$CODEFORGE_LANE_SCRATCH/lane-run/dispatch.log") == 'runner diagnostics' ]] || fail 'missing dispatch stderr'
TEST_TIMEOUT=3m TEST_STATUS=failure TEST_DISPATCH_RC=6 run 6 dispatch dispatch 'brief file' --branch chore/example --base develop --timeout 3m
contains 'status: "failure"'
run 0 dispatch dispatch 'brief file' --branch chore/example --base develop
contains 'status: "no_op"'
run 0 result result "$CODEFORGE_LANE_SCRATCH/lane-run/dispatch.json"
contains 'status: "no_op"'
TEST_BAD_JSON=1 run 1 dispatch dispatch 'brief file' --branch chore/example --base develop
TEST_BAD_JSON=1 TEST_DISPATCH_RC=6 run 6 dispatch dispatch 'brief file' --branch chore/example --base develop
run 2 dispatch dispatch 'brief file' --branch chore/example
run 2 dispatch dispatch 'brief file' --branch chore/example --base
run 2 dispatch dispatch 'brief file' --branch --base develop
run 2 dispatch dispatch 'brief file' --branch chore/example --base develop --unknown
run 2 dispatch dispatch missing --branch chore/example --base develop
run 2 dispatch dispatch

unset CODEFORGE_LANE_DISPATCH
ln -s "$T/bin/fake-dispatch" "$T/bin/dispatch-hetero.sh"
run 0 dispatch dispatch 'brief file' --branch chore/example --base develop
CODEFORGE_LANE_REPO="$T/missing" run 1 dispatch dispatch 'brief file' --branch chore/example --base develop
unset CODEFORGE_LANE_SCRATCH
run 1 mods-test --scratch "$T/missing" mods-test 'work tree'
run 0 mods-test mods-test "$TEST_WORKTREE" --scratch "$T/scratch space"
run 0 result --scratch "$T/missing" result "$T/result.json"

printf 'lane-run.test.sh: %s checks passed\n' "$checks"
