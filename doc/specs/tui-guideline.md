# 終端介面規範（TUI guideline）

> 適用於所有我們出貨的終端介面，包括 codeforge statusline、forge-stats（band／pane）、autopilot live mod（band、分頁列、pane、待決卡）和艦隊畫面（G23 分頁、G33 band 欄）。
> 目的是讓同一個符號、同一個顏色、同一個字在每個畫面上都是同一個意思；畫面再窄、再矮，最重要的那件事也還看得到。
> 這份文件是規範的單一來源。autopilot 的 `docs/plans/2026-10-13-live-pane-design-system.md`（以下簡稱 DS）負責 live mod 內部的實作細節。兩者衝突時以本文為準，並回頭修正 DS。
> 狀態：v1（PM G34，2026-11-03）。本文所說的「現況」都附出處，數字以程式碼為準。

## 1. 空間合約：誰擁有哪幾列、哪幾欄

### 1.1 版面上的四塊

由上而下：

| 區塊 | 擁有者 | 高度 | 載入範圍 |
|---|---|---|---|
| 對話區 | Claude Code | 其餘高度 | — |
| AbovePrompt band | autopilot live mod，在上；forge-stats，在下 | autopilot 的 band 是 1 列（`BAND_ROWS = 1`），分頁打開時往下長；forge-stats 拿到的是 autopilot 傳下來的 `maxRows`／`bodyColumns` | live mod 是全域 plugin；forge-stats 在 G33 之後也改成全域 |
| 提示列 | Claude Code | — | — |
| statusline | codeforge statusline（Rust，一次性的行程） | 完整版 5 列；精簡或無寵物時 2 列 | 全域 settings.json，每個 session 都有 |
| 側邊 pane | live mod（`autopilot-live`）或 forge-stats（`/forge-stats layout pane`） | 整個畫面高 | 寬度 ≥144 且全螢幕時，live mod 才會自動開 pane |

規則：

1. **疊放順序固定**：在 band 裡，autopilot 在上、forge-stats 在下。下層只能畫上層透過 `next({...e, props: {maxRows, bodyColumns}})` 給它的空間，不能自己往上長。`maxRows` 缺或不是數字，就當作 0，不畫。
2. **`bodyColumns` 預留 2 格**：引擎回報的寬度可能比實際能畫的多 2 格（mod-ui spec）。行寬的上限是 `bodyColumns - 2`。
3. **1 列時歸上層**：band 只有 1 列時，那一列是 autopilot 的分頁列，forge-stats 原樣回傳 `next(e)`（B32）。沒有 autopilot 的 session 例外：那一列給 forge-stats 的一行摘要（§2）。
4. **每一項都能單獨跑**（B31）：少了任何一個 plugin，其他的照樣能畫，不能留下空洞或錯誤字樣。資料來源不在時，那一欄或那一列直接不畫。

### 1.2 寬度分級

每個畫面依自己拿到的寬度分級。下面是審查時必須逐一檢查的寬度：24、56、86、100、144 以上。

| 畫面 | 分級（欄） |
|---|---|
| autopilot band 分頁列 | `<80`：判定、`●N ⊘N`、`⚑N`。`80–119`：加單元進度條、`QC`、3 格花費條、一個配額窗。`120–159`：加 stage 和完整計數。`≥160`：全部 |
| forge-stats band（B32 後；G33 會加上艦隊欄） | `<26`：一行摘要。`24–55`：Ferris。`56–85`：Ferris｜寵物。`≥86`：Ferris｜寵物｜記憶與大腦。迷你 Ferris 寬 16 格，所以門檻都往下減 8 |
| statusline | 寬度由 `CODEFORGE_WIDTH` 決定，預設 100。`≥90` 才畫右欄寵物圖 |
| pane | live 的 pane 在 `≥144` 欄且全螢幕時自動打開；預設寬 76（`paneWidth`），最窄 20 |
| 審查截圖 | 固定 100 欄（§9） |

autopilot band 的捨棄順序見 live `model.ts` 的 band 寬度表：配額 → 截短 stage → 單元進度條 → `⊘` 數 → `●` 數 → 縮短判定字。判定符號、`ⓘ`、`⚑N` 永遠不捨。

### 1.3 高度分級（band）

