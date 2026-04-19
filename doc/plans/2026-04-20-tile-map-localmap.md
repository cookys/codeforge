# Plan — Tile-Grid Local Map (Revival World 風)

Date: 2026-04-20
Status: **Draft — awaiting user green-light to promote to project**
Branch (when promoted): `feature/tile-map-localmap`
Size: **L**（新 render 模式 + keypress toggle + 跨檔；3+ files；可能含 settings persist）

## Motivation

目前 `src/tui/panels/local_map.rs` 是 linear list：

```
📍 Local Map
▶ src/daemon   [🧟2]
  src/cli      [✓]
  src/db       [—]
```

Revival World 的地圖面板（`rw_screenshot3/4.png`）示範了另一種做法：
**tile-based overhead 2D grid**，每個房間是一個帶顏色的色塊，建築/地區名
直接嵌在 tile 內。幾個關鍵觀察：

1. 色塊語意化 —— 綠=公園、藍=河、紫/黃=商業建築，**一眼分辨類別**，不需 legend
2. CJK 名稱直接嵌入 tile —— 「市政大樓 / 企業大樓 / 中央銀行」，不用 side-legend
3. 玩家/其他玩家用 `★ ☆ ♀ ♂` 小符號疊在 tile 上
4. 右側獨立 widget：城市名 + 時間 + 天氣 + 羅盤

檔案系統天然是 2D tree —— 每個 top-level dir 是一格，比 MUD room graph
更簡單（不用 auto-map from MOVE，directory 層級本身就是拓樸）。

## Final Goal

> 把 Local Map 從線性清單升級成 tile-grid，每個 top-level dir 渲染成
> 帶顏色邊框 + CJK 名稱 + mob badge 的色塊；pet 位置用 `@` overlay；
> 可按鍵在 list / grid 之間切換；在 panel 太窄時自動 fallback list。

### Success Criteria

| # | 條件 | 門檻 | 驗證 |
|---|------|------|------|
| 1 | Tile 渲染正確 | 10×3 tile border + 名稱 + badge CJK-safe 對齊 | `cargo test render_tile_*` |
| 2 | Grid flow 正確 | 10 個 dir 在 40×20 面板 → 4×3 排版不重疊 | `cargo test compute_grid_*` |
| 3 | Color 語意分類 | 5 個 zone kind 各有獨立 color（rust/memory/daemon/tui/db） | unit + ANSI snapshot test |
| 4 | `@` overlay 指向 current room | cwd = `src/cli/...` → `src` tile 顯示 `@` | unit test |
| 5 | 鍵盤 `g`/`l` 切換 | TUI 內按鍵即時切換，無 flicker | 手動 + event test |
| 6 | 面板 <30 cols 自動 fallback list | 測試 60-cols Standard mode 時 map=24 cols → fallback | layout test |
| 7 | 全綠 | `cargo test` + `cargo clippy --bin` 零新 warning | CI-style |

### Scope Boundary

**Include**:
- `src/tui/panels/local_map_tile.rs`（新）—— tile primitive + grid compute
- `src/tui/panels/local_map.rs` —— 加 `render_grid()` alongside existing `render()`，dispatcher
- `src/tui/events.rs` —— `g`/`l` keypress
- `LocalMapPanel` state struct（模仿 `ZoaPanel` 模式）持 `display_mode: DisplayMode`
- Zone kind → color mapping（hardcoded 5-7 種，與 spec §1 對齊）
- spec §5 更新 + CLAUDE.md 提到新鍵盤快捷鍵

**Exclude**（下一輪再做）:
- 顯示模式跨 session 持久化（settings table）—— **第一輪 in-memory only**
- `fog of war`（未 scan dir 顯示 `??`）—— 需要追蹤 "已 scan" 狀態，scope creep
- 二層級 drill-down（Enter 進入 subdir 的 tile 網格）
- Revival World 的右側小 widget（羅盤 + 時鐘 + 天氣）—— 獨立 project
- `★ ☆` 其他玩家位置 —— Phase 5a Nation P2P 範疇

## Design Sketch

### Tile 尺寸

預設 **10 cols × 3 rows**：

```
┌────────┐
│ daemon │   ← dir name（CJK-safe 截斷到 8 chars/cols）
│ 🧟 3 @ │   ← badge + pet marker
└────────┘
```

若 panel 太窄（<30 cols）→ fallback existing list render。
若 panel 很寬（Wide mode ≥120 cols）→ tile 維持 10×3，更多 tile per row。

### Grid flow

```
panel 40 cols × 20 rows
tile 10×3 → 4 tiles 橫排、6 tiles 縱列 = 最多 24 格
```

```rust
struct GridLayout {
    cols: usize,         // tiles per row (floor(panel_w / tile_w))
    rows: usize,         // rows of tiles (floor(panel_h / tile_h))
    capacity: usize,     // cols * rows
    origin_x: u16,       // left padding when grid < panel_w
    origin_y: u16,
}

fn compute_grid(rooms: &[RoomSummary], panel_w: usize, panel_h: usize)
    -> (GridLayout, Vec<(TileCoord, &RoomSummary)>, usize /*overflow*/)
```

超出 capacity 的 room 不渲染；panel 右下角放 `…+N more`。

### Color mapping

按 zone kind（`src/world/` 已有這個概念）：

