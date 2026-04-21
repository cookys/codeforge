# Plan — Tile-Map Zone Color: Paint-Layer Integration (B11)

Date: 2026-04-21
Status: **Draft — awaiting user green-light to promote to project**
Branch (when promoted): `feature/zone-color-paint-layer`
Size: **L**（touches 每個 panel render + paint 層資料模型 refactor；≥ 5 files；行為不變 panel + 新上色 panel 並存）
Tracks: `doc/BACKLOG.md#B11`

## Motivation

tile-map-localmap（2026-04-21 merge）把 `zone_color(directory) → termcolor::Color` 的 mapping 做完 + unit test 通過，但 `src/tui/panels/local_map_tile.rs::zone_color` 目前掛 `#[allow(dead_code)]` —— paint 層沒接上。

為什麼沒一起做：`PositionedLine.text: String` + `paint` 的整行 `Print(&text)` 只能「一整行一個顏色」，tile 要的是 **border 上色、名稱/badge 保持 default fg** —— 需要 per-char（更精確說：per-range）著色。這是 render / paint 層的資料模型 refactor，不屬於 tile-map-localmap scope。

本 plan 專治此事，順便完成 tile-map-localmap 的 success criterion #3 ANSI snapshot test。

## Final Goal

> 把 render 層從「純文字 Vec<String>」升級到「帶樣式的 Vec<StyledLine>」，
> 讓 paint 能 emit per-range SetForegroundColor / ResetColor；
> local_map tile-grid 的 border 依 `zone_color(dir)` 著色，名稱/badge 保留
> default fg；其他 panel 行為完全不變（用單色 span 包裝既有 render 輸出）；
> 移除 `zone_color` 的 `#[allow(dead_code)]`；加 ANSI snapshot test 證明
> 逃脫序列真的在 paint 輸出裡。

### Success Criteria

| # | 條件 | 門檻 | 驗證 |
|---|------|------|------|
| 1 | `StyledSpan` / `StyledLine` 型別出現並通過 unit test | visible-width 保持 CJK-safe、plain_text() helper 正確 | `cargo test tui::styled` |
| 2 | 所有既有 panel（pet / combat_log / zoa / local_map List）視覺**不變** | paint 輸出與 main 同條件下**邏輯等價**（顏色 = 全部預設） | golden test vs main snapshot |
| 3 | Grid tile 的 border 有 ANSI fg escape | `paint` 到 `Vec<u8>` 後 grep `\x1b\[[0-9;]*3[1-7]m` 至少 1 次 | ANSI snapshot test |
| 4 | `zone_color` `#[allow(dead_code)]` 移除且 `cargo clippy` 不報 warning | clippy zero-diff vs main（含新 code） | CI-style |
| 5 | 全綠 | `cargo test` + `cargo clippy --bin` 零新 warning | CI-style |
| 6 | tile-map-localmap README 的 criterion #3 更新為完全達成 | 移除「部分達成」注記 | manual |
| 7 | B11 從 BACKLOG 移除 | archive + INDEX reconcile | manual |

### Scope Boundary

**Include**:
- `src/tui/styled.rs`（新）—— `StyledSpan`、`StyledLine`、`plain_text()`、`visible_width()` helpers + pad/clip to width
- `src/tui/render.rs` —— `PositionedLine.text: String` → `PositionedLine.spans: Vec<StyledSpan>`；`paint` 改走 `SetForegroundColor` / `ResetColor` queue
- 每個 panel 的 render：
  - `panels/pet.rs`、`panels/combat_log.rs`、`panels/zoa.rs`、`panels/local_map.rs` (List path) —— 用單色 span wrap（no visual change）
  - `panels/local_map_tile.rs::render_tile` + `render_grid` —— border 用 `zone_color(&dir)`，名稱/badge 用 default
- 既有測試遷移 —— `l.text.contains(...)` → `l.plain_text().contains(...)`（新 helper）
- 新 ANSI snapshot test
- 移除 `#[allow(dead_code)]` on `zone_color`
- tile-map-localmap README criterion #3 標註更新 + BACKLOG B11 移除

