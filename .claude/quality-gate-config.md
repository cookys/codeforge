# Quality Gate — CodeForge Config

## Commands

### S-size gate
```bash
cd /path/to/codeforge && cargo check 2>&1
```

### L-size gate
```bash
cd /path/to/codeforge
cargo check 2>&1
cargo clippy -- -D warnings 2>&1
cargo test 2>&1
```

### H-size gate (hotfix)
```bash
cd /path/to/codeforge
cargo check 2>&1
cargo test 2>&1
```

## Completeness Scan (run before every commit)

Scan for placeholder/stub patterns:
```bash
grep -rn 'todo!\|unimplemented!\|#\[ignore\]\|panic!("TODO")' src/ --include='*.rs'
```
Any match = blocked. Fix or escalate.

## Code Review Dispatch

Use `superpowers:requesting-code-review` skill:
```bash
BASE_SHA=$(git log --oneline | grep "last clean commit" | awk '{print $1}')
HEAD_SHA=$(git rev-parse HEAD)
```
- **S**: run after completeness scan, before commit
- **L**: run once per phase AND as final gate before merge
- **H**: run with Critical-severity filter only

## Clippy Configuration

No custom `clippy.toml` — use defaults with `-D warnings` for L/H.
For S-size: `cargo check` is sufficient (clippy adds overhead).

## Known Clean Patterns

These patterns are intentional, not bugs:
- `unwrap_or(0)` in DB count queries — always returns 0 on error, safe
- `unwrap_or_else(|_| PathBuf::from("."))` in Context::load — safe fallback
- `u64 as u32` in XP overflow fix — guarded by `.min(10_000_000)` cap

## Doc-Code Drift — two layers

**Layer 1 — deterministic gate (reliable, IS a gate for doc-touching changes):**
```bash
python3 scripts/check-doc-drift.py    # links · fences · version-sync · cli-surface · roadmap-consistency
```
Zero-variance checks that always catch their class. Exit 0 = green. Runs in CI
(`.github/workflows/ci.yml` → `doc-drift` job) on every push/PR to main, and
should be run before any doc-touching merge. **This is the reliable stopping
condition** — a green run means those classes are genuinely clean (unlike the
LLM sweep, whose "clean" is just an unproven sample).

**Layer 2 — LLM full sweep (discovery, NOT a gate):**
The `doc-drift-scoped` / `doc-code-drift-audit` workflows (see
`.claude/dev-flow-config.md` → Doc-Code Drift Audit) are non-deterministic
DISCOVERY tools — run periodically / on big changes to find NEW drift classes.
Do not loop them to zero (proven non-convergent — see `.claude/doc-audit-state.json`).
When a sweep finds a recurring/mechanizable class, **demote it into Layer 1**
(add a check to `scripts/check-doc-drift.py`) so it becomes reliably caught.

## Exit Codes

| Code | Meaning | Action |
|------|---------|--------|
| 0 | All clear | Proceed |
| 1 | cargo check/clippy/test failure | Fix before proceeding |
| non-zero | Completeness scan match | Fix placeholder, then re-run |
