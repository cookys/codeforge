#!/usr/bin/env bash
# scripts/in-dir.sh <dir> <cmd> [args...] — run one command inside <dir> and exit with its status.
# The Bash tool resets the shell to the start folder after most calls, so every call used to open with `cd <dir> && ...`
# (574 + 403 repeats in the ar_l2 baseline of 2026-11-21). This is that prefix, once, with a dir check that fails loudly.
set -euo pipefail
dir="${1:-}"
[ -n "$dir" ] && [ -d "$dir" ] || { echo "in-dir: not a directory: ${dir:-<none>}" >&2; exit 2; }
shift
[ "$#" -gt 0 ] || { echo "usage: in-dir.sh <dir> <cmd> [args...]" >&2; exit 2; }
cd "$dir"
exec "$@"
