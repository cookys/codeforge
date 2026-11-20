#!/usr/bin/env bash
# Test for check-architecture-coverage.sh: a fixture map missing one subcommand must fail
# (exit 1, names it); the complete fixture, a 5-cell row and a deny-list hit are each checked.
set -euo pipefail
cd "$(dirname "$0")/.."

W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/src/cli" "$W/plugins/forge-stats/hooks"
: >"$W/src/cli/install.rs"
: >"$W/plugins/forge-stats/hooks/register.tsx"
cat >"$W/bin" <<'BIN'
#!/usr/bin/env bash
if [ "$#" -eq 1 ] && [ "$1" = "--help" ]; then
  printf 'Commands:\n  alpha  a\n  beta   b\n  help   h\n\nOptions:\n'
fi
BIN
chmod +x "$W/bin"

# shellcheck disable=SC2016
row() { printf '| %s | live | `x/**` | doc/a.md@abc1234 | CLI | `true` |\n' "$1"; }
mk() { # mk <file> <rows...>
  f=$1; shift
  { printf '## 功能地圖\n\n| 名稱 | 狀態 | 在哪裡 | 依據 | 互動介面 | 驗收方式 |\n|---|---|---|---|---|---|\n'
    printf '%s\n' "$@"; printf '\n## 缺口清單\n'; } >"$f"
}
run() { # run <arch> -> sets RC, OUT
  set +e
  OUT=$(ARCH_FILE=$1 ROOT=$W CODEFORGE_BIN=$W/bin DENY_FILE=${2:-/nonexistent} ./scripts/check-architecture-coverage.sh 2>&1)
  RC=$?
  set -e
}
fail() { echo "FAIL: $*" >&2; echo "$OUT" >&2; exit 1; }

mk "$W/ok.md" "$(row 'Alpha [aliases: alpha]')" "$(row 'Beta [aliases: beta]')" \
  "$(row 'Script [aliases: check-all.sh, package.sh, release-local.sh, repro-check.sh, fmt.sh, check.sh, run.py, install.sh]')"
run "$W/ok.md"
[ "$RC" -eq 0 ] || fail "complete map should pass"

mk "$W/miss.md" "$(row 'Alpha [aliases: alpha]')" \
  "$(row 'Script [aliases: check-all.sh, package.sh, release-local.sh, repro-check.sh, fmt.sh, check.sh, run.py, install.sh]')"
run "$W/miss.md"
[ "$RC" -eq 1 ] || fail "map missing beta must exit 1"
grep -q 'subcommand: beta' <<<"$OUT" || fail "must name beta"
grep -q 'unmapped: 1' <<<"$OUT" || fail "must count 1"

# shellcheck disable=SC2016
mk "$W/cells.md" "$(row 'Alpha [aliases: alpha]')" "$(row 'Beta [aliases: beta]')" \
  '| Script [aliases: check-all.sh, package.sh, release-local.sh, repro-check.sh, fmt.sh, check.sh, run.py, install.sh] | live | `x` | doc/a.md@abc1234 | CLI | |'
run "$W/cells.md"
[ "$RC" -eq 1 ] || fail "5-cell row must exit 1"

printf 'zzdenyzz\n' >"$W/deny.txt"
{ cat "$W/ok.md"; echo 'zzdenyzz'; } >"$W/deny.md"
run "$W/deny.md" "$W/deny.txt"
[ "$RC" -eq 1 ] || fail "deny hit must exit 1"
grep -q 'deny-list hit' <<<"$OUT" || fail "must report deny hit"
if grep -q 'zzdenyzz' <<<"$OUT"; then fail "deny value leaked into output"; fi
echo "check-architecture-coverage.test: ok"
