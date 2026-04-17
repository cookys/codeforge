# CodeForge Phase 2c — TUI + Local Map

> 狀態：✅ **完成並合併**（2026-04-17，merge commit on main）
> 建立：2026-04-17
> Branch：`feature/phase2c-tui`（已合併並刪除）
> 前置：Phase 2b ✅（MOB + combat + loot）
> Spec 參考：`doc/specs/codeforge-mud-engine.md` §1（Local Map）+ §5（TUI 渲染架構）
> 模式：CEO level 3（autonomous）

## Completion Summary

All 6 phases (P1-P6) + QG + 2 review rounds done in a single session.

- **Tests**: 233 passing（baseline 164 → +69）。Debug + release 皆綠，stable across 3 consecutive runs。
- **Perf**: `render_budget_under_16ms_average` 通過；debug mode < 16ms avg。
- **Clippy**: Phase 2c 所有檔零 warning。
- **Review**: round 1 抓到 2 個 IMPORTANT（Notify lost-wakeup race + keyboard task 無法合作退出，會吃 shell keystroke）；round 2 verified RESOLVED（mpsc::channel(1) + Arc<AtomicBool> running flag）。APPROVED FOR MERGE。

### Post-review fix highlights
- 切換 shutdown 通道從 `Arc<Notify>` 到 `tokio::sync::mpsc::channel::<()>(1)`，buffered capacity 1 解 lost-wakeup race。
- 加 `Arc<AtomicBool> running` flag — main loop `store(false, Release)`，keyboard task 每次 poll 前 `load(Acquire)`，100ms 內乾淨退出，不會吃 user 的下一個 shell keystroke。

## Project Goal

> **Final goal**：新指令 `codeforge tui` 啟動一個 alt-screen TUI 面板，顯示 pet status + combat log + local map 三區塊；1 Hz 從 SQLite 讀取 derived state 做 diff 渲染；鍵盤 `q` / Ctrl-C / Esc 乾淨退出（restore terminal，不污染 scrollback）。
>
> **Success criteria**（可量化、可驗證）：
> 1. `cargo test` 全綠，新增 ≥ 15 個 test（panel renderers + local_map data + layout 算術 + terminal guard）
> 2. 純 panel renderer 函式（data → `Vec<Line>`）的 golden string test：給定固定資料輸出固定字串
> 3. Local Map：給一個 repo fixture（3 個頂層目錄、mobs.origin_path 指向其中 2 個）→ render 包含「目錄名 + MOB 數 + ▶ 當前目錄」
> 4. Terminal guard drop test：模擬 panic，確認 `LeaveAlternateScreen` 被呼叫（透過 guard 的 Drop 而非只靠正常 exit）
> 5. 渲染 budget `< 16ms` 平均（一個完整 diff render pass；60 FPS 餘裕）
> 6. `cargo clippy` Phase 2c 檔零 warning
> 7. Code review round 2 無 CRITICAL/IMPORTANT finding
>
> **Scope boundary**：
> - **INCLUDE**：
>   - `codeforge tui` 指令 + alt-screen 進出
>   - 3 區塊 layout：PetStatus / LocalMap / CombatLog
>   - 1 Hz refresh + keyboard quit（q / Esc / Ctrl-C）
>   - Schema v4：`mobs.origin_path TEXT`（nullable）+ scanner 寫入
>   - Local Map 資料 provider：依頂層目錄分組，標示 ▶ 當前目錄
>   - 終端狀態 RAII guard（panic-safe）
> - **EXCLUDE（延後）**：
>   - World Map → Phase 3a
>   - tmux split 整合 → 不綁 tmux，單一 terminal 即可運作
>   - Pet animation / sixel → Phase 4
>   - AI Commentary 顯示 → Phase 3c
>   - Zone Mastery bars → Phase 3d
>   - Strategy Mode 快捷鍵 → Phase 3b
>   - Interactive actions（`use item` / `strategy` 在 TUI 內）→ Phase 3e
>   - MUD 式 scroll region（取代 alt-screen）→ 不做；alt-screen 已涵蓋「不污染 scrollback」需求

## Architecture Decisions

