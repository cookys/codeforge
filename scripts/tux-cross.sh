#!/usr/bin/env bash
# Shared-TUI plan §4.3: pinned tux bytes and both sides' golden/contract tests.
# Usage: scripts/tux-cross.sh [--release]
# AUTOPILOT_DIR defaults to ../autopilot beside the main clone, including from worktrees.
# Sibling absent or pre-tux: skip in development, failure for release.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
release=0
case "${1:-}" in
  '') [ "$#" = 0 ] || exit 2 ;;
  --release) [ "$#" = 1 ] || exit 2; release=1 ;;
  *) echo 'usage: tux-cross.sh [--release]' >&2; exit 2 ;;
esac
ap=${AUTOPILOT_DIR:-$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)/../../autopilot}
skip() {
  if [ "$release" = 1 ]; then echo "tux-cross: FAIL ($1; release needs the sibling)" >&2; exit 1; fi
  echo "tux-cross: skip: $1"
  exit 0
}
[ -d "$ap" ] || skip "sibling absent ($ap)"
[ -f "$ap/mods/tux/VERSION" ] || skip 'sibling has no tux (mods/tux/VERSION, P4b)'
ap=$(cd "$ap" && pwd)
"$here/tux-lock.sh"
python3 - "$here/../plugins/forge-stats/vendor/tux/VENDOR.lock" "$ap/mods/tux" <<'PY'
import hashlib, json, pathlib, sys
lock = json.loads(pathlib.Path(sys.argv[1]).read_text())
root = pathlib.Path(sys.argv[2])
try:
    assert int((root / 'VERSION').read_text()) == lock['tux_version'], 'tux_version differs; run tux-sync.sh'
    files = {}
    for path in sorted(root.rglob('*')):
        assert not path.is_symlink(), f'sibling symlink: {path.relative_to(root)}'
        if path.is_file():
            files[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    assert files == lock['files'], 'sibling file list or sha256 differs; run tux-sync.sh'
except (OSError, ValueError, AssertionError) as error:
    sys.exit(f'tux-cross: FAIL ({error})')
PY
# Direct node invocation plus a recursion guard for sibling runners that call codeforge back.
(cd "$ap" && TUX_CROSS_RUNNING=1 node --test mods/tux/golden.test.js mods/live/panel-contract.test.js)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp -R "$here/../plugins/forge-stats" "$work/forge-stats"
TUX_GOLDEN_DIR="$ap/mods/tux/golden" python3 "$here/tux-golden-data.py" "$work/forge-stats/tests/tux-golden-data.ts"
TUX_CROSS_RUNNING=1 TUX_GOLDEN_DIR="$ap/mods/tux/golden" claude plugin test "$work/forge-stats"
echo 'tux-cross: ok'
