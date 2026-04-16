# CodeForge Backlog

Last audited: 2026-04-16

Items confirmed low-priority or blocked by external dependencies. Each item has a trigger condition for when to pick it up.

---

## B1 — Signal Cursor Race Condition (Concurrent dream runs)

**Area**: `src/dream/compile.rs`, `src/db/schema.sql` (signal_cursors)
**Premise**: If two `codeforge dream` calls run simultaneously (e.g. SessionEnd hook + manual), the signal cursor may be read twice before either write completes, causing duplicate L1 entries.
**Trigger**: Pick up when Phase 2 daemon is implemented (daemon will own all write operations, eliminating concurrent CLI calls).
**Mitigation in place**: `PRAGMA busy_timeout=5000` ensures one waits; risk is low for Phase 1 single-user.

---

## B2 — CJK-Aware FTS5 Search Tokenizer

**Area**: `src/db/schema.sql` (memory_fts), `src/memory/fts.rs`
**Premise**: FTS5 uses `porter unicode61` tokenizer which doesn't handle CJK character segmentation. Searching for Chinese terms like "重複" may miss matches split across FTS tokens.
**Trigger**: Pick up when user reports CJK search misses, or when FTS corpus grows to >500 entries.
**Note**: ICU tokenizer or external jieba segmentation needed. External dependency — needs survey first.

---

## B3 — Badge System Implementation (Phase 1 stub)

**Area**: `src/pet/badges.rs`, `src/db/schema.sql` (badges table)
**Premise**: Badge table exists in schema but `badges.rs` contains only struct definitions. No badge award logic exists.
**Trigger**: Pick up in Phase 2b (loot system) — badges are natural loot drops from MOB kills.

---

## B4 — dream decay + track Operations (Stub)

**Area**: `src/dream/` (decay.rs, track.rs assumed)
**Premise**: `codeforge dream` supports `decay` and `track` ops in the runner but the actual decay/track logic may be minimal or stub.
**Trigger**: Pick up when L1 knowledge base grows to >200 entries and signal age becomes meaningful.

---

## B5 — Ingest Parser: ChatGPT Export Format

**Area**: `src/cli/ingest.rs`
**Premise**: `ingest --source chatgpt` is declared in CLI but parser may not handle all ChatGPT export variants (JSON vs markdown).
**Trigger**: Pick up if user requests ChatGPT migration or if a second user adopts CodeForge.

---

## B6 — `.env.example` Auto-validation on Init

**Area**: `src/cli/init.rs`
**Premise**: `codeforge init` creates directory structure but doesn't validate that `ANTHROPIC_API_KEY` is set. Dream compile will silently fail without it.
**Trigger**: Pick up in any Phase 1 polish pass. Small S-size task.

---

## B7 — Memory Projection to AGENTS.md

**Area**: `src/` (projection module placeholder)
**Premise**: The architecture mentions `projection/` for writing memory context to `AGENTS.md`, but this is not yet implemented.
**Trigger**: Pick up before Phase 2 when sub-agents need context from codeforge memory.

---

## B8 — Phase 2 Planning: Daemon Framework

**Area**: New — `src/daemon/`
**Premise**: Phase 2a daemon requires tokio runtime, unix socket IPC, 60s tick loop, and crossterm TUI. Significant L-size work.
**Trigger**: Pick up after Phase 1 is stable for 2+ weeks in production use. Read `doc/specs/codeforge-mud-engine.md` first.
**Prerequisites**: think-tank for IPC design, survey for TUI library choice (crossterm vs ratatui).

---

## B9 — Clippy Lint: Forbid `&s[..N]` on User Strings

**Area**: Build tooling / `src/` conventions
**Premise**: CJK truncation bug hit 4 files in Phase 1. A custom clippy lint or grep CI check would prevent recurrence.
**Trigger**: Pick up when Phase 2 code volume increases (more new files = more risk). S-size: add to a `scripts/check-cjk-safe.sh`.
