#!/usr/bin/env bash
# 反向探針：疊在實拍上的稿（R1 rev5，r1_export.py 輸出）只遮掉實拍格子；設計稿畫的格子違規照樣要擋。
# 用法：check3-drawn.test.sh <out-r1 目錄>
set -euo pipefail
T="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SRC="$1"; K=120x40-s2
W="$(mktemp -d)"; trap 'rm -rf -- "$W"' EXIT
run() { CC_THEMES="$T/cc-themes.json" PYTHONDONTWRITEBYTECODE=1 python3 "$T/check3.py" "$W" >/dev/null 2>&1; }
fresh() { rm -rf -- "${W:?}/$K"; cp -r "$SRC/$K" "$W/$K"; }
fail=0
fresh; if run; then echo "ok   $K original passes"; else echo "FAIL $K original should pass"; fail=1; fi
# 題目列改回 claude 色：淺色 3.15 < 4.5，要紅
fresh; python3 - "$W/$K/roles.json" <<'PY'
import json, sys
p = sys.argv[1]; R = json.load(open(p))
for row in R:
    if row and row[0][0].startswith('│') and any(s.startswith('▸ ') for s, _, _ in row):
        for seg in row:
            if seg[2] != 'live' and seg[0].strip() and seg[0] != '│':
                seg[1] = 'claude'
json.dump(R, open(p, 'w'), ensure_ascii=False)
PY
if run; then echo "FAIL $K title back to claude colour should be red"; fail=1; else echo "ok   $K title back to claude colour -> red"; fi
# band 上設計稿畫的字出現 sha：要紅（R7 不因 drawn 遮罩而漏）
fresh; b=$(python3 -c "import json;print(json.load(open('$W/$K/band.json'))[0]+1)"); sed -i "${b}s/^‣ 要你決定：/‣ 要你決定sha/" "$W/$K/drawn.txt"
if run; then echo "FAIL $K drawn band with sha should be red"; fail=1; else echo "ok   $K drawn band with sha -> red"; fi
exit $fail
