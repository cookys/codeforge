# Claude Code mod UI — 開發依據（codeforge / forge-stats）

> **狀態**：已覆核（2026-10-31）。對照 Claude Code **2.1.314** 的 mod 型別檔（`claude-code` 模組，
> 由 plugin-authoring skill 產生的 `index.d.ts`）與 plugin-authoring `reference.md`，加上 owner 環境實測。
> 原始調查與逐條覆核表（80 條：62 確認、1 錯、12 誇大、2 無來源、3 待實測）放在 gitignored 的
> `.codeforge/research/`；本文件只收**覆核後成立**的結論，並標明哪些仍是「待實測」。
> 出處用型別／符號名稱，不用行號（行號隨版本漂移）。Claude Code 升版後，涉及 API 的段落要重新對照型別檔。

## 1. 一句話

mod 是一個有自己版面引擎的 UI 層（flex `Box`、`Text`、`Button`、`Input`/`Select`、hover 浮動卡、
可捲動視窗、`Raster` 逐格色塊動畫），大約是「一個 TUI 應用的八成」。在 owner 的 Windows 終端上，
**沒有真像素圖片**；像素風一律走 `Raster` 半格。

## 2. 能力階梯（已覆核）

| 階 | 做法 | API | surface |
|---|---|---|---|
| 0 文字 | `Box` / `Text` | `$.ui.resolve(e)` 取得元件表 | 全部 |
| 1 樣式 | `color`（theme key 或任意色字串）、`dimColor`、`bold`、`italic`、`underline`、`inverse`、`wrap` | `TextProps`、`Color = ThemeKey \| string` | 全部（`size`/`mono` 終端忽略） |
| 2 版面 | flex、padding/margin、`borderStyle`/`borderColor`、`overflow`、`position:'absolute'` | `BoxProps` | 全部 |
| 3 互動 | `Button`（`hotkey`、`variant`、`autoFocus`、`action`）、`Input`、`Select`、`Link`、`Markdown`、`Code` | `ButtonProps` 等 | `Input`/`Select` 不含 mobile |
| 4 hover | keyed `Box` 的 `hover` 覆寫；`absolute` + `display:'none'` → hover 時 `flex` = 不取焦的浮動卡，不呼叫 hook | `BoxHoverProps` | 全部；終端需 mouse |
| 5 捲動 | 引擎捲動（滾輪、PgUp/PgDn），或 mod 自行切片 + `$.ui.scroll` | `$.ui.scroll` | terminal |
| 6 Raster | `columns × rows` 格，每格 little-endian u32 `[codePoint, fg 0x00RRGGBB, bg]`，base64；`0x01000000` = 終端預設色；`$.ui.blit` 不重繪直接換格 | `RasterProps`、`RasterBlitArgs` | **僅 terminal** |
| 7 Image | `{png}` / `{rgba,width,height}` bytes，或 `{file}` | `ImageProps`、`ImageSource` | 僅 kitty、Ghostty、iTerm2 3.5+（iTerm2 另需 fullscreen rendering）；tmux 3.3+ 只收 bytes，且要 `allow-passthrough` + `focus-events` |
| 8 Client | 自帶 `surface.every`、`onPointer`、`onKey`（任意鍵） | `Client` 模組 | terminal / desktop；**Client 內沒有 Raster/Image** |

**Raster 字元規則**：每格一個**寬 1 的 BMP 字元**（方塊、框線、點字皆可）；違規整棵樹被拒。
Extended_Pictographic（emoji 類）字元就算通過驗證也不要用。

**Pane 版位**：`fullscreen` 且 ≥110 欄時停靠右側，否則 inline 在 prompt 上方；多個 pane 變 tab。
**mod 自己開（unasked）要 ≥144 欄**——`session.start`、timer、`focus` 開的都算 unasked；使用者手動開過
同 id 且沒關，才降到 110 欄。forge-stats 在 `session.start` 自動開 pane，所以窄終端看不到，要靠 `/forge-stats`。
版面模式每個 session 啟動時固定，不逐 client 決定。

## 3. 互動：可行與限制（已覆核）

- **鍵盤**：`Button.hotkey` 只能一位數字或一個小寫字母，而且只在該 site 握有鍵盤時有效
  （ctrl+x tab、點擊、或 `$.ui.open({ focus: true })`）。沒有全域快捷鍵（`action` 綁引擎既有 chord 例外）。
  取焦的 pane 裡**方向鍵是捲動**，不是在按鈕間移動；只有 `rows` 開的 dialog 才用方向鍵走按鈕。
  任意鍵（WASD、`/` 搜尋）只能走 `Client.onKey`，代價是 Client 裡不能放 Raster。
