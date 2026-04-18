# TUI Foundation — adaptive layout + attach + Zoa base

Started: 2026-04-18
Branch: `feature/tui-foundation`
Plan: `doc/plans/2026-04-18-tui-foundation.md`

## Project Goal

> **Final goal**: 讓 `codeforge tui` 能在任何 terminal 尺寸下穩定運作、能自動寄生 Claude Code 的 tmux session、並為 Zoa ASCII 動畫鋪好 rendering 管線。
>
> **Success criteria**:
> 1. TUI 在 60×10 不 panic（`cargo test` + 手動 resize）
> 2. 三段 breakpoint (Narrow/Standard/Wide) 欄位分配正確（`cargo test layout::*`）
> 3. `codeforge attach` 在 `$TMUX` 下能 split pane 並跑 tui（手動）
> 4. 非 tmux 環境給非 0 exit + 指示訊息（`cargo test` + 手動）
> 5. Zoa placeholder 可 render idle 4-frame cycle（`cargo test zoa::*`）
> 6. Wide mode（≥120 cols）allocate 24 cols 給 Zoa（layout test）
> 7. 全綠：`cargo test` + `cargo clippy` 零 warning
>
> **Scope boundary**:
> - **Include**: LayoutMode enum + breakpoints、Zoa placeholder module（1 emotion 4 frames）、`codeforge attach` subcommand、spec §1 TUI 章節更新
> - **Exclude**: Zoa 多情緒 frame sets（→ Phase 4）、mood→emotion mapping（→ Phase 4）、tile-grid map（→ 獨立 project）、Windows tmux（待 CC 支援）、iTerm2 native split

## Progress

| Phase | Status | Tests | Commit |
|-------|--------|-------|--------|
| P1 Adaptive Layout | ✅ | +20 | `26a5686` |
| P2 Zoa Foundation | ✅ | +14 | `251dfcd` |
| P3 `codeforge attach` | ✅ | +11 | `3c0c9a8` |
| P4 Integration + doc | ✅ | +5 | `cd00c9c` |
| QG cleanup | ✅ | 585 passed | `260d11e` |
| Review r1 (3 IMPORTANT) | ✅ | — | `67e9473` |
| Review r2 | ✅ CLEAN | — | — |
| Merge to main | ✅ | 585 passed | `3a65709` |

**Result**: 555 → 585 tests (+30, matches plan estimate). All success
criteria satisfied. Clippy clean on all new code.

Final archive: 2026-04-18
