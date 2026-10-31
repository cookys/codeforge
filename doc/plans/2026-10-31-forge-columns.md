# forge-stats band：依終端寬度自動排 1～5 欄（M）

- **logical_plan_id**: `codeforge-forge-columns-2026-10-31`
- **Size**: M · branch `feature/forge-columns` · base `6f7f8d4`
- **上層專案**: `doc/projects/2026-10-31-statusline-band/`

## 0. 背景

owner（2026-10-31，卡 d-7ca99c54 後的訊息）：「你可以做偵測寬度自動依程度擺上欄位嗎？1/2/3/4/5 這樣？」
同日 statusline 改成 mod 在跑就不畫寵物（`6f7f8d4`），forge 分頁成了唯一看得到寵物的地方。
現況固定三欄：速度 34｜Ferris｜寵物＋記憶＋大腦（吃剩下的寬度），寬螢幕第三欄大半空白。

## 2.5 Global Constraints

- **欄位與優先序**（左到右就是優先序，窄時從右邊收）：
  1. `ferris`：sprite 寬（完整 24、迷你 16）
  2. `pet` 30：名字 Lv、村莊、XP 條、`ATK · DEF · HP`、`SUP · VER`、牠的一句話（迷你級不畫這句）
  3. `speed` 34：沿用現有速度欄
  4. `recent` ≥ 46：最近請求表，沿用 pane 的 `recentLines`（表頭＋每列 tok/s、首字、輸出、模型，子代理標 `sub`），列數＝可用列數−1
  5. `memory` 28：記憶、大腦兩盞燈
- **排法**：`bandLayout(bodyColumns, spriteWidth)` 依序加欄，每欄前（第一欄除外）加 `BAND_GAP` 2；下一欄放不下就停（嚴格依優先序，不跳欄）。
  多出來的寬度只給 `recent`（有它時），其他欄固定寬。24 寬 sprite 的門檻：1 欄 24（實際 26 起，`bandTier` 不到 26 欄是一列摘要）、2 欄 56、3 欄 92、4 欄 140、5 欄 170。
  `recent` 46：子代理列是 33 格＋模型名，46 放得下 13 字元的模型名（review R1 Minor）。
- 分級（`bandTier`）、blit、`pet_shown`、標籤與摘要文字級都不變；`bodyColumns < 26` 仍是一列摘要。
- 不用 emoji 呈現字元；格式化只走 `./scripts/fmt.sh`（本次只動 mod，無 Rust 改動）。

## 3. 檔案

| 檔案 | 改動 |
|------|------|
| `.claude/skills/forge-stats/hooks/model.ts` | `bandLayout`、`BandColumn`、欄寬常數；移除 `bandColumns` |
| `.claude/skills/forge-stats/hooks/register.tsx` | 欄位繪製改依 `bandLayout`；`bandPetLines` 與 `bandMemoryLines` 拆開；`recent` 欄用 `recentLines` |
| `.claude/skills/forge-stats/tests/band.test.tsx` | 門檻測試（每個欄數的邊界 ±1）、5 欄繪製、迷你級欄寬 |
| `doc/specs/codeforge-mod-ui.md`、`CHANGELOG.md`、`README.md` | 欄位說明 |

## 4. Phases

### P1（M）— 實作與測試
- **Acceptance**：`claude plugin test`、`tsc`、`claude plugin validate`、doc-drift 全綠；門檻邊界測試每個欄數兩側都有；
  owner 在真終端拉寬拉窄看到欄數跟著變。

## Review log

- R0 author：depth-0（claude-opus-5-5），2026-10-31。
