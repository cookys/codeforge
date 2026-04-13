# ASCII Pet Artist — CodeForge Village Mascot Skill

設計 CodeForge 村莊 pet sprite 的參考規格與技法。

## 硬性約束

```
ascii_small (statusline 用)：
  - 最多 4 行（對應 render_full rows 0-3）
  - 每行寬度 ≤ ART_W（目前 ART_W = 10）
  - 顏色由 village.rgb() 決定，art 本身只有一個顏色

ascii_full (pet card / 未來 TUI 用)：
  - 最多 8 行
  - 每行寬度 ≤ 20
```

## 設計原則（從 ASCII art 社群整理）

### 1. Silhouette-first
小尺寸下輪廓即角色。先定外形，再加細節。
外框用重字元（`@#WM`），邊緣/紋理用輕字元（`.,'`）。

### 2. Anchor points（決定辨識度的 2-3 個特徵）
每種動物只要抓住 2-3 個特徵，其他可以省略：
- 螃蟹：波紋殼頂 `~^~` + 眼睛 `oo` + 蟹爪延伸
- 貓頭鷹：圓眼 `ovo` + 嘴 `v` + 羽冠
- 貓：尖耳 `/\` + 眼 `^.^` + 鬍鬚
- 地鼠：突眼 `o o` + 門牙 `> <`
- JS 混沌生物：問號 `??` + 困惑眼 `o_o`

### 3. 有機曲線三劍客：`_ ~ ^`
這三個字元組合 `_~^~_` 可以模擬圓弧、殼紋、毛髮，
比直角字元更「活」。

### 4. 對稱 + 少量不對稱
水平對稱讓動物一眼認出，
垂直方向（頭重腳輕或相反）製造視覺重量感。

### 5. 腳是選配
4 行限制下，腳通常讓 sprite 變醜。
底部用 `\___/` 或 `~~~~~` 作乾淨收尾比腳好看。

### 6. 對角消鋸齒
邊緣用 `/`, `\`, `_`, `~` 代替直角，
是小 sprite 最有效的品質提升。

## Unicode 強化選項

```
半格 blocks（雙倍垂直解析度，6行→等效12pixel）：
  ▀ U+2580  Upper half
  ▄ U+2584  Lower half
  █ U+2588  Full block
  ▌ U+258C  Left half
  ▐ U+2590  Right half

Box drawing（結構線）：
  ─ │ ┌ ┐ └ ┘  基本
  ╭ ╮ ╰ ╯      圓角
```

## 工作流程

1. 先確認 anchor points（2-3 個）
2. 在下面的格紙草稿（10×4 grid）手繪輪廓
3. 用 `_~^` 磨外緣
4. 放到 `village.rs` 的 `ascii_small` 測試（`codeforge statusline` 觀察）
5. 更寬的 `ascii_full` 可以加更多細節

## 10×4 格紙模板

```
0123456789
----------
          row 0
          row 1
          row 2
          row 3
```

## 現有角色參考

### Ferris（Rust / 橘色）—— 螃蟹
canonical 來源：rust-lang/ferris-says（@Diggsey）

original canonical（太寬，需壓縮）：
```
   _~^~^~_
\) /  o o  \ (/
  '_   -   _'
  / '-----' \
```

適配版（≤10 wide）：
```
 _~^~^~_
\/  oo  \/
 '_ -- _'
  /----\
```

### Spam（Python / 金黃）—— 貓頭鷹
```
  ,_^_,
 ((o,o))
  ):::(
  " | "
```

### Blueprint（TypeScript / 藍色）—— 貓
```
 /\___/\
( =^.^= )
 )     (
  \___/
```

### Gopher（Go / 青色）—— 地鼠
```
  ,___,
 (o   o)
 (  >  )
  '---'
```

### Wat（JavaScript / 琥珀）—— 混沌生物
```
  ??!??
 (#o_o#)
  )   (
  |___|
```

## Phase 2：ANSI art 升級計畫

Phase 1 statusline 用純 ASCII（單色 village.rgb()）。
Phase 2 daemon TUI pane 寬 24-30 chars、高 16-20 rows，升級為真正的 ANSI art：

**原理：半格 blocks + 多色 = 立體感**
```
▀ 上半格（高光）  ▄ 下半格（陰影）  █ 全格（實心）
亮色在上/左 → 高光；暗色在下/右 → 陰影 → 球體/立體感
```

**升級路徑（Phase 2）：**
- `Village` 新增 `ansi_art: &'static str` 欄位（含嵌入式 ANSI 色碼）
- 或用 `ansi_frames: &'static [&'static str]`（多幀 idle 動畫）
- Renderer 改為 per-character 顏色控制（不再是整體 village.rgb()）
- 目標畫布：24 chars × 20 rows = 24×40 有效像素（半格）

**Phase 2 之前：** ascii_small 維持現有 ASCII 單色設計。

---

## 新村莊 checklist

加新村莊時，在 `village.rs` 新增一個 `Village { ... }` 項目，確認：

- [ ] `id` 小寫，對應偵測語言的識別符
- [ ] `ascii_small` ≤ 4 行，每行 ≤ 10 字元
- [ ] `ascii_full` ≤ 8 行，每行 ≤ 20 字元
- [ ] `Village::rgb()` match 新增對應的 truecolor 值（參考 ANSI256 palette）
- [ ] `color: Color::xxx` 用 termcolor 最接近的顏色（向下相容）
- [ ] 在 statusline 測試：`echo '{"model":"test"}' | codeforge statusline`
