# Statusline Band — forge-stats 三欄 band + statusline 探測切換

> Size L · branch `feature/statusline-band` · started 2026-10-31

## Project Goal

> **Final goal**: forge-stats 在 prompt 正上方畫一條三欄 band（**tok/s｜像素 Ferris｜記憶/腦**），可用 `/forge-stats layout band|pane` 切換版面；`codeforge statusline` 偵測到同一 session 的 mod 心跳就縮成精簡版（不重複顯示寵物），沒有心跳就照舊印完整版。
>
> **Success criteria**（門檻 + 驗證方式）：
> 1. **三欄 band**：terminal surface 的 `AbovePrompt` 畫出三欄——速度（最近 tok/s、TTFT）、Ferris `Raster`（24×24 px = 24 欄×12 列，沿用待機動畫，卡 d-5d96fcb1 研究結論）、記憶/腦燈與寵物數值；在 autopilot band 改成疊加之前，這條 band 只在沒有 autopilot band 的 session 出現。驗證：`claude plugin test` mount `AbovePrompt` 找到 Raster 與速度、記憶文字；`bodyColumns` 不夠三欄時依序降級（測試釘住降級規則）。
> 2. **版面切換**：`/forge-stats layout band|pane`（預設 band），選擇存 `$.store`、重開保留；band 版面不自動開 pane，pane 版面不畫 band。驗證：mod 測試兩種版面各自只畫對應位置，store 讀回正確。
> 3. **心跳**：band 版面時 mod 定期寫 `<live_base>/codeforge/mod-alive/<session_id>.json`（`$.session.id()`）。驗證：mod 測試推進時鐘後 `fs.write` 收到正確路徑與內容。
> 4. **statusline 探測切換**：stdin `session_id` 有對應且新鮮（門檻在 plan 定）的心跳 → 精簡版（≤ 2 行、無寵物列與圖）；沒有心跳／過期／別的 session → 完整版。驗證：Rust 測試四種情況；無心跳時輸出與舊版 binary **逐 byte 相同**。
> 5. **記憶/腦資料**：mod 第三欄的資料由 codeforge 每次 statusline 寫進該 session 自己的 `codeforge/session/<sid>.json`（`codeforge.session-view/1`；plan review 改的：`pet.json` 是所有 session 共用，不放單一專案的燈號）。驗證：Rust 測試檔名與欄位；mod 讀不到或讀不懂時第三欄畫「—」不壞。
> 6. **不退化**：既有 Rust 測試與 mod 測試全綠；owner 在 Windows Terminal 與 WezTerm 目視 band 正常、statusline 有縮、關掉 claude 後 statusline 回到完整版。
>
> **Scope boundary**:
> - 包含：mod band 版面 + 版面切換 + 心跳；statusline 精簡版 + 探測；`pet.json` 加記憶/腦欄位；README/CHANGELOG/spec。
> - 不包含：mod 改全域載入（仍只在 codeforge repo；所以 statusline 只在 codeforge repo 的 session 會縮）；其他村落 sprite（B30）；statusline 本身改三欄（卡 d-feb71b14 的 B 方案，未選）。

## Decisions

| 日期 | 決策 | 來源 |
|------|------|------|
| 2026-10-31 | 即時的（tok/s、動畫）放 mod；慢變的留 statusline；mod 畫 prompt 正上方三欄 band，statusline 偵測心跳自動縮 | 卡 d-feb71b14 → owner「A 好，開工」 |
| 2026-10-31 | band 用 24×24（B）：研究顯示 32×32 是可辨識角色的最小尺寸、16×16 近 icon；A 的 16×12 太小 | owner 問「B 的大小是不是所有 pixel art 都會比較舒服」；divoom / sprite-ai / aseprite 社群 |
| 2026-10-31 | 探針實測：AbovePrompt 鏈裡 autopilot live band 在 repo skills mod 之上且不呼叫 next → forge-stats 的 band hook 永遠輪不到；要合併必須 autopilot 那邊組合下層 | U4 spike，`.codeforge/research/statusline-band/band-probe-*.json`（loaded、hint 有，above 無） |
| 2026-10-31 | 走 X1：autopilot band 改成疊在下層內容上面（autopilot 自己的任務，已用 fleet 請 `autopilot--claude` 排，msg `01M68HGMJQF52M9MX1B1BMQ50N`）；codeforge 做 24×24 三欄 band 在其下；表現層另開設計（BACKLOG B31） | 卡 d-c5f5c6fd →「先 X1，表現層另開設計」 |
| 2026-10-31 | 同 session 判定：statusline 用 stdin `session_id`，mod 用 `$.session.id()`（transcript 檔名） | 型別文件 `claude-code.d.ts` session.id；`src/cli/statusline.rs` `raw["session_id"]` |

