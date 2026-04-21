# Tile-Grid Local Map — Revival World 風

Started: 2026-04-21
Branch: `feature/tile-map-localmap`
Plan: `doc/plans/2026-04-20-tile-map-localmap.md`（完整設計細節在 plan，這裡只追進度）

## Project Goal

> **Final goal**: 把 Local Map 從線性清單升級成 tile-grid，每個 top-level
> dir 渲染成帶顏色邊框 + CJK 名稱 + mob badge 的色塊；pet 位置用 `@`
> overlay；可按鍵在 list / grid 之間切換；在 panel 太窄時自動 fallback
> 回 list。
>
> **Success criteria**:
> 1. Tile 10×3 border + 名稱 + badge 對齊、CJK-safe（`cargo test render_tile_*`）
> 2. 10 個 dir 在 40×20 面板 → 4×3 grid 不重疊（`cargo test compute_grid_*`）
> 3. 5 zone kind 各有獨立 color（rust / memory / daemon / tui / db，unit + ANSI snapshot）
> 4. cwd=`src/cli/...` → `src` tile 顯示 `@`（unit test）
> 5. TUI 內按鍵 `g`/`l` 即時切換 list/grid，無 flicker（手動 + event test）
> 6. panel <30 cols 時自動 fallback list render（layout test）
> 7. `cargo test` 全綠 + `cargo clippy --bin` 零新 warning
>
> **Scope boundary**:
> - **Include**: LocalMapPanel 狀態 + DisplayMode enum、local_map_tile.rs
>   (tile primitive + grid compute)、zone-kind color mapping（heuristic 不
>   join world table）、keypress toggle（in-memory mode state）、spec §5 更新
> - **Exclude**: settings 持久化（下一輪）、fog-of-war（需 scan 狀態追蹤）、
>   drill-down subdir、Revival World 的右側羅盤/時鐘/天氣 widget、其他
>   玩家 marker（Phase 5a Nation 範疇）

## Progress

| Phase | Status | Tests | Commit |
|-------|--------|-------|--------|
| P1 Tile primitive | ⏳ pending | — | — |
| P2 Grid flow | ⏳ pending | — | — |
| P3 Mode toggle + events | ⏳ pending | — | — |
| P4 `@` overlay + doc | ⏳ pending | — | — |
| QG (check + clippy + test) | ⏳ pending | — | — |
| Review r1 | ⏳ pending | — | — |
| Fix r1 findings | ⏳ pending | — | — |
| Review r2 | ⏳ pending | — | — |
| Merge + archive | ⏳ pending | — | — |

**估**: ~36 tests，2 新 files（`local_map_tile.rs`）+ 3 modified

Last updated: 2026-04-21
