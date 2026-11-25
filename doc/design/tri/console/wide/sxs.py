"""單步驟示意並排頁（PM 01M8A18Z、01M8A1AK）：舊稿第 56 步 | 新稿 209x49 | 圖外說明欄。

用法：python3 sxs.py <舊稿 out 目錄> <新稿 out-wide 目錄> <輸出 html>
畫面內不放任何註記；區域與新增操作只寫在右邊的說明欄。
"""
import html
import json
import os
import sys

OLD, NEW, DEST = sys.argv[1:4]
HERE = os.path.dirname(os.path.abspath(__file__))
THEMES = json.load(open(os.path.join(HERE, '..', '..', 'brief', 'tools', 'cc-themes.json')))
LIGHT, DARK = THEMES[0], THEMES[2]
ROLES = ('text', 'inactive', 'subtle', 'suggestion', 'success', 'error', 'warning', 'merged', 'claude', 'promptBorder', 'remember')


def load(d, k):
    rows = json.load(open(os.path.join(d, k, 'roles.json')))
    return [[[t, ROLES.index(r) if r in ROLES else 0] for t, r, _ in row] for row in rows]


# 舊稿第 1 列是「設計稿註記」：新規則不准有標註，並排時一併拿掉，只比畫面本身
old = load(OLD, '209x49-s56')
old[0] = [[' ' * 209, 0]]
DATA = {'old': old, 'new': load(NEW, '209x49-s56')}

REGIONS = [
    ('右側 pane（94 欄，宿主上限）', [
        '上半：這張卡的全部步驟和網址，不用捲',
        '艦隊 8 個 agent 各一列：狀態、tok/s、走勢、在做什麼',
        '派工、審查進行中的輪次',
        '下半：其他 8 頁各一列摘要，前面是跳頁鍵',
    ]),
    ('底部 band（17 列，宿主上限）', [
        '左欄：4 張卡全列，每張已選哪個選項',
        '左欄下方：送出時會送哪幾張、哪幾張留著',
        '右欄：問題、建議、a/b/c 三個選項含理由',
        '右欄：m 附註、g 網址、s 送出直接列出',
        '最下兩列：全域狀態、9 個頁籤',
    ]),
    ('上方對話區（115 欄）', ['宿主的對話，唯讀，不放可按的東西（R1）']),
]
ADDED = [
    '4 張卡同時看得到選擇狀態（舊稿只看得到目前這張）',
    '步驟 3、4 不用 ↓ 捲（舊稿「下面還有 2 列」）',
    '送出前就看得到會送哪幾張（舊稿要按 s 才預覽）',
    '附註內容直接顯示（舊稿要按 m 才看得到）',
    '其他 8 頁的摘要常駐，數字鍵直接跳（舊稿要切頁）',
    '艦隊每個 agent 的 tok/s 常駐（舊稿要切到 7 艦隊）',
    '派工、審查進行中的輪次常駐（舊稿要切到 3、4）',
    'e 展開來龍去脈（舊稿常駐佔 10 列，新稿收起，R7）',
]
REMOVED = [
    '頂端「設計稿註記」列（不准有標註）',
    '常駐的來龍去脈框：收成 pane 裡一列，按 e 展開',
    'PgUp/PgDn 捲來龍去脈、↑↓ 捲步驟：內容都放得下，不用捲',
]


def ul(items):
    return '<ul>' + ''.join(f'<li>{html.escape(x)}</li>' for x in items) + '</ul>'


legend = ''.join(f'<h3>{html.escape(t)}</h3>{ul(xs)}' for t, xs in REGIONS)
legend += f'<h3>比舊稿多的操作</h3>{ul(ADDED)}<h3>拿掉的</h3>{ul(REMOVED)}'


def css(t):
    return ''.join(f'--{k}:{t[k]};' for k in ROLES)


page = f'''<!doctype html><html lang=zh-Hant><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Console 寬螢幕示意</title>
<style>
:root{{--bg:#fff;--fg:#1f2328;--mut:#57606a;--line:#d0d7de;--tbg:#fff;{css(LIGHT)}}}
:root[data-theme=dark]{{--bg:#0d1117;--fg:#e6edf3;--mut:#9da7b3;--line:#30363d;--tbg:#000;{css(DARK)}}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#0d1117;--fg:#e6edf3;--mut:#9da7b3;--line:#30363d;--tbg:#000;{css(DARK)}}}}}
body{{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:15px/1.6 system-ui,"Noto Sans TC",sans-serif}}
h1{{font-size:22px;margin:0 0 4px}} h2{{font-size:17px;margin:0 0 6px}} h3{{font-size:15px;margin:14px 0 2px}} ul{{margin:0;padding-left:20px}}
.mut{{color:var(--mut)}} .row{{display:flex;gap:24px;align-items:flex-start;overflow-x:auto}} .leg{{min-width:380px;max-width:420px}}
.term-grid{{background:var(--tbg);font:13px/15px "DejaVu Sans Mono","Noto Sans Mono CJK TC",monospace;box-shadow:0 0 0 4px var(--tbg);margin:4px;white-space:pre;display:flex;flex-direction:column;font-kerning:none;font-variant-ligatures:none}}
.term-grid .ln{{display:flex;flex:none;height:15px}} .term-grid i{{font-style:normal;display:inline-block;height:15px;line-height:15px;text-align:center;overflow:visible}} .term-grid i.n{{width:8px}} .term-grid i.w{{width:16px}}
{''.join(f'.r-{k}{{color:var(--{k})}}' for k in ROLES)}
</style>
<h1>Console 寬螢幕 · 單步驟示意</h1>
<div class=mut>操作員實測 209×49（tmux client 209×51、狀態列 2 列）· 同一步：2 待決 · 等你動手 · 步驟很長 · 左舊稿、中新稿，說明只寫在右欄</div>
<div class=row>
<div><h2>舊稿（第 56 步）</h2><div class=ph data-k=old></div></div>
<div><h2>新稿</h2><div class=ph data-k=new></div></div>
<div class=leg><h2>說明欄</h2>{legend}</div>
</div>
<script>
const D={json.dumps(DATA, ensure_ascii=False, separators=(',', ':'))};
const RL={json.dumps(list(ROLES))};
const W=c=>{{const p=c.codePointAt(0);return (p>=0x1100&&p<=0x115f)||(p>=0x2e80&&p<=0xa4cf)||(p>=0xac00&&p<=0xd7a3)||(p>=0xf900&&p<=0xfaff)||(p>=0xfe30&&p<=0xfe4f)||(p>=0xff00&&p<=0xff60)||(p>=0xffe0&&p<=0xffe6)}};
for(const ph of document.querySelectorAll('.ph')){{const g=document.createElement('div');g.className='term-grid';g.style.width=209*8+'px';
 for(const r of D[ph.dataset.k]){{const ln=document.createElement('div');ln.className='ln';for(const [t,ro] of r){{for(const ch of t){{const i=document.createElement('i');i.className=(W(ch)?'w':'n')+' r-'+RL[ro];i.textContent=ch;ln.appendChild(i)}}}}g.appendChild(ln)}}ph.appendChild(g)}}
const q=new URLSearchParams(location.search).get('theme');if(q)document.documentElement.dataset.theme=q;
</script>
'''
open(DEST, 'w').write(page)
print('wrote', DEST)
