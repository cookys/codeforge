#!/usr/bin/env bash
# shotcheck.py 反向探針：單色圖必須 FAIL，有內容的圖必須 OK。
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
PY
if python3 "$here/shotcheck.py" "$d/blank.png" >/dev/null; then echo "FAIL: 空白圖沒被擋下"; exit 1; fi
python3 "$here/shotcheck.py" "$d/text.png" >/dev/null || { echo "FAIL: 有內容的圖被誤判"; exit 1; }
echo "shotcheck.test: ok"
