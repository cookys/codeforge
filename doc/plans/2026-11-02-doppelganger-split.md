# Plan — Doppelganger 分身怪：重複程式碼偵測 + 擊殺分裂（B10）

**Status**: draft（plan review 前）
**Project**: [`doc/projects/2026-11-02-doppelganger-split/`](../projects/2026-11-02-doppelganger-split/README.md)
**Proposal**: [`doc/proposals/2026-04-18-doppelganger-split.md`](../proposals/2026-04-18-doppelganger-split.md)（Decision 段：A/A/A + 範圍追加）
**Size**: L · branch `feature/doppelganger-split`

## 0. Context

MUD 層的 daemon 每 10 tick 掃一次 codebase（`src/daemon/mob_scanner.rs`），把 code smell 變成怪：殭屍（TODO）、Boss（長函式）、精英（分支多）、幽靈（沒用到的 `use`）。spec §2 還有分身怪 🪞「重複程式碼區塊，分裂，清不完」，`MobKind::Doppelganger`、loot（Pattern Fragment + 30% Abstract Gem）、Doppelganger Ward（2× Abstract Gem → `suppress_doppelganger_split` 2 天）都已存在，但：

- scanner 沒有任何重複偵測，分身怪從不生成 → Abstract Gem 拿不到 → Ward 正常玩做不出來；
- 分身怪被殺時沒有分裂邏輯 → Ward 的 effect 沒有消費端，`codeforge inventory` 印警語。

owner 決定（d-b8ef81a6、d-2bfdc585）：分裂參數 A/A/A，範圍含 scanner 偵測。

## 1. Problem

讓分身怪這條路徑從頭到尾玩得到：scanner 偵測重複區塊產生分身怪 → 擊殺掉 Abstract Gem → 可合成 Ward；分身怪被殺時依 A/A/A 分裂，Ward 生效時不分裂。

## 2. OKR / KRs

**Objective**：分身怪在遊戲裡出現、會分裂、可被 Ward 壓制。

- **KR1**：含兩段相同非瑣碎程式碼區塊（≥ §3.1 門檻）的目錄，`scan_dir` 產出 ≥ 1 隻 Doppelganger；改掉其中一段後 0 隻。
- **KR2**：只有瑣碎行（import、空行、單獨大括號、註解）重複時 0 隻。
- **KR3**：同一目錄連掃兩次，產生的分身怪名稱與 origin_path 完全相同（`persist_scan` 第二次插入 0 列）；檔案列舉順序不同（測試打亂）結果相同。
- **KR4**：第 0 代被殺 → 同 tick 新增第 1 代（`parent_id`、`generation=1`、stats = 父 × 0.7 ceil ≥ 1、同 name／origin_path）；第 1 代 → 第 2 代；第 2 代被殺不分裂。
- **KR5**：`SuppressDoppelgangerSplit` 對該 zone 或全域生效時不分裂；過期後恢復。
- **KR6**：舊 DB 升級後 `parent_id`、`generation` 欄位存在，舊列 `generation=0`，資料不掉；新裝 DB 也有這兩欄。
- **KR7**：`codeforge inventory` 不再印 split 未實作的警語。
- **KR8**：gate 全綠（fmt、clippy `-D warnings`、`cargo test`、check-doc-drift、check-cjk-safe），不新增 crate。

## 2.5 Global Constraints（原文照抄進每個 dispatch）

1. 不新增 crate 依賴。
2. 所有 mobs 寫入都在 daemon tick 的同一個 transaction 內（`.claude/rpg-engine-spec.md`：daemon 擁有 derived state 寫入）；CLI 不寫 mobs。
3. scanner 的錯誤不得讓 tick 失敗（沿用 `rate_limited_scan_with_dir` 吞錯 + stderr）。
4. 使用者看得到的字串用正體中文；CJK 截斷用 `.chars().take(N)`；格式化只用 `./scripts/fmt.sh`。
5. 測試不碰 `CODEFORGE_SCAN_DIR` 等 process-global env（用 `_with_dir` 注入）。

## 2.6 Change-policy decisions

- Schema：只加欄位（ALTER-guard，沿用 `upgrade_mobs_origin_path` 模式），不 bump `schema_version`，不改既有欄位。
- 既有四種怪的偵測結果不變（除了 §3.2 的檔案排序讓「哪些檔先被掃」變得確定）。

## 3. 設計

### 3.1 重複區塊偵測（scanner）

