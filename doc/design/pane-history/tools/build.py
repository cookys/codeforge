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




def _plain(line: str) -> str:
    return B.CSI_RE.sub("", B.OSC_RE.sub("", line))


def crop_pane(lines: list) -> list:
    """只留 pane 範圍：從 Claude Code 標頭之後第一個框線角開始，到提示列 ❯ 上方的分隔線為止。"""
    plain = [_plain(x) for x in lines]
    start = next((i for i, t in enumerate(plain) if i > 3 and any(c in t for c in "┌╭")), None)
    prompt = next((i for i, t in enumerate(plain) if t.lstrip().startswith("❯")), None)
    if start is None or prompt is None or prompt <= start:
        return lines
    end = prompt
    while end > start and (not plain[end - 1].strip() or set(plain[end - 1].strip()) <= set("─")):
        end -= 1
    return lines[start:end]


def grid_html(raw: str, crop: bool = False) -> str:
    """ANSI → 固定格子 HTML；每列補空白到最寬那列的欄數。crop=True 只留 pane 範圍。"""
    text = raw.replace("\r\n", "\n").replace("\r", "\n")
    lines = text.split("\n")
    if crop:
        lines = crop_pane(lines)
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
CSS = '\n:root{--bg:#0f1217;--card:#161b24;--line:#2f3747;--fg:#e5e9f0;--mut:#b4bdcc;--acc:#ebcb8b;--accfg:#14161b;--link:#88c0d0;--guess:#e8a48c;}\n:root[data-theme="light"]{--bg:#ffffff;--card:#f6f8fa;--line:#c9d1d9;--fg:#1f2328;--mut:#4d5560;--acc:#9a6700;--accfg:#ffffff;--link:#0550ae;--guess:#a40e26;}\n*{box-sizing:border-box}\nbody{margin:0;padding:16px;background:var(--bg);color:var(--fg);\n font-family:"Noto Sans TC","PingFang TC","Microsoft JhengHei",system-ui,sans-serif;\n font-size:15px;line-height:1.7;line-break:strict;word-break:normal;overflow-wrap:anywhere}\n.wrap{max-width:1240px;margin:0 auto}\nh1{font-size:20px;line-height:1.6;margin:0 0 4px}\n.ask{font-size:16px;margin:0 0 4px}\n.ask b{color:var(--acc)}\n.live{margin:0 0 14px;color:var(--mut)}\n.live b{color:var(--fg)}\n.bar{display:flex;justify-content:flex-end;gap:8px;margin:-36px 0 12px}\nbutton{font:inherit;font-size:13px;line-height:1.6;background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:4px;padding:2px 10px;cursor:pointer}\n.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(380px,1fr));gap:14px}\n.card{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:10px 12px}\n.card.is-live{border:2px solid var(--acc)}\n.name{font-size:15px;line-height:1.7;margin:0 0 6px}\n.no{font-weight:700;white-space:nowrap;margin-right:6px}\n.badge{background:var(--acc);color:var(--accfg);border-radius:3px;padding:0 6px;font-size:12px;margin-left:6px;white-space:nowrap}\n.thumb{height:230px;overflow:hidden;position:relative;border-radius:4px;background:#14161b;cursor:zoom-in}\n.thumb .term-grid{position:absolute;left:0;top:0;transform-origin:0 0}\n.noshot{height:230px;display:flex;align-items:center;justify-content:center;border:1px dashed var(--line);border-radius:4px;color:var(--mut);font-size:14px;padding:12px;text-align:center}\ndetails{margin-top:6px;font-size:13px;line-height:1.7;color:var(--mut)}\nsummary{cursor:pointer}\ndetails dl{display:grid;grid-template-columns:5em 1fr;gap:0 8px;margin:6px 0 0}\ndetails dt{color:var(--mut)}details dd{margin:0;color:var(--fg)}\ncode{font-family:"JetBrains Mono","DejaVu Sans Mono",monospace;color:var(--link)}\n.guess{color:var(--guess)}\npre,.doc{white-space:pre-wrap;font-size:12px;line-height:1.6;background:var(--card);border:1px solid var(--line);border-radius:4px;padding:8px;color:var(--fg)}\n.foot{margin-top:18px}\n/* 終端格子：每個字元固定 1ch / 2ch，不依賴字型寬度 */\n.term-grid{font-family:"DejaVu Sans Mono","Noto Sans Mono CJK TC",monospace;font-size:13px;line-height:17px;color:#d8dee9;background:#14161b;white-space:nowrap;line-break:auto}\n.term-grid .ln{display:block;height:17px;white-space:pre}\n.term-grid i{display:inline-block;font-style:normal;height:17px;overflow:hidden;text-align:center;vertical-align:top}\n.term-grid i.n{width:8px}.term-grid i.w{width:16px}\n#zoom{display:none;position:fixed;inset:0;z-index:50;background:rgba(0,0,0,.8);overflow:auto;padding:24px;cursor:zoom-out}\n#zoom.on{display:block}\n#zoom .term-grid{position:static;transform:none}\n@media (max-width:700px){.cards{grid-template-columns:1fr}.bar{margin:0 0 10px;justify-content:flex-start}.thumb,.noshot{height:200px}}\n\n.cards{grid-template-columns:repeat(auto-fit,minmax(min(100%,380px),1fr))}\n.card{min-width:0} h2{margin-top:2em} h3{font-size:16px;line-height:1.7}\na{color:var(--link)} img{max-width:100%;height:auto}\nblockquote{margin:8px 0;padding:0 12px;border-left:3px solid var(--line)}\n.term-grid{font-family:"DejaVu Sans Mono","Noto Sans TC","PingFang TC","Microsoft JhengHei",monospace;line-height:1.7;background:#14161b;color:#d8dee9}\n.term-grid .ln,.term-grid i{height:1.7em}.term-grid i.n{width:1ch}.term-grid i.w{width:2ch}\n.thumb{height:230px}.hero .thumb{height:540px}\n.shot-light .term-grid{background:#fff;color:#1f2328}.shot-light{background:#fff}\n.theme-light{display:none}:root[data-theme="light"] .theme-dark{display:none}:root[data-theme="light"] .theme-light{display:block}\n#zoom .term-grid{transform-origin:0 0}\n@media(max-width:700px){.hero .thumb{height:240px}}\n\n.toc{font-size:16px;margin:0 0 8px;padding:8px 12px;border:1px solid var(--line);border-radius:6px;background:var(--card)}\n.note{color:var(--mut);font-size:14px}\n.plist{list-style:none;padding:0;margin:0}.plist li{padding:8px 0;border-bottom:1px solid var(--line)}\n.pq{color:var(--mut);font-size:14px}.src{margin-left:8px}.miss{color:var(--guess);font-weight:700}\n.plist details{margin-top:2px}\n.vlist{display:flex;flex-direction:column;gap:14px}\n.vrow{display:flex;gap:16px;align-items:flex-start;background:var(--card);border:1px solid var(--line);border-radius:6px;padding:10px 12px}\n.vrow .vtext{flex:1 1 280px;min-width:0}.vrow .vshot{flex:0 0 760px;max-width:100%;min-width:0}\n.vrow.hero{flex-direction:column;gap:6px}.vrow.hero .vtext{flex:none;width:100%}.vrow.hero .vshot{flex:none;width:100%}\n.what{font-size:14px;margin:0}\n.vshot .thumb,.hero .thumb,.vrow details .thumb{height:auto}figure{margin:6px 0}figcaption{font-size:13px;color:var(--mut)}.vshot .noshot{height:120px}\n@media(max-width:1100px){.vrow{flex-direction:column}.vrow .vtext{flex:none;width:100%}.vrow .vshot{flex:none;width:100%}}\n'
CSS += "\n.term-grid{display:flex;flex-direction:column;white-space:pre}.term-grid .ln{flex:none}\n"
CSS += "\npre,h1,button{line-height:1.7}figure{margin:12px 0}\n"
CSS += "\n.term-grid{letter-spacing:calc(8px - 1ch);font-kerning:none;font-variant-ligatures:none}.term-grid i.n{width:8px}.term-grid i.w{width:16px}\n"
JS = "\n(function(){\nvar R=document.documentElement,t=null;try{t=localStorage.getItem('pane-history-theme')}catch(e){}\nif(!t)t=(window.matchMedia&&matchMedia('(prefers-color-scheme: light)').matches)?'light':'dark';\nvar q=new URLSearchParams(location.search);if(q.get('theme'))t=q.get('theme');R.setAttribute('data-theme',t);\ndocument.getElementById('theme').onclick=function(){t=R.getAttribute('data-theme')==='light'?'dark':'light';R.setAttribute('data-theme',t);try{localStorage.setItem('pane-history-theme',t)}catch(e){};fit();};\nfunction fit(){document.querySelectorAll('.thumb').forEach(function(b){var g=b.firstElementChild;if(!g.offsetWidth)return;var auto=b.closest('.vshot,.hero,.vrow details');var s=auto?Math.min(1,b.clientWidth/g.offsetWidth):Math.min(b.clientWidth/g.offsetWidth,b.clientHeight/g.offsetHeight);g.style.transform='scale('+s+')';if(auto)b.style.height=Math.ceil(g.offsetHeight*s)+'px';});}\ndocument.addEventListener('toggle',fit,true);new MutationObserver(fit).observe(R,{attributes:true,attributeFilter:['data-theme']});fit();if(document.fonts)document.fonts.ready.then(fit);addEventListener('resize',fit);\nif(q.get('only')){var keep=false;[].forEach.call(document.querySelector('main').children,function(e){if(e.tagName==='H2')keep=(e.id===q.get('only'));if(!keep&&e.tagName!=='H1')e.style.display='none';});fit();}\nvar z=document.getElementById('zoom');\ndocument.querySelectorAll('.thumb').forEach(function(b){b.onclick=function(){z.innerHTML='';var c=b.firstElementChild.cloneNode(true);z.appendChild(c);z.classList.add('on');c.style.transform='scale('+Math.min(1,(z.clientWidth-48)/c.offsetWidth)+')';};});\nz.onclick=function(){z.classList.remove('on');z.innerHTML='';};\naddEventListener('keydown',function(e){if(e.key==='Escape')z.onclick();});\nif(q.get('measure')){\n var grids=[].slice.call(document.querySelectorAll('.thumb .term-grid')).map(function(g){var w={};[].forEach.call(g.querySelectorAll('.ln'),function(l){w[l.offsetWidth]=1});var cd=g.closest('[data-no]');return {card:cd?cd.dataset.no:'',scale:g.style.transform,cols:+g.dataset.cols,lineWidths:Object.keys(w).map(Number)};});\n function lh(sel){var e=document.querySelector(sel);if(!e)return null;var c=getComputedStyle(e);return {sel:sel,font:c.fontFamily,fs:parseFloat(c.fontSize),lh:parseFloat(c.lineHeight),ratio:+(parseFloat(c.lineHeight)/parseFloat(c.fontSize)).toFixed(3),lineBreak:c.lineBreak};}\n var o={theme:t,scrollW:R.scrollWidth,clientW:R.clientWidth,pageH:R.scrollHeight,\n  gridsAllOneWidth:grids.every(function(g){return g.lineWidths.length===1}),grids:grids,\n  text:[lh('body'),lh('.ask'),lh('.name'),lh('.live'),lh('details'),lh('details dd'),lh('.noshot')]};\n var p=document.createElement('pre');p.id='measure-out';p.textContent=JSON.stringify(o);document.body.appendChild(p);}\n})();\n"

