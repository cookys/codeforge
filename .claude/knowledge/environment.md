# Environment — CodeForge

<!-- last-verified: 2026-05-05 -->

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
