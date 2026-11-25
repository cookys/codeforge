#!/usr/bin/env python3
"""Validate the history catalog, exact citations, reciprocal links and capture receipts."""
import importlib.util
import json, re
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parent.parent
REPO = ROOT.parents[2]
principles = json.loads((ROOT/'principles.json').read_text())
versions = json.loads((ROOT/'versions.json').read_text())
assert len({p['id'] for p in principles}) == len(principles)
assert len({v['id'] for v in versions}) == len(versions)
pids = {p['id'] for p in principles}
assert {p['in_R'] for p in principles} >= {f'R{i}' for i in range(1,8)}
for p in principles:
    assert p['status'] in ('有原話','缺出處')
    assert p['status'] != '有原話' or p['quotes']
    assert set(p['versions']) == {v['id'] for v in versions if p['id'] in v['principles']}
    for q in p['quotes']:
        if re.match(r'PM [0-9A-Z]{8}', q['ref']):  # 出處是 PM 的 fleet 訊息（不在 repo 內）
            assert q['date'] and q['quote']
            continue
        file, line = q['ref'].rsplit(':',1)
        path = Path(file) if Path(file).is_absolute() else REPO/file
        assert q['date'] and q['quote'] in path.read_text().splitlines()[int(line)-1], q
for v in versions:
    for q in v['operator_feedback']:
        file,line=q['ref'].rsplit(':',1)
        path=Path(file) if Path(file).is_absolute() else REPO/file
        assert q['quote'] in path.read_text().splitlines()[int(line)-1]
rows = [v for v in versions if v['ap_sha']]
lineage = [v for v in rows if v['id'].startswith('V')]
assert len(lineage) == 27
notes = (ROOT/'shots/notes.txt').read_text()
present = missing = 0
for v in versions:
    assert {'id','name','plain','ap_sha','date','kind','changed','principles','operator_feedback','shots','sources'} <= v.keys()
    assert len(v['plain']) <= 20
    assert set(v['principles']) <= pids | {'?'}
    assert v['kind'] in ('release','draft','rejected')
    if v['ap_sha']:
        for theme,path in v['shots'].items():
            if path:
                file=ROOT/path
                assert file.is_file() and file.stat().st_size
                assert file.with_suffix('.txt').is_file()
                assert file.parent.name == v['ap_sha'][:9]
                present += 1
            else:
                # --no-shoot append rows deliberately have no capture receipt.
                if v['id'].startswith('V'):
                    assert v['ap_sha'][:9]+'/'+theme+' FAIL' in notes
                missing += 1
    else:
        meta=json.loads((ROOT/'drafts'/v['id']/'meta.json').read_text())
        assert all(meta.get(k) for k in ('id','author','date','status','sources','adoption'))
        assert v['sources']==meta['sources']
# Exercise the actual sandbox setup; read-only snapshots prove this test does not
# change the real login configuration or credential file.
spec=importlib.util.spec_from_file_location('shoot',ROOT/'tools/shoot.py')
shoot=importlib.util.module_from_spec(spec);spec.loader.exec_module(shoot)
real=[Path.home()/'.claude.json',Path.home()/'.claude/.credentials.json']
assert shoot.public_text(str(Path.home())+"/demo").endswith("/demo")
assert len(shoot.public_text(str(Path.home()))) == len(str(Path.home()))
before={p:p.read_bytes() for p in real if p.is_file()}
work=None
try:
    work,data,home,cwd,env=shoot.sandbox_setup('/missing',tempfile.gettempdir(),'verify','light')
    config=Path(home)/'.claude.json'
    assert not config.is_symlink() and not (Path(home)/'.claude').is_symlink()
    assert json.loads(config.read_text())['theme']=='light'
    assert json.loads((Path(home)/'.claude/.claude.json').read_text())['theme']=='light'
finally:
    if work:shutil.rmtree(work)
assert all(p.read_bytes()==data for p,data in before.items())
print(f'PASS {len(principles)} principles, {len(lineage)} lineage rows, {len(versions)-len(rows)} drafts; {present} captures, {missing} missing states; copied HOME verified')
