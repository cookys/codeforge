import importlib.util, sys, pathlib, os, subprocess, json, re
sys.dont_write_bytecode = True
ENV = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
# usage: render.py <out dir of designer> <dest dir> <label>
# 用 repo 自己的 pane-history/tools（cdpshot、shotcheck、build）；格寬 8px（geo cellW 實測），版面寬照這個算，不然右邊會被截掉
tools = pathlib.Path(__file__).resolve().parents[2] / 'pane-history' / 'tools'
spec = importlib.util.spec_from_file_location('b', tools / 'build.py'); b = importlib.util.module_from_spec(spec); sys.modules['b'] = b; spec.loader.exec_module(b)
src, dst, label = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
dst.mkdir(parents=True, exist_ok=True)
summary = []
for d in sorted(p for p in src.iterdir() if re.fullmatch(r'\d+x\d+-s\d+', p.name)):
    w, h = map(int, d.name.split('-')[0].split('x'))
    cells = ''
    for theme in ('dark', 'light'):
        b.DEFAULT_FG, b.DEFAULT_BG = ('#1f2328', '#ffffff') if theme == 'light' else ('#d8dee9', '#000000')
        g = b.grid_html((d / (theme + '.ans')).read_text(), crop=False)
        cells += f'<figure><figcaption>{label} · {d.name} · {theme}</figcaption><div class="thumb shot-{theme}">{g}</div></figure>'
    stack = w > 130
    html = dst / f'{d.name}.html'
    html.write_text(f'<!doctype html><meta charset=utf-8><style>{b.CSS}body{{padding:8px;background:#777}}.sbs{{display:flex;flex-direction:{"column" if stack else "row"};gap:12px;align-items:flex-start}}figure{{margin:0}}figcaption{{color:#fff}}.thumb{{height:auto;overflow:visible}}.thumb .term-grid{{position:static}}</style><div class=sbs>{cells}</div>')
    pw = w * 8 + 60 if stack else w * 8 * 2 + 80  # 格寬實測 8px（geo cellW）
    ph = (h * 15 + 40) * (2 if stack else 1) + 40
    png = dst / f'{d.name}.png'; geo = dst / f'{d.name}.geo.json'
    rc1 = 0
    if not (png.exists() and geo.stat().st_size if geo.exists() else 0):
      with open(geo, 'w') as f:
        rc1 = subprocess.run(['node', str(tools / 'cdpshot.mjs'), 'file://' + str(html), str(png), str(pw), str(ph), '-'], stdout=f, timeout=200).returncode
    r = subprocess.run(['python3', str(tools / 'shotcheck.py'), str(png), str(geo)], capture_output=True, text=True, timeout=300, env=ENV)
    try: j = json.loads(r.stdout)
    except Exception: j = {'ok': False, 'err': r.stdout[-200:] + r.stderr[-200:]}
    frameless = '╭' not in (d / 'screen.txt').read_text()
    no_esc = [t for t in ('dark', 'light') if b'\x1b' not in (d / (t + '.ans')).read_bytes()]
    if no_esc: j['ok'] = False; j['note'] = 'ans without ESC: ' + ','.join(no_esc)
    if not j.get('ok') and j.get('blank_ok') and frameless and j.get('frame_segments') == 0: j['ok'] = True; j['note'] = 'frameless screen: no frame segment to check'
    summary.append({'shot': d.name, 'cdp_rc': rc1, 'ok': j.get('ok'), 'frameless': frameless, 'frame_segments': j.get('frame_segments'), 'blank_ok': j.get('blank_ok'), 'max_gap_px': j.get('max_gap_px')})
    print(d.name, rc1, j.get('ok'), j.get('max_gap_px'), flush=True)
(dst / 'shotcheck.json').write_text(json.dumps(summary, ensure_ascii=False, indent=1))
bad = [s for s in summary if not s['ok'] or s['cdp_rc']]
print('TOTAL', len(summary), 'BAD', len(bad)); sys.exit(1 if bad else 0)
