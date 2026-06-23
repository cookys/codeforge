# CodeForge Backlog

Last audited: 2026-06-18

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

## ~~B9 — Clippy Lint: Forbid `&s[..N]` on User Strings~~ ✅ Done 2026-06-23

**Area**: Build tooling / `src/` conventions
**Premise**: CJK truncation bug hit 4 files in Phase 1. A custom clippy lint or grep CI check would prevent recurrence.
**Shipped** (`615562b`→`bfa349e`, S-size): `scripts/check-cjk-safe.sh` deterministic gate + CI `cjk-safe` job (sibling of `check-doc-drift.py`). Chose a bash/awk grep-style gate over a custom clippy lint (no `dylint` toolchain dependency; matches the existing deterministic-gate pattern). Flags start-anchored integer-literal byte slices `[..N]` / `[..=N]` (the documented `&s[..N]` form) — precision-over-recall, zero current false positives. Escape hatch `// cjk-ok:`. **Known gap** (by design, would need type info to disambiguate from safe Vec idioms): open-end `[N..]`, range `[N..M]`, and variable-bound `[..n]` forms are NOT flagged — string slicing with those still relies on the `.chars().take(N)` convention + review.

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

## ~~B14 — 其他機器部署 ship-online（new binary + install --hooks）~~ ✅ 一鍵命令 Done 2026-06-23

**Area**: 各機器 `~/.local/bin/codeforge` + `~/.claude/settings.json` + `~/.config/mnemos.env`
**Premise**: ship-online 的跨專案 dream→ship 鏈 + cleanupPeriodDays 只在本機(twgs-revival)部署。settings.json 是 per-machine,使用者有多台機器（cleanupPeriodDays 原始問題就是「每台都要」）。每台需:(1) 裝 0.0.4+ binary 到 `~/.local/bin`;(2) 跑 `codeforge install --hooks`（寫 global dream→ship 鏈 + cleanupPeriodDays);(3) 用 Mnemos 的機器建 `~/.config/mnemos.env` opt-in。
**Shipped** (`codeforge bootstrap`, L-size 2026-06-23): trigger 的「一鍵命令」方案完成 —— `src/cli/bootstrap.rs` 薄 orchestrator 收斂 install --all + fmt pin toolchain（B19）+ Mnemos opt-in 報告。best-effort、idempotent、`--dry-run`/`--quiet`。

**Per-machine runbook**（每台其他機器執行）:
```bash
# 1. 裝/更新 binary（在該機器的 codeforge clone 內）
cargo install --path .            # → ~/.cargo/bin/codeforge
# 2. 一鍵設定（取代舊三步手動流程）
codeforge bootstrap               # install --all + fmt toolchain + mnemos 狀態
#    或先預覽： codeforge bootstrap --dry-run
# 3. （只有用 Mnemos 的機器）opt-in —— bootstrap 只會「報告」不自動建：
#    建 ~/.config/mnemos.env（或設 MNEMOS_INGEST_URL）後重跑 bootstrap 確認 ✓
# 4. 之後維護：在該 clone `git pull && codeforge bootstrap`（fmt pin 隨 pull 走、toolchain self-install）
```
注意：bootstrap 不自動裝 binary 本身（你得先有 binary 才能跑它）、不自動建 mnemos.env（opt-in 刻意手動）。既有非-codeforge statusLine 不會被 clobber（會警示，要接管用 `install --all --force`）。
**Status 2026-06-23**: 一鍵命令上線（本機驗證 dry-run 正確）。各機器仍需逐台執行上述 runbook（CLI 只能本機跑，無法遠端代部署）—— 但已從「記三步」收斂成「一行」。

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

---

## B17 — session-digest A′ 落地的 review 殘留(低優先)

