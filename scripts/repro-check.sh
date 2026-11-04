#!/usr/bin/env bash
# Prove a release tarball is reproducible (B43): build HEAD twice from clean checkouts at two
# different paths, each with its own fresh target dir, and require byte-identical archives.
#
#   ./scripts/repro-check.sh <target> [package.sh options, e.g. --cross --toolchain 1.85]
#
# The checkouts are detached worktrees of HEAD (committed state only; uncommitted edits are not
# built) at paths of different lengths, removed afterwards. Two full release builds: minutes, not
# for check-all. Exit 0 = identical (prints the sha256); exit 1 = they differ (prints both, and
# which packed files differ).
set -euo pipefail
cd "$(dirname "$0")/.."
[ $# -ge 1 ] || { sed -n '2,10p' "$0"; exit 2; }
target="$1"
shift

head="$(git rev-parse HEAD)"
base="$(mktemp -d)"
a="$base/a"
b="$base/second-checkout/deeper"
# shellcheck disable=SC2329  # invoked by the EXIT trap
cleanup() {
  git worktree remove --force "$a" 2>/dev/null || true
  git worktree remove --force "$b" 2>/dev/null || true
  rm -rf "$base"
}
trap cleanup EXIT
mkdir -p "$(dirname "$b")"
git worktree add -q --detach "$a" "$head"
git worktree add -q --detach "$b" "$head"

# Each build runs package.sh inside its own checkout with CARGO_TARGET_DIR unset, so each gets a
# fresh target dir under its own path (nothing cached is shared between the two builds).
echo "repro-check: $target at $(git rev-parse --short HEAD), build 1 of 2 ($a)" >&2
arch_a="$(cd "$a" && env -u CARGO_TARGET_DIR ./scripts/package.sh "$target" --out "$base/out-a" "$@" | tail -1)"
echo "repro-check: build 2 of 2 ($b)" >&2
arch_b="$(cd "$b" && env -u CARGO_TARGET_DIR ./scripts/package.sh "$target" --out "$base/out-b" "$@" | tail -1)"
arch_a="$base/out-a/$(basename "$arch_a")"
arch_b="$base/out-b/$(basename "$arch_b")"

sum_a="$(sha256sum "$arch_a" | cut -d' ' -f1)"
sum_b="$(sha256sum "$arch_b" | cut -d' ' -f1)"
if [ "$sum_a" = "$sum_b" ]; then
  echo "repro-check: identical $(basename "$arch_a") sha256 $sum_a"
  exit 0
fi
echo "repro-check: DIFFERENT" >&2
echo "  $sum_a  (path $a)" >&2
echo "  $sum_b  (path $b)" >&2
mkdir -p "$base/xa" "$base/xb"
tar xzf "$arch_a" -C "$base/xa"
tar xzf "$arch_b" -C "$base/xb"
diff -rq "$base/xa" "$base/xb" >&2 || true
exit 1