**Exclude**（下一輪再做）:
- Background colour / bold / italic / underline —— 本輪只做 fg colour
- Grid tile 內容（名稱、badge）的 per-char 色彩漸層
- 主動 repaint dirty region optimization —— 保持現有 `Clear::All` + 全重繪
- Theme system（user 自訂 zone color mapping）—— zone_color 仍 hardcoded
- List mode 的 row 著色 —— 暫維持純白，視覺改動集中在 Grid

## Design Sketch

### 新型別

```rust
// src/tui/styled.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    // bg / bold / italic 等留給未來；本輪只做 fg
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

impl StyledLine {
    pub fn plain(text: impl Into<String>) -> Self { ... }  // 單一 default-fg span
    pub fn plain_text(&self) -> String { ... }             // concat 所有 span 的 text
    pub fn visible_width(&self) -> usize { ... }           // UnicodeWidthStr on plain_text
    pub fn pad_to_width(&self, w: usize) -> Self { ... }   // 補空白 span 到 w cols
}
```

### PositionedLine 改動

```rust
// src/tui/render.rs
pub struct PositionedLine {
    pub col: u16,
    pub row: u16,
    pub spans: Vec<StyledSpan>,   // was: text: String
}
```

**Option A**: 直接改 field，破壞 API。所有 caller 同時遷移。
**Option B**: 加 `spans` field 並保留 `text` deprecated alias。漸進遷移。

> **Design decision #1（待 user 拍板）**：A 還是 B？
> - A 簡潔、無兼容性負擔，但 diff 量大（所有 test + panel 同時改）
> - B 漸進、可 phase-by-phase land，但會留一個 interim state 要後續清理
> - 推薦 **A**：改動是 refactor 性質，一次做到位反而單純

### Paint 層

```rust
// src/tui/render.rs::paint
for line in &frame.lines {
    if line.spans.is_empty() { continue; }
    queue!(out, cursor::MoveTo(line.col, line.row))?;
    for span in &line.spans {
        match span.fg {
            Some(color) => {
                queue!(out, SetForegroundColor(color), Print(&span.text), ResetColor)?;
            }
            None => {
                queue!(out, Print(&span.text))?;
            }
        }
    }
}
```

### Tile render 著色

```rust
// src/tui/panels/local_map_tile.rs::render_tile
pub fn render_tile(room: &RoomSummary, width: usize, height: usize) -> Vec<StyledLine> {
    let color = zone_color(&room.directory);
    let inner = width - 2;

    // Top border: 整條用 zone color
    let top = StyledLine {
        spans: vec![StyledSpan {
            text: format!("┌{}┐", "─".repeat(inner)),
            fg: Some(color),
        }],
    };

    // Name row: border│ default name │border
    let name = StyledLine {
        spans: vec![
            StyledSpan { text: "│".into(), fg: Some(color) },
            StyledSpan { text: pad_to_width(&room.directory, inner), fg: None },
            StyledSpan { text: "│".into(), fg: Some(color) },
        ],
    };

    // ... badge row 同理
}
```

### Panel 遷移（無視覺改動）

其他 panel 的 render function 把目前的 `Vec<String>` 改成 `Vec<StyledLine>`，
每個字串用 `StyledLine::plain(s)` 包裝即可 —— fg=None 會走 paint 層的
default branch，輸出零 ANSI escape，與 main 完全等價。

### Test 遷移

```rust
// Before:
assert!(lines[1].contains("Ferris"));

// After:
assert!(lines[1].plain_text().contains("Ferris"));
```

批次替換可以用 sed，對 90%+ test 有效；剩下 edge case 手動調。

> **Design decision #2（待 user 拍板）**：要不要在 StyledLine 實作 `Display`
> trait 讓 `{line}` 直接印 plain_text？這樣既有 test 幾乎不用改，但會
> 隱藏「這是帶樣式的 line」的事實，未來 bug 較難 trace。
> 推薦 **不做** —— 顯式 `plain_text()` 比較清楚。

### ANSI Snapshot Test

```rust
#[test]
fn grid_tile_border_emits_ansi_fg_escape() {
    // ... build frame with LocalMapPanel{Grid} + mobs in "src" ...
    let mut sink: Vec<u8> = Vec::new();
    paint(&frame, &mut sink).unwrap();
    let raw = String::from_utf8_lossy(&sink);
    // zone_color("src") = Red = ANSI fg 31
    assert!(
        raw.contains("\x1b[38;5;") || raw.contains("\x1b[31m"),
        "grid tile must emit fg color escape"
    );
}
```

