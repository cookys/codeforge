#!/usr/bin/env bash
# scripts/pet-cross.sh's sibling handling: absent or pre-P2 sibling = skip in dev, failure with --release.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

out=$(AUTOPILOT_DIR="$T/none" "$here/pet-cross.sh") || fail "absent sibling failed in dev"
case "$out" in *"skip: sibling absent"*) ;; *) fail "absent sibling: no skip line ($out)" ;; esac
! AUTOPILOT_DIR="$T/none" "$here/pet-cross.sh" --release 2>/dev/null || fail "absent sibling passed --release"

mkdir -p "$T/pre/mods/live"
out=$(AUTOPILOT_DIR="$T/pre" "$here/pet-cross.sh") || fail "pre-P2 sibling failed in dev"
case "$out" in *"skip: sibling has no pet-tab reader"*) ;; *) fail "pre-P2 sibling: no skip line ($out)" ;; esac
! AUTOPILOT_DIR="$T/pre" "$here/pet-cross.sh" --release 2>/dev/null || fail "pre-P2 sibling passed --release"

# A sibling whose reader drops everything must fail (the cross test is not blind).
printf 'export function readPet() { return { pet: null } }\n' >"$T/pre/mods/live/panel.ts"
! AUTOPILOT_DIR="$T/pre" "$here/pet-cross.sh" >/dev/null 2>&1 || fail "a reader that drops the pet passed"
echo "pet-cross.test.sh: ok"
