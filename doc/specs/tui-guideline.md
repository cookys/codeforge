# 終端介面規範（TUI guideline）

> 適用於所有我們出貨的終端介面，包括 codeforge statusline、forge-stats（band／pane）、autopilot live mod（band、分頁列、pane、待決卡）和艦隊畫面（G23 分頁、G33 band 欄）。
> 目的是讓同一個符號、同一個顏色、同一個字在每個畫面上都是同一個意思；畫面再窄、再矮，最重要的那件事也還看得到。
> 這份文件是規範的單一來源（`doc/specs/codeforge-mod-ui.md` §5 已改成指向本文）。autopilot 的 `docs/plans/2026-10-13-live-pane-design-system.md`（以下簡稱 DS）負責 live mod 內部的實作細節。兩者衝突時以本文為準，並回頭修正 DS。
> 狀態：v1（PM G34，2026-11-03）。本文所說的「現況」都附出處，數字以程式碼為準。

## 1. 空間合約：誰擁有哪幾列、哪幾欄

### 1.1 版面上的四塊

由上而下：

| 區塊 | 擁有者 | 高度 | 載入範圍 |
|---|---|---|---|
| 對話區 | Claude Code | 其餘高度 | — |
| AbovePrompt band | autopilot live mod，在上；forge-stats，在下 | autopilot 的 band 是 1 列（`BAND_ROWS = 1`），分頁打開時往下長；forge-stats 拿到的是 autopilot 傳下來的 `maxRows`／`bodyColumns` | live mod 是全域 plugin；forge-stats 在 G33 之後也改成全域 |
| 提示列 | Claude Code | — | — |
| statusline | codeforge statusline（Rust，一次性的行程） | 完整版 5 列；精簡版（forge-stats band 心跳新鮮時）或沒有寵物時 2 列（`statusline.rs` `render_full`／`render_strip`） | 全域 settings.json，每個 session 都有 |
| pane | live mod（`autopilot-live`）或 forge-stats（`/forge-stats layout pane`） | Claude Code 決定：寬終端時停靠在側邊（docked），否則疊在對話區上（inline）；兩種都經 `e.props.bodyColumns`／`scroll.bodyRows` 拿到尺寸 | live 在寬度 ≥144 且全螢幕時自動開；其餘手動 |

規則：

1. **疊放順序固定**：在 band 裡，autopilot 在上、forge-stats 在下。下層只能畫上層透過 `next({...e, props: {maxRows, bodyColumns}})` 給它的空間，不能自己往上長。`maxRows` 缺或不是數字，就當作 0，不畫。
2. **`bodyColumns` 預留 2 格**：引擎回報的寬度可能比實際能畫的多 2 格（mod-ui spec）。行寬的上限是 `bodyColumns - 2`。
3. **第 1 列歸上層，第 2 列給下層**（PM 裁定 B，G33）：band 的第 1 列永遠是 autopilot 的分頁列。分頁沒開、`maxRows ≥ 2`，而且 forge-stats 的心跳寫著 `summary_row: true` 時，autopilot 把第 2 列交給 forge-stats（`maxRows = 1`）；舊版 forge-stats 沒有這個欄位，1 列時會回速度標籤，所以不交給它（一個 band 只有一個速度，BL-0359）。forge-stats 在那列畫一行摘要（§2.2）；沒東西可畫就原樣 `next(e)`，band 維持 1 列。分頁打開時照 §1.3。沒有 autopilot 的 session：第 1 列就是 forge-stats 的一行摘要。
4. **每一項都能單獨跑**（B31）：少了任何一個 plugin，其他的照樣能畫，不能留下空洞或錯誤字樣。資料來源不在時，那一欄或那一列直接不畫。

### 1.2 寬度分級

每個畫面依**自己拿到的寬度**分級（band 用 `bodyColumns`，pane 用 `paneWidth`，statusline 用 `CODEFORGE_WIDTH`），不看終端總寬。下面每個分段都互斥。

**forge-stats band**（B32 後；`model.ts` `bandLayout`／`bandTier`：Ferris 24 格或迷你 16 格、寵物 30、記憶與大腦 28、間隔 2；`BAND_MIN_COLUMNS = 26`）：

| 拿到的寬度 | 完整 Ferris（≥12 列） | 迷你 Ferris（6–11 列） |
|---|---|---|
| `<26` | 一行摘要 | 一行摘要 |
| `26–47` | Ferris | Ferris |
| `48–55` | Ferris | Ferris｜寵物 |
| `56–77` | Ferris｜寵物 | Ferris｜寵物 |
| `78–85` | Ferris｜寵物 | Ferris｜寵物｜記憶與大腦 |
| `≥86` | Ferris｜寵物｜記憶與大腦 | 同左 |

艦隊資訊依 §2.1 比寵物和 Ferris 重要，所以不跟著欄位排：forge-stats 在迷你版和完整版時，如果拿到的列數比該級最低列數至少多 1（迷你 ≥7、完整 ≥13），第一列先畫一行艦隊摘要（§2.2 格式，不含寵物；有 atom 才畫），各欄從第二列起排。剛好 6 或 12 列時不畫摘要列，各級門檻不變。autopilot forge 分頁給的是 `FORGE_FULL_ROWS = 12` 列，所以 forge 分頁裡不畫摘要；平常的摘要在 band 第 2 列（§1.1 規則 3）。

