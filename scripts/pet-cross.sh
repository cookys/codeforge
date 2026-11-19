#!/usr/bin/env bash
# pet-cross (shared-TUI plan §4.3, P3 until tux-cross replaces it at P5): forge-stats' panel/1 against
# autopilot's pet-tab reader in the sibling checkout, plus autopilot's own pettab tests.
#
# Usage: scripts/pet-cross.sh [--release]
#   AUTOPILOT_DIR  the autopilot checkout (default: ../autopilot next to the main clone)
# Sibling absent, or without the P2 reader (mods/live/panel.ts): prints "skip: ..." and exits 0;
# with --release (hangar live pin, release-local.sh) that is a failure (exit 1).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
release=0
[ "${1:-}" = --release ] && release=1
# Default: ../autopilot beside the main clone (also from a worktree, whose own parent is .claude/worktrees).
ap=${AUTOPILOT_DIR:-$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)/../../autopilot}

skip() {
  if [ "$release" = 1 ]; then echo "pet-cross: FAIL ($1; release needs the sibling)" >&2; exit 1; fi
  echo "pet-cross: skip: $1"
  exit 0
}
[ -d "$ap/mods/live" ] || skip "sibling absent ($ap)"
[ -f "$ap/mods/live/panel.ts" ] || skip "sibling has no pet-tab reader (mods/live/panel.ts, P2)"

PET_CROSS_AP=$(cd "$ap" && pwd) node --test "$here/pet-cross/cross.test.mjs"
if [ -f "$ap/mods/live/pettab.test.js" ]; then node --test "$ap/mods/live/pettab.test.js"; fi
if [ -f "$ap/mods/live/panel-contract.test.js" ]; then node --test "$ap/mods/live/panel-contract.test.js"; fi
echo "pet-cross: ok"
