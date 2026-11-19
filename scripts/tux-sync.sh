#!/usr/bin/env bash
# Pin a committed autopilot mods/tux tree; never read its working-tree files.
# Usage: AUTOPILOT_DIR=/path/to/autopilot scripts/tux-sync.sh <autopilot-sha>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
ap=${AUTOPILOT_DIR:-$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)/../../autopilot}
[ "$#" = 1 ] || { echo 'usage: tux-sync.sh <autopilot-sha>' >&2; exit 2; }
sha=$(git -C "$ap" rev-parse --verify "$1^{commit}")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git -C "$ap" archive "$sha" mods/tux/ | tar -xf - -C "$work"
python3 - "$work/mods/tux" "$sha" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
version = int((root / 'VERSION').read_text().strip())
files = {}
for path in sorted(root.rglob('*')):
    if path.is_symlink():
        raise SystemExit(f'tux-sync: refusing symlink {path}')
    if path.is_file():
        files[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
if 'VENDOR.lock' in files:
    raise SystemExit('tux-sync: upstream tree contains VENDOR.lock')
lock = dict(autopilot_sha=sys.argv[2], tux_version=version, files=files)
(root / 'VENDOR.lock').write_text(json.dumps(lock, indent=2) + '\n')
PY
vendor="$here/../plugins/forge-stats/vendor/tux"
mkdir -p "$(dirname "$vendor")"
rm -rf "$vendor"
mv "$work/mods/tux" "$vendor"
TUX_GOLDEN_DIR="$vendor/golden" python3 "$here/tux-golden-data.py"
echo "tux-sync: ok ($sha)"
