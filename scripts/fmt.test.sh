#!/usr/bin/env bash
# Tests for scripts/fmt.sh — the pinned formatter wrapper.
#
# Never touches the real rustup/cargo: fake ones live in $t/bin (first on PATH)
# and append their argv to a log file. Each control below asserts one behavior;
# a control that passes when it should not aborts loudly via fail().
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
FMT="$ROOT/scripts/fmt.sh"

t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT

mkdir -p "$t/bin"
LOG="$t/log"
export LOG

# Read the pin from fmt.sh itself — never hard-code the toolchain version.
PIN=$(grep -E '^PIN="[^"]+"' "$FMT" | sed -E 's/^PIN="([^"]+)".*/\1/')
if [ -z "$PIN" ]; then
  echo "fmt.test.sh: could not read PIN from $FMT" >&2
  exit 1
fi

# Fake rustup: logs argv; `rustup run` exits $RUSTUP_RUN_EXIT (default 0);
# `rustup toolchain install` prints a marker to stdout (fmt.sh must send it to stderr).
cat > "$t/bin/rustup" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "rustup $*" >> "$LOG"
case "$1" in
  run)
    exit "${RUSTUP_RUN_EXIT:-0}"
    ;;
  toolchain)
    echo "fake-rustup-install-output"
    exit 0
    ;;
  *)
    exit 1
    ;;
esac
EOF

# Fake cargo: logs argv; exits $CARGO_CHECK_EXIT when --check is present,
# otherwise $CARGO_EXIT (default 0).
cat > "$t/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "cargo $*" >> "$LOG"
case " $* " in
  *" --check "*)
    exit "${CARGO_CHECK_EXIT:-0}"
    ;;
esac
exit "${CARGO_EXIT:-0}"
EOF
chmod +x "$t/bin/rustup" "$t/bin/cargo"

controls=0

mkdir -p "$t/out"

fail() {
  echo "fmt.test.sh: FAIL — $1" >&2
  exit 1
}

# Run fmt.sh with the fake bin dir on PATH plus a bare system PATH that must
# not provide real rustup/cargo. Env overrides are passed via the environment.
# stdout/stderr land in $t/out/$name.{out,err}; exit code in $t/out/$name.rc.
run_fmt() {
  local name=$1
  shift
  mkdir -p "$t/out"
  PATH="$t/bin:/usr/bin:/bin" \
    "$FMT" "$@" >"$t/out/$name.out" 2>"$t/out/$name.err"
  echo $? > "$t/out/$name.rc"
}

reset_log() {
  : > "$LOG"
}

log_line() {
  sed -n "${1}p" "$LOG"
}

# --- Control 1: no rustup on PATH → exit 1, stderr mentions rustup, cargo never called.
BARE_PATH="/usr/bin:/bin"
if command -v rustup >/dev/null 2>&1 || command -v cargo >/dev/null 2>&1; then
  fail "control 1 premise broken: bare PATH $BARE_PATH resolves a real rustup or cargo"
fi
reset_log
set +e
PATH="$BARE_PATH" "$FMT" >"$t/out/c1.out" 2>"$t/out/c1.err"
rc=$?
set -e
[ "$rc" -eq 1 ] || fail "control 1: fmt.sh exited $rc, want 1 (no rustup on PATH)"
grep -q rustup "$t/out/c1.err" || fail "control 1: stderr does not mention rustup: $(cat "$t/out/c1.err")"
[ ! -s "$LOG" ] || fail "control 1: cargo/rustup was called despite missing rustup: $(cat "$LOG")"
controls=$((controls + 1))

# --- Control 2: pinned toolchain present → no install, cargo called exactly `+<PIN> fmt --all`.
reset_log
run_fmt c2
[ "$(cat "$t/out/c2.rc")" -eq 0 ] || fail "control 2: fmt.sh exited $(cat "$t/out/c2.rc"), want 0"
! grep -q 'rustup toolchain install' "$LOG" \
  || fail "control 2: toolchain install ran although pinned toolchain is present: $(cat "$LOG")"
[ "$(log_line 1)" = "rustup run $PIN rustfmt --version" ] \
  || fail "control 2: rustup probe is '$(log_line 1)', want 'rustup run $PIN rustfmt --version'"
[ "$(log_line 2)" = "cargo +$PIN fmt --all" ] \
  || fail "control 2: cargo argv is '$(log_line 2)', want 'cargo +$PIN fmt --all'"
controls=$((controls + 1))

# --- Control 3: --check is passed through → cargo argv ends with --check.
reset_log
run_fmt c3 --check
[ "$(cat "$t/out/c3.rc")" -eq 0 ] || fail "control 3: fmt.sh exited $(cat "$t/out/c3.rc"), want 0"
[ "$(log_line 2)" = "cargo +$PIN fmt --all --check" ] \
  || fail "control 3: cargo argv is '$(log_line 2)', want it to end with --check"
controls=$((controls + 1))

# --- Control 4: cargo's exit code is returned (fake cargo fails under --check → fmt.sh exits 1).
reset_log
set +e
CARGO_CHECK_EXIT=1 PATH="$t/bin:/usr/bin:/bin" "$FMT" --check \
  >"$t/out/c4.out" 2>"$t/out/c4.err"
rc=$?
set -e
[ "$rc" -eq 1 ] || fail "control 4: fmt.sh exited $rc, want 1 (fake cargo failed under --check)"
controls=$((controls + 1))

# --- Control 5: toolchain missing → `rustup toolchain install <PIN> --component rustfmt --no-self-update` runs before cargo.
reset_log
set +e
RUSTUP_RUN_EXIT=1 PATH="$t/bin:/usr/bin:/bin" "$FMT" \
  >"$t/out/c5.out" 2>"$t/out/c5.err"
rc=$?
set -e
[ "$rc" -eq 0 ] || fail "control 5: fmt.sh exited $rc, want 0 after install"
[ "$(log_line 2)" = "rustup toolchain install $PIN --component rustfmt --no-self-update" ] \
  || fail "control 5: second logged call is '$(log_line 2)', want 'rustup toolchain install $PIN --component rustfmt --no-self-update'"
[ "$(log_line 3)" = "cargo +$PIN fmt --all" ] \
  || fail "control 5: cargo not called after install (line 3: '$(log_line 3)')"
controls=$((controls + 1))

# --- Control 6: install output goes to stderr, stdout stays empty.
[ ! -s "$t/out/c5.out" ] || fail "control 6: stdout not empty when installing: $(cat "$t/out/c5.out")"
grep -q 'fmt.sh: installing pinned toolchain' "$t/out/c5.err" \
  || fail "control 6: install progress message missing from stderr"
grep -q 'fake-rustup-install-output' "$t/out/c5.err" \
  || fail "control 6: rustup install output did not reach stderr"
controls=$((controls + 1))

echo "fmt.test.sh: ok ($controls controls)"
