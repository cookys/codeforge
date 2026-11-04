#!/usr/bin/env bash
# Tests for scripts/lib/pack-tarball.sh + scripts/check-archive-hermetic.py (B43). No cargo build:
# fake staged files stand in for the binary. Run by scripts/check-all.sh.
#   green: a pack_tarball archive passes the check, and packing the same content from a second
#          stage path, with other file mtimes and modes, gives byte-identical output;
#   red:   a plain `tar czf` archive (wall-clock mtimes, real owner, gzip timestamp), a gzip header
#          timestamp, entries out of byte order, a pax extended header, stored owner names, a
#          gzip comment field, a second gzip member, a GNU header atime (also in a second extension header), bytes past the archive end,
#          and a file holding a forbidden path each FAIL.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source-path=SCRIPTDIR/..
. scripts/lib/pack-tarball.sh

w="$(mktemp -d)"
trap '[ -n "${KEEP:-}" ] || rm -rf "$w"' EXIT
epoch=1700000000
name="codeforge-0.0.0-test"
fails=0
ok()   { echo "  ok   $*"; }
nope() { echo "  FAIL $*"; fails=$((fails + 1)); }
check() { python3 -I scripts/check-archive-hermetic.py "$@" 2>"$w/err"; }

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

# metadata `tar -tv` hides: a pax extended header, and stored owner/group names
tar --format=pax --pax-option=comment=leak --sort=name --mtime="@$epoch" --owner=0 --group=0 \
    --numeric-owner -cf - -C "$w/a" "$name" | gzip -n >"$w/pax.tar.gz"
if check "$w/pax.tar.gz" --epoch "$epoch"; then nope "pax header passed"; else ok "pax extended header rejected"; fi
grep -q "pax extended header" "$w/err" || nope "pax: wrong reason: $(cat "$w/err")"
tar --format=gnu --sort=name --mtime="@$epoch" --owner=leakuser:0 --group=leakgroup:0 \
    -cf - -C "$w/a" "$name" | gzip -n >"$w/uname.tar.gz"
if check "$w/uname.tar.gz" --epoch "$epoch"; then nope "owner names passed"; else ok "stored owner/group names rejected"; fi
grep -q "stores owner names" "$w/err" || nope "uname: wrong reason: $(cat "$w/err")"

# gzip metadata past the first member's FNAME/MTIME: a comment field, and a second member
tar --format=gnu --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -cf "$w/clean.tar" -C "$w/a" "$name"
python3 -I - "$w/clean.tar" "$w/comment.tar.gz" "$w/a/$name" <<'EOF'
import sys, zlib
data = open(sys.argv[1], "rb").read()
c = zlib.compressobj(9, zlib.DEFLATED, -zlib.MAX_WBITS)
body = c.compress(data) + c.flush()
hdr = b"\x1f\x8b\x08\x10\x00\x00\x00\x00\x02\x03" + sys.argv[3].encode() + b"\x00"  # FCOMMENT = a path
trailer = (zlib.crc32(data) & 0xFFFFFFFF).to_bytes(4, "little") + (len(data) & 0xFFFFFFFF).to_bytes(4, "little")
open(sys.argv[2], "wb").write(hdr + body + trailer)
EOF
if check "$w/comment.tar.gz" --epoch "$epoch"; then nope "gzip comment passed"; else ok "gzip comment field rejected"; fi
grep -q "FCOMMENT" "$w/err" || nope "comment: wrong reason: $(cat "$w/err")"
half=$(( $(stat -c %s "$w/clean.tar") / 2 ))
{ head -c "$half" "$w/clean.tar" | gzip -n; tail -c +"$((half + 1))" "$w/clean.tar" | gzip -n; } >"$w/multi.tar.gz"
if check "$w/multi.tar.gz" --epoch "$epoch"; then nope "two gzip members passed"; else ok "multi-member gzip rejected"; fi
if ! grep -q "more than one member" "$w/err" || grep -q Traceback "$w/err"; then nope "multi: wrong reason: $(cat "$w/err")"; fi

