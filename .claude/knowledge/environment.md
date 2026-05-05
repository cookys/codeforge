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
