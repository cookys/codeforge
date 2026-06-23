# Environment — CodeForge

<!-- last-verified: 2026-06-22 -->

## rustfmt drift — format via pinned `scripts/fmt.sh`, never bare `cargo fmt`

**Date**: 2026-06-23
**Problem**: 14 files silently drifted out of fmt-compliance. Root cause: both
local default and CI used a *rolling* `stable` rustfmt; code committed under an
older stable was re-wrapped (`format!`/`say()`/`assert_eq!` long-line wrapping)
by a newer stable. RFC 2437 explicitly refuses formatting stability *across*
toolchain versions — only *within* one version. So unpinned stable + "nobody
re-ran fmt" = accumulating drift; CI `fmt` job goes red on the next stable bump.
**Solution**: `scripts/fmt.sh` — single source of the rustfmt toolchain version
(`PIN`), self-installs it, formats via `cargo +$PIN fmt` (`+toolchain` override =
highest rustup precedence → pins ONLY formatting). Build/test/clippy stay on
rolling stable; MSRV stays in `Cargo.toml` `rust-version` (NO `rust-toolchain.toml`
— a `channel` pin there would drag the whole build onto the fixed version, the
thing this repo avoids; and dtolnay/rust-toolchain doesn't read the toml anyway).
CI `fmt` job runs `./scripts/fmt.sh --check` (non-bypassable backstop); S/L/H
quality gates run it too. **Never run a bare `cargo fmt`** — it uses your default
toolchain's rustfmt and re-introduces drift. Bump `PIN` deliberately in a
dedicated commit that re-runs `./scripts/fmt.sh` repo-wide.
**Sibling drift — clippy (handle differently: fix-forward, do NOT pin)**: the same
rolling-stable bump that drifted fmt also surfaced 5 new `cargo clippy -D warnings`
errors on previously-clean code (2× `nonminimal_bool`, 1× `trim_split_whitespace`,
2× `incompatible_msrv`). Unlike fmt, lints must be **fixed forward**, never pinned to
an old clippy — new lints are valuable. Note the MSRV ones were a *real* latent bug:
`u64::is_multiple_of` is stable only since 1.87 but Cargo.toml declares MSRV 1.85, so
the code wouldn't compile on 1.85/1.86; fixed with `% n == 0` (keeps the 1.85 claim
true). clippy reads `rust-version` from Cargo.toml, so the CI `clippy` job IS the MSRV
guard. Build/clippy stay on rolling stable on purpose — only fmt is pinned.
**Related**: deterministic-gate family — see `gate-patterns.md` (确定性 > 記憶);
fmt is now a script-driven deterministic gate like `check-doc-drift.py` / `check-cjk-safe.sh`.

## `install --all` 在 statusLine 衝突時會整個 bail（hooks 沒裝）

**Date**: 2026-06-23
**Problem**: `install::run` 先 patch statusLine、再 patch hooks，settings 最後才原子寫入。
若使用者已有非-codeforge 的 statusLine 且沒帶 `--force`，`patch_statusline` 會在 hooks
區塊**之前** `bail!` → 整個 `install --all` 中止，**global hooks（dream→ship / recall /
cleanupPeriodDays）一個都沒裝**。`codeforge bootstrap` 第一版直接用 `install --all`，在
這個常見情況下等於沒裝 pipeline，卻只報一個軟 warning（review 抓到的 MAJOR）。
**Solution**: 在 install 上組合時，若要 hooks 一定要落地、又不想 clobber 既有 statusLine：
`install --all` 失敗就 fallback `install --hooks`（hooks-only 不碰 statusLine）。bootstrap
step 1 即如此（`src/cli/bootstrap.rs` step1_lines）。warning + `--force` 只針對 statusLine。
**Related**: best-effort orchestrator 設計 —— 每步錯誤隔離、報告後續跑、最後彙整 needing-attention。

## `~/.cargo/bin` not on PATH for Claude Code spawned shells