**必查寬度的結果**（≥12 列）：

| 寬度 | autopilot band 分頁列 | forge-stats band | statusline | pane |
|---|---|---|---|---|
| 24 | `<80` 級：判定、`●N ⊘N`、`⚑N`（`BAND_MIN_COLS = 30`，再窄只留判定符號、`ⓘ`、`⚑N`） | 一行摘要 | 不適用（寬度由 `CODEFORGE_WIDTH` 決定） | 可畫（≥20） |
| 56 | `<80` 級 | Ferris｜寵物 | 同上 | 可畫 |
| 86 | `80–119` 級：加單元進度條、`QC`、3 格花費條、一個配額窗 | Ferris｜寵物｜記憶與大腦 | 同上 | 可畫 |
| 100 | `80–119` 級 | 同上 | 預設寬度；`≥90` 畫右欄寵物圖 | 可畫；不會自動打開 |
| 144+ | `120–159` 級；`≥160` 全部 | 同上 | 同上 | 全螢幕時 live pane 自動打開（`PANE_MIN_COLUMNS = 144`） |

autopilot band 寬度不夠時的捨棄順序，依 §2.1：配額 → 截短 stage → 單元進度條 → `●` 數 → `⊘` 數 → 縮短判定字。判定符號、`ⓘ`、`⚑N` 永遠不捨。現況偏差：live `model.ts` band 寬度表先拿 `⊘` 再拿 `●`，跟優先序相反，列入 §12。

**pane 類畫面**（live 各分頁含艦隊、待決卡、forge-stats pane）：寬度取 `bodyColumns`（live 預設 76，最窄 20：`model.ts` `DEFAULT_PANE_COLS`、`paneWidth`），任何寬度都照 §6「一個項目一列」排。pane **沒有高度分級**：內容比 `scroll.bodyRows` 高時捲動（live 用引擎的捲動，待決分頁自己捲），pane 被縮小也一樣。現況偏差：forge-stats pane 至少畫 30 欄（`register.tsx` 的 `Math.max(30, bodyColumns - 2)`），pane 比 30 欄窄時會超寬，列入 §12。

**statusline**：寬度是 `CODEFORGE_WIDTH`（預設 100），不是終端寬，也沒有高度輸入；列數固定（完整版 5 列、精簡版 2 列）。每一列在寬度內依自己的降級層級排（`statusline.rs`，例如第 3 列的 Level 1–6，依序拿掉中央燈號、版本等），`<90` 拿掉右欄寵物圖（`ART_THRESHOLD`）。設成 24／56／86 時走同一套降級；最後一層以字元數截（`.chars().take`），是 §6.3 的偏差，列入 §12。



### 1.3 高度分級（band）

| 拿到的列數 | forge-stats | autopilot |
|---|---|---|
| 0 | 不畫，原樣 `next(e)` | 不畫 |
| 1 | 一行摘要（有 autopilot 時，這是 autopilot 交下來的第 2 列；沒有 autopilot 時是第 1 列）（§2.2） | 分頁列；分頁沒開而且還有列時，把第 2 列交給 forge-stats |
| 2–5 | 一行摘要 | 分頁列，加上分頁打開時的內容 |
| 6–11 | 迷你 Ferris（16×6）加各欄 | 同上 |
| ≥12 | 完整版 | forge 分頁內容最多 12 列（`FORGE_FULL_ROWS`） |

forge 分頁打開但下方不到 12 列時，autopilot 只畫自己的速度圖，不呼叫 forge-stats（它拿到 0 列）；其他分頁打開時也一樣不呼叫。

現況（`register.ts` band hook）：autopilot 只在 **forge 分頁打開**且下方有 ≥12 列、≥26 欄時才呼叫 forge-stats；分頁沒開時只回傳自己那一列，所以平常看不到 forge-stats。PM 已裁定改成上面的「第 2 列給下層」（B），由 G33a 實作，autopilot 審查。forge 分頁打開時維持現況：下方 ≥12 列才呼叫，forge-stats 拿到的是完整版。

### 1.4 終端尺寸和拿到的尺寸

終端的總高度和總寬度由 Claude Code 分配給對話區、band、提示列、statusline 和 pane。本規範**不分配終端尺寸**，只規定每個畫面在拿到多少空間時怎麼畫。statusline 自己決定印 5 列或 2 列（依心跳，不依終端高度）；終端太矮時，Claude Code 怎麼裁切不歸我們管。測試和審查一律以「拿到的尺寸」為準。

## 2. 資訊優先序：1 列時要留下什麼

### 2.1 全域順序

由高到低：

1. **要人處理的事**（`need_you`）：待決卡數、`⚑` 殘留。
2. **卡住**（`stalled`／`failed`）：自己的判定、艦隊的 `⊘` 數。
3. **正在跑**（`running`）、**等待中**（`waiting`）的數量。
4. 進度、用量。
5. 裝飾：寵物、Ferris 動畫、走勢圖。

不管哪個畫面，寬度不夠時都從清單尾端開始拿掉，不從頭拿。

