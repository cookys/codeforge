#!/usr/bin/env bash
# e2e-mnemos-chain.sh — full-chain proof of the CodeForge ↔ Mnemos L1 loop.
#
# Proves, in ONE pass against the LIVE local Mnemos daemon:
#   ship → ingest → extract → SessionStart-inject → cite → citation_count++
# plus B18 auto-cite-on-ship (a ship run auto-detects a referenced atom).
#
# fuchikoma ROADMAP P1.1 acceptance (mnemos CLAUDE.md §5 Gen-2 e2e chain).
#
# Safety: uses TEST-TAGGED, append-only data only.
#   - a synthetic repo `codeforge-e2e-<nonce>` in a throwaway $HOME/$CODEFORGE_DIR,
#   - one L1 lesson whose title carries a unique nonce,
#   - which becomes exactly ONE tagged ledger document + ONE atom in the live brain.
# It NEVER deletes or mutates real atoms — the only mutations are cite bumps on the
# nonce atom it just created. Nonce-tagged residue can later be isolated by title.
#
# Determinism: the ship digest is forced onto the rule-based passthrough (restricted
# PATH with no claude/agy/codex + unset ANTHROPIC_API_KEY) so the atom title equals
# the L1 heading verbatim (nonce preserved end-to-end). No LLM in the loop.
#
# Usage:  scripts/e2e-mnemos-chain.sh
# Env:    MNEMOS_INGEST_URL  (default http://127.0.0.1:8845)
set -euo pipefail

BASE_URL="${MNEMOS_INGEST_URL:-http://127.0.0.1:8845}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${CODEFORGE_BIN:-$REPO_ROOT/target/debug/codeforge}"

pass() { printf '  \033[32m✓\033[0m %s\n' "$*"; }
fail() { printf '  \033[31m✗ FAIL:\033[0m %s\n' "$*" >&2; exit 1; }
step() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }

command -v jq  >/dev/null || fail "jq not installed"
command -v git >/dev/null || fail "git not installed"

step "0. preconditions"
[ -x "$BIN" ] || { echo "  building codeforge…"; (cd "$REPO_ROOT" && cargo build >/dev/null 2>&1); }
[ -x "$BIN" ] || fail "codeforge binary not found at $BIN"
code=$(curl -s -m5 -o /dev/null -w '%{http_code}' "$BASE_URL/health" || true)
[ "$code" = "200" ] || fail "Mnemos daemon not healthy at $BASE_URL (health=$code) — start it first"
pass "live Mnemos daemon healthy at $BASE_URL"

# ---- isolated throwaway environment -----------------------------------------
NONCE="e2e$(date -u +%Y%m%d%H%M%S)$$"
DATE="$(date -u +%F)"
TITLE="e2e ship chain marker $NONCE"
TMP_HOME="$(mktemp -d)"
REPO="$TMP_HOME/proj/codeforge-e2e-$NONCE"
CFDIR="$REPO/.codeforge"
MINIBIN="$TMP_HOME/minibin"
SLUG="$(printf '%s' "$REPO" | sed 's/[^a-zA-Z0-9]/-/g')"
TSDIR="$TMP_HOME/.claude/projects/$SLUG"
cleanup() { rm -rf "$TMP_HOME"; }
trap cleanup EXIT

mkdir -p "$CFDIR/store/concepts" "$MINIBIN" "$TSDIR"
# Force rule-based passthrough (deterministic, no LLM) by shadowing the three
# headless digest engines with failing stubs, PREPENDED to the normal PATH so git
# and coreutils still resolve. Cleaner + more portable than stripping PATH to git.
for eng in claude agy codex; do
  printf '#!/bin/sh\nexit 127\n' > "$MINIBIN/$eng"
  chmod +x "$MINIBIN/$eng"
done

# a real git repo whose branch name IS the nonce, with an empty-message commit so
# derive_topic() emits exactly one token (the nonce) — a multi-token topic drops a
# freshly-ingested atom below Mnemos' relevance threshold, so keep it single-token.
git -C "$REPO" init -q
git -C "$REPO" config user.email e2e@codeforge.local
git -C "$REPO" config user.name  e2e
git -C "$REPO" config commit.gpgsign false
git -C "$REPO" checkout -q -b "$NONCE"
echo "marker $NONCE" > "$REPO/README"
git -C "$REPO" add README
git -C "$REPO" -c commit.gpgsign=false commit -q --allow-empty-message -m ""

# one L1 lesson, created today, title carrying the nonce.
cat > "$CFDIR/store/concepts/e2e-$NONCE.md" <<EOF
---
type: concept
topic: e2e-chain-$NONCE
created: $DATE
updated: $DATE
sources: []
links: []
refs: 0
last_ref: null
strength: 1.0
status: active
origin: dev
---

