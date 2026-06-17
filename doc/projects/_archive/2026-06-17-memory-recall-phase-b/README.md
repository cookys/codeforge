# memory-recall Phase B (B16 / Tier 2 — T2.2 + T2.3)

> Size: **L** · Branch: `feature/memory-recall-phase-b` · Started: 2026-06-17
> Plan: [`doc/plans/2026-06-17-memory-recall-phase-b.md`](../../plans/2026-06-17-memory-recall-phase-b.md)
> Design spec 母本:[`doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`](../../proposals/2026-06-16-memory-recall-and-stolen-patterns.md)

## Project Goal

> **Final goal**:落地 Phase B 中兩件現在就有感、不碰熱路徑的偷 —— dream-compile mem0 式對賬(T2.2)+ 統一 ranking(T2.3) —— 讓記憶迴圈不累積 stale/矛盾、recall 浮現品質用 recency×importance×relevance 統一排序。
> **Success criteria**:見 plan SC1–SC3(三因子排序測試、ADD/UPDATE/DELETE/NOOP 對賬測試、`cargo test`+`clippy` 全綠)。
> **Scope boundary**:只做 T2.2 + T2.3。**T2.1(async worker)OUT → BACKLOG B16**(觸發條件未成立)。Tier 3 留 Phase C。

## Phases

| Phase | 內容 | Status |
|-------|------|--------|
| P0 | T2.3 統一 ranking — `recall::rank()` strength×recency×refs 三因子 | pending |
| P1 | T2.2 mem0 對賬 — compile.rs ADD/UPDATE/DELETE/NOOP + dedup 整合 | pending |
| QG | cargo check + clippy + test + requesting-code-review | pending |

## Decisions

- **2026-06-17**:T2.1 async worker defer 到 BACKLOG。daemon 不保證在跑、dream 跑遍所有 global SessionEnd → 純 enqueue 會回歸 ship-online 的「永遠 distill」保證;conditional+fallback 雖安全但紅利窄又動熱路徑,觸發條件(覺得卡)未成立。設計已收斂留檔在 plan「OUT OF SCOPE」段。

## Notes

- 既有 `src/dream/dedup.rs` + `decay.rs` 是 T2.2/T2.3 的整合點(避免兩套去重/strength 邏輯打架)。
- frontmatter 已有 `status`(active/superseded/archived)、`refs`、`last_ref` —— T2.2/T2.3 用既有欄位,預期不需新 migration。
