# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

> Cargo.toml is already at `0.0.5`; this section is the pending **0.0.5** release (not yet git-tagged). Rename to `## [0.0.5] - <date>` and tag `v0.0.5` at release time.

### Added

- **記憶 recall Phase B(T2.2 + T2.3)** — `doc/plans/2026-06-17-memory-recall-phase-b.md`:
  - **T2.3 統一 ranking** — `recall::rank` 從「strength 單因子排序」改為複合分數 `importance(strength) × recency × citation(refs)`。recency = `0.5^(age/90 天)`,age 對齊「active 集合裡最新的 effective date」(= `updated` 與 `last_ref` 取較新),純函式無 clock。`refs`/`last_ref` 本地引用訊號首次進入排序;近期被引用的舊筆記會復活到前排。
  - **T2.2 mem0 式對賬** — `dream::compile` 在把新事實寫成新 slug 前,先 FTS 找既有相關 active 條目;有的話交 Haiku 判 ADD/UPDATE/DELETE/NOOP(JSON-in-text)。UPDATE 併入既有條(保留 created/strength/refs、union sources/links、bump updated);DELETE 把舊條標 `superseded-by:<slug>`(同 `dedup` 慣例)再寫新條;NOOP 不寫。一律偏向 ADD:無 API key / FTS 無命中 / 解析失敗 / target 越界都退回 ADD,絕不遺失知識。`CompileResult` 新增 `l1_noop`,dream 輸出印「N 對賬略過」。
  - T2.1(async worker)依 2026-06-17 決策 defer 到 BACKLOG B16(觸發條件未成立、且擾動剛上線的 SessionEnd 鏈)。
- **本地 recall(無 mnemos 的 READ 路徑)** — `codeforge memory context [--max-tokens][--hook]`:讀本地 active L1 → 複合分數排序(importance × recency × citation,見 T2.3)→ budget 截斷成 **lean ranked index**(~1500 token,非 dump)→ 印 markdown 或(`--hook`)SessionStart `hookSpecificOutput.additionalContext` JSON;無 active L1 則 no-op。每條帶 citation `topic`,詳情走 `codeforge memory search`(progressive disclosure)。`codeforge install --hooks`/`--all` 把它接進 global SessionStart(非 plugin,因 CC issue #16538)。對稱於中央的 `mnemos-cli context`。完成記憶迴圈 absorb→distill→store→**recall** 的 READ 側。偷自 prior art(claude-mem lean-index/citation、SuperBrain「injection not storage」)。
- `doc/specs/codeforge-memory-contract.md` — 共享 state(L0/L1/L2/improvement-queue)的 producer/consumer 明文契約 + L1 frontmatter schema(含 reserved `nature` 欄位)+ READ-path 注入契約。

### Removed

- `src/projection/`(`project_to_claude_memory`)死碼 —— 機制錯(寫進 Claude 自有的 auto-memory 目錄,丟入檔不可靠載入),由 `codeforge memory context` 的 SessionStart additionalContext 注入取代。

### Changed

- **dream compile 改為對賬式,不再盲目同 slug 覆蓋** — 新事實落在新 slug 時會先和既有相關條目對賬(T2.2);同 slug 仍 in-place 合併,且合併後一律回到 `active`(新訊號落在 superseded slug 視為「重新觀察」而復活,不會把新知識寫進看不到的死檔)。無行為破壞、無需重裝 hook;`memory context` 的浮現順序會因 T2.3 三因子排序而改變(近期被引用的條目排更前)。
- **記憶 pipeline 跨專案上線** — `codeforge dream --quiet` → `codeforge ship --no-hook` 的 SessionEnd 鏈從 codeforge-clone-only 的 `--project-hooks` 移到 global `--hooks`/`--all`,在每個專案 session 結束都 per-project 萃取（hook CWD = 專案 root → per-cwd `.codeforge`）。`--project-hooks` 因 `ensure_in_codeforge_repo` 只能在 clone 跑,無法覆蓋其他專案;唯有 global 路徑能跨專案。`--project-hooks` 現只保留 dev scripts（check-improvements / check-dev-flow）。
- `codeforge install --hooks`/`--all` 現會寫入 `cleanupPeriodDays: 3650`(只填未設值,`--force` 才覆蓋使用者選擇)。Claude Code 預設 30 天回收 session transcript,會在 dream/ship 萃取前刪掉;拉高留存讓 raw material 存活。
- **LLM backend 改為 fallback 鏈(新增 `src/llm.rs`)** — dream/ship/commentary 的 digest 不再「Haiku-or-passthrough」。新鏈:`claude -p` headless(Claude Code CLI,免 API key,預設 Opus,品質最高;`CODEFORGE_DIGEST_MODEL` 可改)→ `ANTHROPIC_API_KEY`(直連 Haiku API)→ rule-based passthrough。`ANTHROPIC_API_KEY` 因此為 optional,只是第二層 fallback。每層降級印 warning。

### Added

- **session-digest 落地重設計(A′)+ per-repo opt-out** — `session-digest.js` 不再倒進全域 `~/.claude/session-digests/`(明文、歸屬錯),改從 cwd 往上找 `.codeforge` 落 per-repo `<repo>/.codeforge/digests/`,找不到即 skip 不寫(非 codeforge 專案零明文落盤)。`dream ingest-digests` 改讀 per-repo dir + 吸完刪檔(明文不長存),過渡期同讀舊全域路徑。新增 `<repo>/.codeforge/no-ship` per-repo opt-out 檔(`ingest` skip + `ship` 硬 no-op)。

- `codeforge ship --no-hook` opt-in gate（`MnemosConfig::opted_in`):僅當 `~/.config/mnemos.env` 存在或 `MNEMOS_INGEST_URL` 設定才送。沒有 Mnemos 的 codeforge-only 使用者照常用 dream 萃取 L1,ship 變乾淨 no-op — 不再每次 session-end 往 `ship-failed/` 堆 dead-letter。互動式 `codeforge ship` 不受 gate(明示動作)。

