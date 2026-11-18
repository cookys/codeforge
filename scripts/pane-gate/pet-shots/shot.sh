#!/usr/bin/env bash
# shot.sh <autopilot-dir> <forge-stats-dir> <icon-id> <out-dir> — real terminal shots of the pet tab, 160x40 and 80x24
# Runs claude in a detached tmux session (scratch cwd), opens the pet tab by its label, captures plain text and ANSI.
set -euo pipefail
ap=$1; fs=$2; icon=$3; out=$4
mkdir -p "$out"
cwd=$(dirname "$0")/cwd
mkdir -p "$cwd"
s="petshot-$icon-$$"
: > "$cwd/.fleet-no-register"  # fleet 6353fad2: shot sessions stay off the patrol list
tmux new-session -d -s "$s" -x 160 -y 40 -c "$cwd" \
  "env CODEFORGE_PET_ICON=$icon AUTOPILOT_LOCALE=zh-TW CODEFORGE_LOCALE=zh-TW LANG=zh_TW.UTF-8 ${EXTRA_ENV:-} claude --plugin-dir '$ap' --plugin-dir '$fs'"
trap 'tmux kill-session -t "$s" 2>/dev/null || true' EXIT
# trust prompt (first run in this cwd) then let the plugins draw
sleep 7
if tmux capture-pane -p -t "$s" | grep -q -i "trust this folder"; then tmux send-keys -t "$s" Down; sleep 0.5; tmux send-keys -t "$s" Enter; fi
sleep 25
cap() { tmux capture-pane -p -t "$s" > "$out/$1.txt"; tmux capture-pane -p -e -t "$s" > "$out/$1.ans"; }
cap "$icon-160x40-closed"
tmux resize-window -t "$s" -x 80 -y 24
sleep 4
cap "$icon-80x24-closed"
