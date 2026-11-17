# 第二輪修正回應與證據

任務：`01M7KRP3A17982XHW7VPJTXSGD`。分支：`feat/fs-pane-v2-r2`。修改前基準：`cf3f9a73909e3358c4dee6dd74134e573028f883`。本文件依指定流程記錄；不啟動 UX 評審。

## 1. 任務重述與驗收

修正 R2-C1、R2-M1、R2-M2、R2-K1、R2-K2。S3 首屏同時顯示 smith1、smith2、codexseat、xaiseat、kimi 與完整「週五 17:00 前只派審查」。無來源與缺權杖呈現包含關係；交辦溢位數與按鍵提示分色；摘要過舊在待決列顯示 warning 尾綴。每項先紅測後修正，逐項執行完整外掛測試。最終重跑三個指定命令，匯出十四份畫面，提交未被 gitignore 排除的檔案。

保留：每區至少一列，餘列依卡住、各池、交辦分配；80x24 上限十四列；只依 ID 去重；各池不用 error；時長不截斷。畫面文字使用正體中文，專有名詞保留。

## 2. 定位與修改前原文

以下行號與原文均可用 `git show cf3f9a7:plugins/forge-stats/hooks/overview-view.ts` 重現。

| 發現 | 行號 | 精確原文 |
|---|---|---|
| R2-C1 | 202 | ``if (tight) add(`另 ${tight} 池吃緊`, 'warning')`` |
| R2-M1 | 78 | ``const noteW = Math.max(noteReserve, a.note ? Math.min(displayWidth(oneLine(a.note)), cols < 110 ? 18 : 32) : 0)`` |
| R2-M2 | 204 | ``if (noSource.length) add(`無來源 ×${noSource.length}`)`` |
| R2-M2 | 207 | ``if (tokens && (slots.pools > 1 || !pools.length)) add(`缺權杖 ×${tokens}`, 'warning')`` |
| R2-K1 | 219 | ``const tail = index === aw.visible - 1 && aw.total > aw.visible ? ` +${aw.total - aw.visible} 件（a）` : ''`` |
| R2-K1 | 220 | ``const state = (a.state === 'need_you' ? '▲ 等操作員' : a.state === 'waiting' ? '等回報' : '進行中') + ' · ' + elapsed(a.sinceMs, now) + tail`` |
| R2-K2 | 161 | ``const budget = cols - 9 - displayWidth(lead + tail) - 3`` |
| R2-K2 | 162 | ``head.push(row('decisions', '待決', [s(lead + (oldest ? ' ' + cut(`${oldest.subject.display}：${oldest.title}`, budget) + tail : count > 0 ? ' · 決策明細尚未提供' : ' · 目前沒有待決'), count > 0 ? 'warning' : 'inactive')], cols, count > 0 ? 'warning' : 'inactive'))`` |

## 3–4. 逐項紅測、修正與驗證

各次執行命令均為 `claude plugin test plugins/forge-stats`。紅測必須是字串或語意色斷言失敗；每項綠測完成後才開始下一項。原始輸出只暫存於 `/tmp/forge-stats-r2-evidence/`，不提交日誌；本文件記錄 exit code、斷言及原始輸出 SHA256。

### R2-C1：吃緊池必須可辨識

紅測 `R2-C1: compact tight pools take priority over summaries at every terminal width` 對 78／118／158 欄、24 列強制只有兩列各池，斷言完整 `smith2 82% · codexseat 54% · xaiseat 39% · kimi 18%`。S1–S3 淺深主題也逐一斷言名稱與百分比。紅測 exit 1（210 pass、3 fail），收到舊字串 `另 4 池吃緊`；輸出 SHA256：`2273e9a2f65da2403d757ce58ead5c8b20c75caf43d416ab08207a7997a06fd3`。

修正：先加入其餘吃緊池名稱與百分比，再用剩餘寬度放完整摘要，預留（u）位置。摘要放不下就整段略去，不讓摘要擠掉吃緊池。

需求取捨：S3 寬度不足以同時放四個吃緊池與無來源／用不完摘要，依 R2-C1 明定優先保留名字，省去摘要；S1、S2 保留有空間的摘要。既有「每種寬度皆有無來源摘要」斷言改成明確斷言窄版無摘要、寬版恰好一列，並新增更嚴格的四池文字斷言。

綠測 exit 0（213 pass、0 fail）；輸出 SHA256：`7e961a04f6230625eecf49178ab81b5452261a884e609f9e3587451817c59a5c`。

### R2-M1：完整備註優先

