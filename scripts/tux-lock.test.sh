#!/usr/bin/env bash
# Negative controls use isolated copies; the committed vendor tree is never edited.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
cp -R "$here/../plugins/forge-stats/vendor/tux" "$work/tux"
export TUX_VENDOR_DIR="$work/tux"
"$here/tux-lock.sh"
python3 - "$work/tux/format.ts" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
b = bytearray(p.read_bytes())
b[0] ^= 1
p.write_bytes(b)
PY
! "$here/tux-lock.sh" >/dev/null 2>&1 || fail 'edited vendor byte passed'
cp "$here/../plugins/forge-stats/vendor/tux/format.ts" "$work/tux/format.ts"
printf 'unlisted\n' >"$work/tux/extra"
! "$here/tux-lock.sh" >/dev/null 2>&1 || fail 'unlisted file passed'
rm "$work/tux/extra"
rm "$work/tux/width.ts"
! "$here/tux-lock.sh" >/dev/null 2>&1 || fail 'missing file passed'
cp "$here/../plugins/forge-stats/vendor/tux/width.ts" "$work/tux/width.ts"
ln -s format.ts "$work/tux/link"
! "$here/tux-lock.sh" >/dev/null 2>&1 || fail 'symlink passed'
echo 'tux-lock.test.sh: ok (5 controls)'
