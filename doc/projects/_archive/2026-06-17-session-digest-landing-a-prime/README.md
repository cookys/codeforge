# session-digest 落地重設計 A′ + per-repo ship opt-out

> 建立:2026-06-17 · branch `feature/session-digest-landing-a-prime`
> 規格來源:`mnemos/docs/projects/HANDOFF-ledger-backfill.md` §「session-digest 落地重設計(A′)」(2026-06-17 定案)
> 架構原則:codeforge = 單機自足 client;mnemos = 私人單一腦、非中央依賴。ship 是可選 best-effort adapter。

## Project Goal

> **Final goal**:session-digest hook 不再把全 session 萃片段倒進全域 `~/.claude/session-digests/`;改為**只在 init 過 `.codeforge` 的 repo 落盤**到該 repo 的 `.codeforge/digests/`,ingest 吸入後**刪檔**;並提供 `<repo>/.codeforge/no-ship` per-repo opt-out。
> **Scope boundary**:
> - include:`session-digest.js`(落點+往上找+skip+cleanup)、`ingest_digests.rs`(per-repo dir + 移 cwd filter + ingest 完刪檔 + 過渡同讀舊路徑)、no-ship opt-out(`ship.rs` + `ingest_digests.rs`)、`.gitignore`、spec §5.2。
> - exclude:cite 回寫(parked)、SessionEnd-vs-cron ship 路徑協調(另案)、day-1 backfill(gated 另案)、digest JSON schema(不動,鬆耦合 contract)、`improvement-queue` 那條線(gate 在 `cwd/.claude/`,別動到)。

### Success criteria(可量化)

1. **未 init repo 不落盤**:在無 `.codeforge` 的目錄跑 `session-digest.js`(餵假 transcript)→ 全域與任何 `.codeforge/digests/` 皆無新檔。驗:`find ~ -path '*/.codeforge/digests/*.json' -newer <ts>` 為空 + `~/.claude/session-digests/` 無新檔。
2. **init'd repo 落 per-repo**:在有 `.codeforge` 的 repo 跑 → digest 出現在 `<repo>/.codeforge/digests/<date>-<sid8>.json`,不在全域。
3. **ingest 完刪檔**:`codeforge dream`(ingest-digests)跑完 → 該 repo `.codeforge/digests/` 對應檔被刪除(非僅標 processed)。
4. **no-ship opt-out**:`<repo>/.codeforge/no-ship` 存在 → ingest-digests 不吸該 repo digest 且 `codeforge ship` 直接 no-op(POST 0 次)。
5. **過渡相容**:舊 `~/.claude/session-digests/<date>-*.json`(帶 cwd)仍被 ingest 以 cwd filter 讀入(讀完刪),不漏既有資料。
6. **回歸**:`cargo build && cargo test && cargo clippy -- -D warnings` 全綠;新增單元測試覆蓋 walk-up / skip / per-repo dir / no-ship。
7. **§6 獨立 review**:改 hook 落點 + ingest 屬 capture 行為,merge 前過 `autopilot:quality-pipeline` / 獨立 reviewer,commit 標 `fix(review):`。

## Phases

| # | Phase | 檔 | 狀態 |
|---|-------|----|------|
| P0 | session-digest.js 落點重設計(walk-up `.codeforge`、per-repo `digests/`、找不到 skip、cleanup per-repo) | `.claude/scripts/session-digest.js` | ✅ `be6ec26` |
| P1 | ingest_digests.rs per-repo dir + 移 cwd filter + ingest 完刪檔 + 過渡同讀舊路徑 | `src/dream/ingest_digests.rs` | ✅ `1529e42` |
| P2 | per-repo no-ship opt-out(ship 端 no-op + ingest 端 skip) | `src/cli/ship.rs`, `src/dream/ingest_digests.rs` | ✅ `8ed4e15` |
| P3 | `.gitignore` + spec §5.2 路徑更新 | `.gitignore`, `doc/specs/codeforge-ship.md` | ✅ `fd78e8b` |
| P4 | §6 獨立 review + merge(finish-flow) | — | ✅ review `aac3a7f` / merge `443e31d` |

## 完成(2026-06-17)

merged 到 main `443e31d`(--no-ff)。770 tests 綠。§6 獨立 review:零 Critical,2 Major
(刪檔競態 → 原子寫+mtime guard;medium 銷毀 → 駁回建議保留+註解)+ Minor 處置完,
`fix(review): aac3a7f`。success criteria 1-6 e2e/單元實證 PASS,7(review)PASS。

**部署待辦(L Session End / 各機)**:① 主機 `cargo install --path .` 重裝 binary;
② reinstall global hooks(讓新 session-digest.js 生效;舊 0.0.4 仍寫全域 → 過渡路徑會吸);
③ 過渡期觀察後砍舊全域 `~/.claude/session-digests/`;④ no-ship opt-out 逐機各自設。

## 接手鐵律(本專案)

- 動工前 fact-check code 現實(已做:A′ 6 點全對得上)。
- 改 LLM/capture 行為 → §6 獨立 review 才 merge,review commit 標 `fix(review):`。
- 別碰 `improvement-queue` 邏輯(另一條線,gate 在 `cwd/.claude/`)。
- 同名 repo 用完整路徑識別,別用 basename。
- git worktree:linked worktree 往上找不到主 `.codeforge` 會漏(已知邊界,記之)。
