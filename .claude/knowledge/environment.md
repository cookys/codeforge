# Environment — CodeForge

<!-- last-verified: 2026-05-15 -->

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

**Date**: 2026-05-15
**Problem**: All 4 scripts (`emit-session`, `session-digest`,
`check-improvements`, `check-dev-flow`) sit in the same dir but only 2
are project-agnostic. `check-improvements.js` and `check-dev-flow.js`
hardcode codeforge's repo layout and would noise-fail or produce false
data when fired in any non-codeforge Claude Code session.
**Solution**: `codeforge install --hooks` only installs the 2
project-agnostic scripts (`emit-session` + `session-digest`) to
`~/.claude/settings.json` (global). The repo-specific ones stay in
`<codeforge>/.claude/settings.json` (project-scoped). V2.2 plan
in `doc/specs/codeforge-install-subcommand.md` adds `--project-hooks`
flag for installing all 4 to a target repo.

## codeforge init 目錄

**Date**: 2026-04-14
**Problem**: 在 sibling repo（例如 `~/projects/<other-repo>/`）執行 `codeforge init` 會把 `.codeforge/` 建在那個 repo，不是目標專案。
**Solution**: 先 `cd ~/projects/<target-repo>/` 再執行 `codeforge init`，或用 `CODEFORGE_DIR` env var 指定。

## codeforge git repo 需要獨立 git config

**Date**: 2026-04-14
**Problem**: 第一次 clone 後，repo 沒有繼承 global git config 的 `user.email` / `user.name`，執行 `git commit` 會報 `Author identity unknown`。
**Solution**:
```bash
cd ~/projects/codeforge
git config user.email "<your-email>"
git config user.name "<Your Name>"
```
或設定 global：`git config --global user.email "..."` + `git config --global user.name "..."`

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