## Scope completeness audit

| Surface | 受影響 | 說明 |
|---|---|---|
| Rust code | 是 | `src/cli/statusline.rs`（探測 + 精簡版）、`src/pet/view.rs`（記憶/腦欄位）、可能新 `src/live` 讀心跳 helper |
| mod code | 是 | `.claude/skills/forge-stats/`：band 版面、layout 指令、心跳、第三欄 |
| User-facing docs | 是 | README（Live context files：pet.json 新欄位、mod-alive 心跳、statusline 精簡版）、CHANGELOG |
| Specs | 是 | `doc/specs/codeforge-mod-ui.md` §7（band + 探測） |
| API / interface | 是 | `pet.json` 加選填欄位（schema 名不變）；新檔案契約 `mod-alive/<sid>.json`（codeforge 讀、mod 寫） |
| Config / settings | 否 | 不改 settings.json；版面選擇存 mod `$.store` |
| Tests | 是 | Rust（探測四情況 + 逐 byte 不變）、mod（band、layout、心跳、降級） |
| Dependent consumers | 是 | 全域 statusline 每個專案都跑：無心跳路徑必須與舊版逐 byte 相同 |
| Migrations / data | 否 | 無 DB 變動 |

## Requirements ledger（owner 原話）

| # | 原話 | 對應 |
|---|------|------|
| R1 | 「取代目前 codeforge statusline 的位置但切成三塊」 | 標準 1、4 |
| R2 | 「能夠切多個排列嗎」 | 標準 2 |
| R3 | 「需要全部合併還是探測成功就切 mod」→ 選探測切換 | 標準 3、4 |
| R4 | 「tok/s 比較及時好像放 mod 比較合適」 | 標準 1（速度欄在 mod） |

## Progress

