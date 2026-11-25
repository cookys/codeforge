"""右側 pane 每步填了幾列（PM 01M8A52M：目標每步 >= 32/40）。用法：python3 pane_fill.py <out-wide-all> [門檻]"""
import os, re, sys, unicodedata
D, need = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 32
def cut(l, a, b):
    out, x = [], 0
    for c in l:
        if a <= x < b:
            out.append(c)
        x += 2 if unicodedata.east_asian_width(c) in 'WF' else 1
    return ''.join(out)
bad, rows = [], []
for name in sorted(os.listdir(D), key=lambda n: int(n.split('-s')[1]) if '-s' in n else 0):
    if not re.fullmatch(r'209x49-s\d+', name):
        continue
    L = open(os.path.join(D, name, 'screen.txt')).read().split('\n')
    n = sum(1 for l in L[1:41] if cut(l, 116, 208).strip())
    rows.append((name.split('-s')[1], n))
    if n < need:
        bad.append(name)
print(' '.join(f's{k}:{n}' for k, n in rows))
print('min', min(n for _, n in rows), 'median', sorted(n for _, n in rows)[len(rows) // 2], 'below', need, ':', len(bad))
sys.exit(1 if bad else 0)
