#!/usr/bin/env bash
# shot-session.sh — one real-terminal shot of Claude Code with a plugin dir, on a private tmux server.
#
#   shot-session.sh --plugin-dir <dir> --out <prefix> [--size 209x49] [--theme dark|light]
#                   [--keys "<k1>|<k2>|..."] [--wait 20] [--cwd <dir>]
#
# Writes <prefix>.txt (plain) and <prefix>.ans (with SGR colors). Exit 0 when the prompt box drew.
#
# Why the config is not sandboxed (B72): a fresh CLAUDE_CONFIG_DIR has no login, so claude never
# renders past onboarding, and copying a token into a sandbox is a credential act (operator card).
# So claude runs with this seat's own config; everything codeforge/autopilot WRITE goes to a
# sandbox: AUTOPILOT_LIVE_DIR, CODEFORGE_DATA_DIR, AUTOPILOT_CARD_MIRROR_DIR, AUTOPILOT_ROOMS_DIR
# all point under a temp dir unless the caller already set them. The cwd is a git-less repo stub
# with .fleet-no-register, so a shot session never joins the fleet patrol list.
set -uo pipefail
plugin="" out="" size=209x49 theme=dark keys="" wait=20 cwd=""
while [ $# -gt 0 ]; do
  case $1 in
    --plugin-dir) plugin=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --theme) theme=$2; shift 2 ;;
    --keys) keys=$2; shift 2 ;;
    --wait) wait=$2; shift 2 ;;
    --cwd) cwd=$2; shift 2 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "shot-session: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$plugin" ] && [ -d "$plugin" ] || { echo "shot-session: --plugin-dir <existing dir> required" >&2; exit 2; }
[ -n "$out" ] || { echo "shot-session: --out <prefix> required" >&2; exit 2; }
case $size in *x*) ;; *) echo "shot-session: --size must be COLSxROWS" >&2; exit 2 ;; esac
plugin=$(cd "$plugin" && pwd)

box=$(mktemp -d "${TMPDIR:-/tmp}/shot-session.XXXXXX")
sock=shot-$$
cleanup() { tmux -L "$sock" kill-server 2>/dev/null; }
trap cleanup EXIT

if [ -z "$cwd" ]; then
  cwd=$box/cwd
  mkdir -p "$cwd/.git" "$cwd/.claude"
  echo 'ref: refs/heads/main' > "$cwd/.git/HEAD"
fi
: > "$cwd/.fleet-no-register"
mkdir -p "$box/live" "$box/data" "$box/mirror" "$box/rooms" "$(dirname "$out")"

envs="AUTOPILOT_LIVE_DIR=${AUTOPILOT_LIVE_DIR:-$box/live} CODEFORGE_DATA_DIR=${CODEFORGE_DATA_DIR:-$box/data}"
envs="$envs AUTOPILOT_CARD_MIRROR_DIR=${AUTOPILOT_CARD_MIRROR_DIR:-$box/mirror} AUTOPILOT_ROOMS_DIR=${AUTOPILOT_ROOMS_DIR:-$box/rooms}"
envs="$envs AUTOPILOT_LOCALE=${AUTOPILOT_LOCALE:-zh-TW} LANG=zh_TW.UTF-8"

tmux -L "$sock" new-session -d -s shot -x "${size%x*}" -y "${size#*x}" -c "$cwd" \
  "env $envs claude --plugin-dir '$plugin' --settings '{\"theme\":\"$theme\"}'"
cap() { tmux -L "$sock" capture-pane -p -t shot; }

# Wait for either the trust dialog or the prompt box. The trust dialog (CC 2.1.332) lists
# "❯ No, exit" first and "Yes, I trust this folder" second: a bare Enter exits claude (the B72
# symptom), so move to the Yes row first. The dialog's own "❯" cursor is not the prompt box,
# so the prompt check skips frames that still show "Enter to confirm".
drew=0
for _ in $(seq 1 "$wait"); do
  sleep 1
  frame=$(cap)
  if grep -q "Enter to confirm" <<< "$frame"; then
    if grep -q "❯ No, exit" <<< "$frame"; then tmux -L "$sock" send-keys -t shot Down; sleep 0.5; fi
    if grep -q "❯ Yes" <<< "$(cap)"; then tmux -L "$sock" send-keys -t shot Enter; fi
    sleep 2; continue
  fi
  if grep -q '❯' <<< "$frame"; then drew=1; break; fi
done
[ "$drew" = 1 ] || { cap > "$out.txt"; echo "shot-session: prompt box never drew (see $out.txt)" >&2; exit 1; }
sleep 3

if [ -n "$keys" ]; then
  IFS='|' read -r -a ks <<< "$keys"
  for k in "${ks[@]}"; do
    case $k in
      Enter|Escape|Tab|Up|Down|Left|Right|C-*|M-*) tmux -L "$sock" send-keys -t shot "$k" ;;
      *) tmux -L "$sock" send-keys -t shot -l "$k" ;;
    esac
    sleep 1.5
  done
  sleep 2
fi

cap > "$out.txt"
tmux -L "$sock" capture-pane -p -e -t shot > "$out.ans"
echo "shot-session: $out.txt $out.ans ($size $theme)"