**Area**: `.claude/scripts/session-digest.js`、`src/dream/ingest_digests.rs`
**Premise**: A′ 落地(2026-06-17,`443e31d`)§6 獨立 review 留下幾個低優先項,已判定接受/駁回,記為 enhancement:
- **mtime-guard 殘留亞秒競態**:ingest 刪檔前比對 mtime,但 coarse-mtime(秒級)FS 上「讀後同一 mtime-tick 內被改寫」仍可能刪到新檔。已配 atomic write 大幅收窄;若要全閉,改 rename-to-staging(`X.json`→`X.json.ingesting` 後處理)+ orphan 回收。實務機率極低(09:00 離峰 cron、session 多不活躍),故未做。
- **`findCodeforgeRoot` 吞 EACCES**:`statSync` 把權限錯一律當「非 repo」→ skip 不寫,fail-safe 正確但不可診斷。可加「非 ENOENT errno 時 log」便於排查。
- **improvement-queue 非原子寫**(pre-existing,本次 out of scope):`session-digest.js` 寫 `improvement-queue.json` 用 `writeFileSync` 但 doc-comment 宣稱 atomically。可比照 digest 改 temp+rename。
**Trigger**: (1) 真的觀察到 worktree/權限情境的靜默 capture gap;或 (2) 多 dream 並行/高頻 PreCompact 環境下出現 digest 競態丟失;或 (3) 順手清 improvement-queue 寫入時。
**Status 2026-06-17**: review 已處置(接受現狀);記為 enhancement。

---

## B18 — spec 設計目標 code 未跟上（doc-code-drift audit 2026-06-18）

**Area**: `src/cli/ship.rs`、`src/mnemos/*`、`src/daemon/combat.rs`、`src/pet/ability.rs`
**Premise**: 2026-06-18 的 doc↔code drift audit（6 領域 + 對抗驗證，48 confirmed findings）發現幾處是「spec 寫了設計、code 尚未實作」（非 doc 寫錯）。已在各 spec 頂部標 `⚠️ NOT YET IMPLEMENTED / CODE LAGS DESIGN`，code 補上時清這些標記：
- **ship 掃 session jsonl**：`codeforge-ship.md` §5/§6.1 設計 ship 讀 `~/.claude/projects/<repo>/*.jsonl` 取 source_evidence locator（uuid/ts/line）+ prompt [session 線索] block。實作只讀 L1 + git；`SourceEvidence::session_jsonl` 是 dead_code。
- **provenance 擴充欄位**：`codeforge-ship.md` §4.3 設計 `raw_signal_count`(從 db 算)/`source_jsonl_paths`/`digest_cost_usd`。`build_provenance` 只產 5 個基本欄位。
- **ship-state 省 digest**：§7.6 設計「已 ship 過 → skip digest + POST」，code 先 digest 才查 already_shipped，只 skip POST（浪費一次 LLM 呼叫）。
- **Pet Ability 戰鬥效果**：`codeforge-mud-engine.md` §2.5 的 Quick Eye/Focus Strike/Tome Sense 效果未實裝；`ability.rs` 只是 display-only catalog，`combat.rs` 無 ability 邏輯。
- **auto-cite-on-ship**（full sweep 2026-06-18 補）：`codeforge-ship.md` §9 設計 ship 結束自動偵測 transcript 引用的 atom → 回寫 cite。實作沒接線；cite-detect 只是手動子命令、不在 SessionEnd hook 鏈。citation 正常 session 不會自動累加。
- **Void Creature MOB + Lv50 legendary 村莊**（full sweep 2026-06-18 補）：`codeforge-mud-engine.md` §2 的 Void（missing test coverage→drain DEF）MOB 未實作（scanner 只產 Zombie/Boss/Elite/Ghost 4 種）；§2.5 Lv50 legendary 表列的 ML / 開源基金會 村莊不存在（實際 5 村莊，且 javascript 無 legendary 條目）。
**Trigger**: 各子項獨立 —— 想讓 ledger 帶 session 證據 / 想要 cost telemetry / 想省重複 digest / 想做 ability 戰鬥深度 / 想自動 cite / 想加 MOB 種類或村莊時，分別開 S~L 實作並清掉對應 spec 的 ⚠️ 標記。
**Status 2026-06-18**: audit 已把 doc 側全部更新成現實 + 標記設計目標;code 側未動,記為 enhancement。純 stale/missing 的 drift（首輪 48 條 + scoped 4 條 + full sweep 22 條）已分別修正落 doc。

---

