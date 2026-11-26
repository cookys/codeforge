#!/usr/bin/env bash
# Install codeforge from the reproducible release package (B64) — the entry point the hangar
# fleet push calls, replacing `cargo install --path .` (which bakes the clone path and $HOME
# into the binary, so the same commit gave a different sha16 on every host).
#
#   ./scripts/install-release.sh [--dest <dir>] [--target <triple>]
#
# <dest> defaults to ${CARGO_HOME:-~/.cargo}/bin; <target> defaults to this host's triple
# (rustc -vV). Packages HEAD through scripts/package.sh (B43: same commit, same bytes) into a
# temp dir, extracts the packed `codeforge` binary and installs it with mode 0755: written to a
# temp name inside <dest> and mv'd over the old binary, so a failure never leaves a half file
# and never touches the previous install. Last line on success:
#   install-release: <dest>/codeforge sha16 <first 16 hex of its sha256> commit <short sha>
# The sha16 is identical on every host for the same commit. Any failure exits non-zero.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$(pwd -P)"

dest="${CARGO_HOME:-$HOME/.cargo}/bin"
target=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dest) dest="$2"; shift 2 ;;
    --target) target="$2"; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "install-release: unknown option: $1" >&2; exit 2 ;;
    *) echo "install-release: unexpected argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$target" ] || target="$(rustc -vV | sed -n 's/^host: //p')"

pkg="${INSTALL_RELEASE_PACKAGE:-$root/scripts/package.sh}"
stage="$(mktemp -d)"
tmpbin=""
cleanup() { rm -rf "$stage"; if [ -n "$tmpbin" ]; then rm -f "$tmpbin"; fi; }
trap cleanup EXIT

archive="$( "$pkg" "$target" --out "$stage" | tail -1 )"
if [ -z "$archive" ] || [ ! -f "$archive" ]; then
  echo "install-release: the package step produced no usable archive: ${archive:-none}" >&2
  exit 1
fi
mkdir -p "$stage/x"
tar xzf "$archive" -C "$stage/x"
binfile="$(find "$stage/x" -type f -name codeforge)"
if [ "$(printf '%s\n' "$binfile" | grep -c .)" -ne 1 ]; then
  echo "install-release: the archive must hold exactly one codeforge binary" >&2
  exit 1
fi

mkdir -p "$dest"
tmpbin="$dest/codeforge.tmp.$$"
install -m 0755 "$binfile" "$tmpbin"
mv -f "$tmpbin" "$dest/codeforge"
tmpbin=""
sha16="$(sha256sum "$dest/codeforge" | cut -c1-16)"
commit="$(git rev-parse --short HEAD)" || { echo "install-release: not in a git checkout" >&2; exit 1; }
echo "install-release: $dest/codeforge sha16 $sha16 commit $commit"