定位：遊戲訊號，不是重構工具。漏抓可以，抓出看起來很蠢的重複（兩個 `}`）不行。

**逐行分類**（trim 後）：

| 類別 | 規則 | 處理 |
|------|------|------|
| 空行 | trim 後為空 | 略過 |
| 註解行 | brace 語言（rs/ts/js/go）：開頭 `//`、`/*`，或整行是 `*`、`*/`，或開頭 `* `（block comment 內文）；Python：開頭 `#` | 略過 |
| 瑣碎行 | 英數字元數 < 3（`}`、`});`、`)`），或只剩英數的內容屬於 {`else`, `return`, `break`, `continue`, `pass`, `default`, `try`, `finally`, `do`, `end`} | 略過 |
| 邊界行 | import／宣告：rs `use `、`mod …;`、`extern crate`；py `import `、`from `；ts/js `import `、`export … from`；go `import`、`package `；屬性與 decorator：rs `#[`、`#![`，py／ts `@` | **切斷**視窗（不讓兩邊不相干的程式碼被縫成一塊） |
| 其餘 | — | 保留（正規化 = trim 後原文） |

**跨行狀態**（逐行分類前先追蹤，整段都不入視窗）：

- brace 語言的 block comment：某行在 `/*` 之後沒有 `*/` → 進入註解狀態，直到含 `*/` 的那行（含）為止，中間每行當註解略過，不論開頭有沒有 `*`。
- Python 三引號字串：行首（trim 後）是 `"""` 或 `'''` 且同一行沒有閉合 → 進入字串狀態直到閉合那行；整段略過。
- 跨行 import／宣告：rs `use …{` 沒以 `;` 結尾 → 直到以 `;` 結尾的行；py `from … import (` → 直到含 `)` 的行；ts/js `import {`、`export {` 沒有 `from`／`;` → 直到含 `from` 或以 `;` 結尾的行；go `import (` → 直到 `)` 那行。整段當邊界：開頭切斷視窗，中間的行全部不入視窗。
- 跨行 rs 屬性：`#[` 開頭但不以 `]` 結尾 → 直到以 `]` 結尾的行；整段當邊界。

**跳過的檔案**：前 5 行含 `@generated` 或 `DO NOT EDIT`；檔名 `*.min.js`、`*.d.ts`、`*.pb.go`、`*_pb2.py`。

**視窗**：在兩個邊界之間的連續保留行上滑動，W = `DUP_WINDOW_LINES` = 8；視窗正規化文字總長 ≥ `DUP_MIN_WINDOW_BYTES` = 200，且 8 行中至少 `DUP_MIN_DISTINCT_LINES` = 4 行不同，否則不入索引。

**雜湊與驗證**：`std::collections::hash_map::DefaultHasher`，逐行 hash（`str` 的 `Hash` 自帶結尾標記，不需組字串）。索引 `HashMap<u64, Vec<(file_idx, kept_line_idx)>>`；每個 hash 最多記錄 `DUP_MAX_OCCURRENCES` = 8 個出現位置：檔案按排序後的順序處理，所以留下的是（相對路徑、保留行號）最前面的 8 個；之後的出現直接忽略（bucket 滿了就不再增加，不會被移除後重建）。這樣同一段程式碼出現在很多檔案時仍會產生分身怪（≤ 3 隻，由排在前面的路徑對決定），配對數也有上限。命中後**逐行比對正規化文字**才算數；hash 只用來挑候選。

**只抓跨檔**：同一檔案內的重複 MVP 不抓（省掉區間重疊判斷，也少掉一大類樣板）。

**配對與合併**：每個合格 hash 的出現位置兩兩配對（只配不同檔，檔案 idx 小的當 a）。依 (a, b) 分組，在組內按 (ka, kb) 排序；兩邊 offset 同時 +1 才算同一塊延續。每組取最長塊的行數（W + 延續數 − 1）。

**選出與命名**：合格的檔案對依（最長塊行數降冪、a 路徑、b 路徑）排序，取前 `MAX_DOPPELGANGERS_PER_SCAN` = 3。名稱 `dup @ <a> ↔ <b>`（scan root 相對路徑、`/` 分隔、a < b 字典序）——**不帶行數或 hash**，重複區塊長大或縮小名稱都不變，re-scan 不會多生一隻；同一對檔案之間的多段重複算同一隻。檔案改名會換身分（接受）。`origin_path` = a。