紅測 `R2-M1: complete notes outrank bars and reset descriptions without an eighteen-cell cap` 與 S1–S3 淺深主題，要求完整 `週五 17:00 前只派審查`；另測更長的 `週五 17:00 前只派審查，保留人工核對` 與窄版移除 bar。紅測 exit 1（211 pass、3 fail），實得 `週五 17:00 前只派…`；SHA256：`d1ad3eda6435dbd2d98c4c992b70de14443985fcc8689461f8f031588d6ed0a1`。

修正：移除備註 18／32 格固定上限，共用備註預留寬度依完整字串計算；窄版空間不足先移除 bar，再縮重置文字及其他敘述。極長備註超過整列時仍依顯示寬度加省略號。欄位對齊測試在無 bar 時改比百分比起點，保留全池對齊要求。

綠測 exit 0（214 pass、0 fail）；SHA256：`eafeadae5ce2ee7d55c576e7db1837f8b70b959a827dc470dc4973af4f9ce390`。

### R2-M2：無來源包含缺權杖

紅測 `R2-M2: missing tokens are a subset of no-source pools at every width` 在三種寬度斷言 `無來源 ×6（缺權杖 2）`，並排除 `缺權杖 ×2`。S1／S2 淺深畫面也斷言完整新字串；滿量 S3 依 R2-C1 省去摘要，另以 78x24、六個無來源但較少吃緊池的 fixture 驗證相同格式。紅測 exit 1（210 pass、5 fail）；SHA256：`411953d7e212e1a841284e30c63194a2173987bcac7d674e47091b7de58453d6`。

修正：缺權杖併進無來源括號，整組一起保留或省去，避免留下不完整的包含關係。單列池 fallback 預留主池、百分比及八格備註；空間不足先省用不完摘要。此例不屬必交 S3（80x24 有兩列池），原本單列測試改為要求完整包含關係、主池與八格備註，並明確斷言省去用不完。

綠測 exit 0（215 pass、0 fail）；SHA256：`418c4d014f0e94a73097ccdc5c0ae4140b3ed83023677ffce5349b55bdfadf3d`。

### R2-K1：交辦尾綴獨立分色

紅測 `R2-K1: assignment overflow count and action have their own tones` 在三種寬度、有／無 buttons 檢查三個精確字串與色彩：`▲ 等操作員 · 2d0h` warning、` · +2 件` inactive、`（a）` actionTone。S2／S3 淺深畫面同步驗證。初次紅測以找不到片段的 TypeError 失敗（exit 1），隨即將斷言改為 optional chaining，重新取得確實的 AssertionError 紅測：exit 1（211 pass、5 fail）；SHA256：`f7bcec96f9807b329bfa9fe3b683f18669edfb15e950051568b386fce2fc8b22`。初次輸出 SHA256：`4161435585963457f3da055ce61f7a3a91cdab08806f16c477a107f649f79117`，不作合格紅測證據。

修正：state 只含本筆狀態與完整時長，件數及按鍵拆為獨立片段，先保留尾綴寬度再分配標題欄。actionTone 沿用各池／卡住：有 buttons 為 merged，無 buttons 為 subtle。

綠測 exit 0（216 pass、0 fail）；SHA256：`df0c441986167a7cf51b6e24e3ab771172dd52b9fd26dc470bbd750623ba6cc7`。

### R2-K2：待決列提示摘要過舊

紅測 `R2-K2: stale brief warns at the end of the decisions row without adding rows` 在三種寬度斷言列末精確片段 `{ text: '（摘要過舊 6m）', color: 'warning' }`，同時保留待決主體、完整 `1h6m` 時長及十四列上限，並覆蓋零待決與新鮮資料。S5-stale 淺深畫面改為直接檢查待決列末，不再只檢查畫面任一位置。紅測 exit 1（214 pass、3 fail）；SHA256：`915927a412630f7521f2c5cef72cac8e707cee9402b11f99e2e954627d1890aa`。

修正：待決列先預留過舊提示寬度，再截標題；尾端加 warning 片段，不新增列。零待決主體仍 inactive，過舊提示獨立 warning；底部分頁的來源年齡提示保留。

綠測 exit 0（217 pass、0 fail）；SHA256：`8cb4ab44591544957033b03203fc52f6a89d64f90884a925d804c16aa4a5eb14`。

### 指定全檢補修：上一輪失效證據引用

首次 `env -u NO_COLOR ./scripts/check-all.sh` exit 1，停止於 doc-drift 的 links 檢查；此前 fmt、clippy、Rust 測試（1304 + 8 pass）、外掛測試、CJK 檢查均過。原始輸出 SHA256：`33013e9d0dc6fa6d6e1f03010da1f0f0e827d7b975bd2e8151db27b0d8b279d6`。