## ~~B19 — 全 repo rustfmt drift（工具鏈版本不一致，CI fmt job 疑似紅燈）~~ ✅ Done 2026-06-23

**Area**: `src/cli/*.rs`、`src/dream/*.rs`、`src/memory/*.rs`、`src/mnemos/*.rs`（14 檔）
**Premise**: 2026-06-23 B9 開發時 `cargo fmt --all -- --check` 報 14 個檔有 fmt diff（`daemon.rs` `install.rs` `mnemos_cli.rs` `ship.rs` `statusline.rs` `compile.rs` `ingest_digests.rs` `l0.rs` `recall.rs` `cite_detect.rs` `config.rs` `context.rs` `digest.rs` `state.rs`）。全是長行被 rustfmt 1.8.0-stable（2026-03-25）重排（`format!` / 長 `assert_eq!` / 長 `say()` 呼叫被拆多行），**非 B9 改動引入**（B9 的 `compile.rs:112` trailing comment 不在 diff 內）。根因疑似 committed code 是在較舊 rustfmt 下 fmt-clean，本機/CI 升到 1.8.0 後規則改變。CI 的 `fmt` job 用 `dtolnay/rust-toolchain@stable`（不 pin 版本），故下次 CI run 的 fmt job 很可能紅燈。
**Shipped** (`feature/rustfmt-pin`, L-size): 治本 —— 不只收乾，還消除漂移源。
- **`scripts/fmt.sh`**：單一 pin 來源（`PIN=1.94`），self-install 該 toolchain，`cargo +$PIN fmt`（`+toolchain` 只 pin fmt、build/test/clippy 維持 rolling stable、MSRV 仍在 Cargo.toml，**不引入 `rust-toolchain.toml`**）。
- **收乾 baseline**：14 檔 618+/191-（whitespace-only），784 tests 綠。
- **三層 enforcement（確定性 > 記憶）**：CI fmt job → `./scripts/fmt.sh --check`（不可繞過守門）；`.claude/quality-gate-config.md` + `dev-flow-config.md` S/L/H gate 都加 `--check`；CLAUDE.md Development Commands 規範一律走 wrapper、禁裸 `cargo fmt`。
- knowledge `environment.md` 記決策（cross-link `gate-patterns.md` 確定性 gate 家族）。
**Status 2026-06-23**: 由「研究怎樣處理最好 + 讓所有 session agent 自動 follow」需求展開。研究背書（RFC 2437 跨版本不保證 fmt 穩定；decouple fmt/build toolchain）。

---

## B20 — `codeforge self-update` 二進位自更新（現代化更新機制）

**Area**: `src/cli/selfupdate.rs`、`Cargo.toml`（self_update dep）、`install.sh`、release pipeline
**Premise**: 安裝/更新機制現狀 —— `install.sh`（curl|sh 首裝，public-readiness 時做的）存在但更新得手動重跑 / `cargo install` / 手動 cp；且 `~/.local/bin` vs `~/.cargo/bin` 雙位置會 stale（本機實際撞到：PATH 上 0.0.4、cargo bin 0.0.5）。
**Shipped** (`feature/self-update`, L-size 2026-06-23): `codeforge self-update [--check]`（self_update crate，GitHub release 後端，ureq+rustls 不重複拉 reqwest）。原地替換 `current_exe` → 不管 binary 在哪都更新對、繞開「哪個在 PATH」問題。`bin_path_in_archive` 對應 release.yml 的 `codeforge-<v>-<target>/codeforge` 結構。install.sh「Next steps」改指向 `bootstrap` + `self-update`。README Updating 一節。
**未竟（需 user 確認後做）**: **尚未發布任何 release** —— `gh release list` 為空，release.yml 只產 draft 從沒 publish。self-update / install.sh 要能運作，必須先 `git tag v0.0.5` → CI build → publish draft。user 2026-06-23 決定「先做 code，release 等確認」。
**Trigger**: 確認後 cut v0.0.5（首發），之後每次發版 self-update 即可跨機更新。可選：release.yml `draft:true`→自動 publish，或保留 draft 手動 publish。
**Status 2026-06-23**: code + docs done；release 待 user 確認發布。
