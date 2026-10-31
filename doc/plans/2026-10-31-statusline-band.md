# Plan — Statusline Band（forge-stats 三欄 band + statusline 探測切換）

> logical_plan_id: `codeforge-statusline-band-2026-10-31` · size L · branch `feature/statusline-band`
> Project: `doc/projects/2026-10-31-statusline-band/README.md`

## 0. Context / thesis

pixel-pet（2026-10-31 合併）讓 forge-stats 在右側 pane 畫 24×24 像素 Ferris。owner 要把它放到 statusline
的位置、切成三塊（卡 d-feb71b14 → A）；即時資料（tok/s、動畫）屬於 mod，慢變資料留 statusline。研究結論：
24×24 比 16×12 耐看（卡 d-5d96fcb1 研究）；AbovePrompt 鏈上 autopilot live band 在 forge-stats 之上且不呼叫
`next(e)`（U4 探針）→ owner 選 X1：autopilot 改成疊在下層上面（autopilot 自己的任務，已用 fleet 交給
`autopilot--claude`），codeforge 做下層三欄 band；表現層另開設計（BACKLOG B31）。
U1 consult（grok-4.6，原文 `.codeforge/research/statusline-band/consult-raw.json`）提供心跳與精簡版的設計。

## 1. Problem

statusline 是事件觸發（閒置 45 秒只跑 2 次），做不到即時 tok/s 與動畫；pane 要夠寬才看得到。要讓 prompt
正上方有一條即時的三欄 band，且 statusline 在 band 顯示 Ferris 時自動縮短、不重複；沒有 band 的地方一切照舊。

## 2. OKR / KRs

- **KR1 心跳**：兩種版面都在 `session.start` 先寫一筆、之後每 2 秒寫 `mod-alive/<sid>.json`（pane 版面 `pet_shown=false`）；`pet_shown` 只在 band 的 Raster 最近一次
  `$.ui.blit` 成功時為 true。mod 測試驗證路徑、內容、blit 被拒時 `pet_shown=false`。
- **KR2 精簡版**：statusline 讀到同 sid、10 秒內、`pet_shown=true` 的心跳 → 輸出 ≤ 2 行、無框、無寵物列；
  其他情況 → 完整版。Rust 測試四種情況；無心跳時輸出與改動前**逐 byte 相同**（舊 binary 對照）。
- **KR3 session 檔**：statusline 每次執行寫 `session/<sid>.json`（記憶/腦燈），mod 第三欄讀它；缺檔顯示「—」。
- **KR4 band**：terminal 的 AbovePrompt 畫三欄（速度｜Ferris：24×24 px 經半格 = 24 欄×12 列的 Raster｜記憶與寵物），寬度不夠依序藏
  記憶欄、速度欄；`maxRows < 12` 或 `bodyColumns < 26` 不畫（`return next(e)`）。mod 測試涵蓋三種寬度與兩種退讓。
- **KR5 版面切換**：`/forge-stats layout band|pane`，存 `$.store`（`layout`），預設 band；band 版面不自動開 pane。
- **KR6 不退化**：Rust 875+、mod 12+ 測試全綠；owner 目視（見 P3 的限制）。

## 2.5 Global Constraints (copied verbatim into every dispatch)