autopilot band 第 1 列的格式（live `model.ts` `layoutBand`）：判定（符號加字）、位置、單元進度、派工計數、待決、花費、配額、`⚑N`，最右邊是分頁名。有待決卡時，「待決」分頁名用粗體加黃色標出；`◆N` 計數只在 `≥160` 欄畫。規範要求：任何寬度下，只要有待決卡，第 1 列都要留一個要你處理的標示（`▲` 或粗體黃色的「待決」），跟判定符號、`ⓘ`、`⚑N` 一樣不捨。窄寬度（24）下現況是否做到還沒驗證，列入 §12。

### 2.2 一行摘要

forge-stats 拿到 1–5 列時畫一行摘要（有 autopilot 時就是分頁列下面的第 2 列，§1.1 規則 3）。從左到右的格式固定，依 §2.1 的優先序排：

```
艦隊 ▲ 1  ✗ 1  ⊘ 2  ● 4  ⋯ 11 · Ferris Lv.3
```

間距照 §3.5：每個計數是一個晶片（符號、半形空格、數字），晶片之間空兩格。

- `▲` 要人處理（`blocked_human`）、`✗` 失敗、`⊘` 疑似卡住、`●` 執行中、`⋯` 等待中，都取自 `autopilot.fleet` atom（G33；`▲` 和 `✗` 的計數由 G33a 加進 atom）。數字為 0 的那一類不畫。
- 「艦隊 ✓」只在五類都是 0，**而且**最後一次抓取成功、沒有 `error` 時才寫。抓取失敗時寫「艦隊 ?」。拿不到 atom（沒有 autopilot）時，整段艦隊不畫。
- 待決卡數和 `⚑` 殘留屬於 autopilot 的分頁列，一行摘要不重複顯示。
- 寬度不夠時，從尾端拿掉：寵物 → `⋯` → `●` → `⊘` → `✗` → 「艦隊」兩字 → `▲`。
- **最窄形式**：最嚴重的那一個非零計數單獨一個（例如 `▲ 1`、`✗ 1`、`⊘ 2`）。這樣也放不下、只剩「艦隊」兩字，或沒有任何可畫的東西（沒有 atom，也沒有寵物）時，把這一列交回：原樣回傳 `next(e)`。不畫空白列。

## 3. 狀態符號與顏色：共用一張表

### 3.1 狀態表

所有畫面都只能用這張表。程式碼的來源是 autopilot `mods/live/ui.ts` 的 `GLYPHS.state`／`STATES`。

| 狀態 id | 符號 | ASCII 備援 | 顏色（主題鍵） | zh-TW | en | 意思 |
|---|---|---|---|---|---|---|
| need_you | ▲ | `^` | warning | 要你處理 | needs you | 等你決定或核准 |
| failed | ✗ | `x` | error | 失敗 | failed | 結束了，而且失敗 |
| stalled | ⊘ | `~` | error | 疑似卡住 | stalled | 安靜太久，可能卡住 |
| findings | △ | `!` | 預設文字色 | 待修 | to fix | 審查有發現，作者在修 |
| running | ● | `*` | suggestion | 執行中 | running | 正在進行 |
| waiting | ⋯ | `w` | inactive | 等待中 | waiting | 已接手，在等某件事（背景工作、相依項目、空出的名額） |
| done | ✓ | `v` | success | 完成 | done | 已完成 |
| pending | ○ | `o` | inactive | 未開始 | not started | 還沒開始 |
| stopped | ◌ | `-` | inactive | 已停 | stopped | 沒做完就停了 |
| skipped | ∅ | `/` | inactive | 不需要 | not needed | 不需要做，略過 |
| unknown | ? | `?` | inactive | 未知 | unknown | 讀不到狀態 |

ASCII 備援也必須只佔 1 格，而且彼此不同（`^ x ~ ! * w v o - / ?`）。只在終端畫不出單格符號時使用；Unicode 的替代符號 `◍`（⊘ 的替代）和 `!`（△ 的替代）是 live `GLYPHS.fallback`，跟上表的 ASCII 欄是兩件事。

結構符號和圖表符號沒有 ASCII 模式；怎麼判斷終端畫不出單格符號（環境變數或 glyph-check 結果檔）現在還沒定，列入 §12。

嚴重度由高到低：`need_you > failed > stalled > findings > running > waiting > stopped > pending > done > skipped > unknown`。合併多個狀態時取最嚴重的（`worst()`）。`unknown` 排最後，所以缺一個對應不會蓋掉真正的問題。

### 3.2 結構符號

`▾ ▸`（展開／收合）、`─ ━`（分隔線、目前分頁）、`└`（明細）、`›`（可跳轉）、`◦`（項目）、`—`（無資料）、`⚑`（殘留）、`ⓘ`（說明；固定佔 2 格，寫成 `'ⓘ '`）、`ϟ`（速度；分頁內部 id 是 `forge`，畫出來的名稱是 `ϟ`）、`↺`（審查輪次）、`◆`（決定）、`◷`（時間）、`★`（推薦）、`↓`（落後上游）、`↑`（領先上游，§3.4 遷移時新增）。圖表符號：`▰ ▱`（量表）、`▁…█`（走勢）、`◐`（你在這裡）。