| 拿到的列數 | forge-stats | autopilot |
|---|---|---|
| 0 | 不畫 | 不畫 |
| 1 | 有 autopilot：原樣傳下，什麼都不畫；沒有 autopilot：一行摘要（§2.2） | 分頁列 |
| 2–5 | 一行摘要 | 分頁列，加上分頁打開時的內容 |
| 6–11 | 迷你 Ferris（16×6）加各欄 | 同上 |
| ≥12 | 完整版 | 分頁內容最多 12 列（`FORGE_FULL_ROWS`） |

## 2. 資訊優先序：1 列時要留下什麼

### 2.1 全域順序

由高到低：

1. **要人處理的事**（`need_you`）：待決卡數、`⚑` 殘留。
2. **卡住**（`stalled`／`failed`）：自己的判定、艦隊的 `⊘` 數。
3. **正在跑**（`running`）、**等待中**（`waiting`）的數量。
4. 進度、用量。
5. 裝飾：寵物、Ferris 動畫、走勢圖。

不管哪個畫面，寬度不夠時都從清單尾端開始拿掉，不從頭拿。

### 2.2 一行摘要

forge-stats 在 2–5 列，或在沒有 autopilot 時只有 1 列，會畫一行摘要。格式固定，從左到右是：

```
艦隊 ⊘2 ●4 ⧗11 · Ferris Lv.3
```

- 艦隊部分只在拿得到 `autopilot.fleet` atom 時才出現（G33）。數字為 0 的那一類不畫。三類都是 0 時，寫「艦隊 ✓」。
- 寬度不夠時，先拿掉寵物，再拿掉 `⧗`，再拿掉 `●`。`⊘` 留到最後。
- 不會出現「什麼都不畫，只剩空白一列」：拿得到列就一定畫東西，拿不到列就整個不畫。

## 3. 狀態符號與顏色：共用一張表

### 3.1 狀態表

所有畫面都只能用這張表。程式碼的來源是 autopilot `mods/live/ui.ts` 的 `GLYPHS.state`／`STATES`。

| 狀態 id | 符號 | ASCII 備援 | 顏色（主題鍵） | zh-TW | en | 意思 |
|---|---|---|---|---|---|---|
| need_you | ▲ | `!!` | warning | 要你處理 | needs you | 等你決定或核准 |
| failed | ✗ | `x` | error | 失敗 | failed | 結束了，而且失敗 |
| stalled | ⊘ | `~`（第二選擇 ◍） | error | 疑似卡住 | stalled | 安靜太久，可能卡住 |
| findings | △ | `!` | 預設文字色 | 待修 | to fix | 審查有發現，作者在修 |
| running | ● | `*` | suggestion | 執行中 | running | 正在進行 |
| waiting | ⧗ | `w` | inactive | 等待中 | waiting | 已接手，在等某件事（背景工作、相依項目、空出的名額） |
| done | ✓ | `v` | success | 完成 | done | 已完成 |
| pending | ○ | `o` | inactive | 未開始 | not started | 還沒開始 |
| stopped | ◌ | `-` | inactive | 已停 | stopped | 沒做完就停了 |
| skipped | ∅ | `/` | inactive | 不需要 | not needed | 不需要做，略過 |
| unknown | ? | `?` | inactive | 未知 | unknown | 讀不到狀態 |

嚴重度由高到低：`need_you > failed > stalled > findings > running > waiting > stopped > pending > done > skipped > unknown`。合併多個狀態時取最嚴重的（`worst()`）。`unknown` 排最後，所以缺一個對應不會蓋掉真正的問題。

### 3.2 結構符號

`▾ ▸`（展開／收合）、`─ ━`（分隔線、目前分頁）、`└`（明細）、`›`（可跳轉）、`◦`（項目）、`—`（無資料）、`⚑`（殘留）、`ⓘ`（說明；固定佔 2 格，寫成 `'ⓘ '`）、`ϟ`（速度）、`↺`（審查輪次）、`◆`（決定）、`◷`（時間）、`★`（推薦）、`↓`（落後上游）。圖表符號：`▰ ▱`（量表）、`▁…█`（走勢）、`◐`（你在這裡）。

### 3.3 規則