## Phase Plan

| Phase | 內容 | 檔案 | 估 test |
|-------|------|------|---------|
| **P1** | 新 `styled.rs` + `StyledSpan` / `StyledLine` + helpers | `src/tui/styled.rs`（new）| +10 |
| **P2** | `PositionedLine` 遷移 + `paint` 走新資料模型；所有 panel render 用 `StyledLine::plain` 包裝（零視覺改動）；既有 test 批次改 `.plain_text()` | `render.rs`、4 panel、~所有 test | +0 (migration) |
| **P3** | `render_tile` / `render_grid` 用 `zone_color` 給 border 上色；移除 `#[allow(dead_code)]` | `local_map_tile.rs` | +5 |
| **P4** | ANSI snapshot test + README criterion #3 更新 + BACKLOG B11 移除 | `render.rs` test、`_archive/2026-04-21-tile-map-localmap/README.md`、`BACKLOG.md` | +3 |

**Total**: ~18 tests，1 新 file，~8 modified

## Risks

1. **Test 遷移炸一大片** —— 既有 render / panel tests 幾乎都 assert
   `l.text.contains(...)`，批次 sed `l\.text` → `l.plain_text()` 應該命中
   ≥90%，剩下可能在 destructure pattern 或 `l.text.len()` 要手動調。
   P2 的 +0 test 是因為「行為不變，既有測試要繼續通過」而非「沒寫新 test」。

2. **ANSI escape 在非 TTY sink 下輸出問題** —— `crossterm::queue!` 對
   `Vec<u8>` sink 會照樣 emit escape，這是 snapshot test 的目標。正式 TUI
   用 `io::stdout()` 是 TTY，行為一致。不是 issue。

3. **zone_color 現在是 `termcolor::Color`，paint 用 `crossterm`** —— 兩個
   crate 的 Color 型別不同。要麼改 zone_color 回傳 `crossterm::style::Color`
   （更好，paint 層用 crossterm，一致），要麼寫 adapter。
   > **Design decision #3（待 user 拍板）**：`zone_color` 回傳型別換成
   > `crossterm::style::Color`？推薦 **換**，termcolor 只是因為 tile-map
   > plan 當時沒確認 paint stack 就寫了 termcolor，現在 paint 是 crossterm
   > 生態，統一更單純。

4. **Wide mode 下 Zoa 也會走新 pipeline** —— Zoa render 目前是 Idle 4 frame
   純字串，遷移到 `StyledLine::plain` 零風險。Phase 4 之後 Zoa 若要加顏色
   自然受益。

5. **paint 的 `Clear::All` 每 tick 清整個 alt-screen** —— 新 ANSI escape
   的 stateful nature 不會跨 frame 汙染，每個 span 自己 `ResetColor`。OK。

## Roadmap 影響

- **tile-map-localmap success criterion #3 完整達成** —— 完成後可把
  `_archive/2026-04-21-tile-map-localmap/README.md` 的「部分達成」註記移除
- **Phase 4 Zoa full impl** —— 本 plan 完成後，Zoa 加 emotion colour
  幾乎是 append：emotion → Color 的 mapping 送進 StyledLine 即可
- **未來 Theme system**（user 自訂 zone color）—— StyledSpan 已抽象到
  per-span fg，plug-in 一個 theme lookup 就能動；不在本 plan 範圍
- **建議執行順序**：本 plan → Phase 4 Zoa（順便遷移 emotion colour）→
  Phase 5a Nation。原因：先把 paint-layer refactor 做完再擴 Zoa，避免
  Phase 4 又要重新走一次 data-model 改動

## 下一步

User 看過後：

1. **先確認 3 個 Design Decisions**（A/A/換 crossterm::Color 為預設）
2. 確認 scope include/exclude、phase 拆分、risks
3. Approve 後把本 plan promote 為 project：
   - `doc/projects/YYYY-MM-DD-zone-color-paint-layer/`
   - 建 branch `feature/zone-color-paint-layer`
   - dev-flow L workflow P1-P4 → QG → 2 輪 review → merge → archive
4. 或改 scope（例如要一併做 bg / bold）可先修本 plan 再 promote