| 決定 | 內容 | 理由 |
|------|------|------|
| TUI 技術 | 直接用 crossterm（`cursor::MoveTo` + `ClearType::UntilNewLine` + alt-screen），不引入 ratatui | spec §5 已指定 crossterm；ratatui 對 3 區塊簡單面板過剩；Cargo.toml 最小化 |
| 資料源 | TUI 純讀 SQLite（WAL + read-only connection），無 IPC 到 daemon | 沿襲 Phase 2a Two-writer rule：TUI 屬於 CLI 的 read-only 面向；daemon 掛掉時 TUI 仍能顯示 pet_snapshot 最後狀態 |
| Refresh | 1 Hz 固定 timer（非 DB trigger）+ 鍵盤 event channel | daemon tick 本來就 60s 一次，1 Hz 已遠高於資料變化頻率；可同時吸 keyboard 無需額外 event pipeline |
| Terminal guard | RAII `TerminalGuard` struct 在 `new()` 進 alt-screen + raw mode，在 `Drop` 離開 | panic / 非正常 exit 不會留壞 terminal；單一 guard 擁有恢復責任，符合「no silent failures」原則 |
| MOB→目錄映射 | 在 `mobs` 表加 nullable `origin_path TEXT`（schema v4），scanner 寫入相對路徑 | 現有 `mobs.name` 有時含 `@ path`，但非結構化；新增欄位讓 Local Map 與未來 Room detail 都能 query |
| Diff render 策略 | 每次完整重繪（clear screen + re-draw）；非逐格 diff | 3 區塊面板 < 60 行，1 Hz 頻率，整體重繪成本遠低於 diff 演算法複雜度；真需要逐格再優化 |
| Scroll region | **不做**；用 alt-screen 取代 | alt-screen 退出後 scrollback 乾淨，體驗優於 scroll region；spec §5.Scroll Region 是 `如果不用 tmux split` 的備案，alt-screen 涵蓋該需求 |

## Success Criteria (KR)

| KR | 驗證 | 狀態 |
|----|------|------|
| `codeforge tui` 指令存在且 clap 解析不報錯 | `cargo build + codeforge tui --help` | ✅ |
| Schema v4 migration 跑過 `origin_path` 欄位存在且 nullable | migrations test | ✅ |
| Scanner 寫入 `origin_path = 相對於 scan root 的 path` | scanner unit test | ✅ |
| Local Map data provider：3 dir × 2 含 mobs → 回傳 3 `RoomSummary` | local_map unit test | ✅ |
| PetStatus panel renderer golden | snapshot test with fixed PetState | ✅ |
| CombatLog panel renderer golden | snapshot test with fixed rows | ✅ |
| LocalMap panel renderer golden with `▶` 當前 dir 標示 | snapshot test | ✅ |
| TerminalGuard Drop 恢復終端 | 模擬 drop 呼叫，驗證 leave alt-screen 執行一次 | ✅ |
| Keyboard quit channel：q / Esc / Ctrl-C 都觸發 shutdown | channel test | ✅ |
| Render budget `< 16ms` avg | perf test with 50 iterations | ✅ |
| `cargo clippy` Phase 2c 零 warning | clippy Pass | ✅ |

## Phases

| # | Phase | Activities | Status |
|---|-------|-----------|--------|
| P1 | Schema v4 | `mobs.origin_path TEXT` nullable + migration test + scanner 寫入 | ✅ |
| P2 | Local Map data provider | `src/tui/local_map.rs`：scan mobs → group by top-level dir → `Vec<RoomSummary>` | ✅ |
| P3 | TUI renderer 骨架 | `src/tui/mod.rs` + `src/tui/layout.rs`：alt-screen / raw mode / region layout / RAII guard | ✅ |
| P4 | Panel renderers | PetStatus / CombatLog / LocalMap 三個純 render function（data → `Vec<Line>`）| ✅ |
| P5 | Event loop | tokio 1Hz timer + keyboard channel + shutdown notifier | ✅ |
| P6 | CLI 整合 | `src/cli/tui.rs` + `main.rs` clap 註冊 + e2e smoke | ✅ |
| QG | Quality gate | `cargo check + clippy + test` + perf KR | ✅ |

## Known risks / open questions

1. **Terminal resize**：初版不支援 live resize（收到 Resize event 後重算 layout）；超出視窗就 clip。若 review flag 再加。
2. **SQLite busy during render**：1 Hz read + daemon 60s write 幾乎不會撞；WAL read 本來就無鎖。但若出現 BUSY，render 應 skip 該 frame 而非 crash。
3. **Alt-screen + SSH / tmux**：crossterm 對這些情境有處理，但 terminal capability 缺失時應 fallback 到「退出 + print 錯誤」。
4. **`origin_path` 相對於誰**：scanner 以 `CODEFORGE_SCAN_DIR`（或啟動時的 `$PWD`）為 root 存相對路徑；Local Map 用同一個 root 做頂層目錄分組。

## Deferred（明確不在 Phase 2c）

- World Map（跨 zone）→ Phase 3a
- tmux split 自動 layout → 不綁；使用者自己 split 即可
- Pet animation frames → Phase 4
- AI commentary 顯示 → Phase 3c
- Strategy Mode hotkey 切換 → Phase 3b
- Active item use / Loot Crafting → Phase 3e
- Mood Decay visual → Phase 3d
