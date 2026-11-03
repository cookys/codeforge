#!/usr/bin/env bash
# Local release (PM G38: no hosted CI).
#
#   ./scripts/release-local.sh [--out <dir>] [--publish [--allow-missing]]
#
# 1. Builds every release target this host can build through scripts/package.sh, with the MSRV
#    toolchain (Cargo.toml rust-version, as the hosted release job did). Linux targets are built
#    with `cross` (podman or docker) and must need glibc <= 2.35 (Ubuntu 22.04, the floor the
#    hosted runners gave); a host-glibc build is refused.
# 2. Verifies the result the way a user meets it: install.sh installs this host's target from a
#    local server (CODEFORGE_DOWNLOAD_BASE) into a temp dir, and the binary must print the version.
#    Other targets are checked for their ELF machine.
# 3. Writes <out>/RELEASE.txt: files, sha256, glibc needs, and the targets it could not build.
#
# It never publishes. --publish creates a DRAFT GitHub release v<version> from the verified files
# (clean tree, HEAD tagged v<version>); turning the draft into a release is the operator's step.
# --publish refuses while a target is missing unless --allow-missing is given.
# macOS targets (x86_64/aarch64-apple-darwin) cannot be built here.
set -euo pipefail
cd "$(dirname "$0")/.."

out="dist"
publish=0
allow_missing=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --publish) publish=1; shift ;;
    --allow-missing) allow_missing=1; shift ;;
    -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
    *) echo "release-local: unknown argument: $1" >&2; exit 2 ;;
  esac
done

version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
msrv=$(grep -m1 '^rust-version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
max_glibc="2.35"
linux=(x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu)
missing=(x86_64-apple-darwin aarch64-apple-darwin)
host="$(rustc -vV | sed -n 's/^host: //p')"

if [ "$publish" = 1 ]; then
  [ -z "$(git status --porcelain)" ] || { echo "release-local: --publish needs a clean tree" >&2; exit 1; }
  [ "$(git rev-parse -q --verify "refs/tags/v$version^{commit}")" = "$(git rev-parse HEAD)" ] \
    || { echo "release-local: --publish needs HEAD tagged v$version" >&2; exit 1; }
  if [ "${#missing[@]}" -gt 0 ] && [ "$allow_missing" = 0 ]; then
    echo "release-local: not buildable here: ${missing[*]} (pass --allow-missing to publish without them)" >&2
    exit 1
  fi
fi

rm -rf "$out"
mkdir -p "$out"
for t in "${linux[@]}"; do
  echo "── $t (cross, $msrv) ──" >&2
  rustup target add --toolchain "$msrv" "$t" >/dev/null
  ./scripts/package.sh "$t" --cross --toolchain "$msrv" --max-glibc "$max_glibc" --out "$out" --version "$version" >/dev/null
done

# verify: every archive matches its .sha256; the ELF machine is the target's; install.sh installs this host's
(cd "$out" && sha256sum -c ./*.sha256 >&2)
machine_of() { case "$1" in x86_64-*) echo "X86-64" ;; aarch64-*) echo "AArch64" ;; esac; }
check="$(mktemp -d)"
server_pid=""
cleanup() { [ -n "$server_pid" ] && kill "$server_pid" 2>/dev/null; rm -rf "$check"; }
trap cleanup EXIT
report="$out/RELEASE.txt"
{
  echo "codeforge $version — local release, built $(date -u +%Y-%m-%dT%H:%M:%SZ) from $(git rev-parse --short HEAD)"
  echo "toolchain $msrv; linux via cross; glibc floor $max_glibc"
  echo
} >"$report"
for t in "${linux[@]}"; do
  name="codeforge-$version-$t"
  tar xzf "$out/$name.tar.gz" -C "$check"
  got=$(readelf -h "$check/$name/codeforge" | sed -n 's/.*Machine: *//p')
  want=$(machine_of "$t")
  case "$got" in *"$want"*) ;; *) echo "release-local: $t binary is '$got', expected $want" >&2; exit 1 ;; esac
  glibc=$(readelf --dyn-syms -W "$check/$name/codeforge" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
  echo "$(cut -d' ' -f1 "$out/$name.tar.gz.sha256")  $name.tar.gz  (glibc >= ${glibc:-none})" >>"$report"
done

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
mkdir -p "$check/srv"
ln -s "$(cd "$out" && pwd)" "$check/srv/v$version"
python3 -m http.server --bind 127.0.0.1 --directory "$check/srv" "$port" >/dev/null 2>&1 &
server_pid=$!
for _ in $(seq 1 50); do
  curl -fs "http://127.0.0.1:$port/" >/dev/null && break
  python3 -c 'import time; time.sleep(0.1)'
done
CODEFORGE_VERSION="v$version" CODEFORGE_DOWNLOAD_BASE="http://127.0.0.1:$port" \
  CODEFORGE_INSTALL_DIR="$check/bin" CODEFORGE_FORCE=1 sh install.sh >&2
installed=$("$check/bin/codeforge" --version)
[ "$installed" = "codeforge $version" ] || { echo "release-local: install.sh installed '$installed'" >&2; exit 1; }
{
  echo
  echo "install.sh verified on $host: $installed"
  echo "not built here: ${missing[*]:-none}"
} >>"$report"
cat "$report" >&2

if [ "$publish" = 1 ]; then
  notes="$check/notes.md"
  awk -v v="$version" '$0 ~ "^## \\[" v "\\]" {on=1; next} on && /^## \[/ {exit} on {print}' CHANGELOG.md >"$notes"
  [ -s "$notes" ] || { echo "release-local: CHANGELOG.md has no [$version] section" >&2; exit 1; }
  gh release create "v$version" --draft --verify-tag --title "v$version" --notes-file "$notes" \
    "$out"/*.tar.gz "$out"/*.tar.gz.sha256
  echo "release-local: draft v$version created; publishing it is the operator's step" >&2
fi
