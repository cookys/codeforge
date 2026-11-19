#!/usr/bin/env bash
# Red/green test for scripts/push-ref-guard.sh through real `git push`es. The remote is configured
# with a GitHub URL (so it counts as public) and rewritten by insteadOf to a local bare repo.
set -euo pipefail
# shellcheck disable=SC2046  # under a pre-push hook git exports GIT_DIR for the real repo
unset $(git rev-parse --local-env-vars)
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

git init -q --bare "$T/pub.git"
git init -q --bare "$T/priv.git"
git init -q -b develop "$T/w"
cd "$T/w"
g() { git -c user.name=t -c user.email=t@example.com "$@"; }
g commit -q --allow-empty -m init
git branch main
git branch feat/x
g tag -a v9 -m v9
git remote add origin git@github-codeforge:example/codeforge.git
git config "url.$T/pub.git.insteadOf" git@github-codeforge:example/codeforge.git
git remote add mirror "$T/priv.git"
mkdir hooks
printf '#!/usr/bin/env bash\nexec "%s/push-ref-guard.sh" "$@"\n' "$here" >hooks/pre-push
chmod +x hooks/pre-push
git config core.hooksPath "$T/w/hooks"

has() { git --git-dir="$1" rev-parse -q --verify "$2" >/dev/null; }
audit="$T/w/.git/codeforge-push-audit.log"
lines() { [ -f "$audit" ] && wc -l <"$audit" || echo 0; }

# Green: develop and main go to the public remote.
git push -q origin develop main 2>/dev/null || fail "develop/main refused"
has "$T/pub.git" refs/heads/develop || fail "develop not pushed"
has "$T/pub.git" refs/heads/main || fail "main not pushed"

# Red: a feature branch, a tag, and a branch pushed under another name are refused.
! git push -q origin feat/x 2>"$T/err" || fail "feature branch to public remote was allowed"
grep -q 'refused refs/heads/feat/x' "$T/err" || fail "no refusal message"
has "$T/pub.git" refs/heads/feat/x && fail "feature branch reached the public remote"
! git push -q origin v9 2>/dev/null || fail "tag to public remote was allowed"
! git push -q origin develop:refs/heads/scratch 2>/dev/null || fail "develop:scratch was allowed"
# Mixed push: one bad ref refuses the whole push.
! git push -q origin develop feat/x 2>/dev/null || fail "mixed push with a feature branch was allowed"

# Red: a malformed override is ignored and writes no audit line.
! CODEFORGE_PUBLIC_PUSH_OVERRIDE="because" git push -q origin feat/x 2>/dev/null \
  || fail "override without an id was accepted"
[ "$(lines)" = 0 ] || fail "malformed override wrote an audit line"

# Green: a well-formed override pushes and writes exactly one audit line.
CODEFORGE_PUBLIC_PUSH_OVERRIDE="01M7RZVD4PHZTS7Y2X5V456DS4: test release branch" \
  git push -q origin feat/x 2>/dev/null || fail "well-formed override refused"
has "$T/pub.git" refs/heads/feat/x || fail "override push did not land"
[ "$(lines)" = 1 ] || fail "override did not write one audit line"
grep -q $'ref=refs/heads/feat/x\t.*override=01M7RZVD4PHZTS7Y2X5V456DS4: test release branch' "$audit" \
  || fail "audit line lacks ref or override"

# Green: deleting the wrong ref from the public remote is allowed (the remedy).
git push -q origin --delete feat/x 2>/dev/null || fail "delete refused"
has "$T/pub.git" refs/heads/feat/x && fail "delete did not land"

# Green: a non-public remote is not checked.
git push -q mirror feat/x v9 2>/dev/null || fail "push to a non-public remote refused"

# Negative control: without the guard the same feature push goes through (the test is not blind).
git config core.hooksPath "$T/w/nohooks"
git push -q origin feat/x 2>/dev/null || fail "control: plain push failed"
has "$T/pub.git" refs/heads/feat/x || fail "control: feature branch did not land without the guard"

# The shipped pre-push hook runs the guard before check-all.
grep -q 'scripts/push-ref-guard.sh" "\$@" || exit 1' "$here/../.githooks/pre-push" \
  || fail ".githooks/pre-push does not run the guard"
echo "push-ref-guard.test.sh: ok"
