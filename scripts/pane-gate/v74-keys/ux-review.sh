#!/usr/bin/env bash
# ux-review.sh <run.py-out-dir> <autopilot-sha> : the v7.4 pane strict UX-director review (PM 01M7XARR, 01M7XBE2).
# Builds the packet from a GREEN run.py run (ux-review.py refuses a red one), then two heterogeneous engines score it in
# parallel: a second engine (V74_UX_SECOND: grok = grok-4.7, the default since PM 01M7Y0F8 while agy's carbon quota is out;
# carbon = gemini-4-carbon-high on agy) and gpt-6-astra (codex). Pass = both say overall >= 8 with no Critical.
# Run it from the Bash tool with run_in_background: true (the harness wakes the session on exit), same as run-gate-b2.sh.
# Output next to the run: ux-review.md, ux-{grok|carbon,astra}.{out,err}, ux-verdict.txt.
# Exit 0 = both pass; 1 = a reviewer scored < 8 or found a Critical; 2 = no packet (red run, private name) or no verdict.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); out=$(cd "$1" && pwd); sha=$2
python3 -I "$here/ux-review.py" "$out" "$sha" --packet "$out/ux-review.md" || exit 2
D=${AUTOPILOT_DIR:-$HOME/projects/autopilot}/scripts/dispatch-author.sh
second=${V74_UX_SECOND:-grok}
case $second in
  grok) sr=(--runner grok --model grok-4.7) ;;
  carbon) sr=(--runner agy --model gemini-4-carbon-high) ;;
  opus) sr=(--runner claude-native --model claude-opus-5-5) ;; # PM 01M7Y4HJ: same family as this seat; the verdict is marked same-family
  *) echo "ux-review: V74_UX_SECOND must be grok, carbon or opus"; exit 2 ;;
esac
bash "$D" "${sr[@]}" --timeout 30m --prompt-file "$out/ux-review.md" > "$out/ux-$second.out" 2> "$out/ux-$second.err" &
pc=$!
bash "$D" --runner codex --model gpt-6-astra --timeout 30m --prompt-file "$out/ux-review.md" > "$out/ux-astra.out" 2> "$out/ux-astra.err" &
pa=$!
pr=
if [ "${V74_UX_REF:-}" = glm ]; then # PM 01M7Y4HJ: on-prem GLM as a third, non-scoring reference; never part of the verdict
  bash "$D" --runner cc-shim --model GLM-5.3 --endpoint glm_local --timeout 30m --prompt-file "$out/ux-review.md" > "$out/ux-glm-ref.out" 2> "$out/ux-glm-ref.err" &
  pr=$!
fi
wait $pc; echo "$second exit=$?"; wait $pa; echo "astra exit=$?"
if [ -n "$pr" ]; then wait "$pr"; echo "glm-ref exit=$?"; python3 -I "$here/ux-review.py" --verdict "$out/ux-glm-ref.out" | sed 's/^/reference (not scored): /' | tee "$out/ux-glm-ref.txt"; fi
python3 -I "$here/ux-review.py" --verdict "$out/ux-$second.out" "$out/ux-astra.out" | tee "$out/ux-verdict.txt"
exit "${PIPESTATUS[0]}"
