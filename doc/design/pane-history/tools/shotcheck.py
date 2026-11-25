#!/usr/bin/env python3
"""截圖自我檢查（只用標準庫）：
1. 空白：最多那個顏色超過 99% 像素 → FAIL。
2. 列距（有 geometry JSON 時）：每段直框線（╭…╰ 同欄）在圖上從頭到尾要連續，中間出現背景色像素就是行距被撐開 → FAIL。
用法：shotcheck.py <png> [<geometry.json>]；任何一項失敗結束碼 1。"""
import sys, zlib, struct, collections, json
def decode(path):
    d = open(path, 'rb').read(); assert d[:8] == b'\x89PNG\r\n\x1a\n'
    i, idat = 8, b''
    while i < len(d):
        n, t = struct.unpack('>I4s', d[i:i+8]); c = d[i+8:i+8+n]; i += 12 + n
        if t == b'IHDR': w, h, bd, ct = struct.unpack('>IIBB', c[:10])
        elif t == b'IDAT': idat += c
    bpp = {2: 3, 6: 4}[ct]; raw = zlib.decompress(idat); st = w * bpp; prev = bytearray(st); p = 0; rows = []
    for _ in range(h):
        f = raw[p]; line = bytearray(raw[p+1:p+1+st]); p += 1 + st
        for x in range(st):
            a = line[x-bpp] if x >= bpp else 0; b = prev[x]; cc = prev[x-bpp] if x >= bpp else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - cc), abs(a - cc), abs(a + b - 2 * cc)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else cc)) & 255
        prev = line; rows.append(bytes(line))
    return w, h, bpp, rows
png = sys.argv[1]; w, h, bpp, rows = decode(png)
px = lambda x, y: rows[y][x*bpp:x*bpp+3]
cnt = collections.Counter(px(x, y) for y in range(0, h, 2) for x in range(0, w, 4))
top, n = cnt.most_common(1)[0]; share = n / sum(cnt.values())
res = {'png': png.rsplit('/', 1)[-1], 'blank_share': round(share, 4), 'blank_ok': share <= 0.99}
if len(sys.argv) > 2:
    geo = json.load(open(sys.argv[2])); segs = []; worst = 0
    for g in geo['grids']:
        for col, y0, y1 in g['segs']:
            x = int(g['left'] + (col * 8 + 4) * g['scale'])
            ya = int(g['top'] + (y0 + 0.5) * g['pitch']); yb = int(g['top'] + (y1 + 0.5) * g['pitch'])
            bg = px(int(g['left'] + (col * 8 + 1) * g['scale']), ya - 2)  # 框線左邊的背景
            # 在框線那一格裡（±2px）找有沒有任何一列整段都是背景色
            gap = run = 0
            for y in range(ya, yb):
                hit = any(px(xx, y) != bg for xx in range(x - 2, x + 3))
                run = 0 if hit else run + 1; gap = max(gap, run)
            segs.append({'col': col, 'rows': [y0, y1], 'max_gap_px': gap}); worst = max(worst, gap)
    res.update({'pitch_px': [round(g['pitch'], 2) for g in geo['grids']], 'frame_segments': len(segs), 'max_gap_px': worst, 'gap_ok': worst == 0 and len(segs) > 0})
ok = res['blank_ok'] and res.get('gap_ok', True); res['ok'] = ok
print(json.dumps(res, ensure_ascii=False)); sys.exit(0 if ok else 1)
