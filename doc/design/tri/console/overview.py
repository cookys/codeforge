"""console 設計稿總覽：盤點表、審查紀錄、68 步 × 3 尺寸的畫面（深淺兩色可切）。

用法：python3 overview.py <out 目錄> <輸出 html> [sha，預設是畫面內容的 sha]
畫面用 out/<W>x<H>-s<N>/roles.json 直接排成文字格子（不嵌 PNG）；像素級的圖用 tools/render.py 另出。
"""
import html
import json
import os
import re
import sys
import unicodedata

OUT, DEST = sys.argv[1], sys.argv[2]
HERE = os.path.dirname(os.path.abspath(__file__))
# 一個檔案寫不進「包含它自己的那個 commit」的 sha。所以標頭寫畫面內容的 sha：
# 也就是最後一個改到 out/、gen.py、review/ 的 commit。交件時報這個 sha（PM 01M89XK1）。
import subprocess
SHA = sys.argv[3] if len(sys.argv) > 3 else subprocess.run(
    ['git', 'log', '-1', '--format=%h', '--abbrev=12', '--', 'out', 'gen.py', 'review', 'out-r1', 'r1_export.py',
     '../../pane-history/drafts/r1-rev5'],
    cwd=HERE, capture_output=True, text=True, check=True).stdout.strip()
SIZES = ('209x49', '120x40', '80x24')
THEMES = json.load(open(os.path.join(HERE, '..', 'brief', 'tools', 'cc-themes.json')))
LIGHT, DARK = THEMES[0], THEMES[2]  # cc-themes.json 順序：light、light-daltonized、dark、dark-daltonized
ROLES = ('text', 'inactive', 'subtle', 'suggestion', 'success', 'error', 'warning', 'merged', 'claude', 'promptBorder', 'remember')
# R1 底部控制（PM 01M8A05M）：r1-rev5 的 8 步，由 r1_export.py 轉成 out-r1/。rev5 疊在實拍上，尺寸是 200x50／120x40／80x24，
# 尺寸按鈕的寬、中、窄各對到一個。
R1_OUT = os.path.join(HERE, 'out-r1')
R1_SIZE = {'209x49': '200x50', '120x40': '120x40', '80x24': '80x24'}
r1_steps = json.load(open(os.path.join(R1_OUT, 'flow.json')))['steps']

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


DATA = {}