def esc(value):
    return html.escape(str(value), quote=True)


def load(name):
    return json.loads((HERE / (name + '.json')).read_text())


def quote_html(quotes):
    return ''.join(f'<blockquote>{"PM 轉述：" if q.get("relayed") else ""}{esc(q["quote"])}<footer>{esc(q["date"])} · {esc(q["ref"])}</footer></blockquote>' for q in quotes)


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
    rendered = grid_html(raw, crop=True)
    DEFAULT_FG, DEFAULT_BG = old
    return f'<div class="thumb shot-{theme}">{rendered}</div>'


def _notes():
    r = {}
    f = HERE / 'shots' / 'notes.txt'
    for line in (f.read_text().splitlines() if f.is_file() else []):
        m = re.match(r'(\w+)/(dark|light) FAIL (.*)', line)
        if m:
            r[(m.group(1)[:8], m.group(2))] = m.group(3).split(':')[0] if 'RuntimeError' not in m.group(3) else m.group(3)
    return r


NOTES = None


def why_missing(v, theme):
    global NOTES
    if NOTES is None:
        NOTES = _notes()
    reason = NOTES.get(((v.get('ap_sha') or '')[:8], theme))
    if not reason:
        return '沒有拍攝紀錄'
    reason = reason.replace('RuntimeError: autopilot-dev 找不到 commit', 'autopilot clone 裡沒有這個 commit（分支未合併、未推）：')
    reason = reason.replace('/live-pane 與 /live 皆未能開啟面板', '這版還沒有 /live-pane 指令，開不出面板')
    return reason


