#!/usr/bin/env bash
# Install .githooks/post-checkout as the repository's post-checkout hook, in the common hooks
# directory shared by every worktree. Run it in the main clone; run it again after the hook changes.
# A copy, not a symlink: a symlink into the working tree would dangle the moment the main clone
# checks out a commit without the file, which is exactly the checkout the hook is there to report.
# Refuses to replace a post-checkout hook that is not an earlier copy of this guard.
set -euo pipefail

top=$(git rev-parse --show-toplevel)
hooks="$(git rev-parse --git-common-dir)/hooks"
src="$top/.githooks/post-checkout"
dst="$hooks/post-checkout"
mark='post-checkout guard for the main clone'

[ -f "$src" ] || { echo "install-checkout-guard: $src not found" >&2; exit 1; }
mkdir -p "$hooks"
if [ -e "$dst" ] || [ -L "$dst" ]; then
  if cmp -s "$src" "$dst" && [ -x "$dst" ]; then
    echo "install-checkout-guard: already installed ($dst)"
    exit 0
  fi
  if ! grep -q "$mark" "$dst" 2>/dev/null; then
    echo "install-checkout-guard: $dst already exists and is not this guard; not replacing it" >&2
    exit 1
  fi
fi
cp "$src" "$dst.tmp.$$"
chmod 755 "$dst.tmp.$$"
mv -f "$dst.tmp.$$" "$dst"
echo "install-checkout-guard: installed $dst"
