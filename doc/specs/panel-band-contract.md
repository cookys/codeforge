# 整合面板 band 的介面約定（O4 寬度分級，2026-11-06）

> 狀態：v1，codeforge 起草，autopilot 審過（`panel-contract-review.md`，逐條附 file:line），改完再請 autopilot 確認一次。
> 依據：操作員 d-f0bcb1f2（O3 為底＋G3 進度列）、PM 2026-11-06「寬度分級 O4 定案」。版面與真機座標：`ui-preview/codeforge/ux-v7/design.md`（不進 git）。
> 規範：`tui-guideline.md` §3、§14。`codeforge.panel/1` 原有欄位的定義在 `codeforge-mod-ui.md` §7，這裡不重複。

## 1. 誰做什麼

| | autopilot（`mods/live`） | codeforge（forge-stats） |
|---|---|---|
| 畫 band | 全部：選單列、左側子選單、主視窗、進度列、快速列、右側摘要欄、框線 | 不畫 band |
| 讀 `forge-stats.panel` | reader 是這一批的新工作（目前 mods/ 沒有任何地方讀它） | — |
| 選級 | 依 band 實際拿到的 `bodyColumns` 選級（§1.1） | 不知道級數，只提供「各種欄寬各一份」排好的列 |
| 資料 | 待送 N 張＝`DecideState.sent`（`mods/live/model.ts`）；圓桌＝`~/.autopilot/rooms/<room>/room.jsonl`；fleet lane＝`FleetSummary.lanes`（`mods/live/fleet.ts`） | 用量（配額比例、今天各模型比例）、記憶／大腦 |
| 寫入 | `autopilot.panel_open_at_ms`（這一批新寫，見 §1.3） | `$.state` `forge-stats.panel`（`codeforge.panel/1`，本文加 `side`） |

### 1.3 `autopilot.panel_open_at_ms`

外殼畫在螢幕上時，autopilot 每 15 秒寫一次 `Date.now()`，關掉就停。forge-stats 看到的值如果在 60 秒內（`QUOTA_WATCH_MS`），就照常更新配額鏡像；比現在還晚的值當作沒有。寫入間隔必須小於 60 秒，否則面板開著時鏡像會中斷。外殼關掉後，最多 60 秒內停止掃描。

### 1.1 選級

- 門檻直接寫在 `bodyColumns` 上：**≥235 → 240 級、≥195 → 200 級、≥155 → 160 級、≥115 → 120 級，其他 → 80 級**。不從終端寬換算（「終端寬 = bodyColumns + 5」只是一台機器量到的差值，引擎沒保證）。
- 分頁列現有的分級（`layoutBand` 的 160／120／80）是另一件事：兩套各自一個常數、一個函式，不互相引用。
- 看的是 band **拿到**的寬度：autopilot 的側邊窗格在 viewport ≥144 時會自動開，吃掉寬度（240 欄的終端實測只剩 145）；那時就照 145 選級。窗格要不要讓出寬度另案處理。
- **高度**：O4 要 12 列。主機給的 `maxRows` < 12 時，退回現有的分頁列版面。

### 1.2 版面不變式

左欄固定 26 欄，主視窗起點固定，題目、選項、反白、快速列四格的列和欄在五級都相同；多出來的寬度只長在主視窗右邊（主視窗最寬 76 欄）。右側摘要只放 Text、不放 Button，所以 Tab 順序和反白在每一級都一樣。反白一律是主機替焦點所在的 Button 畫的樣式；mod 自己不畫 inverse（ui-lint L1b／L9）。

## 2. `side`：右側摘要欄的內容（加在 `codeforge.panel/1`，選填）

```ts
side?: {
  usage:      { tiers: { '30': PanelLine[]; '40': PanelLine[]; '50': PanelLine[] } } // ≤ 3 列：最吃緊帳號的 5h、7d（量表＋比例，放得下就加重置倒數）；有其他帳號時第 3 列寫幾個、最高多少
  usage_full: { tiers: { '40': PanelLine[] } }                                       // ≤ 7 列：5h、7d、空一列、今天用量前 3 名模型的比例條
  memory:     { tiers: { '30': PanelLine[]; '40': PanelLine[]; '50': PanelLine[] } } // ≤ 2 列：記憶 L1 條數、大腦狀態（只寫字，不用狀態符號）
}
```

外層欄位(型別的唯一來源是 `plugins/forge-stats/types/index.d.ts`;這裡列出來方便對照):

