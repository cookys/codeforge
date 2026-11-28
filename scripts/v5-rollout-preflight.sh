#!/usr/bin/env bash
# v5-rollout-preflight.sh — dry-run preflight inventory for console v5 env-key rollouts (B79).
# Read-only by design: prints one inventory line + one rollback-plan line per account,
# and takes no snapshot itself. The rollout owner (hangar) runs the printed backup step.
# A host/account is passed as a repeatable triple so the script never guesses paths:
#   v5-rollout-preflight.sh --account <name> --settings <settings.json> --bin <codeforge> [--env-key <KEY>] [--expect <KEY=VALUE>]
# --env-key defaults to AUTOPILOT_LIVE_V5 (the console v5 frame key; the v7.4-frame sibling
# AUTOPILOT_LIVE_V75 is what the operator seat carried per memory verify-on-operator-config).
# Exit: 0 inventory printed; 1 a triple is unreadable; 2 usage error. Never writes anything.
set -euo pipefail

ENV_KEY='AUTOPILOT_LIVE_V5'
accounts=(); settings=(); bins=(); expect=()

usage() { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

while [[ $# -gt 0 ]]; do
  case $1 in
    --account)  [[ $# -ge 2 ]] || usage; accounts+=("$2"); shift 2 ;;
    --settings) [[ $# -ge 2 ]] || usage; settings+=("$2"); shift 2 ;;
    --bin)      [[ $# -ge 2 ]] || usage; bins+=("$2"); shift 2 ;;
    --env-key)  [[ $# -ge 2 ]] || usage; ENV_KEY=$2; shift 2 ;;
    --expect)   [[ $# -ge 2 ]] || usage; expect+=("$2"); shift 2 ;;
    -h|--help)  usage ;;
    *) usage ;;
  esac
done

n=${#accounts[@]}
if [[ $n -eq 0 || $n -ne ${#settings[@]} || $n -ne ${#bins[@]} ]]; then usage; fi

sha16() { sha256sum -- "$1" | cut -c1-16; }
env_value() { # print the KEY value from a settings.json env block, or absent
  python3 -I - "$1" "$2" <<'PY'
import json, sys
try:
    data = json.load(open(sys.argv[1], encoding='utf-8'))
except (OSError, ValueError):
    print('unreadable'); sys.exit(0)
env = data.get('env') if isinstance(data, dict) else None
print(env.get(sys.argv[2], 'absent') if isinstance(env, dict) else 'absent')
PY
}

rc=0
printf '# v5-rollout-preflight (dry-run; nothing was read into any store, nothing written)\n'
for ((i = 0; i < n; i++)); do
  a=${accounts[i]}; s=${settings[i]}; b=${bins[i]}
  [[ -r $s ]] || { echo "account=$a RESULT=settings-unreadable path=$s" >&2; rc=1; continue; }
  [[ -x $b ]] || { echo "account=$a RESULT=bin-missing path=$b" >&2; rc=1; continue; }
  v=$(env_value "$s" "$ENV_KEY")
  printf 'account=%s settings=%s %s=%s settings_sha16=%s bin=%s bin_sha16=%s\n' \
    "$a" "$s" "$ENV_KEY" "$v" "$(sha16 "$s")" "$b" "$(sha16 "$b")"
  printf 'rollback=%s: cp -p "%s" "%s.pre-rollout"  # take BEFORE the rollout; restore = cp -p "%s.pre-rollout" "%s"\n' \
    "$a" "$s" "$s" "$s" "$s"
done

for e in ${expect[@]+"${expect[@]}"}; do
  k=${e%%=*}; want=${e#*=}
  for ((i = 0; i < n; i++)); do
    have=$(env_value "${settings[i]}" "$k")
    [[ $have == "$want" ]] || { echo "expect-miss account=${accounts[i]} $k want=$want have=$have" >&2; rc=1; }
  done
done
exit $rc
