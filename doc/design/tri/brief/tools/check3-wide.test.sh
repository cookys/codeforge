#!/usr/bin/env bash
# 反向探針：寬螢幕稿（console/wide/gen_wide.py）band 裡的 ├┤ 分隔線只在框線那一欄才算對齊。
# 用法：check3-wide.test.sh <out-wide 目錄>
set -euo pipefail
T="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SRC="$1"; K=209x49-s56
W="$(mktemp -d)"; trap 'rm -rf -- "$W"' EXIT
run() { CC_THEMES="$T/cc-themes.json" PYTHONDONTWRITEBYTECODE=1 python3 "$T/check3.py" "$W" >/dev/null 2>&1; }
fresh() { rm -rf -- "${W:?}/$K"; cp -r "$SRC/$K" "$W/$K"; }
fail=0
fresh; if run; then echo "ok   $K original passes"; else echo "FAIL $K original should pass"; fail=1; fi
# 分隔線的 ├ 往右移一欄（左框線那格變空白）：要紅
fresh; python3 - "$W/$K/screen.txt" <<'PY'
import sys; p = sys.argv[1]; L = open(p).read().split('\n')
i = next(k for k, l in enumerate(L) if l.startswith('├'))
L[i] = ' ├' + L[i][2:]
open(p, 'w').write('\n'.join(L))
PY
if run; then echo "FAIL shifted divider should fail"; fail=1; else echo "ok   shifted divider fails"; fi
exit $fail
