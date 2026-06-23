#!/usr/bin/env bash
# Deterministic CJK-safe string-slicing gate for CodeForge.
#
# Enforces the project HARD RULE (CLAUDE.md + MEMORY.md): NEVER byte-index a
# string with `&s[..N]` — it panics on a non-UTF-8-boundary (every multi-byte
# CJK character). Always use `.chars().take(N).collect::<String>()` instead.
# This bug hit 4 files in Phase 1; this guard turns the rule into a check that
# can never silently regress (sibling of scripts/check-doc-drift.py).
#
# What it flags: START-anchored integer-literal byte slices in src/**/*.rs —
#   [..N]   [..=N]
# This is the exact documented HARD-RULE form (`&s[..N]`, take-first-N): a
# hardcoded take-first offset on a slice almost always means "take N chars"
# intent, which is precisely the CJK-panic form, and is rare on Vec/&[u8].
#
# Precision over recall — a CI gate that fires on safe code gets disabled.
# Deliberately NOT flagged (would be dominated by safe Vec idioms / need type
# info to disambiguate, so net noise — keep using .chars() for strings anyway):
#   - open-end / range literals: `[N..]` `[N..M]`  (e.g. Vec drop-first `v[1..]`)
#   - variable-bound ranges:     `[..n]` `[n..]`   (e.g. `&buf[..len]`)
#
# Escape hatch: append `// cjk-ok: <reason>` (the colon is required — an
# unanchored "cjk-ok" would let words like "cjk-okay" disable the check) on the
# offending line for a genuinely-safe case (ASCII-guaranteed string, or a
# Vec/byte slice). Mirrors clippy's #[allow]. Pure-comment lines (the trimmed
# line starts with `//`) are skipped automatically; an INLINE comment that
# merely mentions a slice will false-positive — silence it with `// cjk-ok:`.
#
# Exit 0 = clean. Exit 1 = at least one violation (printed with file:line).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src"

echo "cjk-safe string-slice gate"
echo "================================"

if [ ! -d "$SRC" ]; then
  echo "[SKIP] no src/ directory at $SRC"
  exit 0
fi

# awk does the per-line work: skip explicit allows (`cjk-ok:`) + pure-comment
# lines, then match the start-anchored integer-literal byte-slice pattern on the
# RAW line. We deliberately do NOT strip inline comments before matching: a
# naive first-`//` cut would drop a real `&s[..N]` that follows a `//` inside a
# string literal (e.g. a URL), creating a false negative on the exact class we
# guard. The cost is that an inline comment merely *mentioning* a slice
# false-positives — silence those with `// cjk-ok:`.
violations="$(
  find "$SRC" -name '*.rs' -print0 \
    | xargs -0 -r awk '
      {
        line = $0

        # explicit per-line allow marker (colon-anchored; see header)
        if (index(line, "cjk-ok:") > 0) next

        # skip pure-comment lines (e.g. doc-comments that mention &s[..3])
        trimmed = line
        sub(/^[ \t]+/, "", trimmed)
        if (trimmed ~ /^\/\//) next

        # start-anchored integer-literal byte slice: [..N] [..=N]
        if (line ~ /\[\.\.=?[0-9]+\]/) {
          printf "%s:%d: %s\n", FILENAME, FNR, line
        }
      }
    '
)"

if [ -n "$violations" ]; then
  echo "[FAIL] byte-index string slice (CJK-panic risk)"
  echo "$violations" | while IFS= read -r v; do
    echo "       - ${v#"$ROOT/"}"
  done
  echo "================================"
  echo "use .chars().take(N).collect::<String>() instead of &s[..N]"
  echo "(if the slice is genuinely safe, append  // cjk-ok: <reason>)"
  exit 1
fi

echo "[PASS] no byte-index string slices"
echo "================================"
echo "cjk-safe gate green"
exit 0
