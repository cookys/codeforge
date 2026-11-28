#!/usr/bin/env python3
# cjk-width.py — 逐列 CJK 寬度檢查：每列顯示寬度（寬字元=2）必須 ≤ pane 寬；附候選教訓列明細。
import re, sys, unicodedata as ud, glob, os
def dw(s): return sum(2 if ud.east_asian_width(ch) in 'WF' else 1 for ch in s)
S = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
SIZES = {'120x40': 120, '209x50': 209}
print('CJK 逐列寬度檢查（寬字元=2，基準=pane 寬；每列寬度必須 <= pane 寬）')
for path in sorted(glob.glob(os.path.join(S, '*', 'KR-*.txt'))) + sorted(glob.glob(os.path.join(S, '*', 'KR-after-*.txt'))):
    name = os.path.relpath(path, S)
    size = os.path.basename(path).replace('KR-', '').replace('.txt', '').split('-')[0]
    if size not in SIZES: continue
    cols = SIZES[size]
    rows = open(path, encoding='utf-8').read().split('\n')
    over = [(i + 1, dw(r)) for i, r in enumerate(rows) if r and dw(r) > cols]
    frame_rows = sorted({dw(r) for r in rows if r.startswith('│') or r.startswith('╭') or r.startswith('╰')})
    kr = [(i + 1, dw(r), re.sub(r'\x1b\[[0-9;]*m', '', r).strip()) for i, r in enumerate(rows) if '候選教訓' in r or ('‣' in r and '│' in r)]
    print('%s  pane=%d  框列寬=%s  超寬列=%s' % (name, cols, frame_rows, over if over else '無'))
    for i, w, t in kr[:2]:
        print('   row %d w=%d | %s' % (i, w, t[:66]))