- `codeforge mnemos-cli context --with-themes` flag — 送 `include_themes=true` 給 Mnemos,在 atoms 前注入一段 theme-summary 區塊。

### Changed

- **收嚴 ledger source** — ship 的 L2 ledger 來源斷開 `absorb`(跨專案吸收的二手知識)、改接 `session-digest`(本 repo 第一手),加上 origin-purity 過濾(只送 `origin != absorbed`)+ idempotency / confidence gating。確保送進 Mnemos 的是本 repo 第一手 coding 經驗。

### Fixed

- `patch_hooks` 改成「先全面 sweep 所有 hook_type 的 codeforge group → 加入當前 entries → collapse 空 array」,讓 hook entry 可跨 hook_type / scope 搬移而不留孤兒。一併修掉:(1) `--project-hooks` re-run 漏 model 非 script 的 dream SessionEnd 條目而 drop/duplicate;(2) pre-marker 安裝的 node hook scripts(`hooks/0.0.3/…` 等無 `_installed_by` marker)在升級 re-install 時與新版並存導致 dual-fire — `is_legacy_codeforge_command` 現以 codeforge scripts 路徑 + 已知 basename 辨識並 sweep。

- **signal_cursors 跨 repo key 碰撞**(上線 blocker) — 全域 `state.db` 的 `signal_cursors` 原以 filename 為 key,多專案共用 `CODEFORGE_DIR` 時不同 repo 的同名日期檔會互相覆蓋 cursor → 漏編譯或重複。改為以 repo-qualified key 區隔。

## [0.0.4] - 2026-06-10

### Added

- `codeforge ship` — CodeForge becomes Mnemos's first streaming source. Digests the day's L1 concepts + git log into an L2 ledger (Haiku `claude-haiku-4-5-20251001`, or rule-based passthrough when no `ANTHROPIC_API_KEY`) and POSTs it to `POST /v1/ingest/ledger`. ULID `ship_id` idempotency (retry never mints a new ULID), 1s/5s/30s backoff, 4xx never retried, failures queued to `~/.codeforge/ship-failed/<ship_id>.json`, `ship-state.json` prevents same-day re-ship. Flags: `--date`, `--dry-run`, `--no-hook` (single-attempt, never blocks SessionEnd), `--resend` (flush queue only). Spec: `doc/specs/codeforge-ship.md`; Mnemos contract `mnemos:docs/specs/10-source-contract.md` §5.1.
- `codeforge mnemos-cli context [--topic][--max][--max-sensitivity]` — fetches relevant atoms from `GET /v1/atoms/context` and formats them as markdown for SessionStart injection. Topic auto-derived from git branch + recent commit subjects when omitted. Degrades gracefully (warn + empty block, exit 0) when Mnemos is unreachable.
- `codeforge mnemos-cli cite <atom_id> [--matched-text]` — citation write-back to `POST /v1/atoms/:atom_id/cite` (§11.1 envelope: ULID `cite_id`, RFC3339 `cited_at`, `client=codeforge_ship/<version>`, `evidence.method=fulltext_match`).
- `codeforge mnemos-cli cite-detect <transcript> [--topic][--max]` — SessionEnd heuristic: full-text matches candidate atom titles against a transcript and cites each hit (high false-positive rate accepted for Sprint 1; Sprint 5+ moves to Haiku judgement).
- Endpoint/auth resolution reads `~/.config/mnemos.env` (`MNEMOS_INGEST_URL` / `MACHINE_ID` / `MNEMOS_TOKEN`), falling back to `http://127.0.0.1:8845`.

