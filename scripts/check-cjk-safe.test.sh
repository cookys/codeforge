#!/usr/bin/env bash
# Behavioral test for scripts/check-cjk-safe.sh.
#
# The gate resolves its repo root from its own location ($0/..), so each control
# copies the gate into a fresh mktemp tree ($t/scripts/) and writes fixtures
# under that tree's src/. Eight controls, each in a fresh src dir.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GATE="$REPO_ROOT/scripts/check-cjk-safe.sh"

t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT

passed=0

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# write_src <case> <relpath> — writes stdin to $t/<case>/src/<relpath>.
write_src() {
  local case="$1" relpath="$2"
  mkdir -p "$t/$case/src/$(dirname "$relpath")"
  cat > "$t/$case/src/$relpath"
}

# run_gate <case> — copies the gate into $t/<case>/scripts/ and runs it there.
# Sets GATE_STATUS (exit code) and GATE_OUTPUT (combined stdout+stderr).
run_gate() {
  local case="$1"
  mkdir -p "$t/$case/scripts"
  cp "$GATE" "$t/$case/scripts/check-cjk-safe.sh"
  if GATE_OUTPUT="$(cd "$t/$case" && bash scripts/check-cjk-safe.sh 2>&1)"; then
    GATE_STATUS=0
  else
    GATE_STATUS=$?
  fi
}

expect_status() {
  local expected="$1" what="$2"
  if [ "$GATE_STATUS" -ne "$expected" ]; then
    fail "$what: gate exited $GATE_STATUS, expected $expected — output: $GATE_OUTPUT"
  fi
  passed=$((passed + 1))
}

# Control 1: plain start-anchored byte slice — must fail and name file:line.
write_src c1 main.rs <<'EOF'
let a = &s[..3];
EOF
run_gate c1
if [ "$GATE_STATUS" -ne 1 ]; then
  fail "control 1 (&s[..3]) passed with exit $GATE_STATUS, expected 1"
fi
grep -q "src/main.rs:1" <<<"$GATE_OUTPUT" \
  || fail "control 1 output did not name src/main.rs:1: $GATE_OUTPUT"
passed=$((passed + 1))

# Control 2: inclusive form [..=N] — must fail.
write_src c2 main.rs <<'EOF'
let a = &s[..=3];
EOF
run_gate c2
expect_status 1 "control 2 (&s[..=3]) passed but should fail"

# Control 3: cjk-ok: escape hatch silences the line.
write_src c3 main.rs <<'EOF'
let a = &s[..3]; // cjk-ok: ascii only
EOF
run_gate c3
expect_status 0 "control 3 (// cjk-ok: allow) failed but should pass"

# Control 4: "cjk-okay" without the colon does NOT disable the check.
write_src c4 main.rs <<'EOF'
let a = &s[..3]; // cjk-okay this one is fine
EOF
run_gate c4
expect_status 1 'control 4 (cjk-okay without colon) passed but should fail'

# Control 5: pure-comment line mentioning a slice — skipped.
write_src c5 main.rs <<'EOF'
fn f() {
    // &s[..3] would panic on CJK boundaries
}
EOF
run_gate c5
expect_status 0 "control 5 (pure comment line) failed but should pass"

# Control 6: safe idioms — open-end, variable bound, chars().take().
write_src c6 main.rs <<'EOF'
fn f() {
    let x = &v[1..];
    let y = &buf[..len];
    let z: String = s.chars().take(3).collect::<String>();
}
EOF
run_gate c6
expect_status 0 "control 6 (safe Vec/variable/chars idioms) failed but should pass"

# Control 7: violation in a nested src dir — must fail and name the nested path.
write_src c7 a/b/c.rs <<'EOF'
let a = &s[..3];
EOF
run_gate c7
if [ "$GATE_STATUS" -ne 1 ]; then
  fail "control 7 (nested src/a/b/c.rs) passed with exit $GATE_STATUS, expected 1"
fi
grep -q "src/a/b/c.rs:1" <<<"$GATE_OUTPUT" \
  || fail "control 7 output did not name src/a/b/c.rs:1: $GATE_OUTPUT"
passed=$((passed + 1))

# Control 8: no src dir — exit 0 with SKIP (run_gate alone creates no src/).
run_gate c8
if [ "$GATE_STATUS" -ne 0 ]; then
  fail "control 8 (no src dir) failed with exit $GATE_STATUS, expected 0"
fi
grep -q "SKIP" <<<"$GATE_OUTPUT" \
  || fail "control 8 output did not contain SKIP: $GATE_OUTPUT"
passed=$((passed + 1))

echo "ok - $passed controls passed"
