# Skill Routing — CodeForge

**Before touching source code, check if the task area has a dedicated skill.**

## Code Area → Skill Map

| Code Area | Required Skill | When |
|-----------|---------------|------|
| `src/pet/state.rs` | `superpowers:requesting-code-review` | Before merge — game loop overflow risks |
| `src/memory/` | `superpowers:requesting-code-review` | Before merge — data integrity |
| `src/cli/` | `superpowers:requesting-code-review` | Before merge — user-facing commands |
| `src/db/` | `superpowers:requesting-code-review` | Before merge — schema + migration safety |
| `src/dream/compile.rs` | Read Anthropic API docs | Before any LLM prompt change |
| Any Phase 2 daemon work | Read `doc/specs/codeforge-mud-engine.md` | FIRST, before writing code |
| Architecture decisions | `autopilot:think-tank` | When 2+ design options exist |
| Tech survey (e.g. crossterm vs ratatui) | `autopilot:survey` | When comparing libraries |
| L-size work | `autopilot:ceo-agent` or `autopilot:dev-flow` | Session start |

## Mandatory Pre-Conditions

### Before any `src/pet/` change:
1. Check `xp_to_next` uses f64 cast + `.min(10_000_000)` cap
2. Verify no `&s[..N]` byte-index truncation in display code

### Before any `src/memory/` change:
1. Verify all string truncation uses `.chars().take(N).collect::<String>()`
2. Check L0 → L1 signal cursor is updated atomically

### Before any Phase 2 daemon work:
1. Read `doc/specs/codeforge-mud-engine.md` daemon architecture section
2. Read `.claude/rpg-engine-spec.md` write-ownership model
3. Confirm IPC socket path is configurable (not hardcoded)

## Session Start Skill Invocation

| Task Type | Skill to Invoke |
|-----------|----------------|
| Bug fix (any area) | `autopilot:dev-flow` (Fix path) |
| New feature, S-size | `autopilot:dev-flow` (S path) |
| New feature, L-size | `autopilot:dev-flow` (L path) → then `autopilot:ceo-agent` if "just results" |
| Research / comparison | `autopilot:survey` |
| Strategic design | `autopilot:think-tank` |
| Session end | `autopilot:learn` (if anything surprised you or took 2+ retries) |
