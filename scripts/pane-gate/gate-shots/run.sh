#!/usr/bin/env bash
# run.sh <autopilot-dir> [label] — real-terminal shots of pages 1-4 with fake fleet data. Output: out/<label>/...
# Needs cwd/.git/HEAD (git-less repo stub); console mode OFF (cwd/.claude/console.json absent) = where the framed pages draw.
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); label=${2:-run}; O=$G/out/$label; mkdir -p "$O"
. "$G/lib.sh"
rm -f "$G/cwd/.claude/console.json"; : > "$G/fleet.log"
# AUTOPILOT_MODS_PANE_PROBE: autopilot (fix/pane-r4+) logs the host's placement/bodyRows per render, to set beside the shots (PM 01M7Q8H2: W1 measured before pin)
# the mod reads HOME for live-pointer / console fallback; real HOME stays except the s5fail scenario (see HOMEBOX)
start() { # start <scenario> <home> [extra claude args]
  echo "$1" > "$G/scenario"; s=gs-$$
  tmux kill-session -t "$s" 2>/dev/null
  : > "$G/cwd/.fleet-no-register"  # fleet skips sessions whose cwd holds this (fleet 6353fad2): a shot session never joins the patrol list
  tmux new-session -d -s "$s" -x 160 -y 40 -c "$G/cwd" "env HOME=$2 AUTOPILOT_MODS_PANE_PROBE=$O/probe-$1${NOPANE:+-closed}.jsonl PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW LANG=zh_TW.UTF-8 claude --plugin-dir $ap ${3:-}"
  sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
  sleep 14; [ -n "${NOPANE:-}" ] && return; key '/live-pane'; sleep 1.5; key Enter; sleep 4
}
stop() { tmux kill-session -t "$s" 2>/dev/null; }
shots() { # shots <tag> <size...>
  local tag=$1; shift
  for sz in "$@"; do
    tmux resize-window -t "$s" -x ${sz%x*} -y ${sz#*x}; sleep 4
    for p in 2 3 4 1; do key "$p"; sleep 2.5
      cap > "$O/$tag-p$p-$sz.txt"; tmux capture-pane -p -e -t "$s" > "$O/$tag-p$p-$sz.ans"; done
  done
}
REALHOME=$HOME
HOMEBOX=$G/home; mkdir -p "$HOMEBOX"; ln -sfn "$REALHOME/.claude" "$HOMEBOX/.claude"; ln -sf "$REALHOME/.claude.json" "$HOMEBOX/.claude.json"
start full "$REALHOME";  shots S1-full 160x40 120x30 80x24 80x20 80x16
# page 0 鍛 (gate b2 r2 astra 13: K1 needs the whole page, not just the keyprobe row)
for sz in 160x40 80x24; do tmux resize-window -t "$s" -x ${sz%x*} -y ${sz#*x}; sleep 4; key 0; sleep 2.5
  cap > "$O/S1-full-p0-$sz.txt"; tmux capture-pane -p -e -t "$s" > "$O/S1-full-p0-$sz.ans"; done; stop
# pane closed: the band's 2nd row is the one shared tab strip (gate b2 r2 carbon N4: X1 second half, batch 1 M6)
NOPANE=1 start full "$REALHOME"; cap > "$O/S1-closed-160x40.txt"; tmux capture-pane -p -e -t "$s" > "$O/S1-closed-160x40.ans"; stop
# light terminal (gate b2 carbon M5): Claude Code's light theme for this session only, via --settings (nothing is written to the global config)
start full "$REALHOME" "--settings '{\"theme\":\"light\"}'"; shots S1-light 160x40 80x24; stop
start small "$REALHOME"; shots S4-small 160x40; stop
start s5stale "$REALHOME"; shots S5-stale-10min 160x40 120x30; stop  # data is 12 min old (name kept for old records)
# spec S5 threshold is 5 min: a 6-minute-old brief must also read as stale (ux-gate s5s6-spec-vs-tool.md)
start s5stale6 "$REALHOME"; shots S5-stale-6min 160x40; stop
start s5fail "$HOMEBOX"; shots S5-console-exit1-nofile 160x40; stop
echo "fleet.log lines: $(wc -l < "$G/fleet.log")"; cut -d' ' -f3- "$G/fleet.log" | sort | uniq -c
