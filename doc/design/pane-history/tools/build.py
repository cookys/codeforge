#!/usr/bin/env python3
"""Build the pane history from versions.json and principles.json. Standard library only.
ANSI grid parsing/measurement adapted from r1-lineage/build2.py; raw files stay intact.
"""
import base64
import html
import json
import pathlib
import re
import sys
import unicodedata
HERE = pathlib.Path(__file__).resolve().parent.parent
B = sys.modules[__name__]
DEFAULT_FG = "#d8dee9"
DEFAULT_BG = "#14161b"

ANSI_16 = (
    "#1d1f21", "#cc6666", "#b5bd68", "#f0c674",
    "#81a2be", "#b294bb", "#8abeb7", "#c5c8c6",
    "#666666", "#d54e53", "#b9ca4a", "#e7c547",
    "#7aa6da", "#c397d8", "#70c0b1", "#eaeaea",
)
CUBE_STEPS = (0x00, 0x5F, 0x87, 0xAF, 0xD7, 0xFF)

UNCERTAIN_SHA_PREFIXES = (
    "8ee171c9",
    "eb9f702e",
    "af43a0844",
    "6a9d4355",
    "36158d86",
    "faa6b4cb",
    "98cf58d0",
    "db0da9e7",
    "a5888833",
    "443278024",
    "885450642",
    "6db7e47e",
)

OSC_RE = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
CSI_RE = re.compile(r"\x1b\[([0-9;:?]*)([A-Za-z])")


def esc_text(s: str) -> str:
    return html.escape(s, quote=False)


def esc_attr(s: str) -> str:
    return html.escape(s, quote=True)