def shots(v):
    out = ''
    for theme in ('dark', 'light'):
        path = v['shots'].get(theme)
        out += f'<div class="theme-{theme}">' + (picture(path, theme) if path else '<p class="noshot">沒有畫面：' + esc(why_missing(v, theme)) + '</p>') + '</div>'
    return out


def plabel(p):
    return p['in_R'] or (p['id'] + ' ' + p['rule'].split('：')[0][:10])


def version(v, principles, hero=False):
    linked = '、'.join(f'<a href="#principle-{esc(p)}">{esc(plabel(principles[p]))}</a>' if p in principles else esc(p) for p in v['principles'])
    return (f'<article class="vrow {"hero" if hero else ""}" id="{"current-r6" if hero else "version-"+esc(v["id"])}" data-no="{esc(v["id"])}">'
            f'<div class="vtext"><h3 class="name">{esc(v["plain"])}</h3>'
            f'<p class="what">改了：{esc(v["changed"])}<br>原則：{linked or "（早於原則表，或這版沒有對應的原則）"}</p>'
            '<details><summary>版本與來源細節</summary>'
            f'<p>{esc(v["name"])} · {esc(v["date"])} · {esc(v["kind"])} · <code>{esc(v["ap_sha"])}</code></p>'
            + quote_html(v['operator_feedback'])
            + ''.join(f'<p>{esc(s)}</p>' for s in v['sources'])
            + ''.join(f'<p><a href="{esc(p)}">{t} 原始 ANSI</a></p>' for t,p in v['shots'].items() if p)
            + f'</details></div><div class="vshot">{shots(v)}</div></article>')