### 3.3 規則

1. **一個符號只有一個意思**：狀態、結構、圖表三組不能共用符號（I20；捲軸是登記過的例外）。圖例分頁的名稱 `?` 跟「未知」同一個字，是第二個登記的例外：分頁名是位置，不是狀態，旁邊永遠有其他分頁名。新畫面不能另外發明符號；需要新意思時，先改這張表。
2. **符號一定只佔 1 格**。驗證狀態（live `GLYPHS.verified`）：2026-10-13 在 owner 的 ssh + tmux 驗過的有 `⊘ △ ○ ◌ ∅ ◦ › ━ ◐ ◍`；待 owner 驗證（`pending-owner`）的有 `↓ ★ ⋯`，以及 `↑`（新增，尚未登記）。`⋯`（U+22EF，East Asian Width N）在 2026-11-04 取代原本的等待符號 `⧗`：owner 的終端字型把 `⧗` 畫得像 `✗`，等待會被看成失敗（PM 裁定；`◷` 已是「時間」、`◔` 太像 `◷`、`◌○◦` 已有其他意思）。其他符號是 live 改版前就在用、沒有個別紀錄的舊符號，比照 `pending-owner`，下次 owner 跑 glyph-check 時一起驗。新符號加進 `NEW_GLYPHS` 時要附 `verified`，填 `YYYY-MM-DD <終端>`，或在 owner 驗證前填 `pending-owner`。驗證方式：owner 在真實終端（ssh + tmux）跑 `node mods/live/glyph-check.js`。East Asian Ambiguous 的符號在 CJK 終端可能變成 2 格，一律要驗。
3. **禁用 emoji**：`\p{Extended_Pictographic}`、U+FE0F、U+20E3 都不能出現（`EMOJI_BANNED`）。唯一例外是 `detail` 列。
4. **顏色只能用主題鍵**：`warning`、`error`、`success`、`suggestion`、`inactive`、`subtle`、`claude`、`merged`，全部透過 `PALETTE` 角色取用。不用原始 RGB，不用背景色，不用 inverse。例外是寵物像素圖和地圖：它們是內容不是狀態（Raster 半格，xterm-256 色，見 `codeforge-mod-ui.md` §5）。
5. **黃色每一列最多一個**：只用在要人處理的那件事。band 有兩列時，autopilot 那列和 forge-stats 那列各自最多一個。粗體只用白名單（`BOLD_USES`）。

### 3.4 現況偏差（要遷移，不在 G34 改）

| 位置 | 現況 | 衝突 | 遷移方向 |
|---|---|---|---|
| forge-stats 記憶／大腦燈號 | `●` 綠／灰（`model.ts` `GLYPH.latest`） | `●` 在共用表裡是「執行中」 | 健康狀態不是工作狀態，不借用狀態符號：只畫字樣（例如 `記憶 正常`、`大腦 離線`），用主題色；沒資料寫 `—` |
| statusline 大腦燈號 | `● ◐ ○ ◌`，原始 RGB（`statusline.rs` brain lights，`BRAIN_GREEN` 等） | `◐` 是「你在這裡」，`○` 是「未開始」，`◌` 是「已停」 | 同上：只畫字樣（正常、降級、離線、未檢查），顏色依 §5 的 ANSI 對照 |
| statusline 量表 | `▮▯`（`statusline.rs` 用量列） | 共用表的量表是 `▰▱` | 改成 `▰▱` |
| statusline 上游 | `⇡⇣`（`statusline.rs` `git_ahead_behind`） | 共用表的落後是 `↓` | 改成 `↑N ↓N`；`↑` 先登記進 live `GLYPHS.structure`（`ahead`），並列入 glyph-check |

### 3.5 間距（晶片與分隔）

一列裡的元件只有五種接法，每種固定寫法，不臨時拼空格：

| 接法 | 寫法 | 例子 |
|---|---|---|
| 晶片內：狀態符號接數字或字 | 半形空格 1 格 | `⊘ 12`、`✓ 完成`、`↺ R3` |
| 晶片內：修飾晶片的文字標籤接符號 | 半形空格 1 格 | `dev ⚑ 2`、`艦隊 ▲ 1` |
| 晶片之間 | 半形空格 2 格 | `⊘ 12  ● 4` |
| 次要資訊（meta、寵物） | `' · '`（暗色） | `… · Ferris Lv.3` |
| band 的組與組之間 | `' │ '` | `↺ R3 │ $329` |

規則：

