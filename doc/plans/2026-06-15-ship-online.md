# Plan — ship 上線（codeforge ship + mnemos-cli cite 端到端）

> Created: 2026-06-15 · Owner: cookys · Size: L · Branch: `feature/ship-online`

## 背景

origin/main 剛 merge 進整個 `src/mnemos/` 模組 + `src/cli/ship.rs` + `src/cli/mnemos_cli.rs`（10 commits）。
code 已編譯過、已接進 CLI dispatch（`src/cli/mod.rs`）。但「上線」最後一哩尚未完成：

1. **SessionEnd hook 鏈 dream → ship 尚未接** — 專案 `.claude/settings.json` 只有 `codeforge dream --quiet`，後面沒 chain `ship`。
2. **未做過真 e2e** — Mnemos server 沒驗證過收得到 POST（contract 對不對得上）。
3. **cleanupPeriodDays 每台手動** — 應收進 `codeforge install`。

觸發脈絡：發現 Claude Code 預設 30 天回收 session transcript（已先把 `~/.claude/settings.json` 的 `cleanupPeriodDays` 拉到 3650 止血）。本機 dream(L0→L1) 知識淬鍊本來就在跑、不依賴 Mnemos；ship 是「匯流到中央 brain」的加值層。

## Final Goal

> codeforge ship + mnemos-cli cite 在 SessionEnd hook 鏈端到端跑起來，本機知識匯流進 Mnemos；cleanupPeriodDays 收進 install，每台 install 即受保護。

## Success Criteria（可量化）

| # | 條件 | 驗證方式 |
|---|------|---------|
| 1 | `codeforge ship` 產出的 ledger envelope POST 到本機 `mnemos serve` `/v1/ingest/ledger` 回 **200 / accepted** | 啟 `~/projects/mnemos` server 實測，看 HTTP code + Mnemos log `event=ingest source=codeforge_ledger` |
| 2 | `codeforge mnemos-cli cite <atom_id>` 回 **200**，Mnemos `citation_count` +1 | 實測 POST + 查 Mnemos 端計數變化 |
| 3 | `codeforge install`（對應分支）寫出的 SessionEnd hook 鏈含 `codeforge ship --no-hook` | install 單元測試斷言 hook 結構（`cargo test` 綠）|
| 4 | `codeforge install` 寫出的 settings.json 含 `cleanupPeriodDays` | install 單元測試斷言 |
| 5 | 全綠 | `cargo check && cargo clippy && cargo test` 零失敗 |
| 6 | global-vs-project hook 放置點以證據定論 | 記錄在本 plan 的「Open Question」段，附判定依據 |

## Scope Boundary

- **IN**: ship/cite e2e 實測（對既有 merged code）、install hook-chain 接線、cleanupPeriodDays into install、e2e 揭出的 contract 修正、CLAUDE.md / `doc/specs/codeforge-ship.md` doc sync。
- **OUT**: Mnemos 端 server code 變更（mnemos repo 的事，只啟動驗證）、Haiku-based cite detection（Sprint 5+）、retry policy 重寫（已 merge）、ship payload 欄位重設計（spec 已定案）。

## Open Question（P1 用證據定論）

**SessionEnd 的 dream → ship 鏈該放 global(`~/.claude/settings.json`) 還是 project(`.claude/settings.json`)?**
- 目標是「從**各** codeforge 蒐集」→ ship 要跑遍所有專案 → 傾向 **global**。
- 但 install.rs 目前把 `codeforge dream` 放 project-hooks 分支（codeforge-clone-only），global 分支是 emit-session/session-digest node scripts。
- P1 先讀「dream 目前實際在哪個 scope 被觸發」再決定，不靠猜。需確認 ship 跨專案跑時 `CODEFORGE_DIR` / per-cwd `.codeforge` 的解析行為。

## Phases

- **P0 — e2e 地基驗證**：啟 `mnemos serve`；`ship --dry-run` 檢視 envelope 對齊 contract §5.1；真 `ship` POST → 200；`mnemos-cli cite` → 200。修 e2e 揭出的 contract mismatch。
- **P1 — install hook 鏈**：`install.rs` SessionEnd 接 `dream → ship --no-hook`（解 Open Question + 加 install 單元測試）。
- **P2 — cleanupPeriodDays into install**：install 寫 settings.json 時補 `cleanupPeriodDays`（+ 單元測試）。
- **P3 — live 驗證 + doc sync**：實跑 install 看活的 hook 鏈會 fire；CLAUDE.md / spec doc sync。

## Quality Gate

`cargo check + cargo clippy + cargo test` → `superpowers:requesting-code-review`（`src/cli/` 命中 routing）→ 修 CRITICAL/IMPORTANT → round 2 → merge。
