#!/usr/bin/env bash
# One entry point for recurring lane commands; paths are never session-specific.
set -euo pipefail

subcmd=usage
started=$SECONDS
audit() {
  local rc=$?
  printf 'lane-run: %s rc=%s %ss\n' "$subcmd" "$rc" "$((SECONDS - started))"
}
trap audit EXIT

usage() {
  cat <<'USAGE'
Usage: lane-run.sh [--scratch DIR] <command> [arguments]
  mods-test <worktree>
  gate <checkout> <out-dir> [--v76|--v75]   (default: --v76)
  dispatch <brief-file> --branch B --base BASE [--timeout 50m]
  result <dispatch-json>
CODEFORGE_LANE_SCRATCH takes precedence over --scratch (required except for result).
Worktree, checkout, out-dir and brief paths are relative to scratch; result paths
are relative to the caller. Absolute paths work in either case.
CODEFORGE_LANE_REPO: codeforge checkout (default: this script's repo).
CODEFORGE_LANE_GATE_WORKTREE: repo containing the gate (default: codeforge checkout).
CODEFORGE_LANE_DISPATCH: dispatcher script (default: dispatch-hetero.sh on PATH).
Outputs are overwritten under <scratch>/lane-run/: mods-test.log, gate.log,
dispatch.json (stdout) and dispatch.log (stderr). Runner exit codes are preserved.
USAGE
}

die() { printf 'lane-run: %s\n' "$*" >&2; exit 2; }

S=''
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
scratch() {
  S=${CODEFORGE_LANE_SCRATCH:-$1}
  [[ -n "$S" ]] || die 'set CODEFORGE_LANE_SCRATCH or pass --scratch DIR'
  cd -- "$S"
  S=$PWD
  mkdir -p "$S/lane-run"
}

mods_test() {
  local worktree=$1 claude_bin log=$S/lane-run/mods-test.log rc=0
  claude_bin=$(command -v claude) || die 'claude is not on PATH'
  claude_bin=$(readlink -f -- "$claude_bin")
  (
    cd -- "$worktree" || exit
    AUTOPILOT_MODS_REAL_CLAUDE=$claude_bin bash mods/live/run-tests.sh
  ) </dev/null >"$log" 2>&1 || rc=$?
  printf 'pass: %s\n' "$(grep -Ec '^\(pass\)' "$log" || true)"
  printf 'fail: %s\n' "$(grep -Ec '^\(fail\)' "$log" || true)"
  printf 'non-speed-tab-fail: %s\n' "$(grep -E '^\(fail\)' "$log" | grep -vc 'speed tab' || true)"
  return "$rc"
}

gate() {
  local checkout out version=${3:---v76} rc=0 log=$S/lane-run/gate.log
  local gate_worktree=${CODEFORGE_LANE_GATE_WORKTREE:-$repo}
  [[ "$version" == --v76 || "$version" == --v75 ]] || die 'gate expects --v76 or --v75'
  checkout=$(readlink -f -- "$1")
  out=$(readlink -m -- "$2")
  [[ -f "$gate_worktree/scripts/pane-gate/v74-keys/run.py" ]] \
    || die "gate script not found under $gate_worktree: set CODEFORGE_LANE_GATE_WORKTREE to the worktree that holds scripts/pane-gate/v74-keys/run.py"
  python3 "$gate_worktree/scripts/pane-gate/v74-keys/run.py" "$checkout" "$version" --out "$out" \
    </dev/null >"$log" 2>&1 || rc=$?
  grep -E '(^|[[:space:]])(FAIL|RESULT)([[:space:]]|$)' "$log" | tail -n 20 || true
  return "$rc"
}

dispatch() {
  local brief=$1 branch='' base='' timeout=50m rc=0 summary_rc=0
  local dispatcher=${CODEFORGE_LANE_DISPATCH:-dispatch-hetero.sh}
  local json=$S/lane-run/dispatch.json log=$S/lane-run/dispatch.log
  shift
  while (($#)); do
    case "$1" in
      --branch|--base|--timeout)
        (($# >= 2)) && [[ -n "$2" && "$2" != --* ]] || die "$1 requires a value"
        case "$1" in
          --branch) branch=$2 ;;
          --base) base=$2 ;;
          --timeout) timeout=$2 ;;
        esac
        shift 2
        ;;
      *) die "unknown dispatch option: $1" ;;
    esac
  done
  [[ -n "$branch" && -n "$base" ]] || die 'dispatch requires --branch and --base'
  [[ -f "$brief" ]] || die "brief file not found: $brief"
  brief=$(readlink -f -- "$brief")
  if [[ "$dispatcher" != */* ]]; then
    dispatcher=$(command -v "$dispatcher") || die 'dispatch-hetero.sh is not on PATH; set CODEFORGE_LANE_DISPATCH'
  fi
  dispatcher=$(readlink -f -- "$dispatcher")
  (
    cd -- "$repo" || exit
    bash "$dispatcher" --no-campaign --no-verify-tip --runner codex --model gpt-6.1-sol --effort high \
      --prompt-file "$brief" --branch "$branch" --base "$base" --timeout "$timeout"
  ) </dev/null >"$json" 2>"$log" || rc=$?
  result "$json" || summary_rc=$?
  if ((rc == 0)); then rc=$summary_rc; fi
  return "$rc"
}

result() {
  python3 - "$1" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as source:
    result = json.load(source)
if not isinstance(result, dict):
    sys.exit("lane-run: dispatch result must be a JSON object")
for key in ("status", "branch", "commit", "files_changed", "insertions", "deletions", "error"):
    print(f"{key}: {json.dumps(result.get(key), ensure_ascii=False)}")
PY
}

main() {
  local scratch_arg='' args=()
  while (($#)); do
    case "$1" in
      --scratch)
        (($# >= 2)) && [[ -n "$2" ]] || die '--scratch requires a directory'
        scratch_arg=$2
        shift 2
        ;;
      *)
        if ((${#args[@]} == 0)); then subcmd=$1; fi
        args+=("$1")
        shift
        ;;
    esac
  done
  set -- "${args[@]}"
  subcmd=${1:-usage}
  (($#)) || { usage; return 2; }
  shift
  repo=${CODEFORGE_LANE_REPO:-$repo}
  case "$subcmd" in
    -h|--help) usage ;;
    mods-test)
      (($# == 1)) || die 'mods-test requires one worktree'
      scratch "$scratch_arg"
      mods_test "$1"
      ;;
    gate)
      (($# == 2 || $# == 3)) || die 'gate requires checkout, out-dir and optional version'
      scratch "$scratch_arg"
      gate "$@"
      ;;
    dispatch)
      (($# >= 1)) || die 'dispatch requires a brief file'
      scratch "$scratch_arg"
      dispatch "$@"
      ;;
    result)
      (($# == 1)) || die 'result requires one JSON file'
      result "$1"
      ;;
    *) die "unknown command: $subcmd" ;;
  esac
}

main "$@"
