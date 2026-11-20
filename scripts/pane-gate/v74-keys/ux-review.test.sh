#!/usr/bin/env bash
# ux-review.test.sh : ux-review.py without engines — refuses a red run, builds a packet from a green one (design pages included,
# blank runs squeezed, styles read from the colour capture), refuses a private email, and reads reviewer verdicts
# (pass / < 8 / Critical / no json). check-all runs it.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
py() { python3 -I "$here/ux-review.py" "$@"; }
mk() {  # mk <dir> <true|false>
  mkdir -p "$1"; cp "$here/fixtures/green-decide.txt" "$1/01-project-decide.txt"; cp "$here/fixtures/design-S3.ans" "$1/01-project-decide.ans"
  printf '{"step":"01-project-decide","ok":%s,"checks":[{"check":"title 待決","ok":%s}],"seen":{"title":"待決"}}\n' "$2" "$2" > "$1/results.jsonl"
}
fail=0
chk() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got $2 want $3"; fail=1; fi; }
has() { if grep -q -- "$2" "$1"; then echo y; else echo n; fi; }

mk "$t/red" false
set +e; py "$t/red" abc123 >/dev/null 2>&1; chk 'red run refused' $? 1; set -e
mk "$t/green" true; py "$t/green" abc123 >/dev/null
p="$t/green/ux-review.md"
chk 'packet written' "$(test -s "$p" && echo y || echo n)" y
chk 'packet has design S10' "$(has "$p" '### 設計稿 S10')" y
chk 'packet names the sha' "$(has "$p" 'autopilot @ abc123')" y
chk 'blank runs squeezed' "$(has "$p" '⟨')" y
chk 'style read from colour capture' "$(has "$p" '選項 b（★）')" y
at='@'; mk "$t/leak" true; sed -i "s/驗收測試卡/驗收測試卡 someone${at}acme-corp.com/" "$t/leak/01-project-decide.txt"  # built at run time: the repo's own leak check scans this file
set +e; py "$t/leak" abc123 >/dev/null 2>&1; chk 'private email refused' $? 1; set -e
chk 'leak packet not written' "$(test -e "$t/leak/ux-review.md" && echo y || echo n)" n

fence=$(printf "\140\140\140"); v() { printf "評論\n%sjson\n%s\n%s\n" "$fence" "$2" "$fence" > "$t/$1"; }
v pass.out '{"overall": 8, "axes": {}, "findings": [{"sev": "Minor"}], "verdict": "PASS"}'
v low.out '{"overall": 7, "axes": {}, "findings": [], "verdict": "FAIL"}'
v crit.out '{"overall": 9, "axes": {}, "findings": [{"sev": "Critical"}], "verdict": "FAIL"}'
printf 'no json here\n' > "$t/none.out"
set +e
py --verdict "$t/pass.out" "$t/pass.out" >/dev/null; chk 'both pass -> 0' $? 0
py --verdict "$t/pass.out" "$t/low.out" >/dev/null; chk 'one < 8 -> 1' $? 1
py --verdict "$t/crit.out" "$t/pass.out" >/dev/null; chk 'Critical at 9 -> 1' $? 1
py --verdict "$t/pass.out" "$t/none.out" >/dev/null; chk 'no json -> 2' $? 2
set -e
exit $fail
