#!/usr/bin/env python3
import re, sys, unicodedata as ud
def dw(s): return sum(2 if ud.east_asian_width(ch) in 'WF' else 1 for ch in s)
def lum(c):
    f = lambda x: (x/255)/12.92 if x/255 <= 0.04045 else (((x/255)+0.055)/1.055)**2.4
    return 0.2126*f(c[0]) + 0.7152*f(c[1]) + 0.0722*f(c[2])
def cr(a, b):
    x, y = lum(a), lum(b)
    return (max(x, y)+0.05)/(min(x, y)+0.05)
def xterm(n):
    if n < 16:
        BASE = [(0,0,0),(205,0,0),(0,205,0),(205,205,0),(0,0,238),(205,0,205),(0,205,205),(229,229,229),(127,127,127),(255,0,0),(0,255,0),(255,255,0),(92,92,255),(255,0,255),(0,255,255),(255,255,255)]
        return BASE[n]
    if n < 232:
        n -= 16; v = [0,95,135,175,215,255]
        return (v[n//36], v[(n//6)%6], v[n%6])
    g = 8 + (n-232)*10
    return (g,g,g)
path, bgname = sys.argv[1], sys.argv[2]
bg = (0x1E,0x1E,0x1E) if bgname == 'dark' else (255,255,255)
rows = open(path, encoding='utf-8').read().splitlines()
cols = {}
for line in rows:
    for m in re.finditer(r'(38|48);5;(\d+)', line):
        cols[xterm(int(m.group(2)))] = m.group(2)
    for m in re.finditer(r'38;2;(\d+);(\d+);(\d+)', line):
        cols[tuple(map(int, m.groups()))] = 'rgb'
print(f'{path}（{bgname} 底）：抽出 {len(cols)} 色')
for c, tag in sorted(cols.items(), key=lambda kv: cr(kv[0], bg)):
    print('  #%02x%02x%02x (code %s) = %.2f' % (c[0], c[1], c[2], tag, cr(c, bg)))
print('  低於 3.0：', [('#%02x%02x%02x'%c, round(cr(c,bg),2)) for c in cols if cr(c,bg) < 3.0] or '無')