1. **符號不黏字**：狀態符號（§3.1）和結構符號（§3.2）後面一律先空 1 格，再接數字或字。例外只有三種：貨幣 `$329`（單位，不是狀態）、固定寬度的 `ⓘ`（§3.2），以及空間本身就是語意的圖表符號（量表 `▰▱`、走勢 `▁…█`）。
2. **標籤屬於晶片**：修飾某個晶片的文字標籤是那個晶片的一部分，寫成「標籤、1 格、符號、1 格、值」（`dev ⚑ 2`）。2 格的間隔只用在晶片和晶片之間，標籤和它的符號之間永遠是 1 格。一行摘要開頭的「艦隊」同理，接第一個晶片只空 1 格。
3. **寫法只有一處**：每個 mod 用一個 helper 組晶片（forge-stats `chipParts` + `CHIP_GAP`；live 的對應 helper 見下方遷移表），分隔字串用常數，不在呼叫處寫 `' '` 字面值。
4. **用測試守住**：每個 mod 都要有一個不變式測試，掃過所有寬度下的輸出，斷言狀態符號後面不直接接非空白字元、標籤和它的符號之間只有 1 格，也不出現連續 3 格空白。新畫面加進來時，要把它的輸出接進這個測試。
5. **寬度照算**：晶片多出來的空格算進寬度，縮窄時照 §2.1 的優先序整個晶片拿掉，不拆開符號和數字。

現況偏差（2026-11-03 盤點，live 這邊由 autopilot 遷移，不在 codeforge 改）：

| 位置 | 現況 | 遷移方向 |
|---|---|---|
| live band 派工計數（`model.ts` `slotDispatch`） | `●3 ⊘1` | `● 3  ⊘ 1` |
| live `stateBadge`（`ui.ts`） | `⊘2` | 改走晶片 helper（`countChip` 已是 `✓ 32`） |
| live 待決、殘留（`slotDecisions`、`slotHygiene`） | `◆7`、`?2`、`⚑3`、`dev↓3` | 晶片寫法 |
| live QC 標記（`slotReview`） | `QC▲` | `QC ▲` |

## 4. 正體中文文案

1. **用白話**：寫「疑似卡住」，不寫「stall」；寫「沒人接」，不寫「unaccepted」。技術名詞（gate、worktree、sha、指令、路徑、id）保留英文，跟中文之間空半形空格：`3 個 worktree 要你處理`。
2. **台灣用語**：設定、回報、檔案、預設、專案、資料、執行中、已進行。不用簡體或中國用語（缺省、默認、文件夾、信息）。
3. **句子要有主詞或受詞**，不寫只剩動詞的命令句。
4. **不拼接翻譯片段**：語序寫在同一個字串裡（`{age} 前`），佔位符兩個語系要一致（`gen-locales --check`）。
5. **長度**：鍵欄標籤 ≤12 格（中文 ≤6 字）；分頁名是 2 個中文字，或登記過的符號（`?` 圖例、`ϟ` 速度）；狀態字見 §3.1，不另外發明。
6. **時間與數字**：經過時間和年齡一律用精簡格式，最多兩個單位（`8h05m`、`3m`，live `fmtd`）；年齡加「前」（`2h05m 前`），寫在同一個翻譯字串裡。百分比取整數、金額 `$175.33`、sha 取 8 位。全部經格式化函式處理（I8）。

## 5. 淺色／深色終端

1. 只用主題鍵，終端換深淺主題時會自動調整（§3.3-4）。statusline 不是 mod，拿不到主題鍵，所以改用 ANSI 基本色（終端依自己的深淺主題調色），不寫死 RGB（§3.4 遷移）。對照：

   | 主題鍵 | statusline ANSI |
   |---|---|
   | warning | 33 黃 |
   | error | 31 紅 |
   | success | 32 綠 |
   | suggestion | 34 藍 |
   | inactive、subtle | 預設前景色加 2（dim）；不用 90 亮黑，它在淺色底上幾乎看不見 |
   | claude、merged | 35 洋紅 |
2. `dim` 是字重，不是顏色。在淺色底上，`dim` 加 `inactive` 會太淡，所以重要資訊不能只靠 dim 表示。
3. 截圖審查時（§9），深色和淺色各附一張。至少要證明 `inactive`、`subtle`、`warning` 在淺色底上讀得出來。

## 6. 截斷

1. **一個項目只佔一列**（DS §2.4，`withToggle`／`fitSegs`）。寬度不夠時依序：先把本文截到最少 8 格，再從尾端整段丟掉 ` · meta`，再截最後一段 meta，最後才繼續截本文。右側的 `▸`／`›` 永遠保留。
2. **說明文字會折行，不截斷**（`wrapLines`）：在空白、`/` 後、中文字之間斷行；收尾標點不放到行首；續行縮排對齊。
3. **截斷一律依顯示寬度**：中文佔 2 格，用 `displayWidth`／`cutProse`。不能依字元數截（`.chars().take(N)` 只能防止切到字的中間，不能保證寬度）。
4. **截斷時加 `…`**：文字從右邊截，`…` 放尾端；id 從左邊截，保留可辨識的尾巴（`…2031601-0e0c`）。
5. **資料端不截**：狀態 atom、快取檔保留原文，由畫面那一端依實際寬度截。唯一例外是防爆上限（例如艦隊 title 200 字元）：超過時在上限處切斷，結尾加 `…`，讓畫面端知道這段已經不完整。

截斷規則對應的檢查：規則 1 有 live「no tab clips value text with … at widths 60/76/90」測試；規則 3、4 有 live I8（左截 id 帶 `…` 前綴、截斷的文字帶 `…` 後綴）；規則 2（折行）和規則 5（資料端上限）目前沒有檢查，列在 §11 待補。

## 7. 不准水平捲動