def screen(size, n, base=OUT, tag=''):
    rows = json.load(open(os.path.join(base, f'{size}-s{n}', 'roles.json')))
    DATA[f'{tag}{n}/{size}'] = [[[t, ROLES.index(fg) if fg in ROLES else 0] for t, fg, _bg in r] for r in rows]
    return f'<div class=ph data-k="{tag}{n}/{size}"></div>'


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
r1_body = ''.join(
    f'<section id="r{s["step"]}"><h3>R1-{s["step"]} · {html.escape(s["title"])}</h3>'
    + ''.join(f'<figure data-size="{z}"><figcaption>{R1_SIZE[z]}</figcaption>{screen(R1_SIZE[z], s["step"], R1_OUT, "R")}</figure>' for z in SIZES)
    + '</section>' for s in r1_steps)

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
.term-grid{{background:var(--tbg);font:13px/15px "DejaVu Sans Mono","Noto Sans Mono CJK TC",monospace;padding:0;box-shadow:0 0 0 4px var(--tbg);margin:4px;white-space:pre;display:flex;flex-direction:column;box-sizing:content-box;font-kerning:none;font-variant-ligatures:none}}
.term-grid .ln{{display:flex;flex:none;height:15px}} .term-grid i{{font-style:normal;display:inline-block;height:15px;line-height:15px;text-align:center;overflow:visible}} .term-grid i.n{{width:8px}} .term-grid i.w{{width:16px}}
{''.join(f'.r-{k}{{color:var(--{k})}}' for k in ROLES)}
body[data-size] figure{{display:none}} body[data-size="209x49"] figure[data-size="209x49"],body[data-size="120x40"] figure[data-size="120x40"],body[data-size="80x24"] figure[data-size="80x24"]{{display:block}}
</style>
<body data-size="120x40">
<h1>Console 設計總覽</h1>
<div class=mut>畫面內容 sha {SHA}；overview 本身在它之後的下一個 commit（檔案寫不進自己所在 commit 的 sha，交件訊息附那個 sha）· console 9 分頁 × 6 狀態加跨頁步驟，共 {len(steps)} 步；另加 R1 底部控制 {len(r1_steps)} 步；每步 3 尺寸 × 深淺兩色 · 步序與按鍵以 out/flow.json、out-r1/flow.json 為準</div>
<nav><span>尺寸</span>{''.join(f'<button data-s="{z}">{z}</button>' for z in SIZES)}<span>色系</span><button data-t=dark>深</button><button data-t=light>淺</button><a href="#inv">盤點表</a><a href="#rev">審查</a><a href="#screens">畫面</a><a href="#r1">R1 底部控制</a></nav>
<h2 id=rev>審查紀錄</h2><div class=wrap><table><tr><th>輪次</th><th>審查結果</th><th>處置表</th></tr>{rev}</table></div>
<p class=mut>檢查：check3 每張畫面（寬度、框線、R1 可按的鍵只在下半部、R6 選項字母順序、R7 band 禁字、R8 預覽和清單勾選一致、網址不斷開、[-] 在框內、兩色對比）全過；R6、R8 各有反向探針（對調 a/b、預覽多列沒選的卡都會被擋）。</p>
<h2 id=inv>盤點表（{len(steps)} 步）</h2><div class=wrap><table><tr><th>步</th><th>id</th><th>分頁</th><th>狀態</th><th>標題</th><th>按鍵 → 下一步</th><th>缺源／過時</th></tr>{inv}</table></div>
<h2 id=screens>畫面</h2>{body}
<h2 id=r1>R1 底部控制（{len(r1_steps)} 步）</h2>
<p class=mut>R1 計畫 rev4 的示意稿 r1-rev5（pane-history/drafts/r1-rev5），疊在現行 live 實拍上畫：pane 的可按鍵只在下半部；放不下時依高度收成較少的列，不擋 band。
三種尺寸是 rev5 量過的 200×50、120×40、80×24（尺寸按鈕寬、中、窄各對一個）。實拍部分（宿主對話區、現行 band、宿主的 [-]）保留原色，不算設計稿；
check3 只判設計稿畫的格子，24 張全過。和 rev5 原稿唯一的差異：卡片題目列改用 text 色加粗，原本的 claude 色在淺色主題對比只有 3.15（文字要 4.5），rev5 已同步改。</p>
{r1_body}
<script>
const D={json.dumps(DATA, ensure_ascii=False, separators=(',', ':'))};
const RL={json.dumps(list(ROLES))};
const W=c=>{{const p=c.codePointAt(0);return (p>=0x1100&&p<=0x115f)||(p>=0x2e80&&p<=0xa4cf)||(p>=0xac00&&p<=0xd7a3)||(p>=0xf900&&p<=0xfaff)||(p>=0xfe30&&p<=0xfe4f)||(p>=0xff00&&p<=0xff60)||(p>=0xffe0&&p<=0xffe6)}};
function draw(ph){{if(ph.firstChild)return;const rows=D[ph.dataset.k];const g=document.createElement('div');g.className='term-grid';g.style.width=(+ph.dataset.k.split('/')[1].split('x')[0])*8+'px';
 for(const r of rows){{const ln=document.createElement('div');ln.className='ln';for(const [t,ro] of r){{for(const ch of t){{const i=document.createElement('i');i.className=(W(ch)?'w':'n')+' r-'+RL[ro];i.textContent=ch;ln.appendChild(i)}}}}g.appendChild(ln)}}ph.appendChild(g)}}
const b=document.body,r=document.documentElement;
function sz(s){{b.dataset.size=s;document.querySelectorAll('figure[data-size="'+s+'"] .ph').forEach(draw);document.querySelectorAll('[data-s]').forEach(x=>x.setAttribute('aria-pressed',x.dataset.s===s))}}
function th(t){{r.dataset.theme=t;document.querySelectorAll('[data-t]').forEach(x=>x.setAttribute('aria-pressed',x.dataset.t===t))}}
document.querySelectorAll('[data-s]').forEach(x=>x.onclick=()=>sz(x.dataset.s));
document.querySelectorAll('[data-t]').forEach(x=>x.onclick=()=>th(x.dataset.t));
sz('120x40');th(matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light');
</script>
'''
open(DEST, 'w').write(page)
print('wrote', DEST, len(page) // 1024, 'KB')
