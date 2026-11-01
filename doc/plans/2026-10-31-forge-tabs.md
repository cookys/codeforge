# forge-stats 分頁版 band：依 maxRows 分三級（M）

- **logical_plan_id**: `codeforge-forge-tabs-2026-10-31`
- **Size**: M · branch `feature/forge-tabs` · base `2ae4d00`
- **上層專案**: `doc/projects/_archive/2026-10-31-statusline-band/`（「分頁協定」一列）

## 0. 背景

owner 定案的 band（經 autopilot 轉達，msg `01M68S9Q1KQSPV2YDVZ8TXQS2G`）：第 1 列左邊 autopilot indicator，右邊分頁
「現況｜待決｜派工｜審查｜forge」，點分頁在下方展開。autopilot 疊在下層之上（X1），只靠 props 跟 forge-stats 溝通：

- forge 分頁未選：每次 render 呼叫一次 `next`，`maxRows=1`、`bodyColumns`=標籤寬；回傳值包進 `Button key="forge"`。
- 選了 forge：同一次 render 先標籤（`maxRows=1`），再展開區（`maxRows≥12`）；展開區一定在後。
- 兩次都原樣傳 `e`，只改 `maxRows`、`bodyColumns`、`scroll.bodyRows`；`requestId` 不動。
- `Button` 的 children 只收字串與 `Text`（ButtonProps 文件：any other element is refused）。

現況 forge-stats `maxRows < 12` 就整個讓出，標籤畫不出來。

## 2.5 Global Constraints

- **分級**（只看 `e.props.maxRows`，terminal surface、`layout === 'band'`、`!hasSurvey` 才參與；否則 `return next(e)`）：
  - `maxRows === 1` → **標籤**：回傳**單一 `Text`**（只含字串與巢狀 `Text`，無 `Box`、無 `Raster`）。內容依
    `bodyColumns`：`≥ 16` → `forge <tps> tok/s`；`10–15` → `<tps> tok/s`；`< 10` → `<tps>t/s`；`<tps>` 是最近一次主對話
    請求的 tok/s（整數），沒有時是 `—`。超出寬度由 `wrap="truncate-end"` 截掉。
  - `2 ≤ maxRows ≤ 5` → **一列摘要**：同樣單一 `Text`：`forge <tps> tok/s <sparkline> · <寵物名> Lv.<n>`（有什麼放什麼）。
  - `6 ≤ maxRows ≤ 11` → **迷你**：`village.sprite_mini`（16×12 px → 16 欄 × 6 列 Raster）＋速度欄＋寵物/記憶欄，每欄最多 6 列；
    沒有 `sprite_mini` → 一列摘要。
  - `maxRows ≥ 12` → **完整**：現行三欄（24×12 Raster）。
  - 迷你與完整需 `bodyColumns ≥ 26`，否則一列摘要。欄位收縮規則沿用 `bandColumns(bodyColumns, sprite.width)`。
- **blit 尺寸**：band 的 blit 用「這次畫上去的那一版 sprite」（模組變數 `bandSprite`），尺寸與掛載的 Raster 相同；pane 仍用完整版。
  文字級（標籤、一列摘要）把 `bandId` 設成 `null`、不 blit。同一次 render 標籤在前、展開區在後，所以展開區留下的 `bandId` 生效。
- **`pet_shown`**：規則不變（band 版面且 2 秒內 band blit 成功）；迷你也算顯示了 Ferris。標籤／摘要沒有 Raster → false → statusline 完整版。
- **`pet.json`**：`village` 加選填 `sprite_mini`（`null` 或 sprite 物件），規則與 `sprite` 相同，只有尺寸固定 16×12；schema 名
  `codeforge.pet-view/1` 不變（只增欄位）。Rust `sprite::validate_size(s, w, h)`；`validate` = 24×24、mini = 16×12。
  mod `parseSprite(raw, w, h)`。
- 不用 emoji 呈現字元；`$` 呼叫規則同 statusline-band §2.5；格式化只走 `./scripts/fmt.sh`。

## 3. 檔案

| 檔案 | 改動 |
|------|------|
| `src/pet/sprite.rs` | `FERRIS_MINI`（16×12、2 幀，來自 `.codeforge/research/statusline-band/mini-draft.json`）、`mini_for_village`、`validate_size`、測試 |
| `src/pet/view.rs` | `village.sprite_mini`、測試 |
| `README.md` / `CHANGELOG.md` / `doc/specs/codeforge-mod-ui.md` §7 | `sprite_mini`、分級 |
| `.claude/skills/forge-stats/types/index.d.ts` | `VillageData.sprite_mini` |
| `.claude/skills/forge-stats/hooks/model.ts` | `parseSprite(raw, w, h)`、`bandTier(maxRows, bodyColumns, hasMini)`、`tabLabel(cols, tps)` |
| `.claude/skills/forge-stats/hooks/register.tsx` | 分級 render、`bandSprite` |
| `.claude/skills/forge-stats/tests/band.test.tsx` | 各級測試 |

## 4. Phases

### P1（M）— 實作與測試
1. 測試先紅：Rust `sprite_mini` 驗證與輸出；mod：`maxRows=1` 三種寬度只有一個 `Text`（無 `Box`/`Raster`）、`maxRows=3` 摘要、
   `maxRows=8` 16×6 Raster 且 blit 的 cells 是 mini 尺寸、`maxRows=12` 24×12、只掛標籤時沒有 band blit 且 `pet_shown=false`、
   先標籤後展開（同 requestId）時 blit 走展開區。
2. 實作。
- **Acceptance**：`cargo test`、clippy `-D warnings`、`./scripts/fmt.sh --check`、doc-drift；`claude plugin test`、`tsc`、
  `claude plugin validate`。
- 與 autopilot 實測：等 autopilot X1 合併（sha 由 fleet 回報）後，owner 在真終端看分頁標籤與展開。

## 5. Risks

| 風險 | 處理 |
|------|------|
| Button 實際包不住回傳的 Text | autopilot 改走備案（靜態「Ferris」標籤 + 我們在右邊畫一列摘要），一列摘要級已支援 |
| 同一次 render 兩次呼叫順序反了 | autopilot 已承諾展開區在後；測試釘住「後到的決定 bandId」 |

## Review log

- R0 author：depth-0（claude-opus-5-5），2026-10-31。