1. **一個符號只有一個意思**：狀態、結構、圖表三組不能共用符號（I20；捲軸是唯一登記過的例外）。新畫面不能另外發明符號；需要新意思時，先改這張表。
2. **符號一定只佔 1 格**：新符號加進 `NEW_GLYPHS` 時要附 `verified`，填 `YYYY-MM-DD <終端>`，或在 owner 驗證前填 `pending-owner`。驗證方式：owner 在真實終端（ssh + tmux）跑 `node mods/live/glyph-check.js`。East Asian Ambiguous 的符號在 CJK 終端可能變成 2 格，一律要驗。
3. **禁用 emoji**：`\p{Extended_Pictographic}`、U+FE0F、U+20E3 都不能出現（`EMOJI_BANNED`）。唯一例外是 `detail` 列。
4. **顏色只能用主題鍵**：`warning`、`error`、`success`、`suggestion`、`inactive`、`subtle`、`claude`、`merged`，全部透過 `PALETTE` 角色取用。不用原始 RGB，不用背景色，不用 inverse。唯一例外是寵物像素圖（Raster 半格，xterm-256 色）。
5. **黃色每個畫面只出現一次**：只用在要人處理的那件事。粗體只用白名單（`BOLD_USES`）。

### 3.4 現況偏差（要遷移，不在 G34 改）

| 位置 | 現況 | 衝突 | 遷移方向 |
|---|---|---|---|
| forge-stats 記憶／大腦燈號 | `●` 綠／灰 | `●` 在共用表裡是「執行中」 | 正常改成 `✓`（success）；沒資料改成 `—`；字樣保留 |
| statusline 大腦燈號 | `● ◐ ○ ◌`，原始 RGB | `◐` 是「你在這裡」，`○` 是「未開始」，`◌` 是「已停」 | 正常 `✓`、降級 `△`、離線 `✗`、還沒檢查 `○`；顏色改用 ANSI 基本色 |
| statusline 量表 | `▮▯` | 共用表的量表是 `▰▱` | 改成 `▰▱` |
| statusline 上游 | `⇡⇣` | 共用表的落後是 `↓` | 改成 `↑N ↓N` |

## 4. 正體中文文案

1. **用白話**：寫「疑似卡住」，不寫「stall」；寫「沒人接」，不寫「unaccepted」。技術名詞（gate、worktree、sha、指令、路徑、id）保留英文，跟中文之間空半形空格：`3 個 worktree 要你處理`。
2. **台灣用語**：設定、回報、檔案、預設、專案、資料、執行中、已進行。不用簡體或中國用語（缺省、默認、文件夾、信息）。
3. **句子要有主詞或受詞**，不寫只剩動詞的命令句。
4. **不拼接翻譯片段**：語序寫在同一個字串裡（`{age} 前`），佔位符兩個語系要一致（`gen-locales --check`）。
5. **長度**：鍵欄標籤 ≤12 格（中文 ≤6 字）；分頁名 2 個字；狀態字見 §3.1，不另外發明。
6. **時間與數字**：`3 分鐘`、`2h05m 前`、百分比取整數、金額 `$175.33`、sha 取 8 位，全部經格式化函式處理（I8）。

## 5. 淺色／深色終端

1. 只用主題鍵，終端換深淺主題時會自動調整（§3.3-4）。statusline 不是 mod，拿不到主題鍵，所以改用 ANSI 16 色的基本色，不寫死 RGB（§3.4 遷移）。
2. `dim` 是字重，不是顏色。在淺色底上，`dim` 加 `inactive` 會太淡，所以重要資訊不能只靠 dim 表示。
3. 截圖審查時（§9），深色和淺色各附一張。至少要證明 `inactive`、`subtle`、`warning` 在淺色底上讀得出來。

## 6. 截斷

1. **一個項目只佔一列**（DS §2.4，`withToggle`／`fitSegs`）。寬度不夠時依序：先把本文截到最少 8 格，再從尾端整段丟掉 ` · meta`，再截最後一段 meta，最後才繼續截本文。右側的 `▸`／`›` 永遠保留。
2. **說明文字會折行，不截斷**（`wrapLines`）：在空白、`/` 後、中文字之間斷行；收尾標點不放到行首；續行縮排對齊。
3. **截斷一律依顯示寬度**：中文佔 2 格，用 `displayWidth`／`cutProse`。不能依字元數截（`.chars().take(N)` 只能防止切到字的中間，不能保證寬度）。
4. **截斷時加 `…`**：文字從右邊截，`…` 放尾端；id 從左邊截，保留可辨識的尾巴（`…2031601-0e0c`）。
5. **資料端不截**：狀態 atom、快取檔保留原文（只設防爆上限，例如 200 字元），由畫面那一端依實際寬度截。

## 7. 不准水平捲動