def draft(v):
    directory = HERE / 'drafts' / v['id']
    meta = json.loads((directory / 'meta.json').read_text())
    files = sorted(directory.glob('*.png')) + sorted(directory.glob('steps/*.png')) + sorted(directory.glob('design-*.txt'))
    figs = []
    for file in files:
        rel = str(file.relative_to(HERE))
        figs.append(f'<figure><figcaption>{esc(file.name)} · <a href="{esc(rel)}">原檔</a></figcaption>{picture(rel)}</figure>')
    mds = sorted(directory.glob('*.md'))
    if not figs:
        for file in mds:
            for block in re.findall(r'```[^\n]*\n(.*?)```', file.read_text(), re.S):
                if any(c in block for c in '┌╭│'):
                    figs.append(f'<figure><figcaption>{esc(file.name)}</figcaption><div class="thumb">{grid_html(block)}</div></figure>')
                    break
    first = figs[0] if figs else '<p class="noshot">這組只有文字說明</p>'
    rest = (f'<details><summary>其餘 {len(figs) - 1} 張</summary>' + ''.join(figs[1:]) + '</details>') if len(figs) > 1 else ''
    docs = ''.join(f'<details><summary>{esc(f.name)} 全文</summary><div class="doc">{esc(f.read_text())}</div></details>' for f in mds)
    kind = {'draft': '設計稿', 'rejected': '被退回'}.get(v['kind'], v['kind'])
    return (f'<article class="vrow hero" data-no="{esc(v["id"])}"><div class="vtext"><h3 class="name">{esc(v["plain"] or v["name"])}（{kind}）</h3>'
            f'<p class="what">{esc(v.get("changed") or "")}<br>原則：' + ('、'.join(esc(x) for x in v['principles']) or '—') + f'</p></div><div class="vshot">{first}</div><div class="vtext">{rest}{docs}'
            '<details><summary>設計稿出處</summary><div class="doc">' + esc(json.dumps(meta, ensure_ascii=False, indent=2)) + '</div></details></div></article>')


