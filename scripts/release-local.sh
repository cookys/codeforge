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
# 3. Writes RELEASE.txt: files, sha256, glibc needs, and the targets it could not build.
#
# Everything is built and verified in a staging dir of this run only; the verified files are then
# copied to <out> (default: dist). <out> is replaced, so it must be absent, empty, or hold a
# previous RELEASE.txt.
#
# It never publishes. --publish creates a DRAFT GitHub release v<version> from the staged,
# verified files (re-checked against their .sha256 right before upload) and needs a clean tree,
# HEAD tagged v<version>, and origin's tag v<version> pointing at HEAD too. Turning the draft into
# a release is the operator's step. --publish refuses while a target is missing unless
# --allow-missing is given. macOS targets (x86_64/aarch64-apple-darwin) cannot be built here.
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
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "release-local: unknown argument: $1" >&2; exit 2 ;;
  esac
done
fail() { echo "release-local: $*" >&2; exit 1; }

version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
msrv=$(grep -m1 '^rust-version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
max_glibc="2.35"
linux=(x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu)
missing=(x86_64-apple-darwin aarch64-apple-darwin)
host="$(rustc -vV | sed -n 's/^host: //p')"

if [ "$publish" = 1 ]; then
  [ -z "$(git status --porcelain)" ] || fail "--publish needs a clean tree"
  head=$(git rev-parse HEAD)
  [ "$(git rev-parse -q --verify "refs/tags/v$version^{commit}")" = "$head" ] \
    || fail "--publish needs HEAD tagged v$version"
  # gh --verify-tag checks only that origin has a tag of that name: check where it points.
  remote=$(git ls-remote --tags origin "refs/tags/v$version" "refs/tags/v$version^{}")
  remote_commit=$(printf '%s\n' "$remote" | awk -v r="refs/tags/v$version^{}" '$2 == r {print $1}')
  [ -n "$remote_commit" ] || remote_commit=$(printf '%s\n' "$remote" | awk -v r="refs/tags/v$version" '$2 == r {print $1}')
  [ "$remote_commit" = "$head" ] \
    || fail "origin's tag v$version points at '${remote_commit:-nothing}', not HEAD $head"
  if [ "${#missing[@]}" -gt 0 ] && [ "$allow_missing" = 0 ]; then
    fail "not buildable here: ${missing[*]} (pass --allow-missing to publish without them)"
  fi
fi

# <out> is replaced at the end: never a directory with other content (`--out .` is the checkout)
if [ -e "$out" ] || [ -L "$out" ]; then
  [ -d "$out" ] && [ ! -L "$out" ] || fail "--out $out is not a directory"
  if [ -n "$(find "$out" -mindepth 1 -maxdepth 1 -print -quit)" ] && [ ! -f "$out/RELEASE.txt" ]; then
    fail "--out $out is not empty and holds no RELEASE.txt; refusing to replace it"
  fi
fi

work="$(mktemp -d)"
stage="$work/dist"
check="$work/check"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then kill "$server_pid" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$stage" "$check"

for t in "${linux[@]}"; do
  echo "── $t (cross, $msrv) ──" >&2
  rustup target add --toolchain "$msrv" "$t" >/dev/null
  ./scripts/package.sh "$t" --cross --toolchain "$msrv" --max-glibc "$max_glibc" --out "$stage" --version "$version" >/dev/null
done

# the exact files this run verifies, copies and uploads
files=()
for t in "${linux[@]}"; do
  files+=("codeforge-$version-$t.tar.gz" "codeforge-$version-$t.tar.gz.sha256")
done
sums=()
for f in "${files[@]}"; do case "$f" in *.sha256) sums+=("$f") ;; esac; done

# verify: every archive matches its .sha256; the ELF machine is the target's; install.sh installs this host's
(cd "$stage" && sha256sum -c "${sums[@]}" >&2)
machine_of() { case "$1" in x86_64-*) echo "X86-64" ;; aarch64-*) echo "AArch64" ;; esac; }
report="$stage/RELEASE.txt"
{
  echo "codeforge $version — local release, built $(date -u +%Y-%m-%dT%H:%M:%SZ) from $(git rev-parse --short HEAD)"
  echo "toolchain $msrv; linux via cross; glibc floor $max_glibc"
  echo
} >"$report"
for t in "${linux[@]}"; do
  name="codeforge-$version-$t"
  tar xzf "$stage/$name.tar.gz" -C "$check"
  got=$(readelf -h "$check/$name/codeforge" | sed -n 's/.*Machine: *//p')
  want=$(machine_of "$t")
  case "$got" in *"$want"*) ;; *) fail "$t binary is '$got', expected $want" ;; esac
  glibc=$(readelf --dyn-syms -W "$check/$name/codeforge" | { grep -o 'GLIBC_[0-9][0-9.]*' || true; } | sed 's/GLIBC_//' | sort -uV | tail -1)
  echo "$(cut -d' ' -f1 "$stage/$name.tar.gz.sha256")  $name.tar.gz  (glibc >= ${glibc:-none})" >>"$report"
done

port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
mkdir -p "$check/srv"
ln -s "$stage" "$check/srv/v$version"
python3 -m http.server --bind 127.0.0.1 --directory "$check/srv" "$port" >/dev/null 2>&1 &
server_pid=$!
ready=0
for _ in $(seq 1 50); do
  if curl -fs "http://127.0.0.1:$port/" >/dev/null; then ready=1; break; fi
  python3 -c 'import time; time.sleep(0.1)'
done
[ "$ready" = 1 ] || fail "the local download server on 127.0.0.1:$port never answered"
CODEFORGE_VERSION="v$version" CODEFORGE_DOWNLOAD_BASE="http://127.0.0.1:$port" \
  CODEFORGE_INSTALL_DIR="$check/bin" CODEFORGE_FORCE=1 sh install.sh >&2
installed=$("$check/bin/codeforge" --version)
[ "$installed" = "codeforge $version" ] || fail "install.sh installed '$installed'"
kill "$server_pid" 2>/dev/null || true
server_pid=""
{
  echo
  echo "install.sh verified on $host: $installed"
  echo "not built here: ${missing[*]:-none}"
} >>"$report"
cat "$report" >&2

rm -rf "$out"
mkdir -p "$out"
for f in "${files[@]}" RELEASE.txt; do cp "$stage/$f" "$out/$f"; done

if [ "$publish" = 1 ]; then
  notes="$check/notes.md"
  awk -v v="$version" '$0 ~ "^## \\[" v "\\]" {on=1; next} on && /^## \[/ {exit} on {print}' CHANGELOG.md >"$notes"
  [ -s "$notes" ] || fail "CHANGELOG.md has no [$version] section"
  (cd "$stage" && sha256sum -c "${sums[@]}" >&2)
  upload=()
  for f in "${files[@]}"; do upload+=("$stage/$f"); done
  gh release create "v$version" --draft --verify-tag --title "v$version" --notes-file "$notes" "${upload[@]}"
  echo "release-local: draft v$version created; publishing it is the operator's step" >&2
fi
