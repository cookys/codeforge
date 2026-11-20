#!/usr/bin/env bash
# ARCHITECTURE.md coverage check. Every external surface must map to a row of the 功能地圖:
#   - each CLI subcommand (top level and one level down), from `codeforge --help`
#   - each hook command install.rs writes, and each project hook script it names
#   - each forge-stats mod component and command (plugins/forge-stats/hooks/register.tsx)
#   - the operator-facing scripts listed in OPERATOR_SCRIPTS below
# Also: every row has six non-empty cells (a `requested` row only three), and no line of
# ARCHITECTURE.md contains an entry of the deny list (only line numbers are printed).
# Prints `unmapped: N` (+ the missing items) and exits 1 when anything is wrong.
#
# Env: CODEFORGE_BIN (default: cargo run -q --), ARCH_FILE, ROOT, DENY_FILE (default /etc/lab/deny.txt).
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="${ROOT:-.}"
ARCH_FILE="${ARCH_FILE:-ARCHITECTURE.md}"
DENY_FILE="${DENY_FILE:-/etc/lab/deny.txt}"
if [ -n "${CODEFORGE_BIN:-}" ]; then BIN=("$CODEFORGE_BIN"); else BIN=(cargo run -q --); fi

OPERATOR_SCRIPTS=(
  scripts/check-all.sh scripts/package.sh scripts/release-local.sh scripts/repro-check.sh
  scripts/fmt.sh scripts/g40-real-data/check.sh scripts/pane-gate/v74-keys/run.py install.sh
)

missing=()
problems=()

# The 功能地圖 table rows (between "## 功能地圖" and the next "## " heading).
rows=$(awk '/^## 功能地圖/{on=1;next} /^## /{on=0} on && /^\|/ && !/^\|[-| ]+\|$/ && !/^\| 名稱/' "$ARCH_FILE")
table_text=$rows

# Alias items of every row, one per line.
aliases=$(printf '%s\n' "$rows" | sed -n 's/.*\[aliases: \([^]]*\)\].*/\1/p' | tr ',' '\n' | sed 's/^ *//;s/ *$//')

# mapped_cmd <words>: `codeforge <words>` appears in the table, or <words> is an alias item.
mapped_cmd() {
  printf '%s\n' "$aliases" | grep -qxF -- "$1" && return 0
  printf '%s\n' "$table_text" | grep -qE -- "codeforge $1([^a-z-]|$)"
}
# mapped_word <token>: the token appears as a whole word anywhere in the table.
mapped_word() {
  printf '%s\n' "$table_text" | grep -qE -- "(^|[^A-Za-z0-9_.-])$(printf '%s' "$1" | sed 's/[][\.*^$/]/\\&/g')([^A-Za-z0-9_-]|$)"
}

# subcommand names from a clap help text on stdin
subs_of() { awk '/^Commands:/{on=1;next} on && /^$/{on=0} on && NF {print $1}' | grep -vx help || true; }

top=$("${BIN[@]}" --help 2>&1 | subs_of)
for s in $top; do
  mapped_cmd "$s" || missing+=("subcommand: $s")
  for t in $("${BIN[@]}" "$s" --help 2>&1 | subs_of); do
    mapped_cmd "$s $t" || missing+=("subcommand: $s $t")
  done
done

# hooks install.rs writes: first two words after `codeforge` (flags dropped), plus project scripts
install_rs="$ROOT/src/cli/install.rs"
while IFS= read -r h; do
  [ -n "$h" ] || continue
  mapped_cmd "$h" || mapped_word "$h" || missing+=("hook: codeforge $h")
done < <(grep -o 'hook_entry(\s*"codeforge [^"]*"' "$install_rs" \
  | sed 's/.*"codeforge //;s/"$//' | sed 's/ -.*//;s/ 2>.*//;s/ ||.*//' \
  | sed -E 's/^(hook [a-z-]+) .*/\1/' | sort -u)
while IFS= read -r js; do
  mapped_word "$js" || missing+=("hook script: $js")
done < <(grep -o '"[a-z-]*\.js"' "$install_rs" | tr -d '"' | sort -u)

# forge-stats mod: components and commands
reg="$ROOT/plugins/forge-stats/hooks/register.tsx"
while IFS= read -r c; do
  mapped_word "$c" || missing+=("mod component: $c")
done < <({ grep -o "component: '[A-Za-z]*'" "$reg" | sed "s/.*'\(.*\)'/\1/"; \
           grep -o "command.register({ name: '[^']*'" "$reg" | sed "s/.*'\(.*\)'/\1/"; } | sort -u)

# operator-facing scripts: the file name (or path) appears in the table
for p in "${OPERATOR_SCRIPTS[@]}"; do
  printf '%s\n' "$table_text" | grep -qF -- "$p" || mapped_word "$(basename "$p")" || missing+=("script: $p")
done

# six cells per row (requested rows: three)
n=0
while IFS= read -r row; do
  n=$((n + 1))
  IFS='|' read -r -a cells <<<"$row"
  # cells[0] is empty (before the first |); real cells are 1..6
  filled=0
  for i in 1 2 3 4 5 6; do
    c="${cells[$i]:-}"
    [ -n "$(printf '%s' "$c" | tr -d '[:space:]')" ] && filled=$((filled + 1))
  done
  status=$(printf '%s' "${cells[2]:-}" | tr -d '[:space:]')
  name=$(printf '%s' "${cells[1]:-}" | cut -c1-40)
  case "$status" in
    requested) [ "$filled" -ge 3 ] || problems+=("row has <3 cells: $name") ;;
    live|building|planned)
      [ "$filled" -eq 6 ] || problems+=("row has $filled/6 cells: $name") ;;
    *) problems+=("bad status '$status': $name") ;;
  esac
done <<<"$rows"

# deny list: print line numbers only
if [ -r "$DENY_FILE" ]; then
  while IFS= read -r term; do
    [ -n "$term" ] || continue
    case "$term" in '#'*) continue ;; esac
    while IFS= read -r ln; do
      problems+=("deny-list hit at $ARCH_FILE line $ln")
    done < <(grep -niF -- "$term" "$ARCH_FILE" | cut -d: -f1 || true)
  done <"$DENY_FILE"
fi

total=$(( ${#missing[@]} + ${#problems[@]} ))
for m in "${missing[@]+"${missing[@]}"}"; do echo "missing: $m"; done
for p in "${problems[@]+"${problems[@]}"}"; do echo "problem: $p"; done
echo "unmapped: $total"
[ "$total" -eq 0 ]