**Date**: 2026-05-15
**Problem**: `codeforge statusline` configured in `~/.claude/settings.json`
as `"command": "codeforge statusline"` silently fails for users who
installed rustup with `--no-modify-path` (or whose dotfiles lack a cargo
PATH block). The Bash tool spawns non-interactive shells that don't
fully source `~/.zshrc`, so `~/.cargo/bin` isn't on PATH → `codeforge`
not found → Claude Code shows no statusline at all (no error visible).
**Solution**: `codeforge install` (shipped this session) writes the
binary's absolute path via `std::env::current_exe()` into settings.json.
Works regardless of PATH.

## Hook scripts under `.claude/scripts/` have different scopes

**Date**: 2026-05-15 | **Re-verified/updated**: 2026-06-22
**Problem**: 4 scripts (`emit-session`, `session-digest`,
`check-improvements`, `check-dev-flow`) sit in the same dir but only 2
are project-agnostic. `check-improvements.js` and `check-dev-flow.js`
hardcode codeforge's repo layout and would noise-fail in any
non-codeforge session.
**Solution (as-shipped, verified 2026-06-22 against `src/cli/install.rs`)**:
- `codeforge install --hooks`/`--all` → **global** `~/.claude/settings.json`,
  across **3 hook types**: SessionStart (`emit-session` + `codeforge memory
  context --hook` local-recall injector), SessionEnd (`emit-session` +
  `session-digest` + `codeforge dream --quiet` → `codeforge ship --no-hook`
  memory pipeline), PreCompact (`session-digest`). Also writes top-level
  `cleanupPeriodDays = 3650`.
- `codeforge install --project-hooks` (**shipped**, not "planned") → the 2
  clone-only dev hooks: `check-improvements` (SessionStart) +
  `check-dev-flow` (PreToolUse), to `$CWD/.claude/settings.json`.
- `codeforge uninstall` reverses both (top-level subcommand, not a flag).

## codeforge init 目錄

**Date**: 2026-04-14
**Problem**: 在 sibling repo（例如 `~/projects/<other-repo>/`）執行 `codeforge init` 會把 `.codeforge/` 建在那個 repo，不是目標專案。
**Solution**: 先 `cd ~/projects/<target-repo>/` 再執行 `codeforge init`，或用 `CODEFORGE_DIR` env var 指定。

## codeforge git repo 需要獨立 git config（⚠️ SUPERSEDED 2026-06-22）

**Date**: 2026-04-14 | **Superseded**: 2026-06-22
**Problem**: 第一次 clone 後 repo 無 `user.email`/`user.name`，`git commit` 報 `Author identity unknown`。
**❌ 舊解（不要用）**: `git config user.email "..."` 改 repo config —— 會跨 session 持續、且 codeforge 需要 noreply email（非個人 email）。
**✅ 現行解**: 用 per-command `-c` override，見下方「Per-command git identity override」條目。codeforge commit 一律 `git -c user.email=2537196+cookys@users.noreply.github.com -c user.name=cookys commit ...`（GitHub email privacy 會擋 gmail）。**不改 config。**

## `gh repo create --push` 含 GitHub Actions workflow 需要 `workflow` scope

**Date**: 2026-05-05 | **Context**: 第一次把 codeforge repo push 到 public GitHub
**Problem**: `gh repo create cookys/codeforge --public --source=. --remote=origin --push` 建好 repo 但 push main 被拒：
```
! [remote rejected] HEAD -> main (refusing to allow an OAuth App to create or update workflow `.github/workflows/ci.yml` without `workflow` scope)
```
gh CLI 預設 OAuth token scopes 是 `repo, gist, read:org` — 沒有 `workflow`。GitHub 對 `.github/workflows/*` 強制要求專用 `workflow` scope。
**Solution**: 互動式刷新加 scope（user 自己跑，因要瀏覽器 + one-time code）：
```bash
gh auth refresh -s workflow
# → 印 one-time code → 開瀏覽器 → 確認 → Authentication complete
git push -u origin main && git push origin <tag>
```
**When this fires**: 任何 repo 含 `.github/workflows/*.yml` 第一次 push 到 GitHub。Repo 已建（即使 push 失敗）— 重 push 即可，不需重 create。