- **焦點**：`$.ui.focus({ requestId, key })`、`autoFocus`、`Pane.isFocused` 都存在（autopilot DS §12.9-5、
  §12.10-8 說「`$` 沒有 focus API」已過時）；`$.ui.focus` 只在 site 已握鍵盤時有效。
- **對話**：`$.ui.ask` = 引擎原生選擇題；取焦 modal 只有 pane 或 `ask`，沒有浮動 modal。
- **通知**：`$.ui.toast`、`$.ui.notify`、`$.audio.play`。
- **`$.ui.status`** 是 plugin 自己的一行 pinned status，不是 Claude Code 的 statusLine。
- **做不到**：轉場／淡入淡出（只能自己算幀 blit）、拖放、浮動取焦 modal。

## 4. 限制與地雷（已覆核）

- **時間預算**：hook 單次 dispatch 10 秒；`next()` 與 `$` 呼叫不計，**但 `$.clock` 的等待會計入**。
  `Client` 每次呼叫最多 1 秒，超時、拋錯或樹過大會被卸載；樹上限 20,000 節點、深 32、序列化 100,000 字元。
- **更新率**：`invalidate` 重繪每秒 ≤10 次（顯示中的 pane／展開的 band／prompt hint 在終端可到 30）；
  `blit` 接受到 120/s、實際顯示約 60/s，多餘的折疊。動畫實際要多快才順，沒有來源，以 owner 目視為準。
- **顏色**：raw hex / Raster RGB **不跟** dark/light theme；只有 theme key 會跟。tmux 內顏色可能被夾到
  256 色（見 §6 實測）。
- **寬度**：`bodyColumns` 可能比實際可畫寬度多約 2 格，排版留邊界；docked 時更窄，要依 `bodyColumns` 自行排版。
  CJK 用顯示寬度，不可用字串長度。
- **字元**：`⏸` 在 tmux 實際被畫成藍色 emoji 框；`⚙ ⚠ ✔ ✖ ▶ ⏹ ⏺ ▪ ▫` 依 Extended_Pictographic 判定同樣禁用。
  東亞 Ambiguous 字元（`◐ ● ○ ▲ △ ◆ ─ ━ └ ·` 等）在 CJK 語系終端可能畫兩格，新字元要在每個終端各自實測。
- **`inverse` + `backgroundColor`** 會變反色文字，不是色塊徽章。
- **熱重載**：存檔即重載，module 變數會丟、`$.state` 保留；狀態放 `$.state`。
- **`$` 呼叫要寫字面參數**：例如 `$.env.get('HOME')`；用變數或 helper 包裝會讓模組載入失敗（2026-10-31 實際踩到）。

## 5. 視覺規範

沿用 autopilot live pane 設計規範（`~/projects/autopilot/docs/plans/2026-10-13-live-pane-design-system.md`）：
語意色盤（一色一義）、粗體白名單、只用驗證過的單格字元、版面格線 0/2/4/12。
**例外**：寵物像素畫與地圖是**內容**不是狀態，Raster 可用自帶 RGB；框線、按鈕、警示等 UI 框架色仍走語意色盤。

## 6. owner 環境實測（2026-10-31）

| 項目 | 結果 |
|---|---|
| tmux | 3.7c；`allow-passthrough` on、`focus-events` on |
| Windows Terminal client | tmux features 含 `sixel`，**無 `RGB`、無 `mouse`** → 視為 256 色、無 hover |
| WezTerm client（20261026） | features 含 `RGB`、`sixel`、`mouse`、`extkeys` |
| Claude Code `tui` 設定 | `~/.claude/settings.json` 設 `"tui": "fullscreen"` → pane 停靠右側 |
| mod `Image`（WezTerm 獨佔 tmux session） | **ALT**。引擎原文：the terminal draws neither kitty placeholder images (probe: graphics reply OK, terminal WezTerm …) nor iTerm2 inline images (probe: XTVERSION WezTerm …) |

由 `Image` 實測推得：引擎是**執行期主動探測**（kitty graphics query + XTVERSION），不是看環境變數；
kitty 路徑要求 **Unicode placeholder**（WezTerm 未實作，wez/wezterm#4531）；iTerm2 路徑以 XTVERSION
判定是否 iTerm2。**結論：owner 的 Windows 終端都無法顯示 mod `Image`**（kitty、Ghostty 無 Windows 版）。
決策：像素畫走 Raster 半格（卡 d-11aaa2bf）。

## 7. codeforge 的分工

- **資料契約**：codeforge 寫 `<live_base>/codeforge/pet.json`（`codeforge.pet-view/1`，見 README
  「Live context files」），mod 只讀檔、不解析人類文字、不 spawn codeforge。新顯示需求優先擴充這份契約。
