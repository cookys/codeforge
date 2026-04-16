# Dev Flow — CodeForge Config

## Size Rules

- **S**: single file, no schema change, no new CLI command → direct commit to main
- **L**: 3+ files OR new CLI command OR schema migration OR new module → feature branch + plan
- **H**: binary panics or silently corrupts data in production → hotfix branch

## Risk Escalation (force L)

LLM API calls / SQLite schema / game state model / IPC protocol / Phase 2 daemon architecture

## Quality Gate

- **S**: `cargo check` (pass = green)
- **L**: `cargo check` + `cargo test` + `superpowers:requesting-code-review`
- **H**: `cargo check` + `cargo test` — Critical findings only

## Build

```bash
cargo build --release
cargo install --path .    # install to ~/.cargo/bin/codeforge
```

## Auto Push

- S commit to main → 自動 `git push`，不需問用戶
- Merge feature branch to main → 自動 `git push`

## Project Paths

- Projects: `doc/projects/`
- Plans: `doc/plans/`
- Archive: `doc/projects/_archive/`
- Backlog: `doc/BACKLOG.md`
- Index: `doc/projects/INDEX.md`
- Plans Index: `doc/plans/INDEX.md`
- Specs: `doc/specs/`

## Naming Conventions

- Plan files: `yyyy-mm-dd-title.md`
- Project dirs: `yyyy-mm-dd-title/`
- Branch: `feature/{title}`

## Session Start — CodeForge Extra Gates

### Orphan Project Detection (MANDATORY)

```bash
cat doc/projects/INDEX.md
```

For each Active project: check if all phases done → archive if so.

### Spec Awareness Gate

Before starting Phase 2+ work, confirm you have read:
- `doc/specs/codeforge-mud-engine.md` — daemon architecture
- `.claude/rpg-engine-spec.md` — write ownership model (daemon owns all writes)
- `.claude/i18n-spec.md` — i18n two-layer design

## Skill Routing

| Code Area | Required Skills |
|-----------|----------------|
| `src/cli/` | `superpowers:requesting-code-review` before merge |
| `src/memory/` | `superpowers:requesting-code-review` (data integrity) |
| `src/pet/state.rs` | `superpowers:requesting-code-review` (game loop overflow risks) |
| Any new Phase 2 daemon work | Read `doc/specs/codeforge-mud-engine.md` FIRST |
| Architecture decisions | `autopilot:think-tank` |
| Tech survey (e.g. crossterm vs ratatui) | `autopilot:survey` |

## Mandatory Task Checklist (ALL sizes)

**ENFORCEMENT RULE**: Task creation is NOT optional for L/H workflows. At L-3 project setup, ALL tasks below MUST be created via TaskCreate. Skipping task creation = skipping the review gate = process violation.

### S workflow
```
- [ ] Phase work
- [ ] Quality gate: cargo check
- [ ] Review loop: code review agent → fix findings
- [ ] Commit + push
```

### L workflow (create ALL these tasks at L-3 project setup — MANDATORY)
```
- [ ] Phase 1..N tasks (one per phase)
- [ ] Quality gate: cargo check + cargo clippy + cargo test  [blockedBy: all phase tasks]
- [ ] Review loop: invoke code-review skill (max 3 rounds)   [blockedBy: quality-gate]
- [ ] Fix review findings: CRITICAL + IMPORTANT              [blockedBy: review-loop]
- [ ] Review round 2: verify fixes pass                      [blockedBy: fix-findings]
- [ ] Merge to main                                          [blockedBy: review-round-2]
- [ ] Post-merge verify: git diff main~1..main
- [ ] Archive project + BACKLOG reconciliation
- [ ] Push
```

### H workflow
```
- [ ] Fix
- [ ] Quality gate: cargo check + cargo test
- [ ] Review loop: code-review Critical only    [blockedBy: quality-gate]
- [ ] Merge + push                              [blockedBy: review-loop]
```

### HARD RULES

1. **Review blocks merge**: Review loop task MUST be completed BEFORE Merge can start. Use `addBlockedBy`. No exceptions.
2. **Round 2 required**: After fixing findings, a verification round MUST run. Only then can merge proceed.
3. **Self-check before merge**: Before marking "Merge" as in_progress — "Is the Review loop task completed?" If NO → STOP.

## BACKLOG Hygiene

### Session End — BACKLOG Pickup
```bash
git log --oneline $(cat .claude/session-start-sha 2>/dev/null || echo "HEAD~10")..HEAD
```
Check if any BACKLOG items have their trigger condition met by these changes.

### BACKLOG Staleness (periodic)
If `Last audited:` in `doc/BACKLOG.md` is >14 days ago: flag to user for full audit.

## Proposals Hygiene

Check `doc/proposals/*.md`. For each proposal >60 days old and not referenced by any active plan/project → flag for review.

## Scope Creep Detection

After every commit, self-check:
```
Has the scope grown beyond original S-size?
  - 3+ commits already made
  - 3+ files in different modules changed
  - User asked for additional features beyond original goal

If yes → re-evaluate as L-size:
  - Create project dir + README + INDEX (retroactive)
  - Record prior commits as completed phases
  - Continue with L workflow tracking
```

## Phase 2 Readiness Checklist

Before starting Phase 2a (daemon):
- [ ] Read `doc/specs/codeforge-mud-engine.md` in full
- [ ] Read `.claude/rpg-engine-spec.md` (daemon write ownership)
- [ ] Confirm tokio is in Cargo.toml with `full` features
- [ ] Plan: create `doc/plans/yyyy-mm-dd-phase2a-daemon.md`
