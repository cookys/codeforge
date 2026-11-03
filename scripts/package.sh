#!/usr/bin/env bash
# Build and package one release target the way install.sh downloads it (G38: no hosted CI).
#
#   ./scripts/package.sh <target> [--toolchain <tc>] [--out <dir>] [--version <v>]
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
while [ $# -gt 0 ]; do
  case "$1" in
    --toolchain) tc="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    -*) echo "package: unknown option: $1" >&2; exit 2 ;;
    *) target="$1"; shift ;;
  esac
done
[ -n "$target" ] || { echo "package: a target is required" >&2; exit 2; }
[ -n "$version" ] || version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')

cargo_cmd=(cargo)
[ -n "$tc" ] && cargo_cmd=(cargo "+$tc")
"${cargo_cmd[@]}" build --release --locked --target "$target"

name="codeforge-${version}-${target}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name" "$out"
cp "target/$target/release/codeforge" "$stage/$name/codeforge"
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
