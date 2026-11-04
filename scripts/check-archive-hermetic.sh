#!/usr/bin/env bash
# Fail when a release tarball carries anything from the build machine or the build moment (B43).
#
#   ./scripts/check-archive-hermetic.sh <archive.tar.gz> --epoch <n> [--forbid <string>]...
#
# Checks, each a reason to exit 1:
#   - the gzip header holds a timestamp or an original file name (gzip without -n);
#   - an entry's mtime differs from <epoch>, or its owner/group is not 0/0;
#   - an entry is a pax extended header (carries atime/ctime);
#   - entries are not in byte order (the build machine's directory-listing order leaked);
#   - any packed file (the binary included) contains a --forbid string, e.g. the checkout path
#     or $HOME, which a build without --remap-path-prefix leaves in panic locations.
# Exit 0 = hermetic. Needs GNU tar.
set -euo pipefail

archive=""
epoch=""
forbid=()
while [ $# -gt 0 ]; do
  case "$1" in
    --epoch) epoch="$2"; shift 2 ;;
    --forbid) forbid+=("$2"); shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "check-archive-hermetic: unknown option: $1" >&2; exit 2 ;;
    *) archive="$1"; shift ;;
  esac
done
[ -n "$archive" ] && [ -f "$archive" ] || { echo "check-archive-hermetic: no archive given" >&2; exit 2; }
[[ "$epoch" =~ ^[0-9]+$ ]] || { echo "check-archive-hermetic: --epoch <seconds> is required" >&2; exit 2; }

bad=()

# gzip header (RFC 1952): byte 3 = FLG (0x08 = FNAME present), bytes 4..7 = MTIME (little endian).
read -r flg m0 m1 m2 m3 < <(od -A n -t u1 -j 3 -N 5 "$archive")
if [ $((flg & 8)) -ne 0 ]; then bad+=("gzip header stores the original file name (use gzip -n)"); fi
if [ $((m0 | m1 | m2 | m3)) -ne 0 ]; then bad+=("gzip header stores a timestamp (use gzip -n)"); fi

want="$(TZ=UTC date -u -d "@$epoch" '+%Y-%m-%d %H:%M:%S')"
while IFS= read -r line; do
  # perms owner/group size date time name
  read -r _perms owner _size day time name <<<"$line"
  if [ "$owner" != "0/0" ]; then bad+=("$name: owner/group $owner, want 0/0"); fi
  if [ "$day $time" != "$want" ]; then bad+=("$name: mtime $day $time, want $want (epoch $epoch)"); fi
  case "$name" in */PaxHeaders*|PaxHeaders*) bad+=("$name: pax extended header (atime/ctime)") ;; esac
done < <(TZ=UTC tar --full-time --numeric-owner -tvzf "$archive")

# Entry order must not depend on the build machine's directory-listing order (tar --sort=name).
if ! tar -tzf "$archive" | LC_ALL=C sort -c 2>/dev/null; then
  bad+=("entries are not in byte order (directory-listing order leaked; use tar --sort=name)")
fi

if [ "${#forbid[@]}" -gt 0 ]; then
  x="$(mktemp -d)"
  trap 'rm -rf "$x"' EXIT
  tar xzf "$archive" -C "$x"
  for s in "${forbid[@]}"; do
    [ -n "$s" ] || continue
    while IFS= read -r f; do
      bad+=("${f#"$x"/}: contains the build-machine string '$s'")
    done < <(grep -rlF -- "$s" "$x" || true)
  done
fi

if [ "${#bad[@]}" -gt 0 ]; then
  echo "check-archive-hermetic: $archive is not hermetic:" >&2
  printf '  - %s\n' "${bad[@]}" >&2
  exit 1
fi
echo "check-archive-hermetic: ok ($archive)" >&2