1. 沒有任何一行比可畫的寬度寬（band 用 `bodyColumns - 2`，pane 用 `paneWidth`）。測試要斷言「每一行的顯示寬度 ≤ 欄寬」，live mod 已有，60、76、90 三種寬度都要過。
2. 表格欄位依優先序取捨，寬度不夠就整欄拿掉，不硬塞。
3. 截圖時也一樣（§9）：100 欄以內要完整可讀。

## 8. 刷新與成本

1. **render 時不准跑子行程**：畫面只讀已經算好的資料（module 變數、`$.state`、快取檔）。外部指令（`fleet`、`git`、`claude --version`）只在計時器或事件裡跑，要有 TTL，同一時間只跑一個，失敗時保留上一次的結果。
2. **有人看才抓**：資料只在有讀者時才抓。例如艦隊資料只在艦隊分頁正在顯示，或 forge-stats band 的心跳是新的時候才抓（G33）。
3. **輸出要有上限**：外部輸出依 UTF-8 位元組設上限（艦隊 `FLEET_MAX_BYTES = 2_000_000`；被引擎截斷時直接拒收）；接受的列數也要設上限（艦隊 `FLEET_MAX_ROWS = 300`），超過的部分只計數。新的資料來源要在設計時寫明這兩個數字。
4. **頻率**（現況；statusline 的刷新間隔是 settings.json 的 `statusLine.refreshInterval`）：live mod 每 5 秒一個 tick（`TICK_MS`）；艦隊 TTL 30 秒（`FLEET_TTL_MS`，`fleet.ts`）；forge-stats 寵物 15 秒、Ferris 動畫 0.5 秒、心跳 2 秒（`register.tsx` 的 `$.clock.every`）。引擎上限（mod-ui spec，CC 2.1.314）：`invalidate` 每秒最多 10 次（終端裡顯示中的 pane、展開的 band、prompt hint 是 30 次），一個 hook 的時間預算是 10 秒。
5. **statusline**：它是一次性的行程，`refreshInterval` 約 2 秒就跑一次，每次都要很快。網路查詢只讀快取，不在熱路徑上等待（已經是這樣：README「hot path … only reads the cache」，`mnemos::health::maybe_spawn_probe` 在背景探測）。現況偏差：每次執行都會跑 `git branch`、`git rev-list`、`claude --version`，以後要改成讀快取：key 是 cwd 加上 HEAD 解析出的 sha 和上游 ref 的 sha（不能只看 `.git/HEAD` 的 mtime，commit、fetch、push 都不會動它），另加約 30 秒的 TTL。key 要靠讀檔算出（`.git/HEAD`、`refs/`、`packed-refs`，worktree 要跟著 `gitdir` 檔），只有快取沒命中或過期時才跑 `git`；用 `git rev-parse` 算 key 就又變成每次執行都跑子行程。

## 9. 審查截圖

1. **寬度 ≤100 欄，附保留顏色的 HTML 預覽**：operator 的檢視器不能左右捲。
2. **兩種 session 都要拍**：一般 repo（例如 pm）和 autopilot repo，因為 band 的疊放不同。需要時深色、淺色各拍一張（§5）。
3. **截圖裡不能有個人資料**：拍之前確認畫面上沒有 email、帳號、權杖。用量分頁的帳號列不要拍。
4. **臨時 session 不能搶 fleet 席位，也不能被誤認**：空的 MCP 設定擋不住登記。claude-lane 會從 claude 行程內部登記（2026-11-05 實測：沒有 mcp-shim 子行程，仍以 `claude-lane` 登記），席位名稱是「cwd 的 basename--claude」，撞名時加 -N。所以臨時 session 只開在 codeforge 目錄或 scratch 目錄，不要開在別的 lane 的 repo（在 pm 目錄開會叫 `pm--claude-N`，容易被誤認成 PM 的席位）；`FLEET_SEAT` 對 claude-lane 無效（它只看 session 的 kind 和 entrypoint，2026-11-05 實測仍登記成 `codeforge--claude-2`）。目前沒有能關閉登記的開關（hangar backlog 576d0e9，F4 之後做 `FLEET_REGISTER=0`）。拍完確認 `fleet peers` 裡那個成員已經 offline。要讓用量分頁讀假資料，`AUTOPILOT_LIVE_DIR` 指向的目錄要同時放 `codeforge/pet.json`（mod 在哪個目錄找到 pet.json，就從那裡讀用量）。不要開在還沒信任的目錄：信任提示一按下去，就會寫進使用者設定。

步驟（在要拍的 repo 目錄執行）：

```sh
tmux new-session -d -s cap -x 100 -y 40
tmux send-keys -t cap "claude --plugin-dir <worktree> [--plugin-dir <第二個 worktree>] --strict-mcp-config --mcp-config '{\"mcpServers\":{}}'" Enter
# 等畫面穩定，操作到要拍的狀態（必要時手動打開 pane）
tmux capture-pane -p -e -t cap > cap.ansi
tmux capture-pane -p    -t cap > cap.txt
python3 <codeforge>/scripts/ansi2html.py cap.ansi > cap.html   # <codeforge> 是 codeforge clone 的路徑
fleet upload cap.html --name <名稱>.html --mime text/html --summary "<說明>"
fleet upload cap.txt  --name <名稱>.txt  --mime text/plain --summary "<說明>"
tmux kill-session -t cap
```

