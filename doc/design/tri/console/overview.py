"""console 設計稿總覽：盤點表、審查紀錄、68 步 × 3 尺寸的畫面（深淺兩色可切）。

用法：python3 overview.py <out 目錄> <輸出 html> <sha>
畫面用 out/<W>x<H>-s<N>/roles.json 直接排成文字格子（不嵌 PNG）；像素級的圖用 tools/render.py 另出。
"""
import html
import json
import os
import re
import sys
import unicodedata

OUT, DEST, SHA = sys.argv[1], sys.argv[2], sys.argv[3]
HERE = os.path.dirname(os.path.abspath(__file__))
SIZES = ('209x49', '120x40', '80x24')
THEMES = json.load(open(os.path.join(HERE, '..', 'brief', 'tools', 'cc-themes.json')))
LIGHT, DARK = THEMES[0], THEMES[2]  # cc-themes.json 順序：light、light-daltonized、dark、dark-daltonized
ROLES = ('text', 'inactive', 'subtle', 'suggestion', 'success', 'error', 'warning', 'merged', 'claude', 'promptBorder')

flow = json.load(open(os.path.join(OUT, 'flow.json')))
steps = flow['steps'] if isinstance(flow, dict) else flow
cov = json.load(open(os.path.join(OUT, 'coverage.json')))
cov_by = {}
for c in cov:
    cov_by.setdefault(c['step'], []).append(f'{c["來源"]} {c["狀態"]}')


def wide(ch):
    return unicodedata.east_asian_width(ch) in 'WF'


def cells(text):
    out = []
    for ch in text:
        e = html.escape(ch)
        out.append(f'<i>{e}</i>' if wide(ch) else e)
    return ''.join(out)


def screen(size, n):
    rows = json.load(open(os.path.join(OUT, f'{size}-s{n}', 'roles.json')))
    lines = []
    for r in rows:
        segs = ''.join(f'<span class="r-{fg if fg in ROLES else "text"}">{cells(t)}</span>' for t, fg, _bg in r)
        lines.append(f'<div class=l>{segs}</div>')
    w = int(size.split('x')[0])
    return f'<div class="term term-grid" style="width:{w}ch">{"".join(lines)}</div>'


def role_css(theme):
    return ''.join(f'--{k}:{theme[k]};' for k in ROLES)


def md_verdict(path):
    if not os.path.isfile(path):
        return None
    t = open(path).read().strip().splitlines()
    blocking = sum(1 for l in t if '｜blocking｜' in l)
    return t[-1].strip(), blocking


REV = os.path.join(HERE, 'review')
rounds = []
for rnd, label in (('r1', '第 1 輪'), ('r2', '第 2 輪'), ('r3', '第 3 輪'), ('r3v', '驗證輪（只驗 r3 修正）')):
    eng = []
    for f in sorted(os.listdir(REV)):
        m = re.match(rf'{rnd}-([a-z]+)\.md$', f)
        if m and m.group(1) != 'dispositions':
            v = md_verdict(os.path.join(REV, f))
            if v:
                eng.append(f'{m.group(1)}：{v[0]}（blocking {v[1]}）')
    disp = f'review/{rnd}-dispositions.md' if os.path.isfile(os.path.join(REV, f'{rnd}-dispositions.md')) else '—'
    if eng or disp != '—':
        rounds.append((label, '；'.join(eng) or '—', disp))
NO_OUTPUT = {'第 2 輪': 'agy 沒有產出（auth_failed，認證失敗）', '第 3 輪': 'grok 沒有產出（quota_exhausted，額度用完）'}

inv = ''.join(
    f'<tr><td>{s["step"]}</td><td><a href="#s{s["step"]}">{html.escape(s["id"])}</a></td><td>{html.escape(str(s["tab"]))}</td>'
    f'<td>{html.escape(s["state"])}</td><td>{html.escape(s["title"])}</td><td><kbd>{html.escape(str(s.get("key_to_next", "")))}</kbd> → {s.get("next") or "—"}</td>'
    f'<td>{html.escape("、".join(cov_by.get(s["step"], [])) or "")}</td></tr>' for s in steps)
rev = ''.join(f'<tr><td>{l}</td><td>{html.escape(e)}{"<br><span class=mut>" + NO_OUTPUT[l] + "</span>" if l in NO_OUTPUT else ""}</td><td>{d}</td></tr>' for l, e, d in rounds)
body = ''.join(
    f'<section id="s{s["step"]}"><h3>{s["step"]}. {html.escape(s["id"])} · {html.escape(str(s["tab"]))} · {html.escape(s["title"])}'
    f' <span class=mut>按 {html.escape(str(s.get("key_to_next", "")))} → {s.get("next") or "—"}</span></h3>'
    + ''.join(f'<figure data-size="{z}"><figcaption>{z}</figcaption>{screen(z, s["step"])}</figure>' for z in SIZES) + '</section>'
    for s in steps)

