#!/usr/bin/env bash
# v5-rollout-preflight.test.sh — controls for scripts/v5-rollout-preflight.sh (B81).
# Sandbox fixture only: never touches a real ~/.claude (real-host preflight is hangar's rollout window).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pf="$repo/scripts/v5-rollout-preflight.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
controls=0

fail() { echo "v5-rollout-preflight.test.sh: $*" >&2; exit 1; }

mk_fixture() { # $1 = dir: a settings.json with the v5 keys off + a fake codeforge bin
  mkdir -p "$1"
  printf '{"env":{"AUTOPILOT_LIVE_V5":"0","AUTOPILOT_LIVE_V75":"0"}}' >"$1/settings.json"
  printf '#!/bin/sh\necho codeforge-fixture\n' >"$1/codeforge"
  chmod +x "$1/codeforge"
}

# 1) inventory columns: every field present, sha16 is 16 hex, env key read from the settings env block.
mk_fixture "$tmp/a"
out="$("$pf" --account team --settings "$tmp/a/settings.json" --bin "$tmp/a/codeforge" \
  --env-key AUTOPILOT_LIVE_V5 --expect AUTOPILOT_LIVE_V5=0)" \
  || fail "dry-run exited non-zero on a good fixture"
grep -Eq '^account=team settings=.* AUTOPILOT_LIVE_V5=0 settings_sha16=[0-9a-f]{16} bin=.* bin_sha16=[0-9a-f]{16}$' <<<"$out" \
  || fail "inventory columns drifted: $out"
controls=$((controls + 1))

# 2) the printed rollback command is valid bash AND actually restores the settings file.
rb_line="$(grep '^rollback=team: ' <<<"$out")" || fail "rollback line missing: $out"
rb_cmd="${rb_line#rollback=team: }"
rb_cmd="${rb_cmd%%  #*}"
bash -n <<<"$rb_cmd" || fail "rollback command is not valid bash: $rb_cmd"
cp -p "$tmp/a/settings.json" "$tmp/a/settings.json.pre-rollout"   # the same backup step the line prints
printf 'MUTATED' >"$tmp/a/settings.json"                          # simulate a bad rollout
eval "$rb_cmd"
cmp -s "$tmp/a/settings.json" "$tmp/a/settings.json.pre-rollout" \
  || fail "rollback command did not restore the settings file"
controls=$((controls + 1))

# 3) dry-run touches nothing: nanosecond mtimes and ctimes identical before/after, no backup file written.
mk_fixture "$tmp/b"
t1="$(stat -c '%y %z %n' "$tmp/b/settings.json" "$tmp/b/codeforge" | sort)"
"$pf" --account t2 --settings "$tmp/b/settings.json" --bin "$tmp/b/codeforge" >/dev/null
t2="$(stat -c '%y %z %n' "$tmp/b/settings.json" "$tmp/b/codeforge" | sort)"
[[ "$t1" == "$t2" ]] || fail "dry-run changed file timestamps"
[[ ! -e "$tmp/b/settings.json.pre-rollout" ]] || fail "dry-run wrote a backup file itself"
controls=$((controls + 1))

# 4) --expect mismatch fails loudly (a rollout keyed off the wrong value must not pass preflight).
if "$pf" --account team --settings "$tmp/a/settings.json" --bin "$tmp/a/codeforge" \
  --expect AUTOPILOT_LIVE_V5=1 >/dev/null 2>&1; then
  fail "--expect mismatch must exit non-zero"
fi
controls=$((controls + 1))

# 5) an unreadable settings path exits non-zero with settings-unreadable (never a silent skip).
if err="$("$pf" --account x --settings "$tmp/nope/settings.json" --bin "$tmp/a/codeforge" 2>&1)"; then
  fail "unreadable settings must exit non-zero"
fi
grep -q 'settings-unreadable' <<<"$err" || fail "wrong failure mode: $err"
controls=$((controls + 1))

echo "v5-rollout-preflight.test.sh: ok ($controls controls)"
