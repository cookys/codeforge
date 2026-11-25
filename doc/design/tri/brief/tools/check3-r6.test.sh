#!/usr/bin/env bash
# 反向探針：寬螢幕（選項列左邊是 index）把選項字母 a/b 對調，check3 必須轉紅；原樣必須是綠的。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
src=${1:-$here/../../v2/out}
tmp=$(mktemp -d); trap 'rm -rf -- "$tmp"' EXIT
export CC_THEMES=$here/cc-themes.json PYTHONDONTWRITEBYTECODE=1
fail=0
step=$(python3 -c 'import json,sys; print(next(x["step"] for x in json.load(open(sys.argv[1])) if x["id"] == "2-N"))' "$src/flow.json")
for shot in "209x49-s$step" "120x40-s$step"; do
  mkdir -p "$tmp/ok" "$tmp/bad"
  cp -r "$src/$shot" "$tmp/ok/"; cp -r "$src/$shot" "$tmp/bad/"
  for f in screen.txt roles.json; do
    sed -i 's/│     a: /│     @: /; s/│     b: /│     a: /; s/│     @: /│     b: /' "$tmp/bad/$shot/$f"
  done
  if cmp -s "$tmp/ok/$shot/screen.txt" "$tmp/bad/$shot/screen.txt"; then echo "FAIL $shot: probe did not change the screen"; fail=1; fi
  if python3 "$here/check3.py" "$tmp/ok" >/dev/null 2>&1; then echo "ok   $shot original passes"; else echo "FAIL $shot original should pass"; fail=1; fi
  if python3 "$here/check3.py" "$tmp/bad" >/dev/null 2>&1; then echo "FAIL $shot swapped letters still pass"; fail=1; else echo "ok   $shot swapped letters -> red"; fi
  rm -rf -- "$tmp/ok" "$tmp/bad"
done
exit $fail
