#!/usr/bin/env bash
# keyprobe.sh <autopilot-dir> <tag> [forge-stats-dir] — at 80x24, press digits and log the selected tab + composer after each (does a digit land in the prompt, does it reach its page?)
# Each line ends ok (composer empty and the strip selects the pressed digit) or BAD; the selected tab comes from the strip's colours (seltab.py), not the
# pane's first row, which a scrolled page fills with "上面還有 N 行" (gate b2 astra M03). page=[...] stays as the first row, for the reader.
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); tag=$2; fsarg=""; [ -n "${3:-}" ] && fsarg="--plugin-dir $(cd "$3" && pwd)"
. "$G/lib.sh"; P=$G/../pet-shots; O=$G/out/keyprobe-$tag; mkdir -p "$O"
rm -f "$G/cwd/.claude/console.json"; echo full > "$G/scenario"; s=kp-$$
: > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
tmux new-session -d -s "$s" -x ${START:-160} -y ${STARTH:-40} -c "$G/cwd" "env PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW CODEFORGE_LOCALE=zh-TW LANG=zh_TW.UTF-8 ${SANDBOX_ENV-CODEFORGE_DATA_DIR=$P/petdata AUTOPILOT_LIVE_DIR=/dev/shm/petshot-1001} claude --plugin-dir $ap $fsarg"
trap 'tmux kill-session -t "$s" 2>/dev/null || true' EXIT
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 14; key '/live-pane'; sleep 1.5; key Enter; sleep 4
for k in ${PRE:-}; do key "$k"; sleep 2.5; echo "pre $k page=[$(cap | grep -m1 -o "│ [^ │]*" | head -1)] composer=[$(composer)]"; done
tmux resize-window -t "$s" -x ${RW:-80} -y ${RH:-24}; sleep 4
n=0
for k in ${KEYS:-2 2 9 9 1 9}; do
  n=$((n+1)); key "$k"; sleep 2.5
  c=$(composer); hdr=$(cap | grep -m1 -o '│ [^ │]*' | head -1)
  cap > "$O/$n-key$k.txt"; tmux capture-pane -p -e -t "$s" > "$O/$n-key$k.ans"; sel=$(python3 -I "$G/seltab.py" < "$O/$n-key$k.ans")
  v=BAD; [ -z "$c" ] && [ "${sel%%:*}" = "$k" ] && v=ok
  echo "$n key=$k tab=[$sel] page=[$hdr] composer=[$c] $v"
  [ -n "$c" ] && { key C-u; sleep .4; }
done
