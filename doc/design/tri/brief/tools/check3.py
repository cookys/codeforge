#!/usr/bin/env python3
"""第三輪設計稿機械檢查：寬度、框線欄位、R1、R6、R7、R8 預覽與勾選一致、深淺對比。輸出 JSON，任何一項失敗結束碼 1。"""
import json, os, re, sys, unicodedata

D = sys.argv[1]
THEMES = json.load(open(os.environ.get('CC_THEMES') or os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cc-themes.json')))
TH = {'light': (THEMES[0], (255, 255, 255)), 'dark': (THEMES[2], (0, 0, 0))}
NEED = {'text': 4.5, 'glyph': 3.0}

def cw(c):
    return 2 if unicodedata.east_asian_width(c) in ('W', 'F') else 1

def dw(s):
    return sum(cw(c) for c in s)

def lum(rgb):
    def ch(v):
        v /= 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = rgb
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)

def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)

def rgb(s):
    return tuple(int(x) for x in s[4:-1].split(','))

def cols_of(line, chars):
    out, x = [], 0
    for c in line:
        if c in chars:
            out.append(x)
        x += cw(c)
    return out

report, fail = {}, False
for name in sorted(os.listdir(D)):
    p = os.path.join(D, name)
    if not os.path.isdir(p) or not re.fullmatch(r'\d+x\d+(-.*)?', name):
        continue
    W, H = (int(x) for x in name.split('-')[0].split('x'))
    lines = open(os.path.join(p, 'screen.txt')).read().rstrip('\n').split('\n')
    roles = json.load(open(os.path.join(p, 'roles.json')))
    r = {'rows': len(lines), 'width_bad': [i for i, l in enumerate(lines) if dw(l) != W]}
    # 框線：每個框的左右邊同一欄
    frames, open_ = [], []
    for i, l in enumerate(lines):
        for x in cols_of(l, '╭'):
            right = cols_of(l, '╮')
            rx = min(c for c in right if c > x)
            open_.append({'top': i, 'left': x, 'right': rx})
        for f in list(open_):
            if i > f['top']:
                ls, rs = cols_of(l, '│╰'), cols_of(l, '│╯')
                if f['left'] in cols_of(l, '╰'):
                    f['bottom'] = i
                    f['ok_bottom'] = f['right'] in cols_of(l, '╯')
                    frames.append(f); open_.remove(f)
                elif not (f['left'] in ls and f['right'] in rs):
                    f.setdefault('misaligned', []).append(i)
    r['frames'] = [{k: f[k] for k in ('top', 'bottom', 'left', 'right')} | {'misaligned': f.get('misaligned', []), 'bottom_ok': f['ok_bottom']} for f in frames]
    frame_bad = len(open_) + sum(len(f.get('misaligned', [])) + (0 if f['ok_bottom'] else 1) for f in frames)
    r['frame_bad'] = frame_bad
    # R1：可按的鍵（merged 角色）只出現在下半部
    clickable_rows = sorted({i for i, row in enumerate(roles) for s, role, k in row if role == 'merged' and s.strip()})
    r['R1_clickable_rows'] = clickable_rows
    r['R1_upper_clickable'] = [i for i in clickable_rows if i < H // 2]
    # R6：選項字母 a/b/c 依序對應第 1/2/3 個選項
    # 整列搜尋：寬螢幕的選項列左邊是 index，字母不在行首，要找框線（│）後面的「x:」
    opt = [m.group(1) for l in lines for m in [re.search(r'│\s*([a-e]): ', l)] if m]
    r['R6_letters'] = opt
    r['R6_ok'] = opt == ['a', 'b', 'c', 'd', 'e'][:len(opt)]  # 沒有選項的畫面（平常、艦隊頁）不適用
    # R7：band（最後一個框）裡不出現 背景／卡住／來源／sha
    band = frames[-1] if frames else None
    band_text = '\n'.join(lines[band['top']:band['bottom'] + 1]) if band else ''
    r['R7_band_extra'] = [w for w in ('背景', '卡住', '來源', 'sha') if w in band_text]
    # (PM 01M89CD1-1) 網址不可從中間斷：畫面上每段網址要嘛是完整網址，要嘛是完整網址的開頭加 …
    full = json.load(open(os.path.join(p, 'urls.json'))) if os.path.isfile(os.path.join(p, 'urls.json')) else []
    urlbad = []
    for i, l in enumerate(lines):
        for m in re.finditer(r'[^\s│]*(?:://|[?&=])[^\s│]*', l):
            t = m.group(0).rstrip('，。；：')
            if t in full or (t.endswith('…') and any(u.startswith(t[:-1]) for u in full)):
                continue
            urlbad.append([i, t])
    r['url_broken'] = urlbad
    # (PM 01M89CD1-2) [-] 收合鍵要在某個框裡（框的上下列之間、左右邊之間）
    outside = []
    for i, l in enumerate(lines):
        for m in re.finditer(r'\[-\]', l):
            x = dw(l[:m.start()])
            if not any(f['top'] <= i <= f['bottom'] and f['left'] < x and x + 3 <= f['right'] for f in frames):
                outside.append([i, x])
    r['ctrl_outside_frame'] = outside
    # 對比
    con = {}
    for th, (t, bg) in TH.items():
        seen = {}
        for row in roles:
            for s, role, k in row:
                if not s.strip():
                    continue
                v = round(ratio(rgb(t[role]), bg), 2)
                key = f'{role}/{k}'
                seen[key] = {'ratio': v, 'need': NEED[k], 'ok': v >= NEED[k]}
        con[th] = seen
    r['contrast'] = con
    r['contrast_bad'] = [f'{th}:{k}' for th, s in con.items() for k, v in s.items() if not v['ok']]
    # R8：送出預覽只列清單上打勾（✓N）的卡，標題的張數和編號也要一致（PM 01M89WW3：預覽列了沒選的第 3 張）
    r['R8_preview'] = None
    hi = next((i for i, l in enumerate(lines) if re.search(r'送出預覽 · \d+ 張', l)), None)
    if hi is not None:
        def cut(l, col):  # 從第 col 個顯示欄切到行尾
            c = 0
            for k, ch in enumerate(l):
                if c >= col:
                    return l[k:]
                c += cw(ch)
            return ''
        x0 = dw(lines[hi][:re.search(r'送出預覽 · \d+ 張', lines[hi]).start()])
        m = re.search(r'送出預覽 · (\d+) 張（([\d、]+)）', lines[hi])
        head = [int(n) for n in m.group(2).split('、')] if m else []
        shown = []
        for l in lines[hi + 1:]:
            if '╰' in l[:2] or not l.startswith('│') and not l.startswith(' '):
                break
            mm = re.match(r'(\d) \S', cut(l, x0))
            if mm:
                shown.append(int(mm.group(1)))
        checked = sorted({int(n) for l in lines for n in re.findall(r'✓(\d)', cut(l, 0)[:x0] if x0 > 4 else l)})
        r['R8_preview'] = {'checked': checked, 'header': head, 'shown': shown,
                           'ok': bool(m) and int(m.group(1)) == len(head) and head == checked and set(shown) <= set(checked)}
    r8_bad = r['R8_preview'] is not None and not r['R8_preview']['ok']
    ok = not r['width_bad'] and frame_bad == 0 and not r['R1_upper_clickable'] and r['R6_ok'] and not r['R7_band_extra'] and not r['contrast_bad'] and not urlbad and not outside and not r8_bad
    r['ok'] = ok
    fail |= not ok
    report[name] = r
json.dump(report, sys.stdout, ensure_ascii=False, indent=1)
print()
for n, r in report.items():
    print(f"{n}: rows {r['rows']} width_bad {len(r['width_bad'])} frames {len(r['frames'])} frame_bad {r['frame_bad']} "
          f"R1_upper {len(r['R1_upper_clickable'])} R6 {r['R6_ok']} R7_extra {r['R7_band_extra']} contrast_bad {r['contrast_bad']} url_broken {len(r['url_broken'])} ctrl_outside {len(r['ctrl_outside_frame'])} R8 {r['R8_preview']} -> {'OK' if r['ok'] else 'FAIL'}", file=sys.stderr)
sys.exit(1 if fail else 0)
