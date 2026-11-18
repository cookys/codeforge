#!/usr/bin/env bash
# nojump.sh <autopilot-dir> — 30 frames (capture-pane -p, 1 s apart) over a scripted session; then runs nojump-check.py
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); O=$G/out/nojump; rm -rf "$O"; mkdir -p "$O"; . "$G/lib.sh"
rm -f "$G/cwd/.claude/console.json"; : > "$G/fleet.log"; echo full > "$G/scenario"; s=gn-$$
: > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
tmux new-session -d -s "$s" -x 160 -y 40 -c "$G/cwd" "env PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW LANG=zh_TW.UTF-8 claude --plugin-dir $ap"
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 40   # first console fetch done; the next one is >= 60 s after it
# step table: frame -> action
act() { case $1 in
  3) key '/live-pane';; 4) key Enter;; 5) key 2;; 6) key 3;; 7) key 4;; 8) key 1;;
  9) key z;; 10) key z;; 11) key j;; 12) key j;; 13) key k;;
  14) key 2;; 15) key l;; 16) key h;; 17) key l;; 18) key s;; 19) key u;;
  20) key 3;; 21) key j;; 22) key o;; 24) key 1;; 2) echo s5stale > "$G/scenario";;
  27) key Escape;;
esac; }
echo "frame 1 scenario full" > "$O/.log"; ts=$(date +%s)
for i in $(seq 1 30); do act $i; sleep 1; cap > "$O/f$(printf %02d $i).txt"; done
tmux kill-session -t "$s" 2>/dev/null
python3 "$G/../nojump-check.py" "$O"; echo "exit=$?"
