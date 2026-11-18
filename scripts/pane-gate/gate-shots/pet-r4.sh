#!/usr/bin/env bash
# pet-r4.sh <autopilot-dir> <forge-stats-dir> [label] — real-terminal shots of the pane's 9 寵 page (fake fleet data + sandbox pet), 160x40 / 120x30 / 80x24
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); fs=$(cd "$2" && pwd); label=${3:-pet}; O=$G/out/$label; mkdir -p "$O"
. "$G/lib.sh"
P=$G/../pet-shots
rm -f "$G/cwd/.claude/console.json"; : > "$G/fleet.log"
echo full > "$G/scenario"; s=gp-$$
: > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
tmux new-session -d -s "$s" -x 160 -y 40 -c "$G/cwd" "env PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW CODEFORGE_LOCALE=zh-TW LANG=zh_TW.UTF-8 CODEFORGE_DATA_DIR=$P/petdata AUTOPILOT_LIVE_DIR=/dev/shm/petshot-1001 claude --plugin-dir $ap --plugin-dir $fs"
trap 'tmux kill-session -t "$s" 2>/dev/null || true' EXIT
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 14; key '/live-pane'; sleep 1.5; key Enter; sleep 4
# spec S6: the default page right after opening must be the overview, not Pet
cap > "$O/pet-default-160x40.txt"; tmux capture-pane -p -e -t "$s" > "$O/pet-default-160x40.ans"
for sz in 160x40 120x30 80x24; do
  tmux resize-window -t "$s" -x ${sz%x*} -y ${sz#*x}; sleep 4
  key 9; sleep 3
  cap > "$O/pet-p9-$sz.txt"; tmux capture-pane -p -e -t "$s" > "$O/pet-p9-$sz.ans"
  echo "$sz composer=[$(composer)]"
done