```ts
type PanelTone = 'subtle' | 'inactive' | 'dim' | 'bold' | 'mono' | 'success'   // 只有文字色和字型樣式，沒有底色和反白
type PanelLine = { text: string; tone?: PanelTone }[]                          // 依序畫出各段，producer 不會裁斷任何一段
type PanelState = {
  schema: 'codeforge.panel/1'
  updated_at_ms: number          // 每次寫入時的 Date.now()
  usage: { tiers: Record<string, PanelLine[]> }
  pet:   { title: string; sprite: SpriteData | null; tiers: Record<string, PanelLine[]> }
  side?: PanelSide               // 本節
}
```

reader 的處理(autopilot 實作，codeforge 認可):
- 沒有檔案、JSON 壞掉、`schema` 不是 `codeforge.panel/1`:一律當作沒有摘要。
- `side` 任何一段文字含 `$`:整份 `side` 都不畫,`usage`／`pet` 照常畫。這和 producer 寫入前的檢查是同一條規則，兩邊各自守住。
- `updated_at_ms` 超過 60 秒沒更新：在右欄第一列標「資料 N 分鐘前」。

- 加欄位不改 schema 字串。reader 忽略不認得的欄位；缺 `panel` 或缺 `side` 不算錯誤：右欄只畫圓桌，不留空洞。
- 讀法同 `tiers`：取不超過欄內寬的最大一份。五級的欄內寬是 32、50、44、45、42，各自取到 30、50、40、40、40。producer 保證每一列不超過自己的 key；band 照常裁到「欄內寬 − 1」並加 `…`。
- 配額來源：配額鏡像目錄存在時只用鏡像（`codeforge-mod-ui.md` §7 規則 7），否則用 `summary.json`。今天各模型的比例來自 `summary.json`。
- **不放金額**：只有比例和條數。producer 在寫入前檢查，任何一列出現 `$` 就整份 `side` 不寫（測試守住）。
- 寫入時機同 `panel/1`：15 秒檢查、有變才寫、至少 30 秒寫一次。超過 60 秒沒更新時，band 在右欄第一列標「資料 N 分鐘前」，不另佔一列。

各級怎麼用：

| 級 | 右側欄（內寬） | 用哪一份 |
|---|---|---|
| 80 | 無 | — |
| 120 | 「圓桌 · 用量」32 | 圓桌（autopilot）＋ `side.usage['30']` |
| 160 | 「圓桌 · 用量」50 | 圓桌＋ `side.usage['50']` |
| 200 | 「圓桌」44、「用量」45 | `side.usage_full['40']` |
| 240 | 「圓桌」42、「用量」42、「fleet · 記憶」44 | `side.usage_full['40']`；第三欄 fleet（autopilot）＋ `side.memory['40']` |

## 3. 符號與顏色（同時改 `tui-guideline.md` §3.2）

顏色一律經 autopilot 的 `PALETTE` 角色，不寫主題鍵字面值（ui-lint L8）。

- `●` 只表示「執行中」（§3.1）。
  - **新訊息**：新增結構符號 `✦`（U+2726，East Asian Width N，單格），意思是「有新的內容」，寫成 `✦ 3`，用**預設前景色**（`claude`／`here` 已是「目前位置」，不能一色兩義）。登記進 live `GLYPHS` 與 `NEW_GLYPHS`，`verified` 先填 `pending-owner`。
  - **fleet lane**：用 §3.1 狀態表（忙碌 `●`、等你 `▲`、閒置 `⋯`）。
- **子選單目前那一項用 `◐`**，和待決清單共用 live 的 `GLYPHS.chart.here`（「你在這裡」）。`▸` 已登記為「收合」，不拿來當選取記號。選單列目前那一項旁的 `▾`（子選單展開在左欄）符合「展開」，照用。
- **框線**（live 和 roundtable 目前都沒有邊框，這是新增能力）：主視窗 `here`、其他視窗 `idle`、視窗內分隔線 `structure`。暗色主題下 `subtle` 框線太淡的問題，由主視窗改用 `here`、其他視窗改用 `idle` 解決。

## 4. 測試與交件

- codeforge：`side` 每一份的每一列顯示寬度 ≤ key；沒有 `$`；沒有 `●`；鏡像存在時只用鏡像；舊資料（沒有配額、沒有模型、沒有 session 燈號）各有一個退路。
- autopilot：選級邊界（114／115、154／155、194／195、234／235）；五級四格座標相同（沿用 ux-v7 的 capture-pane 量測腳本；反白只量主機畫的）；`maxRows` < 12 的退路；缺 `side` 時右欄只畫圓桌。
- 每個新檢查都附負控制（先紅再綠）。
- 合併前：兩邊各自 `--no-ff` 進各自的 develop，五級真機截圖（只裁到面板）和兩邊的 sha 一起回 PM。
- 疊層三條規範（§14.1）這一版用不到；用到時在當時的 Claude Code 版本重測。
