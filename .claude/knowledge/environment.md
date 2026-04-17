# Environment — CodeForge

<!-- last-verified: 2026-04-17 -->

## zsh 報 `permission denied: /tmp/claude-XXXX-cwd`

**Date**: 2026-04-17
**Problem**: Bash 偶發出現 `zsh:1: permission denied: /tmp/claude-532a-cwd`（cross-session），整段指令異常中止、exit 1。
**Root cause**: 本機 `/tmp/claude-*-cwd` 由 user `twgs-dev` 擁有（664 twgs-dev），Claude Code harness 用這些檔案追蹤 cwd；當 harness 想寫回 cwd 檔時遇到跨 user 權限衝突。環境特定（共用機器），其他純個人機不會遇到。
**Solution**: 這是 harness 層級的訊息，使用者工作流不受影響，可忽略。不要把它當成指令本身的失敗——若看到該訊息跟著非零 exit code，真正的錯誤通常在前幾行。檢查實際指令的 stdout/stderr，不是這行 cwd 提示。
**Signature**: `zsh:1: permission denied: /tmp/claude-[0-9a-f]+-cwd`

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