定位：`round-1-responses.md:31` 的連結標籤 `[01 紅]` 與目標 `(round-1-evidence/01-c2-red.json)`，以及同檔另三十一處 `round-1-evidence/*` Markdown 連結，指向基底 `cf3f9a7` 已刪除的檔案。此失敗與本輪 pane 修正無關，但必須修正才能滿足指定全檢。

最小補修：只將這三十二處失效連結改成標籤加歷史路徑，並註明來源提交 `6f08a98` 與 `git show` 查閱方式。每個路徑先經 `git cat-file -e` 核對歷史物件存在；上一輪行為裁定、數字與結論不改，不恢復任何日誌或 receipt。補修驗證 `python3 scripts/check-doc-drift.py` exit 0（links、fences、version-sync、cli-surface、roadmap-consistency 全過）；SHA256：`dfe6be0718f9a2234cc8558b40bf382c2e3515f15bedf6fc140b563cfd919849`。隨後重新執行全部三個指定命令。

第二次全檢 exit 1，因本文件逐字引用舊 Markdown 連結作範例，links 檢查連 inline code 也會掃描；已改為分開列出標籤與路徑，不修改檢查規則。該次輸出 SHA256：`c3366620f4997b1aa66a812975ec1ae012dd05b5b3fc72c497f6ed28365be479`。修正後再跑 `python3 scripts/check-doc-drift.py`，exit 0、五項全過，SHA256：`dfe6be0718f9a2234cc8558b40bf382c2e3515f15bedf6fc140b563cfd919849`。

### R2-M1 補測：長備註也先於固定重置欄

定位補充：原始 `overview-view.ts` 的 `const fiveW = cols < 110 ? 0 : five !== '' || noteW > 0 ? 24 : 0` 仍固定占用 24 格，且寬版 bar 原本最低四格。新增 40／64／108 格備註的精確字串斷言，確認只要整列放得下，就不能因這些欄位而截斷。紅測 exit 1（216 pass、1 fail），收到被截短的「核對」字串；SHA256：`4d93f6784f4a2baca1066dbb3cd4d0ca0d13772505bf8fda9e7ef5f03953e7af`。

最小補修：計算主體、百分比與完整備註後，必要時讓寬版 bar 歸零，再整欄省去 5h 重置欄，避免保留半截重置敘述。綠測 exit 0（217 pass、0 fail）；SHA256：`905b3d0b893aff0c6a59da07c7847af1004bba46a08cf8e5f0e7f54cd5aae052`。

### R2-C1 補測：120／160 欄只剩一列池

定位補充：原始 `function compactPoolBody(a: FleetAccount, cells: number)` 固定 `const note = a.note ? cut(a.note, 8) : ''`，而第一項修正的 `slots.pools > 1` 條件仍讓單列寬版顯示分類數。以 118／158 欄、23 列強制只分一列池，新增完整主池備註及四個其餘吃緊池名字的斷言；既有 158 欄單列測試同步改成具體名字。紅測 exit 1（216 pass、2 fail）；SHA256：`bfa83028912ebb638ac8f67e1312c72e9780c3e29d46ec3d0228caabba130510`。

最小補修：單列寬版也優先放名字，預留完整主池備註；bar 不足就省去，其他摘要只用剩餘寬度。80x23 的原有單列 fallback 保留分類數與八格備註；必交 80x24 仍使用兩列池。綠測 exit 0（218 pass、0 fail）；SHA256：`8c6007374ec03fc6a0a6b2a28c3d14c371b4af2ff22f1a096655058b8989b1b6`。

## 5. 最終驗證、變更清單與保留範圍

評審 diff：`git diff cf3f9a73909e3358c4dee6dd74134e573028f883..feat/fs-pane-v2-r2`。確切提交 SHA 由交付訊息提供，提交訊息末尾包含指定 Fleet-Task。

未做／範圍外：未啟動 UX 評審（第三輪由派發方執行）；未部署、合併或推送；未改 band、Rust、`~/.claude`、資料 schema、去重或高度分配。未提交任何被 gitignore 排除的檔案，原始日誌留在 `/tmp`。實體終端的目視放行仍由操作員處理。小於必交尺寸的 80x23 單列 fallback 保留主池、八格備註與分類數；120／160 欄單列已覆蓋完整名字與主池備註，未增加高度。


補測前的全量驗證（開始時間為 UTC；補測後的最終結果另列下方）：

| 命令 | 開始時間 | exit code／結果 | 原始輸出 SHA256 |
|---|---|---|---|
| `claude plugin test plugins/forge-stats` | 2026-11-17T07:42:57.118612+00:00 | 0（217 pass、0 fail） | `3f10323d2d658a79a3132752466d4117d681c1bab410e31e267bbcb79491939f` |
| `UX_SHOTS_DIR=<worktree>/doc/design/ux-gate/2026-11-17-pane-v2/shots python3 plugins/forge-stats/tests/export-ux-shots.py` | 2026-11-17T07:43:06.447936+00:00 | 0（匯出 14 份；內部外掛測試 217 pass、0 fail） | `84c4aa38973938c04a9231fc40f2284912593c234a60d33d26498f6a0b7ad25e` |