回報裡附兩個 blob sha。`--plugin-dir` 指向開發中的 worktree 時，引擎會在該目錄產生 `tsconfig.json`；拍完用 `git status` 看，只有未追蹤（`??`）的才刪。forge-stats 的 `tsconfig.json` 有進 git，不要刪。兩個 plugin 都在開發中時（例如 G33 同時改 live mod 和 forge-stats），各給一個 `--plugin-dir`：autopilot 是 worktree 根目錄，forge-stats 是它自己的 plugin 根目錄（G33 前是 `<codeforge>/.claude/skills/forge-stats`，G33 搬移後以新位置為準）。拍之前用 `/plugin` 確認載入的是開發中的版本。

淺色截圖：先在臨時 session 裡用 `/theme` 把 Claude Code 主題切成淺色，再拍，HTML 加 `--light`（`scripts/ansi2html.py` 的選項，只改 HTML 底色）。主題是使用者層級的設定，拍完一定要用 `/theme` 切回原本的主題。只把深色截圖套上淺色 HTML 底色，證明不了主題鍵在淺色主題下讀得出來。

## 10. 無障礙：不能只靠顏色

1. 狀態的符號、顏色、字樣三者同時出現（DS §3.2，I21）。只有兩種精簡場合可以省略字樣：同狀態 chip 下的項目，以及 band 的數字徽章。兩者旁邊的符號都已經表達了狀態。
2. 圖例（`?` 分頁）列出每個符號和它的字樣（I15）。
3. `NO_COLOR`：環境變數非空時生效。statusline 已經遵守（`statusline.rs` `color_enabled`）：改成純文字，燈號改成文字，例如 `memory:active`。mod 的規則是：不設任何顏色與 dim，粗體保留；寵物像素圖（Raster）不畫，改畫文字（例如 `Ferris Lv.3`）。live mod 和 forge-stats 目前都還沒做，列為後續工作（§12）。
4. 要人處理的事，除了黃色，還要有粗體或 `▲` 其中之一。

## 11. 機械化檢查（已有／待補）

| 規則 | 檢查 | 狀態 |
|---|---|---|
| 一符號一意思、單格寬、禁 emoji | live `ui-lint` L3、I5、I20；`glyph-check.js` | 已有（live） |
| 只用主題鍵、粗體白名單 | live L1、L8、L9 | 已有（live） |
| 行寬 ≤ 欄寬 | live「widths 60/76/90」測試 | 已有（live）；待補：寬度 20（pane 最窄）、24、56（band 必查寬度），以及 forge-stats |
| 數字經格式化函式 | live I8 | 已有（live） |
| 文案長度、語系一致 | `gen-locales --check` | 已有（live）；forge-stats `strings.ts` 待補 |
| 截斷帶 `…`、左截 id | live I8 | 已有（live） |
| 說明文字折行、資料端上限加 `…` | — | 待補 |
| render 時不跑子行程 | live `process` allowlist（lint） | 已有（live）；statusline 待補 |
| 跨畫面共用一張符號表 | — | 待補：forge-stats／statusline 遷移後，加一條比對 live `GLYPHS` 的測試 |
| 符號不黏字、晶片間距（§3.5） | forge-stats `band.test.tsx` 不變式測試（每個寬度掃過一行摘要） | forge-stats 已有；live 待補（隨 §3.5 遷移表一起做） |

## 12. 後續工作（本文要求、但現況還沒做的）

| 項目 | 依據 | 負責 | 驗收 |
|---|---|---|---|
| 分頁沒開時，autopilot 把第 2 列交給 forge-stats 的一行摘要（PM 裁定 B） | §1.1、§1.3、§2.2 | G33a（autopilot band）＋G33（forge-stats） | live 測試：分頁沒開、`maxRows ≥ 2` 時呼叫下層並疊在第 2 列；下層 `next(e)` 時 band 維持 1 列。100 欄 pm／autopilot session 截圖都看得到艦隊摘要 |
| forge-stats 在 `maxRows = 1` 時畫一行摘要（現況：1 列時什麼都不畫） | §1.3 | G33 | forge-stats 測試：`maxRows = 1` 時畫出摘要；沒東西可畫時回傳 `next(e)` |
| atom 加 `▲` 要人處理、`✗` 失敗的計數 | §2.2 | G33a | live 測試：讀者 plugin 讀到這兩個欄位 |
| forge-stats、statusline 的符號和顏色遷移 | §3.4、§5 | codeforge（另開單） | 比對 live `GLYPHS` 的測試通過 |
| mod 遵守 `NO_COLOR` | §10 | live、forge-stats | 設 `NO_COLOR=1` 的測試：沒有顏色屬性、沒有 Raster |
| statusline 不再每次執行都跑 `git`、`claude --version` | §8 | codeforge | 快取命中時沒有子行程（測試以假的 PATH 驗證） |
| forge-stats 行寬檢查、文案檢查 | §11 | codeforge | 新增測試 |
| forge-stats 改成全域 plugin（不只 codeforge clone 載入） | §1.1 | G33 | 在非 codeforge repo 的 session 裡看得到 forge-stats |
| forge-stats 迷你版和完整版第一列畫艦隊摘要 | §1.2 | G33 | 測試：有 atom 且列數 ≥7／≥13 時第一列是摘要；剛好 6／12 列時不畫摘要 |
| forge-stats 心跳加 `summary_row: true` | §1.1 | G33 | statusline 和 live 都讀得到；舊版沒有這個欄位時不拿第 2 列 |
| band 第 1 列在任何寬度都保留待決標示 | §2.1 | live（另開單） | 寬度 24 有待決卡時的 band 測試 |
| ASCII 模式的觸發條件、結構符號的處理 | §3.2 | live＋codeforge | 規範補上觸發方式 |
| live band 捨棄順序改成先 `●` 後 `⊘` | §1.2 | live（另開單） | band 寬度測試 |
| forge-stats pane 不再至少畫 30 欄 | §1.2、§7 | codeforge | 寬度 20 的行寬測試 |
| statusline 截斷改用顯示寬度 | §1.2、§6.3 | codeforge | 中文內容在窄寬度下不超寬的測試 |
| 舊符號和 `↑` 登記進 live `NEW_GLYPHS`／glyph-check，owner 驗證 | §3.3 | live＋owner | `verified` 欄位齊全 |
| `codeforge-mod-ui.md` §5 改成指向本文 | 前言 | 本單（G34） | 已改 |