def xterm256_hex(idx: int) -> str:
    idx = max(0, min(255, idx))
    if idx < 16:
        return ANSI_16[idx]
    if idx < 232:
        n = idx - 16
        r = CUBE_STEPS[n // 36]
        g = CUBE_STEPS[(n // 6) % 6]
        b = CUBE_STEPS[n % 6]
        return f"#{r:02x}{g:02x}{b:02x}"
    v = 8 + 10 * (idx - 232)
    return f"#{v:02x}{v:02x}{v:02x}"


def apply_sgr(st: dict, raw_params: str) -> None:
    if not raw_params:
        st.update(fg=None, bg=None, bold=False, dim=False, rev=False)
        return
    parts = raw_params.split(";")
    i = 0
    while i < len(parts):
        tok = parts[i]
        if ":" in tok:
            sub = [int(x) if x.isdigit() else 0 for x in tok.split(":")]
            head = sub[0]
            if head in (38, 48) and len(sub) >= 3:
                key = "fg" if head == 38 else "bg"
                if sub[1] == 5:
                    st[key] = xterm256_hex(sub[2])
                elif sub[1] == 2:
                    rgb = sub[-3:]
                    st[key] = f"#{rgb[0]:02x}{rgb[1]:02x}{rgb[2]:02x}"
            i += 1
            continue
        p = int(tok) if tok.isdigit() else 0
        if p == 0:
            st.update(fg=None, bg=None, bold=False, dim=False, rev=False)
        elif p == 1:
            st["bold"] = True
        elif p == 2:
            st["dim"] = True
        elif p == 7:
            st["rev"] = True
        elif p == 22:
            st["bold"] = False
            st["dim"] = False
        elif p == 27:
            st["rev"] = False
        elif 30 <= p <= 37:
            st["fg"] = ANSI_16[p - 30]
        elif p == 38:
            if i + 2 < len(parts) and parts[i + 1] == "5" and parts[i + 2].isdigit():
                st["fg"] = xterm256_hex(int(parts[i + 2]))
                i += 2
            elif i + 4 < len(parts) and parts[i + 1] == "2":
                r = int(parts[i + 2]) if parts[i + 2].isdigit() else 0
                g = int(parts[i + 3]) if parts[i + 3].isdigit() else 0
                b = int(parts[i + 4]) if parts[i + 4].isdigit() else 0
                st["fg"] = f"#{r:02x}{g:02x}{b:02x}"
                i += 4
        elif p == 39:
            st["fg"] = None
        elif 40 <= p <= 47:
            st["bg"] = ANSI_16[p - 40]
        elif p == 48:
            if i + 2 < len(parts) and parts[i + 1] == "5" and parts[i + 2].isdigit():
                st["bg"] = xterm256_hex(int(parts[i + 2]))
                i += 2
            elif i + 4 < len(parts) and parts[i + 1] == "2":
                r = int(parts[i + 2]) if parts[i + 2].isdigit() else 0
                g = int(parts[i + 3]) if parts[i + 3].isdigit() else 0
                b = int(parts[i + 4]) if parts[i + 4].isdigit() else 0
                st["bg"] = f"#{r:02x}{g:02x}{b:02x}"
                i += 4
        elif p == 49:
            st["bg"] = None
        elif 90 <= p <= 97:
            st["fg"] = ANSI_16[8 + p - 90]
        elif 100 <= p <= 107:
            st["bg"] = ANSI_16[8 + p - 100]
        i += 1


def style_attr_for(st: dict) -> str:
    if st["rev"]:
        fg = st["bg"] or DEFAULT_BG
        bg = st["fg"] or DEFAULT_FG
    else:
        fg = st["fg"]
        bg = st["bg"]
    decls = []
    if fg:
        decls.append(f"color:{fg}")
    if bg:
        decls.append(f"background:{bg}")
    if st["bold"]:
        decls.append("font-weight:700")
    if st["dim"]:
        decls.append("opacity:.6")
    return f' style="{";".join(decls)}"' if decls else ""


def col_box_val(cells: list) -> int:
    return sum(c[2] for c in cells)


def emit_chunk(chunk: str, st: dict, cells: list, col_box: list) -> None:
    sattr = style_attr_for(st)
    col = col_box[0]
    for ch in chunk:
        if ch == "\t":
            spaces = 8 - (col % 8)
            for _ in range(spaces):
                cells.append((f'<i class="n"{sattr}>', " ", 1))
                col += 1
            continue
        if ch == "\xa0":
            ch = " "
        if ord(ch) < 32 or ord(ch) == 127:
            continue
        if unicodedata.combining(ch) and cells:
            prev_open, prev_txt, prev_w = cells[-1]
            cells[-1] = (prev_open, prev_txt + esc_text(ch), prev_w)
            continue
        is_wide = unicodedata.east_asian_width(ch) in ("W", "F")
        cls = "w" if is_wide else "n"
        w = 2 if is_wide else 1
        cells.append((f'<i class="{cls}"{sattr}>', esc_text(ch), w))
        col += w
    col_box[0] = col



def _lum(h):
    c = [int(h[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def _ratio(a, b):
    x, y = sorted([_lum(a), _lum(b)], reverse=True)
    return (x + 0.05) / (y + 0.05)


def _mix(a, b, t):
    ca = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    cb = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(ca, cb))


_fix_cache = {}


def _readable(fg, bg):
    """把對比不足 4.6 的字色往白（或黑）推，直到過 4.6；保留色相。"""
    key = (fg, bg)
    if key in _fix_cache:
        return _fix_cache[key]
    target = "#ffffff" if _lum(bg) < 0.4 else "#000000"
    out, t = fg, 0.0
    while _ratio(out, bg) < 4.6 and t < 1:
        t += 0.05
        out = _mix(fg, target, t)
    _fix_cache[key] = out
    return out


def fix_style(line_html):
    """每個格子：dim 改成混色而不是 opacity，再把字色調到對比 ≥4.5（R5／lab 閘）。"""
    def one(m):
        st = m.group(1)
        d = dict(kv.split(":", 1) for kv in st.split(";") if ":" in kv)
        bg = d.get("background", B.DEFAULT_BG)
        fg = d.get("color", B.DEFAULT_FG)
        if not (bg.startswith("#") and len(bg) == 7):
            bg = B.DEFAULT_BG
        if not (fg.startswith("#") and len(fg) == 7):
            fg = B.DEFAULT_FG
        if d.pop("opacity", None):
            fg = _mix(fg, bg, 0.4)
        d["color"] = _readable(fg, bg)
        return ' style="' + ";".join(f"{k}:{v}" for k, v in d.items()) + '"'
    line_html = re.sub(r' style="([^"]*)"', one, line_html)
    return line_html.replace('<i class="n">', '<i class="n" style="color:' + _readable(B.DEFAULT_FG, B.DEFAULT_BG) + '">').replace('<i class="w">', '<i class="w" style="color:' + _readable(B.DEFAULT_FG, B.DEFAULT_BG) + '">')




def grid_html(raw: str) -> str:
    """ANSI → 固定格子 HTML；每列補空白到最寬那列的欄數。"""
    text = raw.replace("\r\n", "\n").replace("\r", "\n")
    lines = text.split("\n")
    while lines and not B.CSI_RE.sub("", B.OSC_RE.sub("", lines[-1])).strip():
        lines.pop()
    st = {"fg": None, "bg": None, "bold": False, "dim": False, "rev": False}
    rows = []
    for raw_line in lines:
        clean = B.OSC_RE.sub("", raw_line)
        cells, pos = [], 0
        for m in B.CSI_RE.finditer(clean):
            B.emit_chunk(clean[pos:m.start()], st, cells, col_box=[B.col_box_val(cells)])
            if m.group(2) == "m":
                B.apply_sgr(st, m.group(1))
            pos = m.end()
        B.emit_chunk(clean[pos:], st, cells, col_box=[B.col_box_val(cells)])
        rows.append(cells)
    cols = max((B.col_box_val(c) for c in rows), default=0)
    out = []
    for cells in rows:
        pad = cols - B.col_box_val(cells)
        body = "".join(o + t + "</i>" for o, t, _ in cells) + '<i class="n"> </i>' * pad
        out.append(f'<span class="ln">{body}</span>')
    return f'<div class="term-grid" data-cols="{cols}" style="width:{cols * 8}px">' + "\n".join(fix_style(x) for x in out) + "</div>"
CSS = '\n:root{--bg:#0f1217;--card:#161b24;--line:#2f3747;--fg:#e5e9f0;--mut:#b4bdcc;--acc:#ebcb8b;--accfg:#14161b;--link:#88c0d0;--guess:#e8a48c;}\n:root[data-theme="light"]{--bg:#ffffff;--card:#f6f8fa;--line:#c9d1d9;--fg:#1f2328;--mut:#4d5560;--acc:#9a6700;--accfg:#ffffff;--link:#0550ae;--guess:#a40e26;}\n*{box-sizing:border-box}\nbody{margin:0;padding:16px;background:var(--bg);color:var(--fg);\n font-family:"Noto Sans TC","PingFang TC","Microsoft JhengHei",system-ui,sans-serif;\n font-size:15px;line-height:1.7;line-break:strict;word-break:normal;overflow-wrap:anywhere}\n.wrap{max-width:1240px;margin:0 auto}\nh1{font-size:20px;line-height:1.6;margin:0 0 4px}\n.ask{font-size:16px;margin:0 0 4px}\n.ask b{color:var(--acc)}\n.live{margin:0 0 14px;color:var(--mut)}\n.live b{color:var(--fg)}\n.bar{display:flex;justify-content:flex-end;gap:8px;margin:-36px 0 12px}\nbutton{font:inherit;font-size:13px;line-height:1.6;background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:4px;padding:2px 10px;cursor:pointer}\n.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(380px,1fr));gap:14px}\n.card{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:10px 12px}\n.card.is-live{border:2px solid var(--acc)}\n.name{font-size:15px;line-height:1.7;margin:0 0 6px}\n.no{font-weight:700;white-space:nowrap;margin-right:6px}\n.badge{background:var(--acc);color:var(--accfg);border-radius:3px;padding:0 6px;font-size:12px;margin-left:6px;white-space:nowrap}\n.thumb{height:230px;overflow:hidden;position:relative;border-radius:4px;background:#14161b;cursor:zoom-in}\n.thumb .term-grid{position:absolute;left:0;top:0;transform-origin:0 0}\n.noshot{height:230px;display:flex;align-items:center;justify-content:center;border:1px dashed var(--line);border-radius:4px;color:var(--mut);font-size:14px;padding:12px;text-align:center}\ndetails{margin-top:6px;font-size:13px;line-height:1.7;color:var(--mut)}\nsummary{cursor:pointer}\ndetails dl{display:grid;grid-template-columns:5em 1fr;gap:0 8px;margin:6px 0 0}\ndetails dt{color:var(--mut)}details dd{margin:0;color:var(--fg)}\ncode{font-family:"JetBrains Mono","DejaVu Sans Mono",monospace;color:var(--link)}\n.guess{color:var(--guess)}\npre{white-space:pre-wrap;font-size:12px;line-height:1.6;background:var(--card);border:1px solid var(--line);border-radius:4px;padding:8px;color:var(--fg)}\n.foot{margin-top:18px}\n/* 終端格子：每個字元固定 1ch / 2ch，不依賴字型寬度 */\n.term-grid{font-family:"DejaVu Sans Mono","Noto Sans Mono CJK TC",monospace;font-size:13px;line-height:17px;color:#d8dee9;background:#14161b;white-space:normal;line-break:auto}\n.term-grid .ln{display:block;height:17px;white-space:pre}\n.term-grid i{display:inline-block;font-style:normal;height:17px;overflow:hidden;text-align:center;vertical-align:top}\n.term-grid i.n{width:8px}.term-grid i.w{width:16px}\n#zoom{display:none;position:fixed;inset:0;z-index:50;background:rgba(0,0,0,.8);overflow:auto;padding:24px;cursor:zoom-out}\n#zoom.on{display:block}\n#zoom .term-grid{position:static;transform:none}\n@media (max-width:700px){.cards{grid-template-columns:1fr}.bar{margin:0 0 10px;justify-content:flex-start}.thumb,.noshot{height:200px}}\n\n.cards{grid-template-columns:repeat(auto-fit,minmax(min(100%,380px),1fr))}\n.card{min-width:0} h2{margin-top:2em} h3{font-size:16px;line-height:1.7}\na{color:var(--link)} img{max-width:100%;height:auto}\nblockquote{margin:8px 0;padding:0 12px;border-left:3px solid var(--line)}\n.term-grid{font-family:"DejaVu Sans Mono","Noto Sans TC","PingFang TC","Microsoft JhengHei",monospace;line-height:1.7;background:#14161b;color:#d8dee9}\n.term-grid .ln,.term-grid i{height:1.7em}.term-grid i.n{width:1ch}.term-grid i.w{width:2ch}\n.thumb{height:230px}.hero .thumb{height:540px}\n.shot-light .term-grid{background:#fff;color:#1f2328}.shot-light{background:#fff}\n.theme-light{display:none}:root[data-theme="light"] .theme-dark{display:none}:root[data-theme="light"] .theme-light{display:block}\n#zoom .term-grid{transform-origin:0 0}\n@media(max-width:700px){.hero .thumb{height:240px}}\n'
CSS += "\n.term-grid{display:flex;flex-direction:column;white-space:pre}.term-grid .ln{flex:none}\n"
CSS += "\npre,h1,button{line-height:1.7}figure{margin:12px 0}\n"
CSS += "\n.term-grid{letter-spacing:calc(8px - 1ch);font-kerning:none;font-variant-ligatures:none}.term-grid i.n{width:8px}.term-grid i.w{width:16px}\n"
JS = "\n(function(){\nvar R=document.documentElement,t=null;try{t=localStorage.getItem('pane-history-theme')}catch(e){}\nif(!t)t=(window.matchMedia&&matchMedia('(prefers-color-scheme: light)').matches)?'light':'dark';\nvar q=new URLSearchParams(location.search);if(q.get('theme'))t=q.get('theme');R.setAttribute('data-theme',t);\ndocument.getElementById('theme').onclick=function(){t=R.getAttribute('data-theme')==='light'?'dark':'light';R.setAttribute('data-theme',t);try{localStorage.setItem('pane-history-theme',t)}catch(e){};fit();};\nfunction fit(){document.querySelectorAll('.thumb').forEach(function(b){var g=b.firstElementChild;var s=Math.min(b.clientWidth/g.offsetWidth,b.clientHeight/g.offsetHeight);if(g.offsetWidth)g.style.transform='scale('+s+')';});}\ndocument.addEventListener('toggle',fit,true);new MutationObserver(fit).observe(R,{attributes:true,attributeFilter:['data-theme']});fit();if(document.fonts)document.fonts.ready.then(fit);addEventListener('resize',fit);\nvar z=document.getElementById('zoom');\ndocument.querySelectorAll('.thumb').forEach(function(b){b.onclick=function(){z.innerHTML='';var c=b.firstElementChild.cloneNode(true);z.appendChild(c);z.classList.add('on');c.style.transform='scale('+Math.min(1,(z.clientWidth-48)/c.offsetWidth)+')';};});\nz.onclick=function(){z.classList.remove('on');z.innerHTML='';};\naddEventListener('keydown',function(e){if(e.key==='Escape')z.onclick();});\nif(q.get('measure')){\n var grids=[].slice.call(document.querySelectorAll('.thumb .term-grid')).map(function(g){var w={};[].forEach.call(g.querySelectorAll('.ln'),function(l){w[l.offsetWidth]=1});return {card:g.closest('.card').dataset.no,cols:+g.dataset.cols,lineWidths:Object.keys(w).map(Number)};});\n function lh(sel){var e=document.querySelector(sel);if(!e)return null;var c=getComputedStyle(e);return {sel:sel,font:c.fontFamily,fs:parseFloat(c.fontSize),lh:parseFloat(c.lineHeight),ratio:+(parseFloat(c.lineHeight)/parseFloat(c.fontSize)).toFixed(3),lineBreak:c.lineBreak};}\n var o={theme:t,scrollW:R.scrollWidth,clientW:R.clientWidth,pageH:R.scrollHeight,\n  gridsAllOneWidth:grids.every(function(g){return g.lineWidths.length===1}),grids:grids,\n  text:[lh('body'),lh('.ask'),lh('.name'),lh('.live'),lh('details'),lh('details dd'),lh('.noshot')]};\n var p=document.createElement('pre');p.id='measure-out';p.textContent=JSON.stringify(o);document.body.appendChild(p);}\n})();\n"

def esc(value):
    return html.escape(str(value), quote=True)


def load(name):
    return json.loads((HERE / (name + '.json')).read_text())


def quote_html(quotes):
    return ''.join(f'<blockquote>{esc(q["quote"])}<footer>{esc(q["date"])} · {esc(q["ref"])}</footer></blockquote>' for q in quotes)


def picture(path, theme='dark'):
    file = HERE / path
    if not file.is_file():
        return '<p class="noshot">未取得畫面；見拍攝紀錄</p>'
    if file.suffix == '.png':
        data = base64.b64encode(file.read_bytes()).decode()
        return f'<img loading="lazy" alt="{esc(file.name)}" src="data:image/png;base64,{data}">'
    raw = file.read_text(errors='replace')
    # Original files are linked below; render their ANSI cells at the captured theme.
    global DEFAULT_FG, DEFAULT_BG
    old = DEFAULT_FG, DEFAULT_BG
    DEFAULT_FG, DEFAULT_BG = ('#1f2328', '#ffffff') if theme == 'light' else ('#d8dee9', '#14161b')
    rendered = grid_html(raw)
    DEFAULT_FG, DEFAULT_BG = old
    return f'<div class="thumb shot-{theme}">{rendered}</div>'


def shots(v):
    out = ''
    for theme in ('dark', 'light'):
        path = v['shots'].get(theme)
        out += f'<div class="theme-{theme}">' + (picture(path, theme) if path else '<p class="noshot">這版尚無'+('深色' if theme=='dark' else '淺色')+'畫面；拍攝失敗見 notes.log</p>') + '</div>'
    return out


def version(v, principles, hero=False):
    linked = '、'.join(f'<a href="#principle-{esc(p)}">{esc(p)} {esc(principles[p]["rule"].split("：")[0][:20])}</a>' if p in principles else esc(p) for p in v['principles'])
    return (f'<article class="card {"hero" if hero else ""}" id="{"current-r6" if hero else "version-"+esc(v["id"])}" data-no="{esc(v["id"])}">'
            f'<h3 class="name">{esc(v["plain"])}</h3>{shots(v)}'
            f'<p>對應原則：{linked}</p><p>改了什麼：{esc(v["changed"])}</p>'
            '<details><summary>版本與來源細節</summary>'
            f'<p>{esc(v["name"])} · {esc(v["date"])} · {esc(v["kind"])} · <code>{esc(v["ap_sha"])}</code></p>'
            + quote_html(v['operator_feedback'])
            + ''.join(f'<p>{esc(s)}</p>' for s in v['sources'])
            + ''.join(f'<p><a href="{esc(p)}">{t} 原始 ANSI</a></p>' for t,p in v['shots'].items() if p)
            + '</details></article>')


def draft(v):
    directory = HERE / 'drafts' / v['id']
    meta = json.loads((directory / 'meta.json').read_text())
    body = f'<article class="card" data-no="{esc(v["id"])}"><h3>{esc(v["name"])} · {esc(v["kind"])}</h3>'
    # Prefer the original visual files. rev5 live captures remain downloadable evidence,
    # not additional design versions.
    files = sorted(directory.glob('*.png')) + sorted(directory.glob('steps/*.png')) + sorted(directory.glob('design-*.txt'))
    for file in files:
        rel = str(file.relative_to(HERE))
        body += f'<figure><figcaption>{esc(file.name)}</figcaption>{picture(rel)}<a href="{esc(rel)}">原檔</a></figure>'
    for file in sorted(directory.glob('*.md')):
        raw = file.read_text()
        body += f'<details><summary>{esc(file.name)}</summary>'
        blocks = re.findall(r'```[^\n]*\n(.*?)```', raw, re.S)
        for block in blocks:
            body += f'<div class="thumb">{grid_html(block)}</div>'
        body += f'<pre>{esc(raw)}</pre></details>'
    body += '<details><summary>設計稿出處</summary><pre>'+esc(json.dumps(meta,ensure_ascii=False,indent=2))+'</pre></details></article>'
    return body


def main():
    versions = load('versions')
    principles = load('principles')
    by_id = {p['id']:p for p in principles}
    rows = [v for v in versions if v['ap_sha']]
    live = next(v for v in rows if v['ap_sha'].startswith('6db7e47e'))
    p_html = ''
    for p in principles:
        refs = '、'.join(f'<a href="#version-{esc(v)}">{esc(v)}</a>' for v in p['versions']) or '尚無對應版本'
        p_html += (f'<article class="card" id="principle-{esc(p["id"])}"><h3>{esc(p["id"])} · {esc(p["rule"])}</h3>'
                   f'<p>{esc(p["in_R"] or "待操作員收進 R 表")} · {esc(p["status"])}</p>'
                   + quote_html(p['quotes'])
                   + ('<p>出處：PM 轉述或原話未取得，不能當操作員逐字原話。</p>' if p['status']=='缺出處' else '')
                   + f'<p>對應版本：{refs}</p></article>')
    drafts = [v for v in versions if not v['ap_sha']]
    body = ('<h1>Pane 設計與版本歷史</h1><div class="bar"><button id="theme" type="button">深色／淺色</button></div>'
            '<p>ANSI 預覽提高不足的字色對比，原始檔保留原色。原則的唯一歸屬：<a href="../pane-design-language.md">pane-design-language R1–R7</a>。這裡保留來源與版本索引。</p>'
            '<h2 id="principles">設計原則</h2><div class="cards">'+p_html+'</div>'
            '<h2 id="current">現行 R6</h2><p>依 2026-11-25 lineage：畫面基準 6db7e47e；沙盒假資料重拍，不是操作員 live 資料。</p>'+version(live,by_id,True)
            +'<h2 id="drafts">設計稿</h2><p>缺稿：carbon 設計畫面、A2／B2 方案、操作員 band-v3-L1/L2/L3 圖、v7.5 設計稿。未重畫；待來源補齊。</p><div class="cards">'
            +''.join(draft(v) for v in drafts)+'</div>'
            +'<h2 id="versions">各版畫面</h2><div class="cards">'+''.join(version(v,by_id) for v in reversed(rows))+'</div>'
            +'<p><a href="shots/notes.log">拍攝紀錄（含失敗）</a> · <a href="versions.json">版本資料</a> · <a href="principles.json">原則資料</a></p>')
    doc = '<!doctype html><html lang="zh-Hant"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Pane 歷史</title><style>'+CSS+'</style></head><body><main class="wrap">'+body+'</main><div id="zoom" role="dialog" aria-label="原尺寸畫面"></div><script>'+JS+'</script></body></html>'
    (HERE/'index.html').write_text(doc)
    print(f'build: {len(principles)} principles, {len(rows)} versions, {len(drafts)} draft groups')


if __name__ == '__main__':
    main()
