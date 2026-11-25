import json, sys, pathlib, html
# usage: compare.py <pkg dir> <map.json>  (pkg/<label>/render/<WxH>-sN.png must exist)
pkg = pathlib.Path(sys.argv[1]); m = json.load(open(sys.argv[2]))
names = [k for k in m if k != 'steps']
for size in ('209x49', '120x40', '80x24'):
    rows = ''
    for i, st in enumerate(m['steps']):
        cells = f'<th>{html.escape(st)}</th>'
        for n in names:
            sn = m[n][i]; p = f'{n}/render/{size}-s{sn}.png'
            cells += f'<td><div class=cap>{n} · s{sn}</div><img src="{p}"></td>' if (pkg / p).exists() else f'<td>缺</td>'
        rows += f'<tr>{cells}</tr>'
    head = ''.join(f'<th>{n}</th>' for n in names)
    (pkg / f'compare-{size}.html').write_text(f'<!doctype html><meta charset=utf-8><title>對照 {size}</title><style>body{{background:#333;color:#eee;font:14px sans-serif;margin:8px}}table{{border-collapse:collapse}}td,th{{border:1px solid #666;padding:4px;vertical-align:top}}th{{white-space:nowrap}}img{{width:{700 if size!="209x49" else 1000}px;display:block}}.cap{{color:#aaa;font-size:12px}}</style><h1>同一步並排 · {size}（上深下淺或左深右淺）</h1><table><tr><th>步驟</th>{head}</tr>{rows}</table>')
    print('wrote', size)
