# CodeForge Backlog

Last audited: 2026-05-05

Items confirmed low-priority or blocked by external dependencies. Each item has a trigger condition for when to pick it up.

---

## B1 — Signal Cursor Race Condition (Concurrent dream runs)

**Area**: `src/dream/compile.rs`, `src/db/schema.sql` (signal_cursors)
**Premise**: If two `codeforge dream` calls run simultaneously (e.g. SessionEnd hook + manual), the signal cursor may be read twice before either write completes, causing duplicate L1 entries.
**Trigger**: Pick up when Phase 2 daemon is implemented (daemon will own all write operations, eliminating concurrent CLI calls).
**Mitigation in place**: `PRAGMA busy_timeout=5000` ensures one waits; risk is low for Phase 1 single-user.
**Status 2026-04-17**: Phase 2a shipped. Daemon now owns derived-state writes (`pet_snapshot`, `combat_log`, `game_world`), but `signal_cursors` are still CLI-written by `dream compile` — this item remains open with unchanged scope.
**Status 2026-06-15**: ship-online 把 `codeforge dream` 移到 global SessionEnd（跑遍所有專案）。**預設 per-cwd `.codeforge` 不受影響**（各專案寫各自 store,無 cross race）；只有 `CODEFORGE_DIR` 共享 store 的使用者,多專案 session 同時結束時 concurrent-dream 機率上升 —— 仍由本 item 既有 scope 覆蓋,`busy_timeout` 緩解。

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

## B12 — `.claude/settings.json` Path Templating + Setup Script

**Area**: `.claude/settings.json`, new `scripts/setup-hooks.sh` or `codeforge init --hooks`
**Premise**: `.claude/settings.json` ships with absolute hook paths (e.g. `/home/codepower/projects/codeforge/.claude/scripts/check-improvements.js`). Claude Code does not yet support relative paths in hook configuration, so every cloner must manually edit 6 paths after cloning. README documents this in "First-time Claude Code hook setup" but it's friction. Two solutions discussed: (a) ship `settings.json.template` with `<REPO_ROOT>` placeholder + a `scripts/setup-hooks.sh` (or `codeforge init --hooks`) that rewrites paths on first run; (b) wait for Claude Code to support relative paths and switch.
**Trigger**: Pick up when (1) a contributor reports broken hooks after clone, OR (2) Claude Code adds relative-path support (then this becomes a sub-task of switching to relative paths), OR (3) > 5 stars on the public repo (signals contributor influx).
**Status 2026-05-05**: Identified during public-readiness audit (`doc/projects/_archive/2026-05-05-public-readiness/`). Deferred out of scope per that L-task's Design Decision 3 to keep scope focused. S-size if option (a), trivial if option (b).

---

## B13 — 真實 production ship e2e（對 live Mnemos server）

**Area**: runtime（`codeforge ship --no-hook` SessionEnd + `~/.mnemos/data/mnemos.db`）
**Premise**: ship-online 專案的 e2e 只在隔離 test DB(`/tmp/ship-e2e`)驗過 → 200。真正對使用者 production Mnemos brain 的 session-end ship（global hook 鏈)尚未端到端跑過一次。已 opt-in（`~/.config/mnemos.env`），但驗證當下 Mnemos server 沒跑 → 本機 session-end ship 會 queue 到 `~/.codeforge/ship-failed/`。
**Trigger**: 下次 Mnemos server 在跑時 —— 確認一次真實 session-end 的 ledger 落進 production brain（查 `documents`/`atoms`），並 `codeforge ship --resend` 清掉累積的 ship-failed/ queue。
**Status 2026-06-15**: ship-online 上線後立即產生;設計上 ship --no-hook 失敗只 queue 不阻塞,故非 blocker。

---

## B14 — 其他機器部署 ship-online（new binary + install --hooks）

