# Pixel Pet — forge-stats 像素寵物動畫

> Branch `feature/pixel-pet` · size L · started 2026-10-31

## Project Goal

> **Final goal**: forge-stats 面板用 Raster 半格（`▀`：上像素 = 前景色、下像素 = 背景色）畫出寵物的像素 sprite，並有待機動畫；sprite 資料由 codeforge 提供（資料契約），mod 只負責畫。
>
> **Success criteria**（每條都有門檻與驗證方式）：
> 1. **契約**：`<live_base>/codeforge/pet.json` 的 village 帶 sprite 資料（寬、高、調色盤、≥2 幀）。驗證：`cargo test` 有契約 shape 測試（先紅後綠）；實跑 `codeforge statusline` 後 `python3 -c` 讀檔印出 `sprite.width/height/len(frames)`。
> 2. **至少一隻寵物有 sprite**（MVP：Rust/Ferris，owner 目前的寵物）；沒有 sprite 的村落 `sprite` 為 null，面板退回 ASCII 圖案。驗證：測試釘住「rust 有 sprite、其他村落為 null 時 mod 畫 ASCII」。
> 3. **mod 畫出 Raster**：面板的寵物區是一個 keyed `Raster`，大小 = sprite 寬 × 高/2 格。驗證：`claude plugin test` 以 mount 找到 Raster 且 `columns/rows` 正確；cells 解碼後每格 codePoint ∈ {`▀` 0x2580, `▄` 0x2584, 空白}，且至少一格是 `▀`（左上常透明，不釘死第一格——consult 修正）。
> 4. **待機動畫**：以 `$.clock.every` 換幀並 `$.ui.blit`；每秒 ≤ 4 幀。驗證：測試推進 mocked clock，blit 被呼叫且 cells 與前一幀不同；owner 在 Windows Terminal 與 WezTerm（tmux 內）目視確認會動。
> 5. **不退化**：868 個既有 Rust 測試、forge-stats 既有 6 個 mod 測試全綠；statusline 輸出不變（快照只多欄位）。
>
> **Scope boundary**:
> - 包含：sprite 資料格式 + Ferris 的 sprite（≥2 幀）+ pet.json 擴充 + mod Raster 畫法與待機動畫 + README/CHANGELOG/文件。
> - 不包含：真像素圖片（owner 決議 d-11aaa2bf：Raster 就好）；其他四個村落的 sprite（MVP 之後另開）；依心情/戰鬥切換動畫；codeforge 自己的 alt-screen TUI 改用 sprite。

## Decisions

| 日期 | 決策 | 來源 |
|------|------|------|
| 2026-10-31 | 下一個升級做 D（像素寵物動畫） | 卡 d-e279b40d |
| 2026-10-31 | 不追真像素圖片，用 Raster 半格 | 卡 d-11aaa2bf；實測紀錄 `.codeforge/research/empirical-2026-10-31.md`（gitignored） |
| 2026-10-31 | sprite 選 A：24×24、2 幀（睜眼↔眨眼兼夾鉗子），每幀 500 ms | 卡 d-42ad374b；選項頁 https://claude.ai/artifact/JmzShkUGC2qsDxvfmtfhCa |

## Design assumptions（research：U1 consult grok-4.6 xhigh，2026-10-31，原文 `consult-out.json`）

- **pixel sprite format**：調色盤 + 每幀定寬 palette-index 字串列（`.` = 透明 index），與 `ascii_small`/`ascii_full` 同樣寫在 `village.rs`；可讀、可 diff、LLM 可畫格。不用 RLE / base64。24×24×2 幀僅數百 byte，statusline 每次原子寫入不需壓縮。
- **契約**：`village.sprite` 為選填欄位（無 sprite = `null` → 面板退回 ASCII）；schema 名維持 `codeforge.pet-view/1`；明示 `transparent` 字元，`height` 必為偶數；JSON 只放 sprite 定義，不放 Raster cells（cells 由 mod 算）。
- **尺寸**：24×24 px → Raster 24 欄 × 12 列；40–60 欄的 pane 還留 16–36 欄給文字。終端格寬:高 ≈ 1:2，半格切開後像素近正方形。
- **half-block raster animation**：上下皆實 `▀`（fg = 上、bg = 下）；上透下實 `▄`（fg = 下、bg = 預設）；皆透 = 空白 + 預設色 `0x01000000`。MVP 2 幀（睜眼 / 眨眼兼 1px 下沉），`$.clock.every(500)` = 2 fps；cells 沒變不 blit；pane 卸載時停 timer。
- **terminal pixel art palette**：調色盤取 xterm-256 立方色（例：Ferris 橘 `[215,95,0]` 即 cube 色），tmux 對 WT client 降到 256 色時不跑色。
- **LLM 作畫流程**：固定寬 grid、先畫對稱再加細節；契約測試釘住：每列長度 = width、height 偶數、只用 palette 內字元、每個非透明色至少出現一次、每幀尺寸一致。

## Scope completeness audit

| 面向 | 是否涉及 | 處理 |
|------|---------|------|
| Source code + tests | 是 | codeforge `src/pet/`（sprite 資料 + view 擴充）；mod（repo 外）`hooks/` + tests |
| User-facing docs | 是 | README「Live context files」的 pet.json schema 加 sprite |
| API / interface | 是 | pet.json schema 加欄位：向後相容（新增 optional 欄位），schema 名不變 `codeforge.pet-view/1` |
| Config templates | 否 | 無新設定 |
| CHANGELOG | 是 | Unreleased → Added |
| Version bump | 否 | 不發版（照現況 Unreleased 累積） |
| Migration guide | 否 | 只新增 optional 欄位，無 breaking |
| Dependent consumers | 是 | forge-stats mod（同步更新）；autopilot 不讀 pet.json |
| Credit | 視情況 | 若 Ferris 像素圖參考 rust-lang/ferris 官方形象，README credit 註明 |
| Dogfood | 是 | owner 本機實際面板目視確認 |

## Requirements ledger（owner 原話）

| # | 原話 | 對應 |
|---|------|------|
| R1 | 「D 像素寵物動畫」（卡 d-e279b40d） | 成功標準 2–4 |
| R2 | 「Raster 就好」（卡 d-11aaa2bf） | 範圍：不做 Image |
| R3 | 「先把 mod 弄好看」 | 成功標準 3–4，沿用 live 設計規範的框架色 |

## Progress

| Phase | 內容 | 狀態 |
|-------|------|------|
| — | intent / proposal / plan / plan-review | ✅（G1 CONDITIONAL，4 條 non-blocking 全採納） |
| P1 | codeforge `village.sprite` 契約 + Ferris 兩幀（`9c01100`） | ✅ 875 tests |
| P2 | forge-stats Raster 半格 + 500 ms 待機動畫（mod，repo 外） | ✅ 11 mod tests |
| P3 | 部署（`cargo install`，live pet.json `24 24 2`）+ owner 目視；mod 收進 `.claude/skills/forge-stats/`（卡 d-959cf107） | ✅ WT 與 WezTerm 皆正常（卡 d-1331c201） |
| review | hetero code loop 3 代（gpt-6.1-sol）：G1 2 條、G2 1 條全修，G3 0 條；receipt exit 0 | ✅ |
