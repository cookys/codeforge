#!/usr/bin/env bash
# Import trinity's self-reported quota from this host's fleet-cli inbox (operator d-e31e39c0).
# Run by codeforge-trinity-import.timer. Accepts only messages whose courier-attested `from` is in
# $CODEFORGE_TRINITY_SENDERS and whose content is a `trinity-usage/1` JSON line. Each message is
# imported once (ids in $STATE/done); the import itself keeps only the newest observation.
# A failed import, or no new data for $CODEFORGE_TRINITY_STALE_H hours after the first success,
# is sent to $CODEFORGE_TRINITY_NOTIFY once (per message / per day).
set -euo pipefail
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
STATE=${CODEFORGE_TRINITY_STATE:-$HOME/.local/state/codeforge/trinity-import}
SENDERS=${CODEFORGE_TRINITY_SENDERS:-cookys/grokbot/trinity cookys/grokbot/fleet-cli}
NOTIFY=${CODEFORGE_TRINITY_NOTIFY:-cookys/aimax395-team/capacity--claude cookys/aimax395-team/codeforge--claude}
STALE_H=${CODEFORGE_TRINITY_STALE_H:-168}
FLEET=${FLEET_BIN:-fleet}
CODEFORGE=${CODEFORGE_BIN:-codeforge}

mkdir -p "$STATE"
touch "$STATE/done" "$STATE/alerted"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*" >> "$STATE/log"; }
notify() { # $1 = alert key (sent once), $2 = text
  grep -qxF "$1" "$STATE/alerted" && return 0
  local a
  for a in $NOTIFY; do "$FLEET" send "$a" "$2" >/dev/null 2>&1 || log "notify $a failed"; done
  echo "$1" >> "$STATE/alerted"
  log "alert $1"
}

if ! "$FLEET" inbox --peek --json > "$work/inbox" 2>"$work/inbox.err"; then
  notify "inbox-$(date -u +%F)" "codeforge trinity-import：讀 fleet-cli 收件匣失敗（$(head -c 200 "$work/inbox.err" | tr '\n' ' ')）"
  exit 1
fi

jq -c --arg s "$SENDERS" '
  ($s | split(" ")) as $ok
  | select(.from as $f | $ok | any(. as $o | $f == $o or ($f | startswith($o + "@"))))
  | select((.content | fromjson? | objects | .schema) == "trinity-usage/1")
  | {id, content}' "$work/inbox" > "$work/msgs"

rc=0
while IFS= read -r m; do
  id=$(jq -r .id <<<"$m")
  grep -qxF "$id" "$STATE/done" && continue
  f="$work/msg.jsonl"
  jq -r .content <<<"$m" > "$f"
  if out=$("$CODEFORGE" usage trinity-import "$f" 2>&1); then
    echo "$id" >> "$STATE/done"
    date +%s > "$STATE/last_ok"
    log "ok $id $(tr '\n' ' ' <<<"$out")"
  else
    rc=1
    log "fail $id $(tr '\n' ' ' <<<"$out")"
    notify "fail-$id" "codeforge trinity-import 失敗：訊息 $id（$(head -c 300 <<<"$out" | tr '\n' ' ')）。下次 timer 會重試。"
  fi
done < "$work/msgs"

# the stale alert starts after the first successful import (trinity may not report yet)
[ -s "$STATE/last_ok" ] || exit "$rc"
age_h=$(( ($(date +%s) - $(cat "$STATE/last_ok")) / 3600 ))
if [ "$age_h" -ge "$STALE_H" ]; then
  notify "stale-$(date -u +%F)" "codeforge trinity-import：已 ${age_h} 小時沒有新的 trinity 配額（門檻 ${STALE_H}h）。trinity 可能沒被叫醒，或回報沒送出。"
fi
exit "$rc"
