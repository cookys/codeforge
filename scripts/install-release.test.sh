#!/usr/bin/env bash
# install-release.sh without a build (B64): INSTALL_RELEASE_PACKAGE points at a fake package.sh
# that produces an archive holding a fake `codeforge` binary (same layout and last-line contract
# as the real one), so only the install mechanics are checked:
#   (a) the binary lands in --dest with mode 0755 and no temp files left behind, and the
#       printed sha16 equals the installed file's sha256 first 16 hex;
#   (b) when the package step fails, the previous dest file is byte-identical afterwards, no
#       temp files are left, and the exit code is non-zero.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

cat >"$T/pkg-ok.sh" <<'EOF'
#!/usr/bin/env bash
# fake package.sh: <target> --out <dir> -> <dir>/codeforge-9.9.9-test.tar.gz, path on last line
set -euo pipefail
out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[ -n "$out" ] || { echo "fake package: no --out" >&2; exit 2; }
stage="$(mktemp -d)"
mkdir -p "$stage/codeforge-9.9.9-test"
printf 'fake codeforge binary\n' >"$stage/codeforge-9.9.9-test/codeforge"
tar --format=gnu --sort=name --owner=0 --group=0 --numeric-owner --mtime=@1000000000 \
  -cf - -C "$stage" codeforge-9.9.9-test | gzip -n >"$out/codeforge-9.9.9-test.tar.gz"
rm -rf "$stage"
echo "$out/codeforge-9.9.9-test.tar.gz"
EOF
cat >"$T/pkg-fail.sh" <<'EOF'
#!/usr/bin/env bash
echo "fake package: simulated build failure" >&2
exit 1
EOF
chmod +x "$T/pkg-ok.sh" "$T/pkg-fail.sh"

dest="$T/dest"
mkdir -p "$dest"

# (a) happy path: 0755 in --dest, printed sha16 is the real sha256 prefix, no leftovers
out="$(INSTALL_RELEASE_PACKAGE="$T/pkg-ok.sh" "$here/install-release.sh" --dest "$dest" --target test-target)"
[ -f "$dest/codeforge" ] || fail "installed binary missing in $dest"
[ "$(stat -c %a "$dest/codeforge")" = "755" ] || fail "mode is not 0755: $(stat -c %a "$dest/codeforge")"
[ "$(ls "$dest")" = "codeforge" ] || fail "temp files left in dest: $(ls "$dest")"
sum16="$(sha256sum "$dest/codeforge" | cut -c1-16)"
commit="$(git -C "$here/.." rev-parse --short HEAD)"
[ "$out" = "install-release: $dest/codeforge sha16 $sum16 commit $commit" ] \
  || fail "last line: got '$out', want 'install-release: $dest/codeforge sha16 $sum16 commit $commit'"

# (b) package failure: previous dest file untouched, non-zero exit, no leftovers
printf 'original\n' >"$dest/codeforge"
orig="$(sha256sum "$dest/codeforge" | cut -d' ' -f1)"
rc=0
INSTALL_RELEASE_PACKAGE="$T/pkg-fail.sh" "$here/install-release.sh" --dest "$dest" --target test-target \
  >/dev/null 2>&1 || rc=$?
[ "$rc" -ne 0 ] || fail "package failure did not exit non-zero"
[ "$(sha256sum "$dest/codeforge" | cut -d' ' -f1)" = "$orig" ] || fail "dest file changed on failure"
[ "$(ls "$dest")" = "codeforge" ] || fail "temp files left in dest after failure: $(ls "$dest")"

echo "install-release.test.sh: ok"