## [0.0.3] - 2026-05-15

### Added

- `codeforge install --project-hooks` + `codeforge uninstall` + `--dry-run`, `--force`, `--yes`, `--settings-path`, `--quiet` flags (full V2.2 surface per `doc/specs/codeforge-install-subcommand.md`). Marker-driven uninstall (`_installed_by: codeforge@<ver>`) preserves user-owned settings.
- `.claude/settings.json` rewritten to use `${CLAUDE_PROJECT_DIR}` (Claude Code's documented project-root env var) instead of hardcoded paths — checked-in file is now portable across every clone, no post-clone path-rewrite needed. Models the [Husky](https://github.com/typicode/husky) / [pre-commit](https://pre-commit.com/) "committed config + relative paths" pattern. `codeforge install --project-hooks` likewise writes `${CLAUDE_PROJECT_DIR}` paths, producing byte-identical commit-friendly output across machines.
- Statusline V2 — unified visual language across minimal & full modes. `▏▕` chip wrappers replaced by `──` dim separators (matches box-drawing language). Git ahead/behind indicator `⇡N⇣M` (powerlevel10k-style). Claude Code version + update banner in both modes. Onboarding hint `→ codeforge adopt` always visible in minimal mode (degrades to `→ adopt` on narrow terminals). Context-pressure dimming when ctx > 80% — Tier-3 elements (separators, box-drawing) shift dimmer so the chat regains visual priority.
- Statusline V2 (full mode): village name collapsed into top border (6→5 rows). ASCII art column auto-drops below 90 cols. `HP/XP` → `♥/✦` symbols. Colon-less stat labels (`ATK 18` not `ATK: 18`). Strategy as `▸ agg` mode indicator. `Memory: active` → k9s-style `memory ● active`. i18n (`en.yaml` + `zh-TW.yaml`) updated to match.

### Fixed

- Project `.claude/settings.json` no longer dual-fires emit-session / session-digest hooks when `codeforge install --all` is run (clone-only scripts stay in project layer; product-wide scripts live in `~/.claude/`). README "First-time Claude Code hook setup" §Layer 1 / Layer 2 documents the split.
- `codeforge dream --quiet` SessionEnd hook restored in project settings (V2.2 portability rewrite had silently dropped it because it's not a `.claude/scripts/*.js` entry).

## [0.0.2] - 2026-05-15

### Added

- `codeforge install` subcommand wires `~/.claude/settings.json` automatically with the binary's absolute path (statusLine MVP). Solves the rustup `--no-modify-path` + dotfiles-without-cargo-block silent-failure case. See `doc/specs/codeforge-install-subcommand.md` for the `--hooks` follow-up plan.
- `codeforge install --hooks` installs global-safe hooks; `--all` installs both statusLine + hooks.
- `doc/getting-started.md` — 5-step linear quickstart (clone → install → wire → init → adopt → verify) with a troubleshooting matrix.
- Distribution scaffolding: `[package.metadata.binstall]` for `cargo-binstall`, release profile (strip + LTO + codegen-units=1), declared MSRV `rust-version = "1.85"` in Cargo.toml, `CHANGELOG.md`, `release.toml` for `cargo-release`. See `doc/specs/codeforge-release-pipeline.md` for the full release pipeline plan.
- GitHub Actions workflows: `release.yml` (4-target matrix build on tag push; cross-compile aarch64-linux; macOS Intel + M-series), `release-smoke.yml` (PR-gate tarball assembly).

## [0.0.1] - 2026-05-15

Initial development version. Phases 1 through 3f shipped; see `README.md` Phase Roadmap.
