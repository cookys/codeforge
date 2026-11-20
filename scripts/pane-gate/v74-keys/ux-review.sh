#!/usr/bin/env bash
# ux-review.sh <run.py-out-dir> <autopilot-sha> : the v7.4 pane strict UX-director review (PM 01M7XARR, 01M7XBE2).
# Builds the packet from a GREEN run.py run (ux-review.py refuses a red one), then two heterogeneous engines score it in
# parallel: gemini-4-carbon-high (agy) and gpt-6-astra (codex). Pass = both say overall >= 8 with no Critical.
# Run it from the Bash tool with run_in_background: true (the harness wakes the session on exit), same as run-gate-b2.sh.
# Output next to the run: ux-review.md, ux-{carbon,astra}.{out,err}, ux-verdict.txt.
# Exit 0 = both pass; 1 = a reviewer scored < 8 or found a Critical; 2 = no packet (red run, private name) or no verdict.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); out=$(cd "$1" && pwd); sha=$2
python3 -I "$here/ux-review.py" "$out" "$sha" --packet "$out/ux-review.md" || exit 2
D=${AUTOPILOT_DIR:-$HOME/projects/autopilot}/scripts/dispatch-author.sh
bash "$D" --runner agy --model gemini-4-carbon-high --timeout 30m --prompt-file "$out/ux-review.md" > "$out/ux-carbon.out" 2> "$out/ux-carbon.err" &
pc=$!
bash "$D" --runner codex --model gpt-6-astra --timeout 30m --prompt-file "$out/ux-review.md" > "$out/ux-astra.out" 2> "$out/ux-astra.err" &
pa=$!
wait $pc; echo "carbon exit=$?"; wait $pa; echo "astra exit=$?"
python3 -I "$here/ux-review.py" --verdict "$out/ux-carbon.out" "$out/ux-astra.out" | tee "$out/ux-verdict.txt"
exit "${PIPESTATUS[0]}"