## Per-command git identity override（HARD RULE 合規 commit）

**Date**: 2026-05-15 | **Context**: Mnemos repo（剛 rename 自 personal-knowledge-base）首次 commit 時報 `Author identity unknown`。MEMORY.md HARD RULE「NEVER update the git config」禁止用 `git config user.email "..."` 解。
**Problem**: Fresh clone / 剛 rename 的 repo 沒有 git config，commit 失敗：
```
fatal: unable to auto-detect email address (got 'codepower@hostname.(none)')
```
傳統解法（修 repo-local config）違反 HARD RULE — config 一改就跨 session 持續，且不同 repo 該用不同 identity（PKB 用 `cookys@stranity.com`、codeforge 用 `2537196+cookys@users.noreply.github.com`）。
**Solution**: 用 `-c` 在 commit 那條 command 上臨時 inject identity，不寫進 config：
```bash
git -c user.name=cookys -c user.email=cookys@stranity.com commit -m "..."
```
每次 commit 都帶 `-c`，hash 出來的 author 就是當下指定的 identity，repo config 始終保持原狀。
**When this fires**:
- 任何剛 clone / 剛 rename / 剛 init 的 repo，**第一次 commit 之前**
- 多 repo 各自不同 identity 的場景（GitHub no-reply email vs personal email vs work email）
- 若 user 明確要求 "set git config for this repo permanently"，才允許 modify config — 預設一律走 `-c` override
**⚠ codeforge 必須用 noreply email**（2026-06-17 踩到）：codeforge push 到 `github.com/cookys/codeforge` 時，GitHub 開了 **email privacy**，用 `cookys@gmail.com` commit 會在 `git push` 被擋（`push declined due to email privacy restrictions`）。**commit 當下就要帶** `-c user.email=2537196+cookys@users.noreply.github.com`（origin 既有 commit 全用此 email，`git log origin/main --format=%ae` 可確認）。若已用 gmail commit 了才發現，補救：`FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch -f --env-filter '...gmail→noreply...' origin/main..HEAD` 重寫該範圍 author/committer email 再 push。
**Related**: `codeforge git repo 需要獨立 git config` 條目歷史紀錄；MEMORY.md HARD RULE「NEVER update the git config」

## `git tag v*.*.*` 會觸發 release.yml 發布 pipeline（不可逆對外動作）

**Date**: 2026-06-22 | **Context**: doc-drift audit 抓到 version-sync drift（Cargo.toml 0.0.5 但無對應 git tag / CHANGELOG section），一度想打 `v0.0.5` tag「對齊」
**Problem**: `.github/workflows/release.yml` 觸發是 `on: push: tags: 'v*.*.*'` —— 推一個 `vX.Y.Z` tag 會**啟動全平台 build + 發 GitHub release**（對外、不可逆）。為了清 doc-audit 的「無 tag」findings 而打 tag，會誤觸發真正的 release。
**Solution**: **不要為了文件一致性打 release tag**。改用 doc 揭露 pending 狀態（CHANGELOG `[Unreleased]` 註明「Cargo.toml 已 bump 到 X.Y.Z、未 tag」）。tag 留給真正發版的刻意動作。autopilot 同理（也 release-on-tag）。
**When this fires**: 任何「想打 tag 對齊版號 / 補歷史 tag」的念頭 —— 先 `grep -A3 "^on:" .github/workflows/release.yml` 確認觸發條件。
**Related**: doc-drift system（auto-memory `reference_doc_drift_system.md`）；`version-sync` 是 `scripts/check-doc-drift.py` 的檢查項