**預算**（都在 tick transaction 內跑，所以要有上限）：

- `SCAN_READ_BUDGET_BYTES` = 32 MiB：本次 scan 累計讀檔位元組，用完就停止整個讀檔迴圈（既有 heuristic 與重複偵測一起停）。
- `DUP_INDEX_BUDGET_BYTES` = 4 MiB：累計餵進重複索引的正規化位元組（每個檔案的保留行要留在記憶體供命中驗證），用完後後面的檔案只跑既有 heuristic、不再入索引。hash 運算量 ≤ W × 4 MiB。
- 每個 hash 最多留 8 個出現位置（上面），配對數因此 ≤ 28／hash。

**名額**：既有 heuristic 照舊收集，但不再在 20 隻時 `break` 讀檔迴圈（要繼續餵重複索引）；收滿 20 隻後只停止收集既有 heuristic 的結果。最後 `specs = 既有結果.take(MAX_MOBS_PER_SCAN − 分身怪數) + 分身怪`——分身怪沒用完的名額自然回補給既有 heuristic。

### 3.2 `scan_dir` 結構調整

現在的檔案迴圈在 20 隻上限時 `break`，後面的檔案根本沒讀；重複偵測需要所有檔案的內容。改成：

1. `collect_source_files` 走訪時就排序：每個目錄先收下全部 entries、依檔名排序，再依這個順序處理檔案與子目錄（深度優先、子目錄也按名稱順序）；`MAX_FILES_TO_SCAN` 只作用在這個已排序的走訪上。理由：`read_dir` 順序各機器不同，檔數超過上限時，被收進來的子集必須由路徑決定。
2. 單次讀檔迴圈：每個檔案讀一次，跑既有四種 per-file heuristic（收滿 `MAX_MOBS_PER_SCAN` = 20 筆後只停止收集，**不**提前預扣分身怪名額、也不 break 迴圈），同時把正規化後的行餵進重複偵測的索引（受 §3.1 的預算限制）。
3. 迴圈結束後跑重複配對，得到 d 隻分身怪（d ≤ `MAX_DOPPELGANGERS_PER_SCAN` = 3）。
4. 輸出 = 既有結果的前 (20 − d) 筆 + 這 d 隻。沒有重複時既有怪仍可到 20 隻，跟現在一樣。

排序會改變既有四種怪在超過上限時「哪些先被掃到」——這是刻意的（讓結果可重現），既有測試若依賴目錄順序要一併調整。

### 3.3 mobs 欄位

- `parent_id INTEGER`（NULL = scanner 生成的第 0 代）
- `generation INTEGER NOT NULL DEFAULT 0`

`schema.sql` 的 `CREATE TABLE mobs` 直接加兩欄（新裝）；`db::migrations::run` 加 `upgrade_mobs_split_columns`（`column_exists` + `ALTER TABLE … ADD COLUMN`，兩欄各自判斷）。`persist_scan` 的 INSERT 不變（generation 走 DEFAULT 0）。

### 3.4 擊殺分裂（combat）

- `AliveMob` 與 `load_alive_mobs` 的 SELECT 加 `generation`、`origin_path`。
- `run_tick` 在迴圈前讀一次 `is_effect_active(SuppressDoppelgangerSplit, zone, now)`（同 `reduce_difficulty` 模式，一 tick 一次）。
- 擊殺分支：`mark_defeated` **先**執行，然後若 `kind == Doppelganger && generation < MAX_SPLIT_GENERATION (2) && !suppress` → `spawn_child`。順序不可顛倒：child 與 parent 同名，parent 還活著時 `INSERT OR IGNORE` 會被 alive unique index 擋掉。
- `spawn_child`（`mob.rs` 的純函式算 stats + combat 的 INSERT）：
  - `name`、`zone_id`、`kind`、`origin_path` = parent；
  - `hp = hp_max = ceil(parent.hp_max × 0.7)`（整數：`(x*7 + 9) / 10`），`atk`、`def` 同法，全部 `.max(1)`；
  - `difficulty` = parent（proposal 未規定；繼承，因為 0.7 已經讓子代變弱）；
  - `parent_id` = parent.id，`generation` = parent.generation + 1，`spawned_at` = now。
- 同一 tick 新生的子代不會被打（`mobs` 已在迴圈前載入）。
- `CombatSummary` 加 `splits: usize`（測試與將來 commentary 用）。
- loot 不用改：子代死亡一樣進 `summary.defeats` → `loot::apply_for_defeats`，kind 是 Doppelganger，掉同一張表。

