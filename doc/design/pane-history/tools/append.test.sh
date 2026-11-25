#!/usr/bin/env bash
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/tools"
cp "$here/build.py" "$scratch/tools/"
python3 - "$here/../versions.json" "$scratch" <<'PY'
import json,sys
from pathlib import Path
rows=json.loads(Path(sys.argv[1]).read_text())
row=next(v for v in rows if v.get('ap_sha','') and v['ap_sha'].startswith('6db7e47e'))
row['shots']={'dark':None,'light':None}
out=Path(sys.argv[2]);(out/'versions.json').write_text(json.dumps([row]));(out/'principles.json').write_text('[]')
PY
fake=0123456789abcdef0123456789abcdef01234567
PANE_HISTORY_ROOT="$scratch" "$here/append.sh" "$fake" --no-shoot
cp "$scratch/versions.json" "$scratch/first.json"
cp "$scratch/index.html" "$scratch/first.html"
PANE_HISTORY_ROOT="$scratch" "$here/append.sh" "$fake" --no-shoot
cmp "$scratch/first.json" "$scratch/versions.json"
cmp "$scratch/first.html" "$scratch/index.html"
python3 - "$scratch/versions.json" "$fake" <<'PY'
import json,sys
from pathlib import Path
v=json.loads(Path(sys.argv[1]).read_text());assert len(v)==2
row=v[-1];assert row['ap_sha']==sys.argv[2];assert row['principles']==['?'];assert row['shots']=={'dark':None,'light':None}
print('PASS synthetic SHA: one append, second invocation leaves JSON and HTML unchanged')
PY
if PANE_HISTORY_ROOT="$scratch" "$here/append.sh" invalid --no-shoot 2>/dev/null; then
    echo 'FAIL invalid SHA accepted' >&2
    exit 1
fi
cmp "$scratch/first.json" "$scratch/versions.json"
echo 'PASS invalid SHA rejected without mutation'
