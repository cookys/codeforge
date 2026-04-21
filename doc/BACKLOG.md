# CodeForge Backlog

Last audited: 2026-04-17

Items confirmed low-priority or blocked by external dependencies. Each item has a trigger condition for when to pick it up.

---

## B1 — Signal Cursor Race Condition (Concurrent dream runs)

**Area**: `src/dream/compile.rs`, `src/db/schema.sql` (signal_cursors)
**Premise**: If two `codeforge dream` calls run simultaneously (e.g. SessionEnd hook + manual), the signal cursor may be read twice before either write completes, causing duplicate L1 entries.
**Trigger**: Pick up when Phase 2 daemon is implemented (daemon will own all write operations, eliminating concurrent CLI calls).
**Mitigation in place**: `PRAGMA busy_timeout=5000` ensures one waits; risk is low for Phase 1 single-user.
**Status 2026-04-17**: Phase 2a shipped. Daemon now owns derived-state writes (`pet_snapshot`, `combat_log`, `game_world`), but `signal_cursors` are still CLI-written by `dream compile` — this item remains open with unchanged scope.

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

## ~~B8 — Phase 2 Planning: Daemon Framework~~ ✅ Done 2026-04-17

Shipped as Phase 2a (`_archive/2026-04-17-phase2a-daemon`). Final design
diverged from initial scope: IPC is SQLite `event_inbox` (Option D) +
tick-driven drain; real-time UX comes from a live read path
(`pet::live_state::LiveState` overlays unseen `event_inbox` XP on top
of `pet_snapshot`), not IPC wake. TUI rendering deferred to Phase 2c.

---

## B9 — Clippy Lint: Forbid `&s[..N]` on User Strings

**Area**: Build tooling / `src/` conventions
**Premise**: CJK truncation bug hit 4 files in Phase 1. A custom clippy lint or grep CI check would prevent recurrence.
**Trigger**: Pick up when Phase 2 code volume increases (more new files = more risk). S-size: add to a `scripts/check-cjk-safe.sh`.

---

## B10 — Doppelganger Split + `SuppressDoppelgangerSplit` Runtime

**Area**: `src/daemon/mob.rs`, `src/daemon/combat.rs`, `src/db/schema.sql`
**Premise**: Phase 3e 的 `Doppelganger Ward` item 可 craft + use，`active_effects` table 正確寫入，但 daemon 沒有 doppelganger split 邏輯 —— 整個 split mechanic（on-defeat spawn / cascade / stats 繼承）在 spec §2 只有一行，需要先補 design 才能寫 code。Ward 目前 storage-only，`codeforge inventory` 會顯示警語提醒玩家。
**Trigger**: `doc/proposals/2026-04-18-doppelganger-split.md` 有 `## Decision` 區塊 —— user 回答三個參數（`split_trigger` / `max_children` / `child_stat_ratio`）後可直接進入 L-size 實作。
**Status 2026-04-18**: Proposal 已寫，等 user decision。預設答案（A/A/A）列在 proposal 文末。

---

## B11 — Tile-Map Zone Color: Paint-Layer Integration

**Area**: `src/tui/render.rs` (`PositionedLine` / `paint`), `src/tui/panels/local_map_tile.rs::zone_color`
**Premise**: Tile-map-localmap project（2026-04-21 merge）把 `zone_color()` 的 directory→Color mapping 實作 + unit tested 完備，但 paint 層沒接上 —— `zone_color` 目前 `#[allow(dead_code)]`。要在 grid tile 的 border 著色（且名稱/badge 保留 default fg），`PositionedLine.text: String` + `paint` 的整行 `Print(&text)` 要改成 `Vec<StyledSpan>` 之類 per-range 著色表示。完成後 spec §5 Local Map Tile-Grid Mode 才真正滿足 "5 zone kind 各有獨立 color — unit + ANSI snapshot test" 的 ANSI 部分。
**Trigger**: (a) user 反映 grid mode 單色太單調想要色區分、(b) Phase 4 Zoa full impl 有類似 per-char styling 需求，兩個工程一起做 amortize 成本、(c) 下一個 TUI UX polish L project。
**Status 2026-04-21**: `zone_color` mapping + unit tests 已 ship 在 feature/tile-map-localmap 分支；`#[allow(dead_code)]` 註記 + 本 backlog 項防止被遺忘。
