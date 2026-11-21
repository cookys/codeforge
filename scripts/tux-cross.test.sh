#!/usr/bin/env bash
# Isolated sibling fixtures: skips, release failures, byte/version drift and runner failures.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
out=$(AUTOPILOT_DIR="$work/none" "$here/tux-cross.sh") || fail 'absent sibling failed in dev'
case "$out" in *'skip: sibling absent'*) ;; *) fail "no absent skip ($out)" ;; esac
! AUTOPILOT_DIR="$work/none" "$here/tux-cross.sh" --release 2>/dev/null || fail 'absent sibling passed release'
mkdir -p "$work/ap/mods/live"
out=$(AUTOPILOT_DIR="$work/ap" "$here/tux-cross.sh") || fail 'pre-tux sibling failed in dev'
case "$out" in *'skip: sibling has no tux'*) ;; *) fail "no pre-tux skip ($out)" ;; esac
! AUTOPILOT_DIR="$work/ap" "$here/tux-cross.sh" --release 2>/dev/null || fail 'pre-tux sibling passed release'
cp -R "$here/../plugins/forge-stats/vendor/tux" "$work/ap/mods/tux"
rm "$work/ap/mods/tux/VENDOR.lock"
# Only test orchestration here; the real sibling tests are exercised by tux-cross itself.
mkdir "$work/bin"
cat >"$work/bin/node" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[ "$PWD" = "$AUTOPILOT_DIR" ]
[ "${TUX_CROSS_RUNNING:-}" = 1 ]
[ "$*" = '--test mods/tux/golden.test.js mods/live/panel-contract.test.js' ]
[ "${TUX_TEST_FAIL:-}" != node ]
STUB
cat >"$work/bin/claude" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[ "${TUX_CROSS_RUNNING:-}" = 1 ]
[ "${TUX_GOLDEN_DIR:-}" = "$AUTOPILOT_DIR/mods/tux/golden" ]
[ "$1 $2" = 'plugin test' ]
[ "${TUX_TEST_FAIL:-}" != claude ]
STUB
chmod +x "$work/bin/node" "$work/bin/claude"
export PATH="$work/bin:$PATH" AUTOPILOT_DIR="$work/ap"
out=$("$here/tux-cross.sh" --release) || fail 'matching sibling failed release'
case "$out" in *'tux-cross: ok'*) ;; *) fail "no ok ($out)" ;; esac
printf '%s\n' "$(($(cat "$here/../plugins/forge-stats/vendor/tux/VERSION") + 1))" >"$work/ap/mods/tux/VERSION"
! "$here/tux-cross.sh" >/dev/null 2>&1 || fail 'version drift passed'
cp "$here/../plugins/forge-stats/vendor/tux/VERSION" "$work/ap/mods/tux/VERSION"
printf '\n' >>"$work/ap/mods/tux/format.ts"
! "$here/tux-cross.sh" >/dev/null 2>&1 || fail 'byte drift passed'
cp "$here/../plugins/forge-stats/vendor/tux/format.ts" "$work/ap/mods/tux/format.ts"
printf 'extra\n' >"$work/ap/mods/tux/extra"
! "$here/tux-cross.sh" >/dev/null 2>&1 || fail 'unlisted sibling file passed'
rm "$work/ap/mods/tux/extra"
! TUX_TEST_FAIL=node "$here/tux-cross.sh" >/dev/null 2>&1 || fail 'node failure passed'
! TUX_TEST_FAIL=claude "$here/tux-cross.sh" >/dev/null 2>&1 || fail 'plugin failure passed'
echo 'tux-cross.test.sh: ok (10 controls)'