- **像素寵物（2026-10-31 上線）**：`village.sprite`（調色盤 + 每幀字元列，24×24 兩幀，`src/pet/sprite.rs`）。
  forge-stats 在 terminal surface 用 `Raster` 半格畫（一格 = 上下兩像素），`$.clock.every(500)` 換幀 `$.ui.blit`，
  cells 沒變就不送；其他 surface 與沒有 sprite 的村落畫 ASCII。對應規則見 `doc/plans/2026-10-31-pixel-pet.md` §2.5。
- **mod 位置**：`.claude/skills/forge-stats/`（repo 內）。專案 skills 資料夾裡的 mod 由 Claude Code 自動載入、互動模式熱重載，
  所以在 codeforge repo 開的 claude 都有面板。`CLAUDE_CODE_PLUGIN_DIRS` 不能寫在專案 settings（只讀環境變數或使用者設定）；
  session 自己的 mods 資料夾綁 session id，`--resume` 後就不載入，不要放那裡。驗證：`claude plugin test .claude/skills/forge-stats`。
- **band 版面（statusline-band，2026-10-31）**：`/forge-stats layout band|pane`（存 `$.store`，預設 band）。band 畫在
  prompt 正上方（AbovePrompt），分級與排欄見下面兩條；記憶與大腦讀 `codeforge/session/<sid>.json`。mod 每 2 秒寫
  `codeforge/mod-alive/<sid>.json` 心跳（`schema`、`session_id`、`written_at_ms`、`layout`）；statusline 讀到自己
  session、未滿 10 秒、`layout: band` 的心跳就縮成 2 行：寵物歸 band（autopilot 的 forge 分頁），statusline 只留不必即時
  更新的資料（owner 2026-10-31 定案；原本另看 blit 是否成功的 `pet_shown`，已拿掉）。**依賴**：AbovePrompt 一次只有一個
  實例，鏈上排在上面的 plugin 不呼叫 `next(e)` 就輪不到 forge-stats；心跳看不到這件事，那種情況下 statusline 仍會縮、寵物
  哪裡都不顯示（BACKLOG B34）。autopilot 的分頁列用 `await next(e)` 疊在 forge-stats 之上（BL-0352），不受影響。
- **分頁版 band（forge-tabs，2026-10-31）**：owner 定案 band 一列＝autopilot indicator＋分頁（現況／待決／派工／審查／forge），
  autopilot 以 `maxRows`/`bodyColumns` 分配空間給下層。forge-stats 依 `maxRows` 分級：1 列＝分頁標籤（一列以 key 標角色的 `Box`：
  speed／trend／sep／pet，各包一個純文字 `Text`，autopilot 依 key 上色，`c5165cf`）、2–5 列或不到 26 欄＝一列摘要、6–11 列＝16×6 迷你 Ferris
  （`village.sprite_mini`）、12 列以上＝完整版。band 的 blit 尺寸跟著畫上去的那一版。autopilot 只問 1 列（標籤）與總列數 −2（≥12
  才展開），所以迷你與摘要兩級只在沒有 autopilot 時出現（B31 的降級原則：每一項都要能單獨跑）。規則見
  `doc/plans/2026-10-31-forge-tabs.md` §2.5。
- **依寬度排欄（forge-columns，2026-10-31）**：迷你與完整版依 `bodyColumns` 排 1～5 欄，左到右即優先序：Ferris｜寵物 30
  （名字等級、村莊、XP、ATK/DEF/HP、SUP/VER）｜速度 34｜最近請求 ≥46（吃掉多的寬度）｜記憶與大腦 28；24 寬 Ferris 的門檻
  24／56／92／140／170 欄（不到 26 欄仍是一列摘要）。規則見 `doc/plans/2026-10-31-forge-columns.md` §2.5。
- **TUI**：mod pane 放一眼看懂的 widget（寵物、tok/s、縮圖）；需要操作的完整介面留給 `codeforge attach`
  （獨立 tmux pane，crossterm alt-screen，可用 sixel——WT 與 WezTerm 都回報支援，畫面未實測）。

## 8. 待實測

1. `CLAUDE_CODE_TMUX_TRUECOLOR=1`（+ tmux `terminal-features 'xterm-256color:RGB'`）是否讓 WT client 出全彩；
   目前未設定。只有 WezTerm 確定受益。
2. hover 在 owner 環境：只有 WezTerm client 有 mouse，WT client 預期無法 hover/點擊。
3. Raster `blit` 經 ssh + tmux 的實際 fps 與延遲。
4. sixel 直接輸出（tmux 視窗）在 WT / WezTerm 的畫面。
5. WezTerm 不經 tmux 直接跑 claude 時 `Image` 是否走別的路徑。