- Heartbeat file: `<live_base>/codeforge/mod-alive/<sid>.json`, `<sid>` = the session id passed through the sanitizer of `crate::live::sanitize_session_id` (keep `[A-Za-z0-9_-]`, every other char → `_`, at most 64 chars, empty → `unknown`); the mod implements the identical rule in TypeScript, iterating Unicode scalars (`for…of`, not UTF-16 units). Both `read_mod_heartbeat` and `write_session_view` sanitize the stdin sid themselves; Rust and TS tests read the existing shared vectors `tests/fixtures/session-id-vectors.json` (`[{input, expected}]`, already used by `src/live.rs`; incl. CJK, emoji, empty, 70-char ids).
- Heartbeat JSON (one line, < 512 bytes): `{"schema":"codeforge.mod-alive/1","session_id":<raw sid>,"written_at_ms":<unix ms>,"layout":"band"|"pane","pet_shown":<bool>}`. Written by the mod in BOTH layouts: once at `session.start` before any timer is started, then every 2000 ms; in the pane layout `layout` is `"pane"` and `pet_shown` is `false`. Never deleted by the mod.
- `pet_shown` is true only when the layout is `band` AND the most recent `$.ui.blit` to the band Raster returned no `deny` within the last 2000 ms (the mod keeps the time and result of its latest actual band blit). Both Rasters skip unchanged cells; Ferris's two frames differ, so a shown band is blitted every 500 ms. A sprite whose frames never change stops blitting, `pet_shown` turns false and the statusline stays full — the safe direction.
- Statusline compacts only when ALL hold: heartbeat file for the stdin `session_id` exists, parses, `schema` matches, `session_id` equals the stdin one, `now_ms - written_at_ms` is in `[0, 10000)`, and `pet_shown == true`. Any read/parse failure or mismatch → the full render, unchanged.
- With no qualifying heartbeat, `codeforge statusline` stdout is byte-identical to the build before this plan, for every input.
- Session view file: `<live_base>/codeforge/session/<sanitized sid>.json` = `{"schema":"codeforge.session-view/1","session_id":<raw stdin sid>,"written_at":<RFC 3339 UTC string, as pet.json>,"memory":{"light":"active"|"empty"|"hidden","active":<active L1 count, integer>},"brain":{"light":"ok"|"degraded"|"offline"|"pending"|"hidden"}}`. Written best-effort (failure → one stderr line, `run()` continues, like `pet::view::publish`) by every statusline run as `crate::live::write_live_json(&base.join("codeforge").join("session"), &format!("{sanitized}.json"), &value)` — the file name never contains a slash. `write_session_view(base, raw_sid, memory_light: &str, active: usize, brain_light: &str)`; `run()` maps its `BrainHealth` and active L1 count to these strings/integer; `written_at` via `chrono::Utc::now().to_rfc3339()` as in `pet.json`. `pet.json` is not changed by this plan.
- Compact render: at most 2 lines, no box frame, no pet rows, no ASCII art; line 0 = model ── cwd ⇡⇣ branch ── version, line 1 = the usage bars (5h / 7d / ctx). It is `render_no_pet`'s strip without its `codeforge adopt` hint (no context-window chip: adding one would change the no-pet output, which must stay byte-identical; corrected during P1).
- Band: drawn only on the `terminal` surface, only when the layout is `band`, `!hasSurvey`, `maxRows >= 12`, `bodyColumns >= 26`, and the snapshot has a valid sprite; otherwise `return next(e)`. Columns: speed 34, Ferris = sprite width (24), memory/pet the rest; gap 2. The Ferris column is the existing 24×24 px sprite drawn by the existing `halfBlockCells`: 24 columns × 12 rows, one cell = two vertical pixels (no resampling). Hide order when narrow: memory/pet column first (needs `bodyColumns >= 24+2+34+2+28 = 90` for all three), then speed (`>= 24+2+34 = 60` for Ferris + speed); below 60 only Ferris.
- Band Raster key `pet-band`; pane Raster key stays `pet-sprite`. The pane blit keeps its literal `requestId: 'forge-stats'`. The band blit's `requestId` is the band instance id the AbovePrompt render hook received as `e.requestId` (`RasterBlitArgs.requestId`: "one of its panes' ids, a tool row's tool_use_id, the band's"), kept in a module variable by the render hook — the one `$` argument in this plan that is a runtime value, by the engine's design; `key` stays the literal `'pet-band'`. No band rendered yet (no stored id) → no band blit, `pet_shown=false`.
- Mod code: every other `$` call keeps literal routing arguments at the call site; no `$.process`; ≤ 4 fps; skip unchanged cells.
- Formatting only through `./scripts/fmt.sh`; never bare `cargo fmt`.

## 2.6 Change-policy decisions

- **Compatibility impact**: `published-compatible` — two new files (`mod-alive/`, `session/`) under the
  codeforge live dir; statusline output unchanged unless a qualifying heartbeat exists (only forge-stats writes one).
  `pet.json` unchanged.
- **Dependency decision**: `none` — `serde_json`, existing live writer; mod uses engine APIs.

## 3. File-structure map

| File | Responsibility |
|------|----------------|
| `src/pet/view.rs` | 加 `write_session_view(base, raw_sid, memory_light, active, brain_light)`（`codeforge/session/<sanitized>.json`）+ 測試 |
| `src/cli/statusline.rs` | `read_mod_heartbeat(base, raw_sid, now_ms) -> bool`；`run()`：合格心跳 → `render_compact`，否則原路徑；每次執行寫 session view；測試 |
| `README.md` / `CHANGELOG.md` | Live context files：`mod-alive/`、`session/`、精簡版行為 |
| `doc/specs/codeforge-mod-ui.md` | §7：band、心跳、autopilot 疊加依賴 |
| `.claude/skills/forge-stats/types/index.d.ts` | `SessionView`、`Layout` 型別 |
| `.claude/skills/forge-stats/hooks/model.ts` | `sanitizeSessionId`、`bandColumns(bodyColumns)`、`parseSessionView` |
| `.claude/skills/forge-stats/hooks/register.tsx` | layout 指令與 store；AbovePrompt band；心跳計時器；band 動畫 |
| `.claude/skills/forge-stats/tests/forge-stats.test.tsx` | band／layout／心跳／降級測試 |

## 4. Phases

### P1 — codeforge：session view + 心跳偵測 + 精簡版（M）
1. 先寫測試（紅）：`read_mod_heartbeat` 各情況（合格、過期 10 s、未來時間、別的 sid、`pet_shown=false`、壞 JSON、sid 含非法字元仍找得到檔）；共用 sid 向量檔；
   `render_compact` ≤ 2 行且不含 `Lv.`；`write_session_view` 內容。
