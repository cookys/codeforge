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

## Phase 2 Readiness Checklist

Before starting Phase 2a (daemon):
- [ ] Read `doc/specs/codeforge-mud-engine.md` in full
- [ ] Read `.claude/rpg-engine-spec.md` (daemon write ownership)
- [ ] Confirm tokio is in Cargo.toml with `full` features
- [ ] Plan: create `doc/plans/yyyy-mm-dd-phase2a-daemon.md`
