# Environment — CodeForge

<!-- last-verified: 2026-04-16 -->

## codeforge init 目錄

**Date**: 2026-04-14
**Problem**: 在 `/home/codepower/projects/codepower/` 執行 `codeforge init` 會把 `.codeforge/` 建在 CodePower 目錄裡，不是目標專案。
**Solution**: 先 `cd /home/codepower/projects/<target-repo>/` 再執行 `codeforge init`，或用 `CODEFORGE_DIR` env var 指定。

## codeforge git repo 需要獨立 git config

**Date**: 2026-04-14
**Problem**: `/home/codepower/projects/codeforge/` 是新 git repo，沒有繼承 global git config 的 user.email/name，執行 `git commit` 會報 "Author identity unknown"。
**Solution**: 
```bash
cd /home/codepower/projects/codeforge
git config user.email "cookys@example.com"
git config user.name "cookys"
```
或設定 global：`git config --global user.email "..."` + `git config --global user.name "..."`