1. 沒有任何一行比可畫的寬度寬（band 用 `bodyColumns - 2`，pane 用 `paneWidth`）。測試要斷言「每一行的顯示寬度 ≤ 欄寬」，live mod 已有，60、76、90 三種寬度都要過。
2. 表格欄位依優先序取捨，寬度不夠就整欄拿掉，不硬塞。
3. 截圖時也一樣（§9）：100 欄以內要完整可讀。

## 8. 刷新與成本

1. **render 時不准跑子行程**：畫面只讀已經算好的資料（module 變數、`$.state`、快取檔）。外部指令（`fleet`、`git`、`claude --version`）只在計時器或事件裡跑，要有 TTL，同一時間只跑一個，失敗時保留上一次的結果。
2. **有人看才抓**：資料只在有讀者時才抓。例如艦隊資料只在艦隊分頁正在顯示，或 forge-stats band 的心跳是新的時候才抓（G33）。
3. **輸出要有上限**：外部輸出依 UTF-8 位元組設上限（艦隊是 2 MB，被截斷時直接拒收）；接受的列數也要設上限，超過的部分只計數。
4. **頻率**：live mod 每 5 秒一個 tick；艦隊 TTL 30 秒；forge-stats 寵物 15 秒、Ferris 動畫 0.5 秒、心跳 2 秒。`invalidate` 每秒最多 10 次（顯示中的 pane 是 30 次）。一個 hook 的時間預算是 10 秒。
5. **statusline**：它是一次性的行程，`refreshInterval` 約 2 秒就跑一次，每次都要很快。網路查詢只讀快取，不在熱路徑上等待（已經是這樣）。現況偏差：每次執行都會跑 `git branch`、`git rev-list`、`claude --version`，以後要改成讀快取，以 cwd 加 `.git/HEAD` 的 mtime 當 key。

## 9. 審查截圖

1. **寬度 ≤100 欄**：operator 的檢視器不能左右捲。在 tmux 裡用 `tmux new-session -x 100 -y 40` 開臨時 session，再用 `tmux capture-pane -p -e` 拍。pane 寬度不夠自動打開時，手動打開。
2. **一定要附保留顏色的 HTML 預覽**：`python3 scripts/ansi2html.py capture.ansi > capture.html`，跟純文字 `.txt` 一起用 `fleet upload` 上傳，回報裡附 blob sha。
3. **臨時 session 不能搶 fleet 席位**：用 `--strict-mcp-config --mcp-config '{"mcpServers":{}}'`。載入開發中的 plugin 用 `--plugin-dir <worktree>`。拍完把引擎產生的 `tsconfig.json` 刪掉。
4. **截圖裡不能有個人資料**：拍之前確認畫面上沒有 email、帳號、權杖。用量分頁的帳號列不要拍。
5. **兩種 session 都要拍**：一般 repo（例如 pm）和 autopilot session，因為 band 的疊放不同。

## 10. 無障礙：不能只靠顏色

1. 狀態的符號、顏色、字樣三者同時出現（DS §3.2，I21）。只有兩種精簡場合可以省略字樣：同狀態 chip 下的項目，以及 band 的數字徽章。兩者旁邊的符號都已經表達了狀態。
2. 圖例（`?` 分頁）列出每個符號和它的字樣（I15）。
3. `NO_COLOR`：statusline 會遵守（改成純文字，燈號改成文字，例如 `memory:active`）。mod 讀得到 `$.env.get('NO_COLOR')`，至少要確保去掉顏色後，符號和字樣仍然看得懂（這條本來就是規則 1 的結果）。
4. 要人處理的事，除了黃色，還要有粗體或 `▲` 其中之一。

## 11. 機械化檢查（已有／待補）

| 規則 | 檢查 | 狀態 |
|---|---|---|
| 一符號一意思、單格寬、禁 emoji | live `ui-lint` L3、I5、I20；`glyph-check.js` | 已有（live） |
| 只用主題鍵、粗體白名單 | live L1、L8、L9 | 已有（live） |
| 行寬 ≤ 欄寬 | live「widths 60/76/90」測試 | 已有（live）；forge-stats 待補 |
| 數字經格式化函式 | live I8 | 已有（live） |
| 文案長度、語系一致 | `gen-locales --check` | 已有（live）；forge-stats `strings.ts` 待補 |
| render 時不跑子行程 | live `process` allowlist（lint） | 已有（live）；statusline 待補 |
| 跨畫面共用一張符號表 | — | 待補：forge-stats／statusline 遷移後，加一條比對 live `GLYPHS` 的測試 |
