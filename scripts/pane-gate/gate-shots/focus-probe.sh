#!/usr/bin/env bash
# focus-probe.sh <ap> <cols> <rows> — open pane, try focus methods, report prompt line + strip after each
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); . "$G/lib.sh"; s=fp-$$
echo full > "$G/scenario"
: > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
tmux new-session -d -s "$s" -x $2 -y $3 -c "$G/cwd" "env PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW LANG=zh_TW.UTF-8 claude --plugin-dir $ap"
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 14; key '/live-pane'; sleep 1.5; key Enter; sleep 4
rep() { echo "== $1"; cap | grep -n '❯' | tail -1; cap | grep -nE '(現況|額度).*(待決|艦隊)' | tail -1; cap | grep -nE '^\s*(│\s*)?(現況|待決|艦隊|額度)' | head -2; }
rep open
key 3; sleep 2; rep "digit 3 (fleet)"
key C-x; sleep .4; key Tab; sleep 1.2; tmux capture-pane -p -e -t "$s" > $G/fp-t0.ans
for n in 1 2 3 4 5 6; do key Tab; sleep 1; tmux capture-pane -p -e -t "$s" > $G/fp-t$n.ans; done
key z; sleep 1.5; rep "band then Tab x6 then z"
tmux kill-session -t "$s"
