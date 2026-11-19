#!/usr/bin/env bash
# Verify every pinned byte and reject unlisted files (including symlinks).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
python3 - "${TUX_VENDOR_DIR:-$here/../plugins/forge-stats/vendor/tux}" <<'PY'
import hashlib, json, pathlib, re, sys
root = pathlib.Path(sys.argv[1])
try:
    lock = json.loads((root / 'VENDOR.lock').read_text())
    assert re.fullmatch(r'[0-9a-f]{40}', lock['autopilot_sha']), 'invalid autopilot_sha'
    assert type(lock['tux_version']) is int, 'invalid tux_version'
    assert lock['tux_version'] == int((root / 'VERSION').read_text()), 'VERSION differs from lock'
    actual = {}
    for path in sorted(root.rglob('*')):
        assert not path.is_symlink(), f'symlink: {path.relative_to(root)}'
        if path.is_file() and path != root / 'VENDOR.lock':
            actual[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    assert actual == lock['files'], 'file list or sha256 differs from VENDOR.lock'
except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
    sys.exit(f'tux-lock: FAIL ({error})')
print(f'tux-lock: ok ({len(actual)} files, tux {lock["tux_version"]})')
PY

TUX_GOLDEN_DIR="${TUX_VENDOR_DIR:-$here/../plugins/forge-stats/vendor/tux}/golden" \
  python3 "$here/tux-golden-data.py" --check
