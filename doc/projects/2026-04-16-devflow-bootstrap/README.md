# Dev-Flow Bootstrap — Codeforge Autonomous Project Infrastructure

> 建立：2026-04-16
> Branch：`feature/devflow-bootstrap`
> CEO Mode：Level 3（Just results）

## Goal

補齊 codeforge 與 CodePower 同等的 dev-flow 品質保障基礎設施。

## Success Criteria

| KR | 驗證方式 | 狀態 |
|----|---------|------|
| SessionEnd hook 正確觸發 `codeforge dream` | hook 執行不報錯 | planned |
| session-digest.js 在 PreCompact/SessionEnd 執行 | 產生 digest JSON | planned |
| check-improvements.js 在 SessionStart 執行 | 警告出現在 session start | planned |
| L-workflow task dependency chain 在 dev-flow-config | 規格完整 | planned |
| Review loop 強制機制在 dev-flow-config | 有 addBlockedBy 規格 | planned |
| 最小 test suite pass（cargo test） | XP overflow + DB schema | planned |
| doc/BACKLOG.md 存在並有 Phase 1 deferred items | 檔案存在且格式正確 | planned |
| doc/plans/INDEX.md 存在 | 檔案存在 | planned |
| CLAUDE.md 有 CodePower ↔ Codeforge 互動模式 | 閱讀確認 | planned |
| .env.example 存在 | 檔案存在 | planned |
| cargo clippy 加入 quality gate | dev-flow-config 更新 | planned |

## Phases

| # | Phase | Status | Commit |
|---|-------|--------|--------|
| P1 | Critical fixes（--quiet flag + archive dir） | planned | — |
| P2 | Session hooks（scripts + settings.json） | planned | — |
| P3 | dev-flow-config 完整化（task chain + review enforcement） | planned | — |
| P4 | 補充設定檔（quality-gate + learn + skill-routing） | planned | — |
| P5 | Docs（BACKLOG + plans/INDEX + .env.example） | planned | — |
| P6 | Tests（pet/state XP overflow + db schema） | planned | — |
| P7 | CLAUDE.md 更新（CodePower 互動 + 完整 conventions） | planned | — |
| QG | Quality Gate + Code Review | planned | — |
