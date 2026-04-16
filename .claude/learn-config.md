# Learn Config — CodeForge

## Auto-Record Triggers

Record to `.claude/knowledge/` without asking when:

| Trigger | Category | File |
|---------|----------|------|
| `cargo check` failed, then fixed after 2+ retries | `build` | `rust-patterns.md` |
| CJK truncation panic (`byte index N is not a char boundary`) | `build` | `rust-patterns.md` |
| SQLite `SQLITE_BUSY` error | `env` | `environment.md` |
| Searched for file/struct 3+ times | `env` | `environment.md` |
| Architecture decision iterated (e.g. daemon IPC protocol) | `arch` | `architecture.md` |
| Anthropic API error pattern fixed after 2+ attempts | `api` | `rust-patterns.md` |

## Knowledge Files

| File | Content |
|------|---------|
| `rust-patterns.md` | CJK safe truncation, overflow fixes, async patterns |
| `environment.md` | codeforge init dir, cargo install path, env vars |
| `architecture.md` | Design decisions for daemon, IPC, memory model |

## Knowledge Graduation Rules

When a knowledge entry is referenced 3+ times in the same category, consider:
1. Promoting to CLAUDE.md Conventions section (universal rule)
2. Adding as a clippy lint if mechanical (e.g. CJK truncation)

## Memory Refresh Triggers

Refresh `~/.claude/projects/codeforge/memory/MEMORY.md` when:
- A project is archived (remove stale project_*.md refs)
- A feedback rule is violated again (confirm rule is still in MEMORY.md)
- After running `autopilot:learn` health audit

## Index Rotation

Keep `INDEX.md` Recent Learnings at ≤10 entries. When exceeding:
```bash
# session-digest.js auto-flags this via improvement-queue
# Manual rotation: remove oldest entries from the table
```
