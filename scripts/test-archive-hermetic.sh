#!/usr/bin/env bash
# Tests for scripts/lib/pack-tarball.sh + scripts/check-archive-hermetic.sh (B43). No cargo build:
# fake staged files stand in for the binary. Run by scripts/check-all.sh.
#   green: a pack_tarball archive passes the check, and packing the same content from a second
#          stage path, with other file mtimes and modes, gives byte-identical output;
#   red:   a plain `tar czf` archive (wall-clock mtimes, real owner, gzip timestamp), a gzip header
#          timestamp, entries out of byte order, and a file holding a forbidden path each FAIL.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source-path=SCRIPTDIR/..
. scripts/lib/pack-tarball.sh

w="$(mktemp -d)"
trap 'rm -rf "$w"' EXIT
epoch=1700000000
name="codeforge-0.0.0-test"
fails=0
ok()   { echo "  ok   $*"; }
nope() { echo "  FAIL $*"; fails=$((fails + 1)); }
check() { ./scripts/check-archive-hermetic.sh "$@" 2>"$w/err"; }

stage_at() {  # stage_at <dir> <mtime> <umask> <file...>: fake release payload, created in that order
  local d="$1/$name" t="$2" m="$3" f
  shift 3
  mkdir -p "$d"
  for f in "$@"; do (umask "$m"; printf '%s\n' "$f" >"$d/$f"); done
  chmod +x "$d/codeforge"
  touch -d "@$t" "$d" "$d"/*
}

echo "test-archive-hermetic:"
# Stage b differs in path, mtimes, umask and creation order (so directory-listing order differs).
stage_at "$w/a" 1600000000 022 codeforge README.md LICENSE zz-extra aa-extra
stage_at "$w/b/deeper" 1650000000 002 aa-extra LICENSE zz-extra README.md codeforge
pack_tarball "$w/a" "$name" "$w/a.tar.gz" "$epoch"
pack_tarball "$w/b/deeper" "$name" "$w/b.tar.gz" "$epoch"

if check "$w/a.tar.gz" --epoch "$epoch" --forbid "$w"; then ok "pack_tarball archive is hermetic"; else nope "pack_tarball archive rejected: $(cat "$w/err")"; fi
if cmp -s "$w/a.tar.gz" "$w/b.tar.gz"; then ok "same content from another path/mtime/umask is byte-identical"; else nope "archives differ: $(sha256sum "$w/a.tar.gz" "$w/b.tar.gz")"; fi

tar czf "$w/plain.tar.gz" -C "$w/a" "$name"
if check "$w/plain.tar.gz" --epoch "$epoch"; then nope "plain tar czf passed"; else ok "plain tar czf rejected"; fi
grep -q 'mtime' "$w/err" || nope "plain tar czf: no mtime reason"

tar --format=gnu --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -cf - -C "$w/a" "$name" | gzip -9 >"$w/gz-stamp.tar.gz"
printf '\x1f\x8b\x08\x00\x01\x00\x00\x00' | dd of="$w/gz-stamp.tar.gz" bs=1 count=8 conv=notrunc status=none
if check "$w/gz-stamp.tar.gz" --epoch "$epoch"; then nope "gzip timestamp passed"; else ok "gzip header timestamp rejected"; fi

tar --format=gnu --mtime="@$epoch" --owner=0 --group=0 --numeric-owner --no-recursion \
    -czf "$w/unsorted.tar.gz" -C "$w/a" "$name" "$name/zz-extra" "$name/aa-extra" "$name/codeforge"
if check "$w/unsorted.tar.gz" --epoch "$epoch"; then nope "unsorted entries passed"; else ok "unsorted entries rejected"; fi

printf 'panicked at %s/src/main.rs\n' "$w" >"$w/a/$name/codeforge"
pack_tarball "$w/a" "$name" "$w/leak.tar.gz" "$epoch"
if check "$w/leak.tar.gz" --epoch "$epoch" --forbid "$w"; then nope "path leak passed"; else ok "path leak rejected"; fi

if [ "$fails" -gt 0 ]; then echo "test-archive-hermetic: $fails failure(s)"; exit 1; fi
echo "test-archive-hermetic: all passed"
