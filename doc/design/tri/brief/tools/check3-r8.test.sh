#!/usr/bin/env bash
# 反向探針 R8：送出預覽和清單勾選要一致。原樣要綠；預覽多列一張沒選的卡、或清單少勾一張，都要轉紅。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
src=${1:?usage: check3-r8.test.sh <out dir>}
tmp=$(mktemp -d); trap 'rm -rf -- "$tmp"' EXIT
export CC_THEMES=$here/cc-themes.json PYTHONDONTWRITEBYTECODE=1
fail=0
step=$(python3 -c 'import json,sys; f=json.load(open(sys.argv[1])); f=f["steps"] if isinstance(f,dict) else f; print(next(x["step"] for x in f if x["id"] == "2-P"))' "$src/flow.json")
run() { # $1 shot, $2 label, $3 sed expr ('' = original)
  rm -rf -- "$tmp/c"; mkdir -p "$tmp/c"; cp -r "$src/$1" "$tmp/c/"
  if [ -n "$3" ]; then
    for f in screen.txt roles.json; do sed -i "$3" "$tmp/c/$1/$f"; done
    if cmp -s "$src/$1/screen.txt" "$tmp/c/$1/screen.txt"; then echo "FAIL $1 $2: probe did not change the screen"; fail=1; return; fi
  fi
  if python3 "$here/check3.py" "$tmp/c" >/dev/null 2>&1; then r=green; else r=red; fi
  want=red; [ -z "$3" ] && want=green
  if [ "$r" = "$want" ]; then echo "ok   $1 $2 -> $r"; else echo "FAIL $1 $2 -> $r (want $want)"; fail=1; fi
}
for shot in "209x49-s$step" "120x40-s$step" "80x24-s$step"; do
  run "$shot" original ''
  run "$shot" 'unchecked card 2' 's/✓2 外掛/·2 外掛/'
done
for shot in "209x49-s$step" "120x40-s$step"; do
  run "$shot" 'preview lists unselected card 3' 's/│ 4 時冶 PL02 定案：/│ 3 時冶 PL02 定案：/'
done
exit $fail
