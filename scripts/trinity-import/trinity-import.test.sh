#!/usr/bin/env bash
# Tests trinity-import.sh against a fake fleet and the real codeforge binary. The live dir is a
# fresh tmpfs dir under /dev/shm (AUTOPILOT_LIVE_DIR must be RAM-backed), so nothing real is read.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
T=$(mktemp -d)
L=$(mktemp -d -p /dev/shm)
trap 'rm -rf "$T" "$L"' EXIT
export T
bin="$root/target/debug/codeforge"
(cd "$root" && cargo build --quiet)
mkdir -p "$T/bin"
cat > "$T/bin/fleet" <<'F'
#!/usr/bin/env bash
case "$1" in
  inbox) [ -e "$T/inboxfail" ] && { echo down >&2; exit 1; }; cat "$T/inbox.json";;
  send) echo "$2|$3" >> "$T/sent";;
esac
F
chmod +x "$T/bin/fleet"
export FLEET_BIN="$T/bin/fleet" CODEFORGE_BIN="$bin" CODEFORGE_TRINITY_STATE="$T/state" \
  AUTOPILOT_LIVE_DIR="$L" CODEFORGE_DATA_DIR="$T/data" CODEFORGE_TRINITY_NOTIFY="cap cf"
fail() { echo "FAIL: $*" >&2; exit 1; }
q="$L/codeforge/usage/trinity-quota.json"
now=$(date -u +%FT%TZ)
earlier=$(date -u -d '-1 hour' +%FT%TZ)
reset=$(date -u -d '+3 days' +%FT%TZ)
msg() { # id from observed pct
  jq -cn --arg id "$1" --arg f "$2" --arg o "$3" --argjson p "$4" --arg r "$reset" \
    '{id:$id, from:$f, content:({schema:"trinity-usage/1",observed_at:$o,pct:$p,resets_at:$r}|tojson)}'
}
{
  msg A cookys/grokbot/trinity@abc "$earlier" 0.2
  msg B cookys/grokbot/trinity "$now" 0.4
  msg C cookys/evil/trinity "$now" 0.9
  msg C2 cookys/grokbot/trinityx "$now" 0.9
  msg F cookys/grokbot/fleet-cli "$earlier" 0.3
  jq -cn '{id:"D", from:"cookys/grokbot/trinity", content:"trinity-usage/1 2026-11-11\n{}"}'
} > "$T/inbox.json"

"$here/trinity-import.sh" || fail "first run"
[ "$(jq .pct "$q")" = 0.4 ] || fail "newest pct not written: $(cat "$q")"
[ "$(sort "$T/state/done" | tr '\n' ' ')" = "A B F " ] || fail "done ids: $(cat "$T/state/done")"
[ ! -e "$T/sent" ] || fail "unexpected notify: $(cat "$T/sent")"

msg E cookys/grokbot/trinity "$earlier" 0.99 >> "$T/inbox.json"
"$here/trinity-import.sh" || fail "older report run"
[ "$(jq .pct "$q")" = 0.4 ] || fail "older report overwrote"
grep -qxF E "$T/state/done" || fail "older report not marked done"

# a well-formed message with no valid line (34 instead of 0.34) fails and alerts once
msg G cookys/grokbot/trinity "$now" 34 >> "$T/inbox.json"
if "$here/trinity-import.sh"; then fail "invalid-only message not reported"; fi
"$here/trinity-import.sh" || true
[ "$(grep -c '^cf|.*訊息 G' "$T/sent")" = 1 ] || fail "invalid-only alert not once: $(cat "$T/sent")"
! grep -qxF G "$T/state/done" || fail "invalid-only message marked done"
[ "$(jq .pct "$q")" = 0.4 ] || fail "invalid-only message changed the quota"
jq -c 'select(.id != "G")' "$T/inbox.json" > "$T/inbox.tmp" && mv "$T/inbox.tmp" "$T/inbox.json"
: > "$T/sent"

touch "$T/inboxfail"
if "$here/trinity-import.sh"; then fail "inbox failure not reported"; fi
"$here/trinity-import.sh" || true
[ "$(grep -c '^cap|' "$T/sent")" = 1 ] || fail "inbox alert not once: $(cat "$T/sent")"
rm "$T/inboxfail"

# an idle trinity sends nothing for a day: no alert (2026-11-14 false alarm at 24h)
echo $(( $(date +%s) - 25 * 3600 )) > "$T/state/last_ok"
"$here/trinity-import.sh"
! grep -q '小時沒有新的 trinity 配額' "$T/sent" || fail "stale alert after 25h (threshold is one weekly window)"
echo $(( $(date +%s) - 169 * 3600 )) > "$T/state/last_ok"
"$here/trinity-import.sh"
grep -q '^cf|.*小時沒有新的 trinity 配額' "$T/sent" || fail "stale alert missing"
echo "trinity-import.test.sh: ok"
