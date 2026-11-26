#!/bin/sh
# doc/BACKLOG.md is a VIEW of the hippo store (.hippo/): the store is the only writer, never edit the file by hand.
#   scripts/backlog-view.sh            rewrite doc/BACKLOG.md from the store
#   scripts/backlog-view.sh --check    exit 1 when the file differs from the store (nothing is written; part of check-all)
# Add or change a row with `hippo new` / `hippo edit` / `hippo set` / `hippo close`, then run this script and commit
# .hippo/ and doc/BACKLOG.md together. The generator is scripts/backlog-restore.py (a copy of hippo's
# scripts/backlog-restore.py).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
[ -f "$root/.hippo/repo.toml" ] || { echo 'backlog-view: no .hippo store in this checkout' >&2; exit 2; }
if [ -z "${HIPPO_BIN:-}" ]; then
  if command -v hippo > /dev/null 2>&1; then HIPPO_BIN=hippo; else HIPPO_BIN=$HOME/.local/bin/hippo; fi
fi
export HIPPO_BIN
header=$(mktemp)
trap 'rm -f "$header"' EXIT
cat > "$header" <<'HDR'
# CodeForge backlog

格式依 autopilot `references/backlog-entry.md`；一列一件，細節放在 Pointer 指的地方。自動派工從 Status=open 且 Trigger-Type=ready 的列取下一件。本檔由 `scripts/backlog-view.sh` 從 hippo 紀錄庫產生，不要手改。

<!-- next-id: {next-id} -->
HDR
if [ "${1:-}" = --check ]; then
  exec python3 -I "$root/scripts/backlog-restore.py" check "$root" --header "$header" --order asc --backlog doc/BACKLOG.md
fi
out=$root/doc/BACKLOG.md
python3 -I "$root/scripts/backlog-restore.py" render "$root" --header "$header" --order asc --backlog doc/BACKLOG.md > "$out.tmp.$$"
mv -f "$out.tmp.$$" "$out"
