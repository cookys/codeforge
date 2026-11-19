#!/usr/bin/env bash
# Pre-push guard (PM 01M7RZVD): origin is a PUBLIC repo; only develop and main may be pushed to it.
# Any other ref (feature branch, tag, HEAD:other) is refused, unless the push carries an explicit
# override, which is then written as one audit line. Deleting a remote ref is always allowed (it is
# the remedy for a wrong push).
#
# Called by .githooks/pre-push as: push-ref-guard.sh <remote-name> <remote-url>, with git's
# pre-push lines on stdin: "<local ref> <local sha> <remote ref> <remote sha>".
#
# Public remote: its configured URL (remote.<name>.url, before any insteadOf rewrite) or the URL git
# hands the hook names GitHub (github.com, or an ssh host alias github-*). Others (a local bare
# mirror, a NAS path) are not checked.
#
# Override, one push only:
#   CODEFORGE_PUBLIC_PUSH_OVERRIDE="<PM message id or card id>: <reason>" git push origin <ref>
# Audit: one line per overridden ref in <git common dir>/codeforge-push-audit.log.
# Test: scripts/push-ref-guard.test.sh.
set -euo pipefail

remote=${1:?remote name}
url=${2:-$remote}
zero=0000000000000000000000000000000000000000

is_github() {
  case "$1" in
    *github.com[:/]* | *@github-*:* | github-*:* | ssh://*github*) return 0 ;;
    *) return 1 ;;
  esac
}
configured=$(git config --get "remote.$remote.url" || true)
is_github "$url" || is_github "$configured" || exit 0

override=${CODEFORGE_PUBLIC_PUSH_OVERRIDE:-}
# The override must name who allowed it: a card id or a 26-char fleet message id, then a reason.
if [ -n "$override" ] && ! printf '%s' "$override" | grep -Eq '^(d-[0-9a-f]{8}|[0-9A-HJKMNP-TV-Z]{26}): .*[^ ]'; then
  echo "push-ref-guard: CODEFORGE_PUBLIC_PUSH_OVERRIDE must be \"<d-xxxxxxxx | 26-char message id>: <reason>\"; ignored." >&2
  override=
fi
audit="$(git rev-parse --path-format=absolute --git-common-dir)/codeforge-push-audit.log"
refused=0

while read -r lref lsha rref _rsha; do
  [ -n "${rref:-}" ] || continue
  # Deleting a remote ref: allowed.
  [ "$lsha" = "$zero" ] && continue
  case "$rref" in
    refs/heads/develop | refs/heads/main) continue ;;
  esac
  if [ -n "$override" ]; then
    printf '%s\tremote=%s\tref=%s\tsha=%s\tlocal=%s\tby=%s\toverride=%s\n' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$remote" "$rref" "$lsha" "$lref" \
      "$(git config user.name || echo unknown)" "$override" >>"$audit"
    echo "push-ref-guard: OVERRIDE $rref -> $remote (audit: $audit)" >&2
    continue
  fi
  echo "push-ref-guard: refused $rref -> $remote: $remote is public; only develop and main may be pushed." >&2
  refused=1
done

if [ "$refused" = 1 ]; then
  echo "push-ref-guard: to push anyway, set CODEFORGE_PUBLIC_PUSH_OVERRIDE=\"<PM msg/card id>: <reason>\"." >&2
  exit 1
fi
exit 0
