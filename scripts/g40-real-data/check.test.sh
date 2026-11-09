#!/usr/bin/env bash
# check.sh resolves a relative <live_dir> against the caller's cwd, not the repo root it cd's into.
# G40_STOP_AFTER_COPY=1 makes check.sh exit once the live dir is copied (no build, no claude).
# Run by scripts/check-all.sh.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/work/live/quota" "$T/work/empty"

out=$(cd "$T/work" && G40_STOP_AFTER_COPY=1 "$HERE/check.sh" ./live 2>&1) || {
  echo "check.test: relative ./live from another cwd failed: $out" >&2
  exit 1
}
case "$out" in
  *"live dir $T/work/live "*) ;;
  *) echo "check.test: relative live dir not resolved against the caller's cwd: $out" >&2; exit 1 ;;
esac

# an exported CDPATH never sends a bare relative name elsewhere
mkdir -p "$T/decoy/live/quota"
out=$(cd "$T/work" && CDPATH="$T/decoy" G40_STOP_AFTER_COPY=1 "$HERE/check.sh" live 2>&1) || {
  echo "check.test: bare live with CDPATH failed: $out" >&2
  exit 1
}
case "$out" in
  *"live dir $T/work/live "*) ;;
  *) echo "check.test: CDPATH changed where a bare live dir resolves: $out" >&2; exit 1 ;;
esac

# a dir without quota/ still errors with the user's own path
if out=$(cd "$T/work" && G40_STOP_AFTER_COPY=1 "$HERE/check.sh" ./empty 2>&1); then
  echo "check.test: a live dir without quota/ passed" >&2
  exit 1
fi
case "$out" in
  *"./empty has no quota/ directory"*) ;;
  *) echo "check.test: the error does not name the user's path: $out" >&2; exit 1 ;;
esac
echo "check.test: ok"