# $TITLE

Synthetic lesson used only to verify the ship→ingest→extract→inject→cite chain
end to end against the live Mnemos daemon. Marker token $NONCE.
EOF

run_cf() { # run codeforge in the isolated env, LLM engines shadowed → passthrough
  env -u ANTHROPIC_API_KEY \
      HOME="$TMP_HOME" \
      PATH="$MINIBIN:$PATH" \
      CODEFORGE_DIR="$CFDIR" \
      CODEFORGE_DATA_DIR="$TMP_HOME/data" \
      MNEMOS_INGEST_URL="$BASE_URL" \
      "$BIN" "$@"
}
# context/cite do no digest; a plain env is fine (no need to shadow engines).
run_cf_net() {
  env HOME="$TMP_HOME" CODEFORGE_DIR="$CFDIR" CODEFORGE_DATA_DIR="$TMP_HOME/data" \
      MNEMOS_INGEST_URL="$BASE_URL" "$BIN" "$@"
}
ctx_json() { curl -s -m10 "$BASE_URL/v1/atoms/context?topic=$NONCE&max=20&max_sensitivity=work"; }
cit_of()   { ctx_json | jq -r --arg t "$TITLE" '.atoms[] | select(.title==$t) | .citation_count' | head -1; }
id_of()    { ctx_json | jq -r --arg t "$TITLE" '.atoms[] | select(.title==$t) | .id' | head -1; }

# ---- 1. SHIP → INGEST → EXTRACT ---------------------------------------------
step "1. ship (passthrough) → POST /v1/ingest/ledger"
out="$(run_cf ship --date "$DATE" 2>&1)" || { echo "$out"; fail "ship exited non-zero"; }
echo "$out" | sed 's/^/    /'
echo "$out" | grep -q "shipped to Mnemos" || fail "ship did not report success"
pass "ship POSTed the ledger"

step "2. ingest + extract landed an atom in the live brain"
ATOM_ID="$(id_of)"
CIT0="$(cit_of)"
[ -n "${ATOM_ID:-}" ] && [ "$ATOM_ID" != "null" ] || fail "nonce atom not found via /v1/atoms/context (ingest/extract broke)"
[ -n "${CIT0:-}" ] || fail "could not read citation_count"
pass "atom extracted: id=$ATOM_ID  citation_count=$CIT0"

# ---- 3. SESSIONSTART INJECTION ----------------------------------------------
step "3. SessionStart-inject surface (codeforge mnemos-cli context)"
inj="$(run_cf_net mnemos-cli context --topic "$NONCE" --max-sensitivity work 2>&1)"
echo "$inj" | grep -qF "$TITLE"   || { echo "$inj" | sed 's/^/    /'; fail "injection markdown missing the atom title"; }
echo "$inj" | grep -qF "$ATOM_ID" || fail "injection markdown missing the atom id"
pass "context injection renders the atom (title + id present)"

# ---- 4. CITE → citation_count++ ---------------------------------------------
step "4. cite write-back (codeforge mnemos-cli cite)"
cout="$(run_cf_net mnemos-cli cite "$ATOM_ID" --matched-text "$TITLE" 2>&1)"
echo "$cout" | grep -q "cited atom" || { echo "$cout" | sed 's/^/    /'; fail "cite did not succeed"; }
CIT1="$(cit_of)"
[ "$CIT1" = "$((CIT0 + 1))" ] || fail "citation_count did not increment ($CIT0 → $CIT1)"
pass "citation_count incremented $CIT0 → $CIT1"

# ---- 5. B18 auto-cite-on-ship -----------------------------------------------
step "5. B18 auto-cite: a ship run auto-detects a referenced atom"
# a session transcript for THIS repo, dated today, that references the atom title.
cat > "$TSDIR/session-$NONCE.jsonl" <<EOF
{"type":"user","text":"working on the loop"}
{"type":"assistant","text":"as noted: $TITLE — reusing that lesson here"}
EOF
aout="$(run_cf ship --date "$DATE" 2>&1)" || { echo "$aout"; fail "second ship exited non-zero"; }
echo "$aout" | sed 's/^/    /'
echo "$aout" | grep -q "auto-cite" || fail "ship did not run auto-cite over the transcript"
CIT2="$(cit_of)"
[ "$CIT2" = "$((CIT1 + 1))" ] || fail "auto-cite did not bump citation_count ($CIT1 → $CIT2)"
pass "auto-cite bumped citation_count $CIT1 → $CIT2 (B18 wired)"

printf '\n\033[1;32mE2E CHAIN GREEN\033[0m — ship→ingest→extract→inject→cite→count++  +  B18 auto-cite\n'
printf '  test atom (append-only, nonce-tagged): %s  title="%s"\n' "$ATOM_ID" "$TITLE"