page = f'''<!doctype html><html lang=zh-Hant><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Console 設計總覽</title>
<style>
:root{{--bg:#fff;--fg:#1f2328;--mut:#57606a;--line:#d0d7de;--tbg:#fff;{role_css(LIGHT)}}}
:root[data-theme=dark]{{--bg:#0d1117;--fg:#e6edf3;--mut:#9da7b3;--line:#30363d;--tbg:#000;{role_css(DARK)}}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#0d1117;--fg:#e6edf3;--mut:#9da7b3;--line:#30363d;--tbg:#000;{role_css(DARK)}}}}}
body{{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:15px/1.6 system-ui,"Noto Sans TC",sans-serif}}
h1{{font-size:22px;margin:0 0 4px}} h2{{font-size:18px;margin:28px 0 8px}} h3{{font-size:15px;margin:24px 0 6px}}
.mut{{color:var(--mut);font-weight:normal}}
table{{border-collapse:collapse;font-size:13px;max-width:100%}} td,th{{border:1px solid var(--line);padding:3px 6px;text-align:left;vertical-align:top}}
.wrap{{overflow-x:auto}} kbd{{font-family:ui-monospace,monospace}}
nav{{position:sticky;top:0;background:var(--bg);padding:6px 0;border-bottom:1px solid var(--line);z-index:1;display:flex;gap:8px;flex-wrap:wrap}}
button{{font:inherit;padding:2px 10px;border:1px solid var(--line);background:var(--bg);color:var(--fg);border-radius:4px;cursor:pointer}}
button[aria-pressed=true]{{border-color:var(--fg)}}
figure{{margin:6px 0 14px;overflow-x:auto}} figcaption{{font-size:12px;color:var(--mut)}}
.term{{background:var(--tbg);font:13px/15px "DejaVu Sans Mono",ui-monospace,monospace;padding:4px;white-space:pre}}
.l{{height:15px;overflow:hidden}} .term i{{font-style:normal;display:inline-block;width:2ch;height:15px;line-height:15px;text-align:center;overflow:hidden;vertical-align:top}}
{''.join(f'.r-{k}{{color:var(--{k})}}' for k in ROLES)}
body[data-size] figure{{display:none}} body[data-size="209x49"] figure[data-size="209x49"],body[data-size="120x40"] figure[data-size="120x40"],body[data-size="80x24"] figure[data-size="80x24"]{{display:block}}
</style>
<body data-size="120x40">
<h1>Console 設計總覽</h1>
<div class=mut>sha {SHA} · 9 分頁 × 6 狀態加跨頁步驟，共 {len(steps)} 步 × 3 尺寸 × 深淺兩色 · 步序與按鍵以 out/flow.json 為準</div>
<nav><span>尺寸</span>{''.join(f'<button data-s="{z}">{z}</button>' for z in SIZES)}<span>色系</span><button data-t=dark>深</button><button data-t=light>淺</button><a href="#inv">盤點表</a><a href="#rev">審查</a><a href="#screens">畫面</a></nav>
<h2 id=rev>審查紀錄</h2><div class=wrap><table><tr><th>輪次</th><th>審查結果</th><th>處置表</th></tr>{rev}</table></div>
<p class=mut>檢查：check3 每張畫面（寬度、框線、R1 可按的鍵只在下半部、R6 選項字母順序、R7 band 禁字、網址不斷開、[-] 在框內、兩色對比）全過；R6 另有反向探針（對調 a/b 會被擋）。</p>
<h2 id=inv>盤點表（{len(steps)} 步）</h2><div class=wrap><table><tr><th>步</th><th>id</th><th>分頁</th><th>狀態</th><th>標題</th><th>按鍵 → 下一步</th><th>缺源／過時</th></tr>{inv}</table></div>
<h2 id=screens>畫面</h2>{body}
<script>
const b=document.body,r=document.documentElement;
function sz(s){{b.dataset.size=s;document.querySelectorAll('[data-s]').forEach(x=>x.setAttribute('aria-pressed',x.dataset.s===s))}}
function th(t){{r.dataset.theme=t;document.querySelectorAll('[data-t]').forEach(x=>x.setAttribute('aria-pressed',x.dataset.t===t))}}
document.querySelectorAll('[data-s]').forEach(x=>x.onclick=()=>sz(x.dataset.s));
document.querySelectorAll('[data-t]').forEach(x=>x.onclick=()=>th(x.dataset.t));
sz('120x40');th(matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light');
</script>
'''
open(DEST, 'w').write(page)
print('wrote', DEST, len(page) // 1024, 'KB')
