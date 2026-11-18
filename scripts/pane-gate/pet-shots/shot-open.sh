#!/usr/bin/env bash
# shot-open.sh <ap> <fs> <icon> <out> — like shot.sh, then clicks the pet tab (SGR mouse press/release on its cell) and captures the open tab at 160x40 and 80x24
set -euo pipefail
ap=$1; fs=$2; icon=$3; out=$4
mkdir -p "$out"; cwd=$(dirname "$0")/cwd; mkdir -p "$cwd"
s="petopen-$icon-$$"
: > "$cwd/.fleet-no-register"  # fleet 6353fad2: shot sessions stay off the patrol list
tmux new-session -d -s "$s" -x 160 -y 40 -c "$cwd" \
  "env CODEFORGE_PET_ICON=$icon AUTOPILOT_LOCALE=zh-TW CODEFORGE_LOCALE=zh-TW LANG=zh_TW.UTF-8 ${EXTRA_ENV:-} claude --plugin-dir '$ap' --plugin-dir '$fs'"
trap 'tmux kill-session -t "$s" 2>/dev/null || true' EXIT
sleep 7
if tmux capture-pane -p -t "$s" | grep -q -i "trust this folder"; then tmux send-keys -t "$s" Down; sleep 0.5; tmux send-keys -t "$s" Enter; fi
sleep 25
click() { # click the cell just left of `[-]` minus 4 (the pet label sits before `  [-]` or `  ›  [-]`)
  local line row col
  row=$(tmux capture-pane -p -t "$s" | grep -n '\[-\]' | head -1 | cut -d: -f1)
  line=$(tmux capture-pane -p -t "$s" | sed -n "${row}p")
  col=$(python3 -c 'import sys,unicodedata as u
l=sys.argv[1]; lab=sys.argv[2]; i=l.rfind(lab); w=sum(2 if u.east_asian_width(c) in "WF" else 1 for c in l[:i]); print(w+1)' "$line" "$2")
  tmux send-keys -t "$s" -l $'\e[<0;'"$col;$row"'M'; tmux send-keys -t "$s" -l $'\e[<0;'"$col;$row"'m'
  echo "clicked $2 at row=$row col=$col"
}
click x "$LABEL"
sleep 3
tmux capture-pane -p -t "$s" > "$out/$icon-160x40-open.txt"; tmux capture-pane -p -e -t "$s" > "$out/$icon-160x40-open.ans"
tmux resize-window -t "$s" -x 80 -y 24; sleep 4
tmux capture-pane -p -t "$s" > "$out/$icon-80x24-open.txt"; tmux capture-pane -p -e -t "$s" > "$out/$icon-80x24-open.ans"
