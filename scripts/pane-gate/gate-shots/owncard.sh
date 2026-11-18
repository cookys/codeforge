#!/usr/bin/env bash
# owncard.sh <autopilot-dir> <label> — page 2 待決 with a card this seat can answer (gate b2 carbon M4 asks for it next to the read-only card).
# Shots: out/<label>/S6-own-p2-{160x40,80x24}.{txt,ans}. Run after run.sh, never beside it (both use cwd/ and scenario).
# The seat's own cards come from the repo's card store, scope-checked against the session (autopilot 01M7PZP3, fixture
# mods/live/pane-attach.test.ts@af47b1fde baseFiles/durableStore): HOME/.autopilot/live-pointer.json -> live_base;
# HOME/.autopilot/session-mode/<sid>.json -> project_key; <live_base>/runs/<key>.json envelope fresh within 180 s;
# <git-common-dir>/autopilot/decisions/<key>.json holds the cards. So this run swaps HOME to a sandbox (its .claude and
# .claude.json link the real ones, as run.sh's HOMEBOX does) and pins the session id.
set -uo pipefail
G=$(cd "$(dirname "$0")" && pwd); ap=$(cd "$1" && pwd); label=$2; O=$G/out/$label; mkdir -p "$O"
. "$G/lib.sh"
H=$G/home-own; LB=$H/.autopilot/live; KEY=$(printf gate-own | sha256sum | cut -c1-16);  # any 16 hex (model.ts isKey); SID=$(python3 -I -c 'import uuid; print(uuid.uuid4())')
GIT=$G/cwd/.git; IDENT="git-common-dir:$GIT"
rm -rf "$H/.autopilot"; mkdir -p "$H/.autopilot/session-mode" "$LB/runs" "$GIT/autopilot/decisions"
ln -sfn "$HOME/.claude" "$H/.claude"; ln -sf "$HOME/.claude.json" "$H/.claude.json"
iso() { date -u -d "@$(( $(date +%s) + ${1:-0} ))" +%Y-%m-%dT%H:%M:%SZ; }
printf '{"schema":"autopilot.live-pointer/1","live_base":"%s","autopilot_home":"%s","written_at":"%s"}\n' "$LB" "$H/.autopilot" "$(iso)" > "$H/.autopilot/live-pointer.json"
printf '{"session_id":"%s","level":"l5","project_key":"%s","root_run_id":null,"expires_at":"%s"}\n' "$SID" "$KEY" "$(iso 7200)" > "$H/.autopilot/session-mode/$SID.json"
cat > "$GIT/autopilot/decisions/$KEY.json" <<EOF
{"schema":"autopilot.decision/2","project_key":"$KEY","repo_identity":"$IDENT","root_run_id":null,"decisions":[
 {"id":"d-own1","question":"記憶線：測試綠了，要合併到 develop 嗎？","status":"open","kind":"decide","opened_at":"$(iso -5400)","background":"假資料：--no-ff，tip 測試全綠。","options":[{"label":"准合併","description":"照常 --no-ff"},{"label":"先看 diff","description":"晚一點再答"}]}
]}
EOF
envelope() { printf '{"schema":"autopilot.runs-live/1","published_at":"%s","valid_for_s":180,"scope":{"project_key":"%s","repo_identity":"%s","root_run_id":null},"runs":[]}\n' "$(iso)" "$KEY" "$IDENT" > "$LB/runs/$KEY.json.tmp" && mv "$LB/runs/$KEY.json.tmp" "$LB/runs/$KEY.json"; }
envelope; ( while :; do sleep 30; envelope; done ) & keep=$!
rm -f "$G/cwd/.claude/console.json"; echo full > "$G/scenario"; s=own-$$
trap 'kill $keep 2>/dev/null; tmux kill-session -t "$s" 2>/dev/null || true' EXIT
tmux new-session -d -s "$s" -x 160 -y 40 -c "$G/cwd" "env HOME=$H PATH=$G/bin:$PATH GATE_DIR=$G AUTOPILOT_CARD_MIRROR_DIR=$G/mirror AUTOPILOT_ROOMS_DIR=$G/rooms AUTOPILOT_LOCALE=zh-TW LANG=zh_TW.UTF-8 claude --session-id $SID --plugin-dir $ap"
sleep 7; if cap | grep -qi "trust this folder"; then key Down; sleep .5; key Enter; fi
sleep 14; key '/live-pane'; sleep 1.5; key Enter; sleep 4
for sz in 160x40 80x24; do
  tmux resize-window -t "$s" -x "${sz%x*}" -y "${sz#*x}"; sleep 4; key 2; sleep 2.5
  cap > "$O/S6-own-p2-$sz.txt"; tmux capture-pane -p -e -t "$s" > "$O/S6-own-p2-$sz.ans"
done
if grep -q '准合併' "$O/S6-own-p2-160x40.txt"; then echo "own card: drawn"
else rm -f "$O"/S6-own-p2-*; echo "own card: NOT drawn (shots removed so the packet never shows a wrong page; check scope files)"; fi
