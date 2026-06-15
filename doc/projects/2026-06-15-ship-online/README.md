# Project — ship 上線

> Status: **Active** · Started: 2026-06-15 · Branch: `feature/ship-online` · Size: L
> Plan: [`doc/plans/2026-06-15-ship-online.md`](../../plans/2026-06-15-ship-online.md)

## Project Goal

> **Final goal**: codeforge ship + mnemos-cli cite 在 SessionEnd hook 鏈端到端跑起來，本機知識匯流進 Mnemos；cleanupPeriodDays 收進 install。
> **Success criteria**: 見 plan §Success Criteria（6 條可量化）。
> **Scope boundary**: IN — ship/cite e2e 實測、install hook 接線、cleanupPeriodDays into install、contract 修正、doc sync。OUT — Mnemos server code、Haiku cite detection、retry 重寫。

## Scope Completeness Audit（L-1.5）

| Dimension | 命中 | 處置 |
|-----------|------|------|
| Source code + tests | ✅ | `src/cli/install.rs`（hook 鏈 + cleanupPeriodDays + tests）；可能 `src/cli/ship.rs` / `src/mnemos/*`（僅 e2e 揭出 bug 才動） |
| User-facing docs | ✅ | CLAUDE.md（ship 上線狀態）、`doc/specs/codeforge-ship.md`（§11 e2e、§12 待實作打勾）→ P3 |
| CLI / interface ref | ➖ | ship/mnemos-cli 介面已定（已 merge），不變 |
| Config templates / examples | ✅ | `cleanupPeriodDays` 寫入 install 產生的 settings.json |
| CHANGELOG entry | ✅ | 上線是 release-worthy → P3 補 CHANGELOG |
| Version bump | ➖ | 視最終變更幅度於 P3 評估（install 行為變更可能 patch bump）|
| Migration / 跨 repo | ✅ | Mnemos server 啟動驗證（不改 Mnemos code）；contract 對齊 `~/projects/mnemos/docs/specs/10-source-contract.md` §5.1 |
| Dogfood target | ✅ | install 改完要在本機重跑驗證活 hook |

## Progress

| Phase | 內容 | 狀態 |
|-------|------|------|
| P0 | e2e 地基驗證（mnemos serve + ship/cite → 200） | ✅ done — ship POST→200 存 document+atom；cite→200 citation_count 0→1。精簡 payload 無 contract mismatch。隔離 test DB 驗證後清除 |
| P1 | install SessionEnd hook 鏈 dream → ship | ✅ done — dream+ship 移到 global SessionEnd（跨所有專案）；project-hooks 只剩 dev scripts；ship `--no-hook` opt-in gate（`~/.config/mnemos.env` 或 `MNEMOS_INGEST_URL`，否則乾淨 no-op 不堆 queue）；patch_hooks 改全面 sweep + collapse，順帶修 V2.2 已知 install bug #1/#2。31 install tests 綠 |
| P2 | cleanupPeriodDays into install | ✅ done — global install 寫 `cleanupPeriodDays: 3650`，只填未設值、--force 才覆蓋。3 新測試綠 |
| P3 | live hook 驗證 + doc sync（CLAUDE.md / spec / CHANGELOG） | ✅ done — 新 binary 裝到 ~/.local/bin(0.0.4);global `install --hooks` 上線 dream→ship 鏈(乾淨無 0.0.3 dupes);建 `~/.config/mnemos.env` opt-in;regenerate committed `.claude/settings.json`(移除 dream);部署中抓到並修掉 pre-marker node hooks dual-fire(legacy 辨識);CHANGELOG/CLAUDE.md/ship spec sync |

## Open Question 定論（P1）

dream→ship 放 **global**。證據:`--project-hooks` 因 `ensure_in_codeforge_repo` 強制只能在 codeforge clone 跑（install.rs:222），非 clone 專案根本無法裝 project-hooks；故跨專案萃取只能走 global SessionEnd。dream/ship 移到 global，project-hooks 只保留 dev scripts。ship 跨專案以 hook CWD=專案 root 解析 per-cwd `.codeforge`（P0 已驗 CODEFORGE_DIR 路徑可動）。

使用者意圖細化:**dream 普世(不需 Mnemos)、ship opt-in(需 Mnemos)**。ship `--no-hook` 自我 gate（`MnemosConfig::opted_in`），codeforge-only 使用者照常用 dream 萃取、ship 乾淨跳過。
