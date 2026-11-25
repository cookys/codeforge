#!/usr/bin/env bash
set -euo pipefail
history="${PANE_HISTORY_ROOT:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)}"
python3 - "$history" "$@" <<'PY'
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys

root = Path(sys.argv[1]).resolve()
parser = argparse.ArgumentParser(description='Append an autopilot pane version and rebuild the history')
parser.add_argument('ap_sha')
parser.add_argument('--no-shoot', action='store_true', help='skip capture (also permits a synthetic full SHA for the isolated test)')
args = parser.parse_args(sys.argv[2:])
if not re.fullmatch(r'[0-9a-f]{7,40}', args.ap_sha):
    parser.error('ap_sha must be a hexadecimal commit id')
ap_repo = os.environ.get('PANE_HISTORY_AP_REPO', os.path.expanduser('~/projects/autopilot-dev'))
resolved = subprocess.run(['git','-C',ap_repo,'rev-parse','--verify',args.ap_sha+'^{commit}'], capture_output=True,text=True)
if resolved.returncode:
    if not args.no_shoot or len(args.ap_sha) != 40:
        parser.error('autopilot commit is unavailable; no changes written')
    sha = args.ap_sha
    changed = '未拍攝追加（合成測試 sha）'
    date = datetime.date.today().isoformat()
else:
    sha = resolved.stdout.strip()
    changed = subprocess.check_output(['git','-C',ap_repo,'show','-s','--format=%B',sha],text=True).strip()
    date = subprocess.check_output(['git','-C',ap_repo,'show','-s','--format=%cs',sha],text=True).strip()
path = root / 'versions.json'
versions = json.loads(path.read_text())
if any(v.get('ap_sha') and (sha.startswith(v['ap_sha']) or v['ap_sha'].startswith(sha)) for v in versions):
    print('already present: '+sha)
    sys.exit(0)
shots = {'dark': None, 'light': None}
if not args.no_shoot:
    for theme in shots:
        # shoot.py owns the four-minute timeout and appends failures before returning.
        subprocess.run([sys.executable,str(root/'tools/shoot.py'),sha,'--theme',theme,'--out',str(root/'shots')],check=False)
        shot = Path('shots') / sha[:9] / (theme+'.ans')
        if (root/shot).is_file():
            shots[theme] = str(shot)
versions.append(dict(id='ap-'+sha[:9],name=changed.splitlines()[0],plain='新增面板版本',ap_sha=sha,date=date,kind='release',changed=changed,principles=['?'],operator_feedback=[],shots=shots,sources=['autopilot commit '+sha]))
# Write atomically, then use the same deterministic builder as the checked-in page.
tmp = path.with_suffix('.json.tmp')
tmp.write_text(json.dumps(versions,ensure_ascii=False,indent=2)+'\n')
tmp.replace(path)
subprocess.run([sys.executable,str(root/'tools/build.py')],check=True)
print('appended: '+sha)
PY