**Area**: 各機器 `~/.local/bin/codeforge` + `~/.claude/settings.json` + `~/.config/mnemos.env`
**Premise**: ship-online 的跨專案 dream→ship 鏈 + cleanupPeriodDays 只在本機(twgs-revival)部署。settings.json 是 per-machine,使用者有多台機器（cleanupPeriodDays 原始問題就是「每台都要」）。每台需:(1) 裝 0.0.4+ binary 到 `~/.local/bin`;(2) 跑 `codeforge install --hooks`（寫 global dream→ship 鏈 + cleanupPeriodDays);(3) 用 Mnemos 的機器建 `~/.config/mnemos.env` opt-in。
**Trigger**: 下次在每台其他機器工作時執行上述三步;或做一個 `codeforge bootstrap` 一鍵命令收斂這流程。
**Status 2026-06-15**: 本機已部署驗證;其他機器待逐台執行。

---

## B15 — improvement-queue surfacing 一般化(project-scoped,非 codeforge-clone-only)

**Area**: `.claude/scripts/check-improvements.js`、`codeforge install`（hook 安裝佈局）
**Premise**: improvement-queue 已做 project 歸屬(session-digest 寫項標 `project: cwd`,2026-06-15 fix `a0169cc`),但**surface 它的 `check-improvements.js` 仍是 codeforge-clone-only**(只裝在 codeforge clone 的 `--project-hooks`,`PROJECT_ROOT` 由 `__filename` 推導恆為 codeforge 根)。後果:
- CodeForge 只 surface 自己的項(已修,正確)。
- **其他專案(CodePower 等)寫進共享 queue 的項,沒有任何地方會在該專案 surface** —— 它們帶了 `project` tag 卻無 surfacing hook 消費。
參考 dream/ship 的做法:把 surfacing 從 clone-only 改成可跨專案(global 或可裝進任一專案的 SessionStart hook),用 hook CWD/`CLAUDE_PROJECT_DIR` 當 `PROJECT_ROOT`,各專案 SessionStart 只報屬於自己(`project === <該專案根>`)的 pending 項。
**Trigger**: 當 (1) 想讓 CodePower(或其他專案)session 也能在 SessionStart 看到自己的 pending improvements,或 (2) 把 check-improvements 納入 global hook 安裝鏈(類似 ship-online 把 dream/ship 移 global)時。需處理 `PROJECT_ROOT` 來源從 `__filename` 改成 hook 提供的 project dir。
**Status 2026-06-15**: project 歸屬(寫端)+ CodeForge 自身 scoping(讀端)已完成;跨專案 surfacing 一般化未做,記為 enhancement。


---

## B16 — memory-recall Phase B / C(偷好偷滿的下一輪)

**Area**: `src/dream/compile.rs`、`src/memory/recall.rs`、SessionEnd hook、L1 schema
**Premise**: Phase A(本地 recall 注入器)已上線(2026-06-16,`local-recall` 專案)。design spec `doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md` 的後續 tier:
- **Phase B(Tier 2)**:T2.2 mem0 式 ADD/UPDATE/DELETE/NOOP 對賬 + T2.3 統一 recency×importance×relevance 排序 → **2026-06-17 開做**(plan `doc/plans/2026-06-17-memory-recall-phase-b.md`)。**T2.1 async fire-and-forget worker 留此 backlog**(見下)。**procedural-atom `nature` 欄位**:只在 frontmatter 留位,分類邏輯歸 Phase C。
- **Phase C(Tier 3,評估後選)**:本地語意 recall(FastEmbed+HNSW,取代/增補 FTS5 keyword)、失敗/卡關為一等記憶、typed observation/relation schema、矛盾偵測 + 兩段式信心衰減。
**Trigger**:
- **T2.1(async worker)**:當 dream/ship 在 SessionEnd 開始有感卡頓時開新 L。設計已收斂(留檔在 phase-b plan「OUT OF SCOPE」段):conditional enqueue + 同步 fallback —— 有 live daemon 才 `dream --background` emit `dream_scheduled` event 快速返回,無 daemon inline 同步跑(維持「永遠會 distill」、不回歸 ship-online)。2026-06-17 user decision defer:觸發條件未成立 + 紅利範圍窄(多數專案無 daemon)+ 動剛上線熱路徑風險不划算。
- **Phase C**:當 keyword recall 品質證實不足(語意)、或想捕捉失敗 pattern 時。
**Status 2026-06-17**:Phase A done;Phase B 的 T2.2+T2.3 開做;T2.1 + Phase C 記為 enhancement,有 design spec 母本與 credits。
