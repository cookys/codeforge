#!/usr/bin/env bash
# tux-sync.sh controls against a fixture autopilot repo; the real repo tree is never touched.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

repo="$t/repo"
mkdir -p "$repo/scripts" "$repo/plugins/forge-stats/tests"
cp "$here/tux-sync.sh" "$here/tux-golden-data.py" "$repo/scripts/"

ap="$t/ap"
mkdir -p "$ap/mods/tux/golden"
printf '1\n' >"$ap/mods/tux/VERSION"
printf 'export const fmt = 1\n' >"$ap/mods/tux/format.ts"
printf 'notes\n' >"$ap/mods/tux/notes.txt"
for n in glyphs width-cases pct-cases dur-cases panel-cases; do
  printf '{}\n' >"$ap/mods/tux/golden/$n.json"
done
export GIT_AUTHOR_NAME=Fixture GIT_AUTHOR_EMAIL=fixture@example.com
export GIT_COMMITTER_NAME=Fixture GIT_COMMITTER_EMAIL=fixture@example.com
git -C "$ap" init -q
git -C "$ap" add -A
git -C "$ap" commit -qm A
shaA=$(git -C "$ap" rev-parse HEAD)

# 1. Sync commit A: lock records the sha, version, and the committed format.ts hash.
AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaA" >/dev/null
vendor="$repo/plugins/forge-stats/vendor/tux"
python3 - "$vendor/VENDOR.lock" "$shaA" "$vendor" <<'PY' || fail 'lock does not match commit A'
import hashlib, json, pathlib, sys
lock = json.loads(pathlib.Path(sys.argv[1]).read_text())
vendor = pathlib.Path(sys.argv[3])
actual = {p.relative_to(vendor).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in vendor.rglob('*') if p.is_file() and p.name != 'VENDOR.lock'}
assert lock['autopilot_sha'] == sys.argv[2], lock['autopilot_sha']
assert lock['tux_version'] == 1, lock['tux_version']
assert lock['files'] == actual, (lock['files'], actual)
PY

# 2. Re-running the same sync is byte-identical.
snapshot() {
  (cd "$vendor" && find . -type f -print0 | sort -z | xargs -0 sha256sum)
}
snap1=$(snapshot)
AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaA" >/dev/null
[ "$(snapshot)" = "$snap1" ] || fail 'second sync changed the vendor tree'

# 3. Commit B changes format.ts and deletes notes.txt: vendor matches B.
printf 'export const fmt = 2\n' >"$ap/mods/tux/format.ts"
rm "$ap/mods/tux/notes.txt"
git -C "$ap" add -A
git -C "$ap" commit -qm B
shaB=$(git -C "$ap" rev-parse HEAD)
AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaB" >/dev/null
[ ! -e "$vendor/notes.txt" ] || fail 'deleted notes.txt still in vendor tree'
git -C "$ap" show "$shaB:mods/tux/format.ts" | cmp - "$vendor/format.ts" \
  || fail 'vendor format.ts does not match commit B'
python3 - "$vendor/VENDOR.lock" "$shaB" <<'PY' || fail 'lock does not match commit B'
import json, pathlib, sys
lock = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert lock['autopilot_sha'] == sys.argv[2], lock['autopilot_sha']
assert 'notes.txt' not in lock['files'], 'deleted file still listed'
assert 'format.ts' in lock['files'], 'format.ts missing from lock'
PY

# 4. An uncommitted working-tree edit is not copied.
printf 'tampered\n' >>"$ap/mods/tux/format.ts"
AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaB" >/dev/null
git -C "$ap" show "$shaB:mods/tux/format.ts" | cmp - "$vendor/format.ts" \
  || fail 'uncommitted working-tree edit leaked into vendor tree'
git -C "$ap" checkout -q -- mods/tux/format.ts

# 5. An unknown sha exits non-zero.
! AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" deadbeefdeadbeefdeadbeefdeadbeefdeadbeef \
  >/dev/null 2>&1 || fail 'unknown sha passed'

# 6. A committed symlink under mods/tux is refused.
ln -s format.ts "$ap/mods/tux/link"
git -C "$ap" add -A
git -C "$ap" commit -qm C
shaC=$(git -C "$ap" rev-parse HEAD)
! AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaC" >/dev/null 2>&1 \
  || fail 'committed symlink passed'
rm "$ap/mods/tux/link"

# 7. A committed upstream mods/tux/VENDOR.lock is refused.
printf 'x\n' >"$ap/mods/tux/VENDOR.lock"
git -C "$ap" add -A
git -C "$ap" commit -qm D
shaD=$(git -C "$ap" rev-parse HEAD)
! AUTOPILOT_DIR="$ap" "$repo/scripts/tux-sync.sh" "$shaD" >/dev/null 2>&1 \
  || fail 'upstream VENDOR.lock passed'

echo 'tux-sync.test.sh: ok (7 controls)'
