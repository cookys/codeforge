#!/usr/bin/env bash
# Build and package one release target the way install.sh downloads it (G38: no hosted CI).
#
#   ./scripts/package.sh <target> [--toolchain <tc>] [--out <dir>] [--version <v>]
#                        [--cross] [--max-glibc <x.y>]
#
# --cross        build with `cross` in a container (old glibc, other architectures), into its own
#                target dir (target/cross) so a native build of the same triple is never packed.
# --max-glibc    refuse a binary that needs a newer glibc than <x.y> (readelf's GLIBC_ versions).
#
# Produces, in <out> (default: dist):
#   codeforge-<v>-<target>.tar.gz          holding codeforge-<v>-<target>/{codeforge,README.md,LICENSE}
#   codeforge-<v>-<target>.tar.gz.sha256   `sha256sum` format: "<hex>  <archive name>"
# — the names install.sh builds (archive="codeforge-${version}-${target}.tar.gz").
# <v> defaults to Cargo.toml's version. The binary is built `--release --locked`, stripped, and,
# when <target> runs on this host, the packed binary is extracted and must print "codeforge <v>".
# Prints the archive path on the last line.
set -euo pipefail
cd "$(dirname "$0")/.."

target=""
tc=""
out="dist"
version=""
use_cross=0
max_glibc=""
while [ $# -gt 0 ]; do
  case "$1" in
    --toolchain) tc="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    --cross) use_cross=1; shift ;;
    --max-glibc) max_glibc="$2"; shift 2 ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    -*) echo "package: unknown option: $1" >&2; exit 2 ;;
    *) target="$1"; shift ;;
  esac
done
[ -n "$target" ] || { echo "package: a target is required" >&2; exit 2; }
[ -n "$version" ] || version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')

tool=cargo
target_dir="${CARGO_TARGET_DIR:-target}"
if [ "$use_cross" = 1 ]; then
  tool=cross
  target_dir="target/cross"
  export CARGO_TARGET_DIR="$target_dir"
  if [ -z "${CROSS_CONTAINER_ENGINE:-}" ] && ! command -v docker >/dev/null 2>&1; then
    export CROSS_CONTAINER_ENGINE=podman
  fi
fi
build_cmd=("$tool")
[ -n "$tc" ] && build_cmd=("$tool" "+$tc")
"${build_cmd[@]}" build --release --locked --target "$target"
bin="$target_dir/$target/release/codeforge"

if [ -n "$max_glibc" ]; then
  need=$(readelf --dyn-syms -W "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
  if [ -n "$need" ] && [ "$(printf '%s\n%s\n' "$need" "$max_glibc" | sort -V | tail -1)" != "$max_glibc" ]; then
    echo "package: $target needs glibc $need, newer than the allowed $max_glibc" >&2
    exit 1
  fi
  echo "package: $target needs glibc ${need:-none} (<= $max_glibc)" >&2
fi

name="codeforge-${version}-${target}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name" "$out"
cp "$bin" "$stage/$name/codeforge"
chmod +x "$stage/$name/codeforge"
cp README.md LICENSE "$stage/$name/"
strip "$stage/$name/codeforge" 2>/dev/null || echo "package: strip skipped for $target" >&2
tar czf "$out/$name.tar.gz" -C "$stage" "$name"
(cd "$out" && sha256sum "$name.tar.gz" >"$name.tar.gz.sha256")

# smoke: the packed binary runs and reports the version, when this host can run it
host="$(rustc -vV | sed -n 's/^host: //p')"
if [ "$target" = "$host" ]; then
  check="$(mktemp -d)"
  tar xzf "$out/$name.tar.gz" -C "$check"
  got="$("$check/$name/codeforge" --version)"
  rm -rf "$check"
  if [ "$got" != "codeforge $version" ]; then
    echo "package: the packed binary says '$got', expected 'codeforge $version'" >&2
    exit 1
  fi
  echo "package: smoke ok ($got)" >&2
fi
(cd "$out" && sha256sum -c "$name.tar.gz.sha256" >&2)
echo "$out/$name.tar.gz"
