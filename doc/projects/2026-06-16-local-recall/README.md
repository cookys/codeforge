# Project — 本地 recall 注入器(Phase A)

> Status: **Active** · Started: 2026-06-16 · Branch: `feature/local-recall` · Size: L
> Plan: [`doc/plans/2026-06-16-local-recall-phase-a.md`](../../plans/2026-06-16-local-recall-phase-a.md)
> Design spec: [`doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`](../../proposals/2026-06-16-memory-recall-and-stolen-patterns.md)

## Project Goal
> **Final goal**: codeforge 每個 session 開始自動把本地 top-N L1 知識(lean ranked index ~1.5K token)注入 context,免 mnemos、免 autopilot。
> **Success criteria**: 見 plan §Success Criteria(7 條)。
> **Scope boundary**: IN — 本地 `memory context` 命令、SessionStart hook 接線、projection 退役、seam 契約、procedural-atom 標記、doc sync。OUT — async worker / mem0 對賬(Phase B)、語意 recall(Tier 3)、skill-distiller(defer)。

## Scope Completeness Audit（L-1.5）
| Dimension | 命中 | 處置 |
|-----------|------|------|
| Source code + tests | ✅ | `src/cli/` 新 `memory context`、`src/memory/l1` rank/budget、`src/cli/install.rs` hook、`src/projection` 退役 |
| User docs | ✅ | CLAUDE.md READ/WRITE 對稱表 → A3 |
| Config templates | ✅ | install 寫 SessionStart entry |
| CHANGELOG / version | ✅ | A3(行為新增,[Unreleased]) |
| Migration | ✅ | projection 退役;install 重部署 |
| Credit / attribution | ✅ | design spec §7 已列 inspired-by;CLAUDE.md/spec 標註 |
| Dogfood | ✅ | 本機重裝驗 live 注入 |

## Progress
| Phase | 內容 | 狀態 |
|-------|------|------|
| A0 | `codeforge memory context`(rank + budget + citation + `--hook` JSON) | pending |
| A1 | install SessionStart 接線 | pending |
| A2 | projection 退役 + seam 契約 + procedural-atom 標記 | pending |
| A3 | doc sync + 重部署 + live 驗證 | pending |
