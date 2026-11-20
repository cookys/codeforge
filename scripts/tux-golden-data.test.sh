#!/usr/bin/env bash
# Bridge generator controls: fresh check, stale goldens, stale output, explicit output path.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
repo=$t/repo
mkdir -p "$repo/scripts" "$repo/plugins/forge-stats/tests" "$t/golden"
cp "$here/tux-golden-data.py" "$repo/scripts/"
export TUX_GOLDEN_DIR=$t/golden
cat >"$t/golden/glyphs.json" <<'JSON'
{"state": {"running": "●", "done": "✓"}, "structure": {"rule": "─"}}
JSON
cat >"$t/golden/width-cases.json" <<'JSON'
{"cases": [{"name": "ambiguous-down", "text": "↓", "width": 1}]}
JSON
cat >"$t/golden/pct-cases.json" <<'JSON'
{"cases": [{"name": "half", "pct": 0.5}]}
JSON
cat >"$t/golden/dur-cases.json" <<'JSON'
{"cases": [{"name": "minute", "secs": 60}]}
JSON
cat >"$t/golden/panel-cases.json" <<'JSON'
{"cases": [{"name": "idle", "tone": "subtle"}]}
JSON
gen() { python3 "$repo/scripts/tux-golden-data.py" "$@"; }
default_out=$repo/plugins/forge-stats/tests/tux-golden-data.ts

gen
out=$(gen --check) || fail 'fresh check failed'
case "$out" in *ok*) ;; *) fail "no ok in check output ($out)" ;; esac

printf '{"cases": [{"name": "ambiguous-down", "text": "↓", "width": 2}]}' >"$t/golden/width-cases.json"
out=$(gen --check 2>&1) && fail 'stale golden value passed check'
case "$out" in *stale*) ;; *) fail "no stale in drifted output ($out)" ;; esac
printf '{"cases": [{"name": "ambiguous-down", "text": "↓", "width": 1}]}' >"$t/golden/width-cases.json"

gen
printf '// hand edit\n' >>"$default_out"
gen --check 2>/dev/null && fail 'hand-edited output passed check'
gen

rm "$default_out"
gen --check 2>/dev/null && fail 'deleted output passed check'
gen

before=$(cat "$default_out")
gen "$t/explicit.ts" || fail 'explicit output path failed'
[ -f "$t/explicit.ts" ] || fail 'explicit output path wrote nothing'
[ "$(cat "$default_out")" = "$before" ] || fail 'explicit output path rewrote the default output'

grep -qF '●' "$default_out" || fail 'non-ASCII glyph escaped in generated ts'
if grep -qF '\u' "$default_out"; then fail 'generated ts contains a unicode escape'; fi

rm "$t/golden/panel-cases.json"
gen --check 2>/dev/null && fail 'missing golden file passed check'

echo 'tux-golden-data.test.sh: ok (7 controls)'
