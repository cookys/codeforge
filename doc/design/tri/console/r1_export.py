"""把 R1 rev5 設計稿（pane-history/drafts/r1-rev5）的 8 步，轉成 console 的 out 格式，讓 check3 和 overview 共用。

用法：python3 r1_export.py <輸出目錄>
每步每尺寸一個目錄 <W>x<H>-s<k>/，內含：
- screen.txt：純文字。
- roles.json：每列切成 [文字, 色彩角色, 對比類別] 的片段。
- press.json：有可按格子的列號，給 check3 的 R1 用。
- drawn.txt：只留設計稿畫的格子，實拍原樣的格子（宿主對話區、宿主的 [-]、現行 band 的字）換成等寬空白；check3 的 R7、網址、[-] 位置看這份。
- band.json：band 所在列（rev5 的 band 是一列，不是框）。
畫面內容由 r1-rev5 gen.py 的 overlay() 產生，與 r1-rev5-interactive.html 同源，這裡不另畫。
實拍格子（終端原色）換成 CC 深色主題裡色差最近的角色，深淺兩色才能用同一套角色上色。
"""
import importlib.util
import json
import os
import sys
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, '..', '..', 'pane-history', 'drafts', 'r1-rev5', 'gen.py')
THEMES = json.load(open(os.path.join(HERE, '..', 'brief', 'tools', 'cc-themes.json')))
DARK = THEMES[2]
ROLES = ('text', 'inactive', 'subtle', 'suggestion', 'success', 'error', 'warning', 'merged', 'claude', 'remember')

spec = importlib.util.spec_from_file_location('r1rev5', SRC)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def rgb(s):
    return tuple(int(x) for x in s[4:-1].split(','))


PAL = {r: rgb(DARK[r]) for r in ROLES}


def role_of(c):
    if c.role in PAL:
        return c.role
    if c.fg is None:
        return 'inactive' if c.dim else 'text'
    return min(PAL, key=lambda r: sum((a - b) ** 2 for a, b in zip(PAL[r], c.fg)))


def need(ch, c):
    if not c.role:  # 實拍原樣保留的格子（宿主畫面、現行 band）：r1-rev5 不改實拍顏色，check3 不判它的對比
        return 'live'
    return 'text' if ch.isalnum() or unicodedata.east_asian_width(ch) in 'WF' else 'glyph'


OUT = sys.argv[1]
SIZE_OF = {'200x50': '200x50', '120x40': '120x40', '80x24': '80x24'}
flow = []
for size in m.SIZES:
    first, _ = m.read_path(size, 0)
    sp = m.layout(size, first)
    for k, (cur, choice, mode, p) in enumerate(m.PATH_STATES, 1):
        base, _ = m.read_path(size, p)
        rows = m.overlay(base, sp, cur, choice, mode, '先跑 canary' if cur == 1 and p >= 4 else '', (1,) if p >= 7 else ())
        segs, press, drawn = [], [], []
        for y, row in enumerate(rows):
            line = []
            for c in row:
                if c is None:
                    continue
                rk = (role_of(c), need(c.ch, c))
                if line and (line[-1][1], line[-1][2]) == rk:
                    line[-1][0] += c.ch
                else:
                    line.append([c.ch, *rk])
                if c.press and y not in press:
                    press.append(y)
            segs.append(line)
            drawn.append(''.join(t if k != 'live' else ' ' * sum(2 if unicodedata.east_asian_width(ch) in 'WF' else 1 for ch in t) for t, _, k in line))
        d = os.path.join(OUT, f'{size}-s{k}')
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, 'screen.txt'), 'w').write('\n'.join(m.plain(r) for r in rows) + '\n')
        json.dump(segs, open(os.path.join(d, 'roles.json'), 'w'), ensure_ascii=False)
        json.dump(press, open(os.path.join(d, 'press.json'), 'w'))
        open(os.path.join(d, 'drawn.txt'), 'w').write('\n'.join(drawn) + '\n')
        json.dump([y for y, r in enumerate(rows) if '要你決定' in m.plain(r)], open(os.path.join(d, 'band.json'), 'w'))
for k, t in enumerate(m.STEPS, 1):
    flow.append({'step': k, 'id': f'R1-{k}', 'tab': 'R1', 'state': m.PATH_STATES[k - 1][2], 'title': t,
                 'key_to_next': '', 'next': k + 1 if k < 8 else None})
json.dump({'steps': flow, 'source': 'pane-history/drafts/r1-rev5/gen.py PATH_STATES'}, open(os.path.join(OUT, 'flow.json'), 'w'), ensure_ascii=False, indent=1)
print('ok', len(flow), 'steps x', len(m.SIZES), 'sizes')
