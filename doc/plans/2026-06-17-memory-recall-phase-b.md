# Plan — memory-recall Phase B (B16 / Tier 2)

> Created: 2026-06-17 · Size: **L** (risk-escalated: LLM prompt + memory data integrity) · Owner: cookys
> Design spec 母本:[`doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`](../proposals/2026-06-16-memory-recall-and-stolen-patterns.md) §3 Tier 2、§6 Phase B
> Phase A(本地 recall 注入器)已上線 2026-06-16(`_archive/2026-06-16-local-recall`)。

## Project Goal

> **Final goal**:落地 Phase B 中兩件**現在就有感、不碰熱路徑**的偷 —— dream-compile mem0 式對賬(T2.2)+ 統一 ranking(T2.3) —— 讓記憶迴圈 (a) 不累積 stale/矛盾、(b) recall 浮現品質用 recency×importance×relevance 統一排序。
> **Scope boundary**:只做 **T2.2 + T2.3**。**T2.1(async worker)本專案 OUT**(2026-06-17 user decision:觸發條件「SessionEnd 開始覺得卡」尚未成立,且紅利範圍窄又動剛上線熱路徑 → 留 BACKLOG,等真的卡再開)。Tier 3 留 Phase C。procedural-atom `nature` 欄位只在 frontmatter 留位,不做分類邏輯(Phase C)。

### Success criteria(可量測)

| # | Criterion | 驗證方式 |
|---|-----------|---------|
| SC1 | **T2.3** `recall::rank()` 用 recency×importance×relevance 三因子加權,`refs`/`last_ref` 進排序公式 | 新增單元測試:同 strength 但 refs 高者排前;近期 updated 者排前;有明確 fixture assert 排序 |
| SC2 | **T2.2** dream-compile 對新事實做 ADD/UPDATE/DELETE/NOOP 對賬,不再盲目同 topic-slug 覆蓋 | 測試:相同事實第二次 compile → NOOP(不新增檔);矛盾事實 → 舊條 status=superseded、新條 active;`cargo test` 綠 |
| SC3 | 全程不回歸:`cargo test` 全綠(現有 + 新增)、`cargo clippy` 無 warning | QG |

## Scope Completeness Audit(L-1.5)

| Dimension | Yes? | 處理 |
|-----------|------|------|
| Source code + tests | ✅ | `src/memory/recall.rs`(T2.3)、`src/dream/compile.rs` + 既有 `dedup.rs`(T2.2) |
| User-facing docs | ✅ | CLAUDE.md「Recall/Dream」段補對賬語意 + 三因子排序(若有提及處) |
| Config templates | ❌ | T2.1 OUT → 不改 `install.rs` SessionEnd hook command |
| CHANGELOG | ✅ | 一條 Phase B(T2.2+T2.3)條目 |
| Version bump | ✅ | `Cargo.toml` 0.0.4 → 0.0.5(記憶 compile/recall 行為變更)。**grep 舊版字串全 repo 確認同步** |
| Migration / 行為變更說明 | ⚠️ | 無 hook 重裝需求。若 T2.2 改既有 L1 的 status 語意,CHANGELOG 註明「dream 改為對賬式,不再盲目覆蓋」 |
| Credit / attribution | ✅ | 已在 proposal §7;README `Inspired By` 若未列 mem0 對賬補上 |
| Dogfood | ✅ | codeforge 自身 dream 即用 |
| Schema migration | ⚠️ | T2.2 用既有 `status` 欄位(active/superseded/archived)即可,不需新 migration。若需 reconciliation 記錄表才加 |

## Phases

### P0 — T2.3 統一 ranking(最低風險,先做)
- `src/memory/recall.rs::rank()`:strength(importance)× recency(對 `updated`/`last_ref` 的時間衰減)× local-citation(`refs`)三因子。
- 純函式 + 既有 `decay.rs` 維護的 strength 對齊;不碰 LLM、不碰 schema。
- 測試:fixtures assert 三因子排序。
- **Gate**:`cargo test` 綠、新測試覆蓋三因子。

### P1 — T2.2 mem0 式對賬(中風險,memory data integrity)
- `compile.rs`:compile 新事實前,用 FTS 找既有相關 L1 → LLM 判 ADD/UPDATE/DELETE/NOOP。
  - ADD:新檔(現狀)。UPDATE:更新既有條 body + `updated`,舊矛盾條 `status=superseded`。NOOP:已存在等價事實,不寫。DELETE:標 archived。
- 與既有 `dedup.rs` 整合(避免兩套去重邏輯打架)。
- 全程 `.chars().take(N)` CJK-safe;sources union 保留(現有合併邏輯)。
- **Gate**:NOOP/UPDATE/supersede 測試綠;`requesting-code-review`(memory 資料完整性)。

## OUT OF SCOPE — T2.1 async worker(→ BACKLOG B16)

2026-06-17 user decision:**不在本專案**。
- **設計已收斂(留檔備用)**:conditional enqueue + 同步 fallback —— 該 store 有 live daemon(pidfile + kill -0)→ `dream --background` emit `dream_scheduled` event 快速返回;無 daemon → inline 同步跑(維持「永遠會 distill」保證,不回歸 ship-online)。整合點:`dream.rs`(--background flag)、`daemon/events.rs`(dream_scheduled handler + throttle)、`install.rs`(hook command + 重裝說明)。
- **不做的理由**:觸發條件「SessionEnd 開始覺得卡」尚未成立;紅利只在有 daemon 的少數專案兌現,複雜度/熱路徑風險不划算。
- **Trigger**:SessionEnd 的 dream/ship 開始有感卡頓時,照上述設計開新 L。

## Quality Gate
`cargo check` + `cargo clippy` + `cargo test` + `superpowers:requesting-code-review`(memory + dream 二區 data-integrity)。

## 執行順序理由
P0 → P1:風險遞增。P0(排序)純函式、立即提升剛上線的 recall;P1(對賬)動 LLM compile 但自足、不碰 SessionEnd 熱路徑。兩者都不擾動 ship-online 鏈。