def main():
    versions = load('versions')
    principles = load('principles')
    by_id = {p['id']:p for p in principles}
    rows = [v for v in versions if v['ap_sha']]
    live = next(v for v in rows if v['ap_sha'].startswith('6db7e47e'))
    p_html = ''
    for p in principles:
        refs = '、'.join(f'<a href="#version-{esc(v)}">{esc(v)}</a>' for v in p['versions']) or '尚無對應版本'
        q = p['quotes'][0] if p['quotes'] else None
        src = (f'「{esc(q["quote"])}」<span class="src">{esc(q["date"])}</span>' if q and p['status'] != '缺出處'
               else '<span class="miss">缺出處</span>' + (f'（PM 轉述：「{esc(q["quote"])}」）' if q else ''))
        p_html += (f'<li id="principle-{esc(p["id"])}"><b>{esc(p["in_R"] or p["id"])}</b> {esc(p["rule"])}'
                   f'<div class="pq">{src}</div>'
                   f'<details><summary>出處與對應版本</summary>' + quote_html(p['quotes']) + f'<p>對應版本：{refs}</p></details></li>')
    drafts = [v for v in versions if not v['ap_sha']]
    body = ('<h1>Pane 設計與版本歷史</h1><div class="bar"><button id="theme" type="button">深色／淺色</button></div>'
            '<nav class="toc"><a href="#principles">1 原則</a> → <a href="#current">2 現行 R6（基底）</a> → <a href="#drafts">3 設計稿</a> → <a href="#versions">4 各版</a></nav>'
            '<h2 id="principles">1 設計原則</h2><p class="note">唯一歸屬：<a href="../pane-design-language.md">pane-design-language R1–R7</a>；R 表以外的條目待操作員收進。</p><ul class="plist">'+p_html+'</ul>'
            '<h2 id="current">2 現行 R6（操作員選定的基底，d-9e5cacc1）</h2>'+version(live,by_id,True)
            +'<h2 id="drafts">3 設計稿</h2><p class="note">缺稿：carbon 設計畫面、A2／B2 方案、操作員 band-v3-L1/L2/L3 圖、v7.5 設計稿。</p><div class="vlist">'
            +''.join(draft(v) for v in drafts)+'</div>'
            +'<h2 id="versions">4 各版畫面（新到舊）</h2><div class="vlist">'+''.join(version(v,by_id) for v in reversed(rows) if v is not live)+'</div>'
            +'<p class="note"><a href="shots/notes.txt">拍攝紀錄（含失敗）</a> · <a href="versions.json">版本資料</a> · <a href="principles.json">原則資料</a></p>')
    doc = '<!doctype html><html lang="zh-Hant"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Pane 歷史</title><style>'+CSS+'</style></head><body><main class="wrap">'+body+'</main><div id="zoom" role="dialog" aria-label="原尺寸畫面"></div><script>'+JS+'</script></body></html>'
    (HERE/'index.html').write_text(doc)
    print(f'build: {len(principles)} principles, {len(rows)} versions, {len(drafts)} draft groups')


if __name__ == '__main__':
    main()
