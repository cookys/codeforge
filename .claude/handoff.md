# Session Handoff — 2026-04-21

> 上個 session 做完 tui-foundation L project 並 archive 後，user 指定把
> tile-map plan promote 為 project，然後 `/clear` 交棒。**不要重新設計**
> tile-map 的架構 —— plan 早已定稿（2026-04-20），本 session 照 plan 執行
> P1-P4 即可。

## 本 session 要做的事 —— TL;DR

1. Read `doc/plans/2026-04-20-tile-map-localmap.md` 全文 —— 設計細節都在
2. Invoke `autopilot:dev-flow`（HARD RULE，code-touch 前必跑）
3. 已在 branch `feature/tile-map-localmap`（從 main `f415a29` 分出）
4. Project dir 已建：`doc/projects/2026-04-21-tile-map-localmap/README.md`
5. 執行 P1 → P2 → P3 → P4 → QG → r1 → fix → r2 → merge → archive，**連續執行，phase 間不問 continue**
6. Tasks 已建好（P1-P4 + QG + 2 輪 review + fixes + merge + archive）—— 跑到哪個就 `TaskUpdate` 到哪個

## 關鍵設計決策（不要改，plan 裡已定）

| 項目 | 決定 | 原因 |
|------|------|------|
| Tile 尺寸 | 10 cols × 3 rows | border + CJK name + badge 三行夠用 |
| Zone kind 來源 | dir-name heuristic | 避免 per-paint DB round-trip |
| Color mapping | rust=Red / memory=Magenta / daemon=White / tui=Cyan / db=Yellow / docs=DarkGrey | 語意化 |
| Mode state | in-memory only 第一輪 | 避免 settings schema 擴充 scope creep |
| Keypress | `g` OR `l` cycle toggle | 兩鍵都 toggle（便於盲打） |
| Fallback 門檻 | panel <30 cols | 低於此 grid 一格都塞不下 |
| Overflow | 右下角 `…+N more` | 不要 clip 無提示 |
| `@` overlay 位置 | tile 第 3 行 badge 右側 | 不覆蓋名稱 |

## Phase 拆分（plan 已寫死）

| Phase | 檔案 | 估 test |
|-------|------|---------|
| **P1** Tile primitive | `src/tui/panels/local_map_tile.rs`（新檔）—— `render_tile(room, width, height, is_current, color)` + zone_kind → Color + CJK-safe name clip | +15 |
| **P2** Grid flow | 同上 —— `compute_grid()` + `render_grid()` 組合 tile | +10 |
| **P3** Mode toggle | `src/tui/panels/local_map.rs` 加 `LocalMapPanel` + `DisplayMode`；`src/tui/events.rs` 加 `ToggleMapMode`；`src/tui/mod.rs` main_loop 接 event | +6 |
| **P4** `@` overlay + fallback + spec | 同 P3 檔案 + `doc/specs/codeforge-mud-engine.md §5` | +5 |

## Git 狀態（本 session 起點）

- 分支：`feature/tile-map-localmap`（已建立，從 main `f415a29`）
- 最新 commit：`f415a29 docs(plans): draft tile-grid local_map plan`
- `git status` 乾淨
- 無 remote，push 是 no-op

## Phase Roadmap 現況

- ✅ Phase 1 / 2a-c / 3a-f 全數完成並封存
- ✅ tui-foundation UX polish（L，2026-04-18）
- 🚧 **tile-map-localmap UX polish（L，本 session 執行）**
- 下一步候選（tile-map 完成後）：
  - Phase 4 Zoa full impl（補 Happy/Tired/Hunting frames + mood mapping）
  - Phase 5a Nation Plugin
  - B10 Doppelganger split（等 user 回 3 問）

## 絕對要遵守的規則（持久，跨 session）

- **一律用正體中文台灣用語回應 user**（code / commit / tool output 英文 OK；
  長 session 容易被工具 drift 掉，每次切回 user 對話前 self-check）
- **Invoke `autopilot:dev-flow` before any code work** —— HARD RULE，
  PreToolUse hook 會擋
- **CJK 截斷**：`.chars().take(N).collect::<String>()`，絕對不用 `&s[..N]`
  （本 project tile name 截斷會踩這個坑，用 `clip_to_width` / `pad_to_width`
  既有 helper）
- **正體中文 panic/error message** 給 user，anyhow::Result 沿用
- **L workflow continuous execution** —— phase 之間不問「要繼續嗎」，
  只在 Board Decision / build fail / context near limit 才停

## 本 project 特有的 pitfall

1. **ANSI colour 在 non-TTY sink 下炸圖** —— render 回傳純 `Vec<String>`，
   colour 在 `paint` 層疊加（既有 `termcolor::StandardStream` pattern）。
   Test assert plain text 不含 ANSI 脆弱；看 `src/tui/render.rs` 既有做法。
2. **Tile 10 cols 裝 CJK dir name** —— 扣 border 2 cols，內容區 8 cols
   = 4 CJK 字。「後端」OK、「代號七七七」要截成「代號七七…」。
3. **`RoomSummary::is_current` 已存在** —— P4 `@` overlay 直接用，不要重造
   current-dir 偵測邏輯。
4. **Grid render 在 Wide mode 容得下 Zoa** —— layout 已分好 region，
   Wide mode map 只有 ~46 cols，tile 10-col 可排 4 個一列；不要動 layout.rs。
5. **Display mode toggle 要 mut borrow** —— 沿用 tui-foundation 的 ZoaPanel
   pattern：`&mut LocalMapPanel` 傳進 `paint_once`，在 `tokio::select!` arm
   裡 safe。
6. **Fallback to list 不是複製 render_list 代碼** —— 直接 call 現有
   `render_list()` function（rename 現有 `render` 為 `render_list`，
   新增 dispatcher `render()` 來 route）。

## Review 要點（r1 會抓的）

從 tui-foundation 的 r1 教訓：
- 確認 `duration_since` / `Instant` 算術都用 `checked_*`
- Keypress events test 要 assert argv 長度 + 每個 index，不只部分
- `paint_once` 裡的 mode check 要在 every paint 重算（terminal resize 可能改）
- dead_code allows 要在 `--bin` target 下驗證（test reachability 不算）

## 入口指令（next session 的你貼著走）

```
# 1) Session start gate
→ invoke autopilot:dev-flow
   ARGUMENTS: L-size project tile-map-localmap 繼續 — 在 feature/tile-map-localmap branch 上，P1-P4 依 plan 執行

# 2) 讀 plan + project README
→ Read doc/plans/2026-04-20-tile-map-localmap.md
→ Read doc/projects/2026-04-21-tile-map-localmap/README.md

# 3) 開工 P1
→ Read src/tui/panels/local_map.rs（現有 render 當作 fallback baseline）
→ Read src/tui/panels/mod.rs（pad_to_width / clip_to_width / vis_width helpers）
→ Write src/tui/panels/local_map_tile.rs（P1 tile primitive）
→ cargo test --bin codeforge tui::panels::local_map_tile
→ git commit P1
→ TaskUpdate P1 completed，P2 in_progress
...
```

## 一句話狀態

Plan 定稿（`doc/plans/2026-04-20-tile-map-localmap.md`），project dir 與
branch 都備好，tasks 佇列就緒 —— 下個 session invoke dev-flow 後直接開
P1 寫 `local_map_tile.rs`，不需重設計，連續執行到 merge + archive。
