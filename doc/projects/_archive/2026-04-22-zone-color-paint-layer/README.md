# Zone Color Paint-Layer Integration (B11)

Started: 2026-04-22
Branch: `feature/zone-color-paint-layer`
Plan: `doc/plans/2026-04-21-zone-color-paint-layer.md`（完整設計、Risks、Design Sketch 在 plan；本檔只追進度）
Tracks: `doc/BACKLOG.md#B11`

## Project Goal

> **Final goal**: 把 render 層從「純文字 `Vec<String>`」升級成「帶樣式的
> `Vec<StyledLine>`」，讓 paint 能 emit per-range `SetForegroundColor` /
> `ResetColor`；local_map tile-grid 的 border 依 `zone_color(dir)` 著色，
> 名稱 / badge 保留 default fg；其他 panel 行為完全不變（用單色 span
> 包裝既有 render 輸出）；移除 `zone_color` 的 `#[allow(dead_code)]`；
> 加 ANSI snapshot test 證明逃脫序列確實在 paint 輸出裡。
>
> **Success criteria**:
> 1. `StyledSpan` / `StyledLine` 型別加 helpers 全綠（`cargo test tui::styled`）
> 2. 既有 panel（pet / combat_log / zoa / local_map List）視覺不變 —— paint 輸出與 main 邏輯等價、全部走 default fg
> 3. Grid tile 的 border 在 paint 輸出裡至少出現 1 次 ANSI fg escape（`\x1b[38;5;...` 或 `\x1b[3[1-7]m`）
> 4. `zone_color` 的 `#[allow(dead_code)]` 移除，`cargo clippy --all-targets` 零新 warning
> 5. 全綠：`cargo test` 零 fail + `cargo clippy --bin` 零新 warning
> 6. tile-map-localmap README criterion #3 移除「部分達成」注記
> 7. B11 從 `doc/BACKLOG.md` 移除
>
> **Scope boundary**:
> - **Include**: `src/tui/styled.rs`(新)、`src/tui/render.rs` PositionedLine/paint 遷移、4 個 panel render 包 StyledLine::plain、`local_map_tile.rs` border 著色、zone_color 換 `crossterm::style::Color`、既有 test 批次改 `plain_text()`、新 ANSI snapshot test、移除 `#[allow(dead_code)]`、tile-map README + BACKLOG 更新
> - **Exclude**: bg / bold / italic / underline、tile 內容 per-char 漸層色、dirty-region repaint optimization、Theme system（user 自訂 zone color mapping）、List mode row 著色

## Design Decisions（user 拍板 2026-04-22）

1. **`PositionedLine` 遷移策略 = A**：直接改 field `text: String` → `spans: Vec<StyledSpan>`，所有 caller 一次遷移。理由：refactor 性質一次做到位單純，interim state 反而要後續清理。
2. **`StyledLine::Display` = 不實作**：用顯式 `plain_text()`；未來 bug 好 trace，不隱藏「這是帶樣式 line」的事實。
3. **`zone_color` 回傳型別 = 換 `crossterm::style::Color`**：與 paint stack 統一（paint 已用 crossterm `queue!` / `SetForegroundColor`），termcolor 是上個 plan 沒對齊的遺留，順手 housekeeping。

## Progress

| Phase | Status | Tests | Commit |
|-------|--------|-------|--------|
| P1 `styled.rs` 型別 + helpers | ✅ done | +12 | `93742f2` |
| P2 PositionedLine 遷移 + paint + 4 panel 包裝 + test migration | ✅ done | +0 (migration) | `fcdaf3c`,`ae4e959` |
| P3 Tile border 著色 + zone_color 換 crate + 移除 allow(dead_code) | ✅ done | +8 (5 clip + 6 style − 3 take_cols) | `1761a97` |
| P4 ANSI snapshot + README + BACKLOG B11 移除 | ✅ done | +3 | `90bde6c` |
| QG (check + clippy + test) | ✅ done (零新 clippy warning) | — | `e3ccc84` |
| Review r1 | ✅ done (2 IMPORTANT + 1 NIT + 3 doc NITs) | — | — |
| Fix r1 findings | ✅ done | — | `284defc` |
| Review r2 | ✅ CLEAN | — | — |
| Merge + archive | ✅ done | — | `be4feb5` |

**實際**: 643 → 666 tests（+23：+12 P1 styled helpers + 0 P2 migration + +8 P3
net (+5 clip_to_width + +6 P3 styling − 3 take_cols removed) + +3 P4 ANSI snapshot）。
1 新 file（`src/tui/styled.rs`, 334 行）+ ~8 modified。zero new clippy
warnings vs main baseline.

## 設計取捨紀錄

### Pre-P1 — 既有 test surface 盤點（from 2026-04-21 digest）

`render.rs` 已存在 3 個 `build_frame_grid_mode_*` tests、`panels/local_map.rs` 有 1 個 `dispatcher_grid_mode_renders_tile_borders`，都會被 P2 的 `PositionedLine.text` → `spans` migration 打到：

| 檔 | 行 | Test |
|----|----|------|
| `src/tui/render.rs` | 569 | `build_frame_grid_mode_renders_tile_borders_in_local_map_region` |
| `src/tui/render.rs` | 634 | `build_frame_grid_mode_shows_at_overlay_for_current_directory` |
| `src/tui/render.rs` | 660 | `build_frame_grid_mode_cjk_directory_survives_pipeline` |
| `src/tui/panels/local_map.rs` | 241 | `dispatcher_grid_mode_renders_tile_borders` |

這些現在 assert **box-drawing 字元**（`┌─┐│└┘`）出現在 `PositionedLine.text` 裡。P2 遷移後改用 `plain_text()` 就能繼續過；assertion 語意（char-level）**不會**跟 P4 的 `grid_tile_border_emits_ansi_fg_escape`（escape-level）重複。

### P3 — termcolor → crossterm Color variant naming 陷阱

`zone_color` 目前用 `termcolor::Color::Ansi256(8)` 給 unknown/target/.git。crossterm 對應不是 `Ansi256` 而是 `AnsiValue`：

```rust
// termcolor
Color::Ansi256(8)

// crossterm::style::Color
Color::AnsiValue(8)
```

其他常用色在兩個 crate 都叫 `Red` / `Magenta` / `White` / `Cyan` / `Yellow`，直接 rename import 即可。變體名這個 gotcha P3 換 crate 時需手動調，不是純機械替換。

## Completion Summary (2026-04-22)

- All 7 success criteria met (see Project Goal 章節 1-7)
- Merge commit: `be4feb5` on `main`, 14 files / 961 insertions / 310 deletions
- Test count: 666 passed / 0 failed (from 643 main baseline, +23 net)
- Clippy: zero new warnings vs `main` (verified via `diff` on sorted unique-warning sets, see `e3ccc84` commit message)
- Dependency footprint: `termcolor` still in `Cargo.toml` but `zone_color` no longer uses it — follow-up opportunity to drop the crate if no other call site needs it
- Phase sequence: P1 → P2.1 → P2.2 → P3 → P4 → QG → r1 → fix → r2 → merge → archive; no rollback, no scope creep, 3 design decisions (#1/#2/#3) all held through implementation

Last updated: 2026-04-22