2. 實作；`run()` 讀 `crate::live::resolve_live_base()`，sid 由 stdin `session_id`。精簡閘門只看 `read_mod_heartbeat(...) == true`（§2.5 六項）：成立 → `render_compact`（有沒有寵物都一樣；沒有寵物時本來就是 2 行的 `render_no_pet` 版面）；不成立 → 改動前的原路徑。
3. 逐 byte 對照：實作前先存一份目前的 binary；build 後兩個 binary 用同一組 stdin（無心跳）比對 stdout（`cmp`），live base 可寫與不可寫各一次。
- **Acceptance**：`cargo test`、`cargo clippy --all-targets -- -D warnings`、`./scripts/fmt.sh --check`、doc-drift 綠；`cmp` 相同；手寫合格心跳後 statusline 只印 2 行。

### P2 — forge-stats：layout、band、心跳（M）
1. 讀型別確認 AbovePrompt 的 blit 位址（`requestId`）；寫測試（紅）：layout 指令與 store；band 三種寬度的欄位；
   `maxRows < 12`、`hasSurvey`、非 terminal → `next(e)`；心跳寫入（mock clock 推進 2 s）與 `pet_shown` 隨 blit
   結果變化；pane 版面不畫 band。
2. 實作（§2.5）。測試另含：t=0 先寫心跳、pane 版面寫 `layout:"pane"`、blit 成功 → `pet_shown` true、被拒或 2 秒沒有成功 blit → false、TS `sanitizeSessionId` 跑 `tests/fixtures/session-id-vectors.json`。
- **Acceptance**：`claude plugin test`、`tsc`、`claude plugin validate` 綠。

### P3 — 部署與目視（S）
1. `cargo install --path .`。
2. owner 目視：autopilot 疊加上線前，band 只在 autopilot band 不畫的 session 出現；可用停用 autopilot 的
   session 驗證（或等 autopilot 版本）。驗：band 三欄、Ferris 會動、statusline 縮成 2 行、`/forge-stats layout pane`
   後 statusline 回完整版、關掉 claude 後 10 秒內 statusline 回完整版。
- **Acceptance**：owner 回報；autopilot 疊加的版本號記入專案 README。

**Dependency map**：P1 → P2（P2 讀 P1 的 session view；P1 讀 P2 的心跳——以 §2.5 契約解耦，可各自測）→ P3。
autopilot 疊加（外部）只影響 P3 目視，不擋 P1/P2 合併。

## 5. Test / validation

Script-gated：cargo test / clippy -D warnings / fmt --check / check-doc-drift；mod test / tsc / validate；
逐 byte `cmp`。Human-gated：P3 目視。

## 6. Risks + inversion

| 什麼會保證失敗 | 對策 |
|---|---|
| 無心跳時 statusline 輸出被改到（全域、每個專案） | §2.5 逐 byte 約束 + `cmp` 驗收 |
| 別的 session 的心跳讓這個 session 縮 | 檔名與內容都比對 sid |
| mod 死掉後 statusline 一直縮 | 10 秒新鮮度；statusline 事件觸發的既有限制（下一次事件才回完整）寫進文件 |
| band 被 autopilot 擋住卻宣稱顯示 | `pet_shown` 只看 blit／render 成功，不看「mod 已載入」 |
| band 太高擠壓對話 | `maxRows < 12` 不畫；可 `layout pane` |
| sid 清理規則兩邊不一致 | §2.5 寫死規則；兩邊各有測試用同一組輸入（含非法字元、超長） |
| statusline 變慢 | 只讀一個 < 512 B 檔；不 spawn |

## 7. Out of scope

表現層統一（B31）；autopilot 的疊加改動（autopilot 自己的任務）；mod 全域載入；其他村落 sprite（B30）；
清理過期心跳檔（檔案很小、每 session 一個；另記 backlog 若累積成問題）。

## 8. Open questions

無（尺寸、合併方式、表現層方向都已由 owner 決定）。

## Review log

- R0 author：depth-0（claude-opus-5-5），2026-10-31。
- Manifest `doc/projects/2026-10-31-statusline-band/plan-review-manifest.json`；rubric `plan-rubric.md`（R1–R11 = §2.5）。機器狀態放 gitignored `.codeforge/research/statusline-band/plan-review/`。
- G1（sol low + grok xhigh，兩席完成）：7 條。R6 accepted_blocker（session view 契約：written_at 型別、memory 條數、檔名不得含斜線）；R8 rejected（把 24 欄×12 列的半格 Raster 誤讀成要縮圖；措辭已釐清）；R4/R1/R2/R3/R11 accepted_nonblocking。
- G2（終局）：5 條。R10 accepted_blocker（band「沒變也 blit」與 skip-unchanged 衝突 → 改成兩個 Raster 都 skip，`pet_shown` 看最後一次成功 blit）；grok R10 rejected（反向建議，被上條取代）；R1/R5/R6 accepted_nonblocking（沿用既有 `tests/fixtures/session-id-vectors.json` 且 TS 逐 Unicode scalar；寫檔失敗一行 stderr、cmp 含不可寫；函式收已映射值）。receipt（g2 + dispositions）exit 0，之後才把 G2 修正寫入本文。
