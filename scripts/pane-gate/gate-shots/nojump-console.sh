#!/usr/bin/env bash
# nojump.sh <autopilot-dir> — 30 frames (capture-pane -p, 1 s apart) over a scripted session; then runs nojump-check.py
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); O=$G/out/nojump-console; rm -rf "$O"; mkdir -p "$O"; . "$G/lib.sh"
mkdir -p "$G/cwd/.claude"; printf %s "{\"schema\":\"autopilot.console/1\",\"mode\":\"program\"}" > "$G/cwd/.claude/console.json"; : > "$G/fleet.log"; echo full > "$G/scenario"; s=gn-$$
: > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
tmux new-session -d -s "$s" -x 160 -y 40 -c "$G/cwd" "env PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW LANG=zh_TW.UTF-8 claude --plugin-dir $ap"
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 40   # first console fetch done; the next one is >= 60 s after it
# step table: frame -> action. The pane cannot take the keyboard under tmux (ctrl+x tab / click tried, 11-18 04:xx), so
# page switches are bare digits from an empty composer (the host's empty-composer digit rule) and every other action is a
# click on the Button the pane draws for it; the composer is checked after every frame (log: .log, must stay clean).
cl() { click "$1" || echo "frame $i: no button [$1]" >> "$LOG"; }
act() { case $1 in
  2) echo s5stale > "$G/scenario";;
5) dig 2;; 6) dig 3;; 7) dig 4;; 8) dig 3;;
  9) cl 'z: ';; 10) cl 'z: ';; 11) cl 'o: 看';;
  12) dig 2;; 13) cl '下張';; 14) cl '上張';; 15) cl '下張';; 16) cl '送出';; 17) cl '撤回';;
  18) dig 4;; 19) cl 'o: ';; 20) cl 'o: ';; 21) dig 1;; 22) dig 1;; 23) cl '✕';; 25) key "/live-pane"; sleep .4; key Enter;; 27) dig 3;; 29) dig 2;;
esac; }
LOG=$O/.log
key "/live-pane"; sleep .4; key Enter; sleep 3   # the pane is open from frame 1
echo "frame 1 scenario full" > "$LOG"; ts=$(date +%s)
for i in $(seq 1 30); do act $i; sleep 1; cap > "$O/f$(printf %02d $i).txt"; c=$(composer); [ -z "$c" ] || echo "frame $i composer dirty: [$c]" >> "$LOG"; done
echo "--- harness log"; cat "$LOG"
tmux kill-session -t "$s" 2>/dev/null
python3 "$G/../nojump-check.py" "$O"; echo "exit=$?"
