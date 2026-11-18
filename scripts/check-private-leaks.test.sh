#!/usr/bin/env bash
# Tests check-private-leaks.sh in throwaway git repos with a synthetic denylist (the real one is
# never read). Planted token/email -> exit 1 and the value is not printed; clean repo -> exit 0;
# the allowance covers exactly its count.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
tok=zzqvhostx
printf '# synthetic\n%s\n' "$(printf %s "$tok" | sha256sum | cut -d' ' -f1)" > "$T/deny"
: > "$T/allow"
run() { CODEFORGE_LEAKS_ROOT="$T/repo" CODEFORGE_LEAKS_DENYLIST="$T/deny" CODEFORGE_LEAKS_ALLOW="$T/allow" "$here/check-private-leaks.sh"; }
git init -q "$T/repo"
printf 'hello world\nmail me@example.com or noreply@anthropic.com\nssh git@github.com:o/r\n' > "$T/repo/clean.txt"
git -C "$T/repo" add -A
run >/dev/null || fail "clean repo must pass"

printf 'line one\nfleet ns/%s-team/member here\n' "$tok" > "$T/repo/doc.md"
printf 'contact someone@%s.io\n' corp-example-co > "$T/repo/mail.md"
git -C "$T/repo" add -A
rc=0; out=$(run 2>&1) || rc=$?
[ "$rc" = 1 ] || fail "planted token/email must exit 1 (rc=$rc)"
grep -qx 'doc.md:2: denylisted token' <<<"$out" || fail "token hit not reported as path:line: $out"
grep -qx 'mail.md:1: non-allowlisted email' <<<"$out" || fail "email hit not reported: $out"
! grep -qE "$tok|corp-example-co" <<<"$out" || fail "guard printed a private value"

printf 'doc.md 1\nmail.md 1\n' > "$T/allow"
run >/dev/null || fail "allowance at exact count must pass"
printf 'doc.md 0\nmail.md 1\n' > "$T/allow"
if run >/dev/null 2>&1; then fail "exceeded allowance must fail"; fi
echo "check-private-leaks.test.sh: ok"
