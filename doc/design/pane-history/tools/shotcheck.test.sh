#!/usr/bin/env bash
# shotcheck.py 反向探針：單色圖必須 FAIL，有內容的圖必須 OK；
# 捲動後的視窗截圖（geo.scrollY）連續框線 OK、斷 20px 必須 FAIL；隱藏 grid 和圖外框線跳過不崩。
set -euo pipefail
d=$(mktemp -d); trap 'rm -r "$d"' EXIT
here=$(cd "$(dirname "$0")" && pwd)
python3 - "$d" <<'PY'
import sys, zlib, struct
def png(path, w, h, f):
    raw = b''.join(b'\0' + b''.join(bytes(f(x, y)) for x in range(w)) for y in range(h))
    ch = lambda t, c: struct.pack('>I', len(c)) + t + c + struct.pack('>I', zlib.crc32(t + c))
    open(path, 'wb').write(b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + ch(b'IDAT', zlib.compress(raw)) + ch(b'IEND', b''))
png(sys.argv[1] + '/blank.png', 200, 100, lambda x, y: (15, 18, 23))
png(sys.argv[1] + '/text.png', 200, 100, lambda x, y: (230, 230, 230) if (x // 4 + y // 6) % 3 == 0 else (15, 18, 23))
import json
bg, fg = (15, 18, 23), (230, 230, 230)
dots = lambda x, y: fg if (x > 40 and (x // 4 + y // 6) % 3 == 0) else bg
png(sys.argv[1] + '/frame.png', 200, 200, lambda x, y: fg if x == 4 and 5 <= y <= 160 else dots(x, y))
png(sys.argv[1] + '/gap.png', 200, 200, lambda x, y: fg if x == 4 and 5 <= y <= 160 and not 60 <= y < 80 else dots(x, y))
# 頁面座標 top=1000、scrollY=1000：框線在圖上 y 7..142；另有隱藏 grid（scale null）和圖外框線
geo = {'scrollY': 1000, 'grids': [{'left': 0, 'top': 1000, 'scale': 1, 'pitch': 15, 'segs': [[0, 0, 9]]},
       {'left': 0, 'top': 1000, 'scale': None, 'pitch': 0, 'segs': [[0, 0, 9]]},
       {'left': 0, 'top': 9000, 'scale': 1, 'pitch': 15, 'segs': [[0, 0, 9]]}]}
json.dump(geo, open(sys.argv[1] + '/geo.json', 'w'))
PY
if python3 "$here/shotcheck.py" "$d/blank.png" >/dev/null; then echo "FAIL: 空白圖沒被擋下"; exit 1; fi
python3 "$here/shotcheck.py" "$d/text.png" >/dev/null || { echo "FAIL: 有內容的圖被誤判"; exit 1; }
python3 "$here/shotcheck.py" "$d/frame.png" "$d/geo.json" >/dev/null || { echo "FAIL: 捲動後的連續框線被誤判"; exit 1; }
if python3 "$here/shotcheck.py" "$d/gap.png" "$d/geo.json" >/dev/null; then echo "FAIL: 斷 20px 的框線沒被擋下"; exit 1; fi
echo "shotcheck.test: ok"
