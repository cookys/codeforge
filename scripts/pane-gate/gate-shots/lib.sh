# sourced: tmux helpers. $s = session
cap() { tmux capture-pane -p -t "$s"; }
click() { # click <label> : SGR press/release on the first cell of the label (last matching row)
  local row line col
  row=$(cap | grep -n -- "$1" | tail -1 | cut -d: -f1); [ -n "$row" ] || { echo "click: no $1" >&2; return 1; }
  line=$(cap | sed -n "${row}p")
  col=$(python3 -I -c 'import sys,unicodedata as u
l=sys.argv[1]; lab=sys.argv[2]; i=l.rfind(lab); print(sum(2 if u.east_asian_width(c) in "WF" else 1 for c in l[:i])+1)' "$line" "$1")
  tmux send-keys -t "$s" -l $'\e[<0;'"$col;$row"'M'; tmux send-keys -t "$s" -l $'\e[<0;'"$col;$row"'m'
}
key() { tmux send-keys -t "$s" "$@"; }
# composer text after the ❯ (empty = the prompt box is clean, so a bare digit reaches the pane strip)
composer() { cap | grep '❯' | tail -1 | python3 -I -c 'import sys; print(sys.stdin.read().split("❯",1)[-1].strip())'; }
# dig <d> : press a page digit from an empty composer; if the composer is dirty, clear it first; log a stray
dig() { local c; c=$(composer); [ -z "$c" ] || { echo "dirty before $1: [$c]" >> "${LOG:-/dev/null}"; key C-u; sleep .4; }
  key "$1"; sleep 1.2; c=$(composer); [ -z "$c" ] || { echo "digit $1 went to composer: [$c]" >> "${LOG:-/dev/null}"; key C-u; sleep .4; return 1; }; }
# the scratch cwd every shot session runs in: a git-less repo stub (cwd/.git/HEAD) so autopilot finds a repo root,
# and .fleet-no-register so fleet (6353fad2+) never lists a shot session on the patrol roster (PM 01M7PMF7)
mkdir -p "$G/cwd/.git" "$G/cwd/.claude"; [ -f "$G/cwd/.git/HEAD" ] || echo 'ref: refs/heads/main' > "$G/cwd/.git/HEAD"; : > "$G/cwd/.fleet-no-register"