**名稱與 scanner 的循環（刻意的，不是 bug）**：子代與 parent 同名，所以子代活著時，scanner 重新送出的第 0 代會被 alive unique index 擋掉；第 2 代死後，這個名字沒有活著的怪，下一次 scan 若重複程式碼還在，就重新生成第 0 代。一個重複區塊在程式碼裡沒修掉之前，每 3 次擊殺會重生一次——跟殭屍、Boss 在 TODO 沒清之前會一直重生同一個道理，也就是 spec 的「清不完」；`max_children` 的上限指的是「一隻 scanner 生成的怪最多連鎖 2 代」。把程式碼裡的重複修掉，循環就停。

### 3.5 Ward

`SuppressDoppelgangerSplit` 已能 craft／use、寫 `active_effects`；§3.4 是它的消費端。Ward 只擋分裂，不擋 scanner 生成第 0 代（spec §3.5「使用後：Doppelganger 不分裂」）。

### 3.6 顯示與文件

- `src/cli/inventory.rs`：拿掉 Ward 的「split 機制尚未實作」警語。
- `src/cli/use_item.rs` 模組註解、`src/daemon/mob.rs` 模組註解、`src/daemon/mob_scanner.rs` 模組首段（現在寫 duplicates 是 Phase 3+ token hashing）、`src/craft/effects.rs` 的 `SuppressDoppelgangerSplit` 註解（現在寫 daemon wiring deferred）：改成已實作的事實——scanner 生成第 0 代，Ward 只在擊殺時抑制分裂。
- `doc/proposals/2026-04-18-doppelganger-split.md`：Status 改為 Implemented，指向本 plan，註明 heuristic 與 proposal 原文的差異（proposal 沒有偵測段）。
- `src/snapshot/render.rs` `KIND_COLUMNS`：**不加**分身怪欄（卡片寬度已滿，註解寫明）。
- `doc/specs/codeforge-mud-engine.md`：line 14 的 STALE 標註、§2 Planned 區塊把 Doppelganger 移到已實作並寫 heuristic 與分裂規則、§3.5 Ward 描述。
- CHANGELOG `[Unreleased] ### Added`。

## 4. Phases

| Phase | 內容 | 驗證 |
|-------|------|------|
| P1 | §3.1 + §3.2 scanner 重複偵測 | KR1–KR3 測試 |
| P2 | §3.3 mobs 欄位 + migration | KR6 測試 |
| P3 | §3.4 + §3.5 分裂與 Ward | KR4–KR5 測試 |
| P4 | §3.6 顯示與文件 | KR7 測試、check-doc-drift |

每個 phase 先寫會紅的測試，再實作。

P1 必要測試（tempdir fixture，`scan_dir` 直接呼叫）：

1. 兩檔各含同一段 10 行非瑣碎程式碼 → 1 隻分身怪，名稱 `dup @ a ↔ b`、origin_path = a。
2. 改掉其中一檔那段 → 0 隻。
3. 只有 import、屬性、空行、`}`、`else {`、註解重複 → 0 隻；另含兩個子案例：(a) 兩檔有同一段 ≥ 8 行、≥ 200 bytes、開頭不帶 `*` 的 block comment 內文；(b) 兩檔有同一段 ≥ 8 行的跨行 grouped `use …{ … };` → 都是 0 隻。
4. 重複區塊從 10 行長到 14 行 → 名稱不變；對同一 DB 連跑兩次 `persist_scan` 第二次插入 0。
5. 同一組檔案以不同建立順序寫入兩個 tempdir → 兩邊結果（名稱、順序）完全相同。
6. 同一段程式碼出現在 12 個檔案（超過每 hash 8 個出現位置）→ 1 至 3 隻，名稱集合只由排在前面的路徑對決定，兩個以不同順序建立的 tempdir 結果相同；不 panic。
7. 先放 20 個會產生殭屍的檔案、字典序排在後面的兩檔才有重複 → 結果含分身怪，總數 ≤ 20。
8. 同一檔案內的兩段重複 → 0 隻。
9. 前 5 行有 `@generated` 的檔案不參與重複偵測。
10. 1001 個來源檔，只有字典序最後的兩檔有重複 → 兩個以不同順序建立的 tempdir 結果相同（都抓不到，因為最後一檔超出 1000 上限；斷言兩邊一致且不含該對）。再做一組只有字典序最前面的兩檔有重複 → 兩邊都抓到。
11. 30 個殭屍檔、沒有任何合格重複 → 恰好 20 隻、0 隻分身怪。
12. 有足夠的既有候選時，分別構造 0、1、2、3 組重複 → 總數都是 20，分身怪各為 0、1、2、3。