# what TarInfo does not expose: a GNU header atime, and bytes after the archive's end marker
python3 -I - "$w/clean.tar" "$w/atime.tar" <<'EOF'
import sys
b = bytearray(open(sys.argv[1], "rb").read())
b[345:357] = b"14540000000\x00"                      # GNU atime field of the first header
b[148:156] = b"        "
b[148:156] = b"%06o\x00 " % sum(b[0:512])            # re-seal the header checksum
open(sys.argv[2], "wb").write(bytes(b))
EOF
gzip -n <"$w/atime.tar" >"$w/atime.tar.gz"
if check "$w/atime.tar.gz" --epoch "$epoch"; then nope "GNU atime passed"; else ok "GNU header atime rejected"; fi
grep -q "GNU header stores atime" "$w/err" || nope "atime: wrong reason: $(cat "$w/err")"
{ cat "$w/clean.tar"; printf 'built in %s\n' "$w"; } | gzip -n >"$w/trailer.tar.gz"
if check "$w/trailer.tar.gz" --epoch "$epoch" --forbid "$w"; then nope "trailing path passed"; else ok "bytes past the archive end rejected"; fi
if ! grep -q "after the end of the archive" "$w/err" || ! grep -q "tar stream contains" "$w/err"; then
  nope "trailer: wrong reason: $(cat "$w/err")"
fi

# an entry with both a GNU long-link and a long-name header: a timestamp in the SECOND header
long="$(printf 'x%.0s' $(seq 1 120))"
mkdir -p "$w/ln/$name"
ln -s "$long-target" "$w/ln/$name/$long"
tar --format=gnu --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -cf "$w/ln.tar" -C "$w/ln" "$name"
python3 -I - "$w/ln.tar" "$w/ln-atime.tar" <<'EOF'
import sys
b = bytearray(open(sys.argv[1], "rb").read())
heads, off = [], 0
while off + 512 <= len(b) and any(b[off:off + 512]):
    heads.append(off)
    size = int(bytes(b[off + 124:off + 136]).split(b"\x00")[0].strip() or b"0", 8)
    kind = bytes(b[off + 156:off + 157])
    off += 512 + (0 if kind in (b"1", b"2") else (size + 511) // 512 * 512)
kinds = [bytes(b[h + 156:h + 157]) for h in heads]
i = next(n for n in range(1, len(kinds)) if kinds[n - 1] in (b"K", b"L") and kinds[n] in (b"K", b"L"))
h = heads[i]                                          # the second of two extension headers
b[h + 345:h + 357] = b"14540000000\x00"
b[h + 148:h + 156] = b"        "
b[h + 148:h + 156] = b"%06o\x00 " % sum(b[h:h + 512])
open(sys.argv[2], "wb").write(bytes(b))
EOF
gzip -n <"$w/ln.tar" >"$w/ln.tar.gz"
gzip -n <"$w/ln-atime.tar" >"$w/ln-atime.tar.gz"
if check "$w/ln.tar.gz" --epoch "$epoch"; then ok "long-name + long-link entry is hermetic"; else nope "long-name entry rejected: $(cat "$w/err")"; fi
if check "$w/ln-atime.tar.gz" --epoch "$epoch"; then nope "atime in the second extension header passed"; else ok "atime in the second extension header rejected"; fi
grep -q "GNU header stores atime" "$w/err" || nope "ext atime: wrong reason: $(cat "$w/err")"

printf 'panicked at %s/src/main.rs\n' "$w" >"$w/a/$name/codeforge"
pack_tarball "$w/a" "$name" "$w/leak.tar.gz" "$epoch"
if check "$w/leak.tar.gz" --epoch "$epoch" --forbid "$w"; then nope "path leak passed"; else ok "path leak rejected"; fi

if [ "$fails" -gt 0 ]; then echo "test-archive-hermetic: $fails failure(s)"; exit 1; fi
echo "test-archive-hermetic: all passed"