歷史引用補修後亦執行一次外掛測試，exit 0（217 pass、0 fail），SHA256：`9b730e8b88a28f3aa3114708f475db29a3649cdfe55a48e79bdc203adbb22193`。


最終重跑結果（全部程式補修之後）：

| 命令 | 開始時間（UTC） | exit code／結果 | 原始輸出 SHA256 |
|---|---|---|---|
| `claude plugin test plugins/forge-stats` | 2026-11-17T07:51:45.175725+00:00 | 0（218 pass、0 fail） | `75bb3cb0fb1ef09513b098a84e53e4c7be955ebef84387d53da55c16a4905756` |
| `UX_SHOTS_DIR=<worktree>/doc/design/ux-gate/2026-11-17-pane-v2/shots python3 plugins/forge-stats/tests/export-ux-shots.py` | 2026-11-17T07:52:17.239020+00:00 | 0（14 份；內部外掛測試 218 pass、0 fail） | `9e8b1433da793f6fc934d8b47ce351d4322a061ca6b5f5dd04e96cc01e48442b` |
| `env -u NO_COLOR ./scripts/check-all.sh` | 2026-11-17T07:54:23.458540+00:00 | 0（所有閘門全綠，含 Rust 1.88 發行封裝冒煙測試） | `3a514d20dd74e6777c248658f818dc4294113a7cddee344aa451fccb50ad4795` |

畫面證據：[S1 160x40 深色](shots/S1-160x40-dark.txt)、[S1 淺色](shots/S1-160x40-light.txt)、[S3 80x24 深色](shots/S3-80x24-dark.txt)、[S3 淺色](shots/S3-80x24-light.txt)、[過舊摘要](shots/S5-stale-160x40-dark.txt)、[十四份截圖的雜湊與列數](shots/manifest.json)。S3 淺深皆為十四列、八筆卡住，五個吃緊池及完整備註可見。另以 Python 逐檔核對 manifest 雜湊、列數及上述 S3 精確文字，exit 0。S4、S6 與 host-theme-provenance 重產後內容未變，故不列入修改清單。

提交前交付核對：十六個變更檔皆未被 gitignore 排除，且全部列於下表；十四份截圖雜湊相符。未加入日誌或 `/tmp` 收據。

每個修改檔案與一行原因：

| 檔案 | 原因 |
|---|---|
| `doc/design/ux-gate/2026-11-17-pane-v2/round-1-responses.md` | 修正基底刪檔留下的三十二個失效連結，保留歷史查閱方式與上一輪裁定。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/round-2-responses.md` | 記錄逐項紅綠驗證、取捨、原始輸出雜湊與交付範圍。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S1-160x40-dark.txt` | 重產滿量畫面與語意色，顯示包含關係的新摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S1-160x40-light.txt` | 重產滿量畫面與語意色，顯示包含關係的新摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S2-120x30-dark.txt` | 重產中等畫面與語意色，顯示新摘要及分色交辦尾綴。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S2-120x30-light.txt` | 重產中等畫面與語意色，顯示新摘要及分色交辦尾綴。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S3-80x24-dark.txt` | 重產窄版畫面與語意色，完整顯示五池、備註與分色交辦尾綴。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S3-80x24-light.txt` | 重產窄版畫面與語意色，完整顯示五池、備註與分色交辦尾綴。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S5-missing-160x40-dark.txt` | 重產缺檔畫面與語意色，沿用新的包含關係摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S5-missing-160x40-light.txt` | 重產缺檔畫面與語意色，沿用新的包含關係摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S5-stale-160x40-dark.txt` | 重產過舊畫面與語意色，待決列加過舊提示並沿用新摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/S5-stale-160x40-light.txt` | 重產過舊畫面與語意色，待決列加過舊提示並沿用新摘要。 |
| `doc/design/ux-gate/2026-11-17-pane-v2/shots/manifest.json` | 更新重產截圖的 SHA256；列數不變。 |
| `plugins/forge-stats/hooks/overview-view.ts` | 修正緊湊池名、備註優先、無來源包含關係、交辦分色與頂部過舊提示。 |
| `plugins/forge-stats/tests/overview.test.tsx` | 新增五項發現的具體字串／色彩回歸測試，保留高度、對齊、時長與單列 fallback 驗證。 |
| `plugins/forge-stats/tests/ux-shots.test.tsx` | 要求實際 S1–S3 池名與完整備註，S2／S3 尾綴分色，以及 S5 頂部過舊提示。 |
