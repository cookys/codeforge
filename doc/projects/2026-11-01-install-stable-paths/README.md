# install-stable-paths — install／bootstrap 對共用 settings.json 安全（v0.0.7）

計畫：[`doc/plans/2026-11-01-install-stable-paths.md`](../../plans/2026-11-01-install-stable-paths.md) · BACKLOG：B35 · Branch：`feature/install-stable-paths`

## Project Goal

> **Final goal**: 在共用（symlink 或進 git 版控）的 settings.json 上跑 `codeforge install`／`bootstrap`，寫出的內容在每台機器、每個版本都一樣，而且寫進共用檔本身；發布 v0.0.7 讓 7840hs 安全執行 bootstrap。
> **Success criteria**: 計畫 §2 的 KR1–KR8，每條都有門檻與驗證方式。
> **Scope boundary**: 包含 install.rs 的 hook／statusLine command、marker、symlink 寫入、共用檔偵測與 `cleanupPeriodDays` 規則、新的 `codeforge hook` 子命令、文件與 v0.0.7 發布；不含 `--project-hooks`、舊快取清理、B31、任何機器上實際跑 install／bootstrap。

## Progress

| Phase | Size | Status |
|---|---|---|
| Plan + hetero plan review | — | 進行中 |
| P1 寫入穿透 symlink + 共用檔偵測 | S | — |
| P2 `codeforge hook` + 不帶路徑與版號的 command + 固定 marker | M | — |
| P3 `cleanupPeriodDays` 依共用與否決定 | S | — |
| P4 文件、版本、發布 | S | — |

Last updated: 2026-11-01
