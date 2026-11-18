#!/usr/bin/env bash
# run-gate-b2.sh <autopilot-worktree-at-batch2-sha> — batch 2 UX gate end to end: real-terminal shots -> packet -> carbon + astra in parallel.
# Run it from the Bash tool with run_in_background: true (the harness wakes the session on exit).
# Output: gate-shots/out/b2-<sha9>/, gate-b2.md, gate-b2-{carbon,astra}.{out,err}. Prepared 11-18 (PM 01M7PM1D) so the gate starts minutes after the sha lands.
set -uo pipefail
S=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); sha=$(git -C "$ap" rev-parse --short=9 HEAD); label=b2-$sha
cd "$S"
bash gate-shots/run.sh "$ap" "$label" > "shots-$label.log" 2>&1; echo "shots rc=$? ($(command ls gate-shots/out/$label | wc -l) files)"
bash gate-shots/owncard.sh "$ap" "$label" >> "shots-$label.log" 2>&1; tail -1 "shots-$label.log"
# spec S6 (Pet sub-page + default page): always shot; forge-stats dir defaults to this repo's plugin
FS_DIR=${FS_DIR:-$(cd "$S/../.." && pwd)/plugins/forge-stats}
bash gate-shots/pet-r4.sh "$ap" "$FS_DIR" "$label" >> "shots-$label.log" 2>&1
for f in pet-default-160x40 pet-p9-160x40 pet-p9-120x30 pet-p9-80x24; do
  [ -s "gate-shots/out/$label/$f.txt" ] || { echo "FAIL: spec S6 shot missing: $f (see shots-$label.log)"; exit 1; }
done; echo "pet shots: ok"
KEYS="2 3 9 1 0 4" timeout 300 bash gate-shots/keyprobe.sh "$ap" "$label" > "keyprobe-$label.log" 2>&1; echo "keyprobe: $(grep -c ' ok$' "keyprobe-$label.log")/6 ok"
[ -n "${NO_REVIEW:-}" ] && { echo "NO_REVIEW: shots + keyprobe only (spot check, PM 01M7Q8H2); probes: $(command ls gate-shots/out/$label | grep -c probe)"; exit 0; }
python3 -I build-b2.py "$sha" "$label" || exit 1
D=${AUTOPILOT_DIR:-$HOME/projects/autopilot}/scripts/dispatch-author.sh
bash $D --runner agy --model gemini-4-carbon-high --timeout 30m --prompt-file "$S/gate-b2.md" > gate-b2-carbon.out 2> gate-b2-carbon.err &
pc=$!
bash $D --runner codex --model gpt-6-astra --timeout 20m --prompt-file "$S/gate-b2.md" > gate-b2-astra.out 2> gate-b2-astra.err &
pa=$!
wait $pc; echo "carbon exit=$?"; wait $pa; echo "astra exit=$?"