| Phase | 內容 | 狀態 |
|-------|------|------|
| — | intent → plan → plan-review（G1+G2，receipt exit 0） | ✅ `52877c8` |
| P1 | statusline 讀心跳、精簡 2 行、session view；無心跳時 stdout 與舊 binary 逐 byte 相同（3 輸入 × live base 可寫/不可寫） | ✅ `15be75a` |
| P2 | forge-stats band、`layout band\|pane`、心跳；mod 23 測試、tsc 0、validate 通過；sid 向量 `.ts` 鏡像 + Rust 比對測試（改壞會紅） | ✅ `3aaab33` |
| P3 | 部署：`cargo install`（舊版備份 `~/.cargo/bin/codeforge.bak.1793437948`）；本 session 寫出 `session/<sid>.json`；全新 `claude -p` 寫出 `mod-alive/<sid>.json`（schema/欄位/live base 與 statusline 一致，`pet_shown:false`）。檔案權限 664（`$.fs.write` 無 mode；上層 `/run/user/1000` 為 700） | 部署 ✅；owner 2026-11-01 目視通過（autopilot 分頁列 + forge 展開區五欄） |
| review | code review G1（sol low）：1 條 Major（心跳時間戳 i64 溢位 panic）→ `e455473` checked_sub + 回歸測試；G2 SHIP-AS-IS 無新意見；receipt exit 0。qc-gate 全綠（883 Rust + 23 mod 測試） | ✅ |
| merge | `5fac514` 合併 main 並推送（QC-Verdict trailer） | ✅ |
| 版面協同 | 2026-10-31 owner 看過疊放截圖後，X1 的版面暫停（疊放 1+12 列太佔版面），兩邊協同設計（autopilot msg `01M68QD9QJMKNG387EMB3TKDCV`）；「讓下層畫」的機制照常審查。codeforge 回覆 `01M68QE6B4W0ERPECHRZN6K6CB`：Ferris 分三級（24×12／迷你 16×6／一列摘要）依 maxRows 決定；由 autopilot 透過 maxRows/bodyColumns 分配空間、持有摺疊鍵；傾向預設摺疊一列。另：`statusLine.refreshInterval=2` 已寫入全域設定（C，備份 `~/.claude/settings.json.bak.1793441222`）；分類改版待卡 d-a3748a8c | 進行中 |
| 分頁協定 | owner 新方向：band 一列＝autopilot indicator＋分頁（現況／待決／派工／審查／forge），點分頁展開內容。與 autopilot 約定（msg `01M68RKSF53GAGHYJ1Y7K1JDW6` / `01M68RM9721GX7Y0GP4Q1F5V8C`）：forge 分頁未選時每次 render 呼叫一次 next（maxRows=1、bodyColumns=標籤寬）；選了時同一次 render 先標籤（maxRows=1）再展開區（maxRows≥12），兩次都原樣傳 e、只改 maxRows/bodyColumns、requestId 不動；autopilot 用 `await next(e)` 的結果當 `Button key="forge"` 的 children → forge-stats 在 maxRows=1 時一律回單一 `Text`（只含字串與巢狀 Text；≥16 欄 `forge 93 tok/s`、10–15 `93 tok/s`、<10 `93t/s`），不用 emoji 呈現字元；展開區 ≥12 完整、6–11 迷你 16×6。只有展開且 Raster blit 成功才 `pet_shown`。owner 定案（msg `01M68S9Q1KQSPV2YDVZ8TXQS2G`）；codeforge 端已實作合併 `05a06af`（plan `doc/plans/2026-10-31-forge-tabs.md`，code review 2 輪 SHIP-AS-IS）並部署；autopilot 端 BL-0352 合併 develop `a327547de`（layout E1，msg `01M6935AFS8WDRFTN7EAXXSAJ4`：摺疊時只取 Text 字串用自家樣式重畫；展開區 maxRows＝總列數−2，<12 不展開，所以迷你級在分頁版不會出現；下層回 undefined/null/engine 時不顯示 forge 分頁；隔離 config 實測完整 Ferris、標籤 `forge — tok/s`） | 兩端 ✅；待 owner 目視 |
| 寵物移出 statusline＋依寬度排欄 | owner 2026-10-31：mod 在跑（心跳 `layout: band`）statusline 就縮成 2 行、只留慢資料（`6f7f8d4`）；拿掉 `$.ui.status` 的 tok/s（`4d21a90`）；展開區依寬度排 1～5 欄 Ferris｜寵物｜速度｜最近請求｜記憶與大腦，門檻 26/56/92/140/170（plan `doc/plans/2026-10-31-forge-columns.md`，review 無 Critical/Major，2 個 Minor 已修，合併 `f498e4c`） | ✅；待 owner 目視 |
| 剩餘 | owner 目視（標準 1、6；autopilot 已在 `a327547de`，owner session `/reload-plugins` 後生效）：autopilot band 改成疊在下層之上（X1，fleet msg `01M68HGMJQF52M9MX1B1BMQ50N`；2026-10-31 依卡 d-e3c60ef0 跟進 `01M68MR4KDK47CWT3NG1XM952X`；autopilot 已接，預計 2026-10-31 20:00 前合併 develop，給 maxRows 扣掉自己的列數）後，在 Windows Terminal／WezTerm 看 band 三欄、Ferris 動、statusline 縮成 2 行、關掉 claude 後回完整版 | 待 X1 |