| Kind | Color | Rationale |
|------|-------|-----------|
| rust | Red | Forge / 火 |
| go | Cyan | 冰原 |
| python | Yellow | Scriptorium 黃色羊皮紙 |
| typescript | Blue | TS Garrison 駐紮藍 |
| javascript | Green | Bazaar 熱鬧 |
| memory / docs | Magenta | Meta / 內省 |
| daemon / db | White | 系統層 |
| (unknown) | DarkGrey | 默認 |

實作：`termcolor::Color` + `ColorSpec.set_fg()`，邊框用該 color，名稱/badge
保持 default fg（可讀）。

### `@` overlay

`RoomSummary::is_current` 已經標了 current dir。tile render 在 badge 行
加 `@` marker（搶佔 badge 右側 2 cols）：

```
┌────────┐
│ daemon │
│🧟 3  @ │    ← current room
└────────┘
```

### Display mode state

```rust
// src/tui/panels/local_map.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    List,   // default — current behavior
    Grid,   // new tile-grid
}

pub struct LocalMapPanel {
    pub display_mode: DisplayMode,
}
```

Render dispatch：

```rust
pub fn render(
    panel: &LocalMapPanel,
    rooms: &[RoomSummary],
    width: usize,
    height: usize,
) -> Vec<String> {
    match panel.display_mode {
        DisplayMode::List => render_list(rooms, width, height),  // existing
        DisplayMode::Grid if width >= MIN_GRID_WIDTH => {
            render_grid(rooms, width, height)
        }
        DisplayMode::Grid => render_list(rooms, width, height),  // graceful fallback
    }
}
```

### Keypress toggle

`src/tui/events.rs` 擴充：

```rust
pub enum TuiEvent {
    Quit,
    ToggleMapMode,  // new
}

// in keyboard task
KeyCode::Char('g') => ToggleMapMode,
KeyCode::Char('l') => ToggleMapMode,  // either key — cycle
```

`main_loop::run` 接到 ToggleMapMode 就 flip `LocalMapPanel::display_mode`。
state 保存在 `run()` 的 local stack，tokio_select 裡 mut borrow 安全
（同 ZoaPanel 模式）。

## Phase Plan

| Phase | 內容 | 檔案 | 估 test |
|-------|------|------|---------|
| **P1** | Tile primitive — `render_tile(room, width, height, is_current, color)` + zone_kind → Color + CJK-safe name 截斷 | `src/tui/panels/local_map_tile.rs` (new) | +15 |
| **P2** | Grid flow — `compute_grid()` 計算 cols×rows，overflow 處理 `…+N`；`render_grid()` 把 tiles 組合 | 同上 | +10 |
| **P3** | Mode toggle — `LocalMapPanel` struct + `DisplayMode` enum + events.rs `g`/`l`；main_loop 接 ToggleMapMode | `src/tui/panels/local_map.rs`, `events.rs`, `mod.rs` | +6 |
| **P4** | `@` overlay + auto-fallback + spec + 手動驗證 | 同 P3 + `doc/specs/codeforge-mud-engine.md §5` | +5 |

**Total**: ~36 tests, ~2 新 files, 3 modified files

## Risks

1. **ANSI 渲染在非 TTY pipe 下破圖** —— `codeforge tui` 永遠是 TTY，但快取
   測試用 `paint` to `Vec<u8>` 需要 strip colour，或 golden test 只測 plain
   frame lines（不管 ANSI escape）。已經是目前 render 測試的做法。
2. **CJK tile 寬度** —— tile 10 cols，扣 border 2 cols，CJK 名可放 4 chars
   (8 visible cols)。「後端」「前端」「核心」等 3-char 名 OK，5-char 以上
   得截斷 + `…`。已有 `pad_to_width` / `clip_to_width` 可用。
3. **Wide mode 與 Zoa 競爭** —— Wide 時 map panel 只有 ~46 cols，剛好容得下
   4 tile per row；Narrow 時 map 被收起（LayoutMode::Narrow），tile 模式
   根本不跑，沒有衝突。
4. **`zone_kind` 來源** —— 現有 `RoomSummary` 只有 `directory` 字串，沒
   zone kind。要從 `src/world/` 的 language→zone mapping 查，或 cheap heuristic
   （`src/` → code zone default blue, `doc/` → memory magenta, `.github/` → tooling grey）。
   **Scope 決策**：用 heuristic，不 join `world` table，避免 DB round-trip
   per paint。
5. **Grid 與 list 切換時的 clear artifact** —— 不同 mode 的 lines 數可能不同，
   `render::paint` 會 `Clear::All` 然後重畫，所以不會殘留。已驗證。

## Roadmap 影響

- **不在 spec roadmap** —— 與 tui-foundation 一樣歸 "UX polish" 軌道
- **獨立於 Phase 4 (Zoa)** —— Phase 4 只填 Zoa 的 frame sets，不會碰 local_map
- **獨立於 Phase 5 (Nation)** —— Nation 加玩家頭像 marker，可以 later append
  到 tile 上；不 block 本 plan
- **建議執行順序**：Phase 4 Zoa full impl → 本 plan → Phase 5a。原因：
  先把 Phase 4 roadmap 完成再做 UX polish，保持 spec 節奏

## 下一步

User 確認後：
1. 把本 plan 升為 project（`doc/projects/2026-04-20-tile-map-localmap/`）
2. 建 branch `feature/tile-map-localmap`
3. 進 dev-flow L workflow P1-P4 → QG → 2 輪 review → merge

或 user 要調整 scope（例如改為持久化 mode、或加 fog-of-war），可先
修本 plan，下一 session 再 promote。
