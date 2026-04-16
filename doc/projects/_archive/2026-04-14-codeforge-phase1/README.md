# CodeForge Phase 1 — Memory CLI + Common Pet

> 建立：2026-04-14
> 歸檔：2026-04-16
> Branch：`feature/codeforge-phase1` (merged + deleted)

## Goal

建立 CodeForge Phase 1：跨 session 記憶管理 CLI + 遊戲化寵物陪伴系統基礎。

## Success Criteria (All PASS)

| KR | 驗證 | 狀態 |
|----|------|------|
| `codeforge learn "text"` 儲存記憶 | `codeforge memory search` 找得到 | ✅ PASS |
| `codeforge dream` 執行 compile/lint/dedup/absorb/decay/track | 無錯誤退出 | ✅ PASS |
| `codeforge statusline` 輸出 ANSI 面板 | 6 行輸出，含 pet stats | ✅ PASS |
| `codeforge pet` 顯示寵物狀態 | 輸出含 level/xp/atk/def/sup/ver | ✅ PASS |
| `codeforge adopt` 選村落 + 寵物 | 互動流程完整 | ✅ PASS |
| SQLite WAL + busy_timeout=5000 | `PRAGMA journal_mode` = wal | ✅ PASS |
| CJK 安全截斷（.chars().take(N)） | 無 byte-index panic | ✅ PASS |
| XP overflow 保護 | u32::MAX 輸入不無限迴圈 | ✅ PASS |
| `codeforge dream --quiet` 靜默模式 | 無 stdout 輸出 | ✅ PASS |

## Phases

| # | Phase | Status | Notes |
|---|-------|--------|-------|
| P1 | 記憶系統（L0 JSONL + L1 SQLite FTS5） | ✅ done | |
| P2 | Dream cycle（compile/lint/dedup/absorb/decay/track） | ✅ done | |
| P3 | Pet system（state/xp/stats/village） | ✅ done | |
| P4 | Statusline（ANSI 6-line panel） | ✅ done | |
| P5 | CLI polish（adopt/ingest/search） | ✅ done | |
| QG | Code review + Phase 1 fixes | ✅ done | CJK truncation + XP overflow + --quiet flag |

## Key Commits

- `cc2e0ca` — Phase 1 完成 KR 全 PASS
- Phase 1 code review fixes: CJK truncation (4 files), XP overflow, dead code removal, busy_timeout

## Deferred to BACKLOG

- B1: Signal cursor race condition
- B2: CJK FTS5 tokenizer
- B3: Badge system
- B9: Clippy lint for CJK truncation
