# Doppelganger 分身怪：重複程式碼偵測 + 擊殺分裂（B10）

**Status**: Active · L · branch `feature/doppelganger-split`
**Owner 決定**: 卡 d-b8ef81a6 → A（proposal 三參數全用預設 A/A/A）；卡 d-2bfdc585 → A（範圍擴大到 scanner 偵測，讓分身怪真的出現）
**Proposal**: [`doc/proposals/2026-04-18-doppelganger-split.md`](../../proposals/2026-04-18-doppelganger-split.md)
**Plan**: [`doc/plans/2026-11-02-doppelganger-split.md`](../../plans/2026-11-02-doppelganger-split.md)

## Project Goal

> **Final goal**: 遊戲裡真的會出現分身怪 🪞（來自重複程式碼），打死會照 A/A/A 規則分裂，Doppelganger Ward 生效時不分裂。
>
> **Success criteria**:
> 1. 對一個含兩段相同程式碼區塊（≥ 門檻行數）的 tempdir 跑 `scan_dir`，結果至少有 1 隻 `MobKind::Doppelganger`；把其中一段改掉後再掃，結果 0 隻。驗證：`mob_scanner` 單元測試（正例 + 負例）。
> 2. 只有 import／空行／單一大括號等瑣碎行重複時不產生分身怪。驗證：負例測試。
> 3. 第 0 代分身怪被擊殺後，同一 tick 內新增 1 隻第 1 代（`parent_id` = 父 id、`generation` = 1、hp／atk／def = 父 × 0.7 向上取整且 ≥ 1、`origin_path` 同父）；第 1 代被殺生出第 2 代；第 2 代被殺不再分裂。驗證：`combat` 單元測試逐代斷言。
> 4. `SuppressDoppelgangerSplit` 對該 zone（或全域）生效時，擊殺不分裂；過期後恢復分裂。驗證：`combat` 單元測試。
> 5. 既有 DB（mobs 沒有 `parent_id`／`generation` 欄位）跑 migrations 後欄位存在、舊列 `generation` = 0、資料不掉。驗證：`db` 升級測試。
> 6. `codeforge inventory` 不再印「split 機制尚未實作」警語。驗證：inventory 測試斷言不含該字串。
> 7. 全套 gate 綠：`./scripts/fmt.sh --check`、`cargo clippy -- -D warnings`、`cargo test`、`python3 scripts/check-doc-drift.py`、check-cjk-safe；main CI 綠。
>
> **Scope boundary**:
> - Include：scanner 重複區塊 heuristic、split 機制（A/A/A）、mobs 兩個新欄位、Ward 消費端、inventory 警語移除、spec／proposal／BACKLOG／CHANGELOG 同步、snapshot 擊殺欄要不要加分身怪（plan 決定）。
> - Exclude：Void Creature 🕳️（測試覆蓋率）、TUI 分身家族樹顯示、badge 觸發、AST／token 級的重複偵測。

## Progress

| Phase | 內容 | 狀態 |
|-------|------|------|
| P1 | scanner 重複區塊偵測 → 產生分身怪 | ✅ `a241b7e` |
| P2 | mobs `parent_id`／`generation` 欄位 + migration | ✅ `8e2802c` |
| P3 | combat 擊殺分裂 + Ward 消費 | ✅ `b30b795` |
| P4 | inventory 警語、spec／文件同步 | ✅ `5cc32c4` |

## Verify notes

- 實測 `scan_dir`（release build，臨時測試、未 commit）：codeforge repo 46 ms、20 隻（含 3 隻分身怪：`ferris-sprite.ts ↔ sprite.rs` 的像素列、`craft.rs ↔ use_item.rs`、`world.rs ↔ snapshot/render.rs`）；autopilot repo 425 ms。對應 U1 consult 標為 insufficient_evidence 的 DefaultHasher 速度疑慮：在 tick 預算內。
- 觀察（範圍外）：scanner 的 skip 清單沒有 `.claude/worktrees`，所以有 worktree 的 repo 會在 worktree 副本之間抓到重複；既有四種怪也一樣會掃進 worktree。
- Code review（hetero，gpt-6-astra low／grok-4.7 high，ledger 在 session scratch）：g1 兩席同抓「行中 `/*` 沒進註解狀態」→ `c2679ad`；g2 grok 抓到該修正讓邊界行失去邊界 → `bd59ca5`；g3 astra SHIP-AS-IS、grok 提「續行內註解含結束符」的窄邊角 → 3 輪上限，depth-0 延後為 BACKLOG B37。receipt 檢查 rc=0。
- 測試：`live::tests::findmnt_tmpfs_selects_xdg_candidate`（`src/live.rs`，B10 未動）全套跑一次失敗、單跑與之後連 3 次全套皆綠 → 既有的不穩定測試，非本次引入。
- 突變探測：拿掉 `pub` 前綴處理、拿掉每檔一個位置、拿掉排序走訪、顛倒「先標記死亡再生子代」——各自都有測試轉紅。

Last updated: 2026-11-02