## 5. 不做

- Void Creature 🕳️（測試覆蓋率）。
- TUI 分身家族樹、snapshot 分身欄、badge、commentary 台詞。
- AST／token 級重複偵測、跨語言重複。

## 6. U1 consult（gpt-6.1-sol low，run `author-1793592773-240316`）與處置

問題：W=6 + 過濾規則、名稱穩定、雜湊、同檔重複、其他坑。全部採納：

| # | consult 意見 | 處置 |
|---|-------------|------|
| 1 | W=6 太小；建議 W=8、≥200 bytes、≥4 行不同 | 採納（§3.1 視窗） |
| 2 | `else {` 有 4 個英數字元，「< 3 英數」擋不住 | 採納：瑣碎行加關鍵字清單 |
| 3 | 開頭 `*` 一律當註解會吃掉 deref | 採納：只認 `*`、`*/`、`* ` |
| 4 | import／屬性刪掉會把不相干程式碼縫起來，應當邊界 | 採納：邊界行切斷視窗 |
| 5 | 名稱不要帶行數或 hash，用排序後的路徑對 | 採納 |
| 6 | DefaultHasher 夠用，但 hash 命中要比對原文；不做 rolling hash | 採納 |
| 7 | 同檔重複 MVP 跳過 | 採納 |
| 8 | 排序要在 1000 檔上限**之前**，否則被截掉的子集仍不確定；選前 3 對要排序，不可依 HashMap 順序 | 採納（§3.2 第 1 點改為走訪時排序） |
| 9 | 合併塊要兩邊 offset 同時前進 | 採納 |
| 10 | 高頻視窗會讓配對變平方；要上限 | 採納：每 hash 最多 8 次 |
| 11 | 8 MiB 要定義清楚；另外要限制讀檔量，否則 1000×2 MiB 都在 transaction 內 | 採納：讀檔 32 MiB + 索引 4 MiB 兩個預算 |
| 12 | 不要在 17 隻時 break 讀檔；沒用完的分身名額可回補 | 採納（§3.1 名額） |
| 13 | 測試要涵蓋：樣板拒絕、長大後名稱不變、目錄順序打亂、高頻輸入上限、一般名額滿了之後才發現的重複 | 採納（§4 P1 測試清單） |

consult 標記為 insufficient_evidence 的一項：DefaultHasher 在 tick 延遲下是否夠快需要實測。處置：不寫進單元測試（計時測試在 CI 上不穩），改在 verify 階段對本 repo 跑一次 `scan_dir` 計時，結果記在 project README。

## 7. Review log

**g1**（gpt-6-astra low CONDITIONAL、grok-4.7 high STOP；8 條全採納，dispositions `2026-11-02-doppelganger-split.g1-dispositions.json`）：

| fp | 席 | 等級 | 處置 |
|----|----|------|------|
| 4dc5dc35、59dd403f | astra、grok | blocking R4 | §3.1 與 §3.2 名額算法互斥 → 刪掉 §3.2 的固定預扣 17，統一為「收滿 20 停收、輸出前 20−d 筆 + d」；加測試 11、12 |
| a041e20d | astra | blocking R1 | 跨行 block comment 與 grouped import 內文會被當程式碼 → §3.1 加「跨行狀態」；測試 3 加兩個子案例 |
| 40ff8206 | grok | blocking R1 | 超過 8 次整個丟掉會讓真重複產出 0 隻，違反 R1 → 改為只留排序最前的 8 個出現位置；測試 6 改預期 |
| e6b6409a | astra | non-blocking R3 | 丟掉後會被重建 → 隨 40ff8206 一起解：bucket 滿了只忽略、不移除 |
| 9e3e4558 | grok | blocking R2 | §3.2 括號那句讀起來像在規定「截斷後再排序」→ 改寫為只有走訪時排序；加 1001 檔測試 10 |
| 9f949a5d、38e61d64 | astra、grok | non-blocking R11 | §3.6 漏了 effects.rs、mob_scanner.rs 模組首段、proposal → 補進 §3.6 |