## 13. 聊天室式 pane：操作區在底、內容往上長

操作員回饋，PO 採用（2026-11-05）：操作區固定在底部，資訊從操作區往上長，視線和手都不用跟著移動。live mod、roundtable、forge-stats 三邊的 pane 都照這一節做，autopilot 的文件連結到這裡，不另寫一份。

**適用範圍**：會變長的 pane。forge-stats：pane 的寵物、用量分頁。autopilot：`mods/live` 的待決卡片和 job 頁、`mods/roundtable`。`mods/viewer`（檔案和 diff 檢視器）只把底部的按鍵提示固定住，內容仍然從頂端往下讀。AbovePrompt band 本來就貼在輸入框上面，高度固定，不在此列。

1. **操作區固定在底部**：選項列、按鈕、輸入列、按鍵提示放在 pane 最底部的幾行，不隨內容捲動，位置不變。
2. **底部錨點**：最新的一項貼在操作區正上方，舊的往上推。dock 的 pane 高度是 `bodyRows`，內容不滿一頁時，空白留在上方。inline 的外框會貼合 tree 的高度，所以不留空白，直接從操作區往上長，最高長到 `bodyRows`。
3. **只捲內容區**：內容超出時可以往上捲看舊的，操作區不動。捲動狀態（例如「↑ 還有 12 行」）顯示在內容區的第一行，不放在操作區。引擎的 body 捲動會捲整棵 tree，所以 mod 要自己切內容區的視窗。做法：hook `ui.scroll`，回傳自己的 offset。內容區高度 = `bodyRows` − 操作區行數 − 捲動提示那一行（有顯示時才扣）。操作區一定要放得下：扣掉操作區後不到 2 行，就不畫捲動提示；一行都不剩，內容區就是 0 行。
4. **追加型內容不搶焦點**：使用者已經往上捲時，新內容進來不跳回底部，改在操作區上方一行顯示「有新訊息 ↓」，按 End 或按那一行回到底部。使用者本來就在底部時，自動跟著最新的。**快照型內容**（整份重新讀取，例如 forge-stats 的用量和寵物分頁）沒有「新訊息」：重新整理後停在原本那一行，原本在底部的留在底部。
5. **resize**：先算操作區的高度，內容區在剩下的高度內重新排版，維持底部錨點。原本在底部的仍在底部，原本往上捲的停在同一則。
6. **操作區高度**：同一個畫面裡固定。捲動內容、內容追加、快照重新整理，都不會改變操作區的高度。只有換卡、換分頁這種使用者自己切換情境的時候才可以變，一變就重新排版，維持底部錨點。forge-stats 的操作區是分頁列加按鍵提示，固定兩行。
7. **按鍵**（兩邊一致）：↑↓ 一行；PgUp／PgDn 一頁；Home 跳到最舊；End 回到最新。這些鍵只送到握有鍵盤的 pane，滾輪不需要焦點。所以人開窗的路徑（斜線命令）要帶 `$.ui.open({ focus: true })`，並且在第一個 `await` 之前呼叫：`focus` 是請求，只在輸入框是空的時候批准；人正握著 band 或 pane 的元素時會被拒絕，所以從 band 按鈕開窗拿不到鍵盤。session 開始時自動開的窗不帶 `focus`，不搶輸入框。

**測試**（每個 mod 都要有）：
- 不滿一頁時，dock 的空白在上方，inline 沒有空白。
- 超出時，捲動前後操作區（forge-stats 的分頁列）所在的螢幕列號不變。這一條是 PM 指定的。
- End 回到最新，Home 到最舊。
- resize 後，原本在底部的仍在底部。
- 快照型重新整理後停在原本那一行。追加型在往上捲時出現「有新訊息 ↓」，在底部時跟著最新。
