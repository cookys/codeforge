# Plan — TUI Foundation (adaptive + attach + Zoa base)

Date: 2026-04-18
Branch: `feature/tui-foundation`
Project: `doc/projects/2026-04-18-tui-foundation/`

## Motivation

`codeforge tui` 目前鎖死 100×40 target + `MAP_FRACTION=0.4`，寄生到 Claude Code 的 tmux split pane 會爆版。
Research（Steam HW Survey 2025-11 / WezTerm / tmux 社群）顯示：

- 1080p 筆電 53%（主流），1440p 27″ 21%
- 現代 terminal 實際 180–220 cols × 45–55 rows
- tmux companion pane 慣例 `-p 30` 30% 寬
- ratatui app 最小可用 60–80 cols（Zellij 以下崩潰）

固定比例在 1080p 會 < 60 cols（崩潰區），必須 adaptive。同時為 Phase 4 的 Zoa ASCII 動畫預留 rendering 管線 —— 不這樣做 Zoa 落地時 layout 要二次改。

## Breakpoint 設計

| Mode | Cols | 畫面 |
|------|------|------|
| Compact | <60 | 不開 tui — fallback 建議 `codeforge statusline` |
| Narrow | 60–79 | pet status(3) + combat log（單欄，map 收起） |
| Standard | 80–119 | pet(3) + map(40%) + combat log(60%) — 現狀 |
| Wide | 120+ | pet(3) + zoa(24) + map + combat log |

## Phase 1 — Adaptive Layout

**Files**: `src/tui/layout.rs` (refactor), `src/tui/main_loop.rs` (route)

**Changes**:
- 加 `LayoutMode { Compact, Narrow, Standard, Wide }` enum
- `LayoutMode::from_size(cols, rows) -> LayoutMode`
- `compute_regions(mode, size) -> Regions { pet, zoa: Option<Rect>, map: Option<Rect>, log: Rect }`
- `main_loop` 在 resize event 時 re-compute
- Compact mode 印 fallback 訊息後退出（不進 alt-screen）

**Tests** (+15):
- `from_size_*` breakpoint boundary（59/60/79/80/119/120）
- `compute_regions_narrow_has_no_map`
- `compute_regions_standard_has_map_40pct`
- `compute_regions_wide_allocates_24_to_zoa`
- `compute_regions_compact_returns_minimal`
- resize simulation

## Phase 2 — Zoa Foundation

**Files**: `src/tui/panels/zoa.rs` (new)

**Changes**:
- `Emotion { Idle, Happy, Tired, Hunting }` — **enum 4 種，P2 只實作 Idle frames**
- `frames_for(emotion) -> &'static [&'static str]`
- `ZoaPanel { emotion, frame_idx, last_tick_at }`
- `render(width, height) -> Vec<String>` — 以目前 frame_idx 取 frame，pad 到 24×18
- `tick(now) -> ()` — 切 frame（250ms/frame = 4 Hz）
- `should_render(width) -> bool` — width >= 24
- Idle frame set：4 × 18-row ASCII，可以用簡單 breathe loop（眼睛 · → º → · → º）

**Tests** (+10):
- `zoa_idle_frames_are_18_rows_24_cols`
- `zoa_render_returns_exactly_height_lines`
- `zoa_tick_cycles_frames`
- `zoa_should_not_render_when_width_lt_24`
- `zoa_unused_emotions_return_placeholder`（其他 3 個先 return idle + TODO）

## Phase 3 — `codeforge attach`

**Files**: `src/cli/attach.rs` (new), `src/main.rs` (clap dispatch)

**Changes**:
- 新 subcommand `codeforge attach [--size N]`
- Detect `$TMUX` env var
- 若有 → spawn `tmux split-window -h -p {size} 'codeforge tui'`，`size` default 30
- 若無 → `anyhow::bail!("not in tmux; start tmux then run: codeforge attach")`
- `--size N` clamp 到 [20, 70]

**Tests** (+5):
- `not_in_tmux_returns_error`
- `size_clamps_to_range`
- `build_tmux_args_default`
- `build_tmux_args_custom_size`
- `env_detection`（mock `$TMUX`）

## Phase 4 — Integration + doc

**Files**: `src/tui/panels/mod.rs`, `src/tui/main_loop.rs`, `doc/specs/codeforge-mud-engine.md`, `README.md`

**Changes**:
- `main_loop` 按 LayoutMode 決定 render 哪些 panel
- Zoa panel 只在 Wide mode render
- spec §1 TUI 章節加 breakpoint 表 + attach 說明
- README 加使用範例：
  ```bash
  codeforge tui                  # standalone
  codeforge attach               # inside tmux, opens companion pane
  codeforge attach --size 40     # override split ratio
  ```

**Tests** (+5):
- end-to-end render snapshot at 60×20、100×40、180×45
- panel routing at each LayoutMode
- doc link validity

## Total

- Tests: +35
- Files touched: 5 new / 3 modified
- Schema change: none
- New CLI command: `codeforge attach`

## Risks

- `tmux split-window` 在非 interactive context 可能 silent fail —— P3 測手動驗證是關鍵
- Zoa ASCII frame 在 different fonts 下對齊可能跑掉 —— 先用簡單 symbol 不用 box-drawing
- resize 中間切換 Wide↔Standard 可能閃爍 —— main_loop 要 debounce resize event

## Roadmap 影響

- Zoa rendering 管線提前到現在；Phase 4 只剩 emotion frames + mood→emotion mapping
- Adaptive layout / attach 新增項（不在 spec roadmap），列入「UX polish」
