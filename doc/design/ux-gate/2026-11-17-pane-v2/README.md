# forge-stats pane v2 評審包

任務：`01M7KRP3A17982XHW7VPJTXSGD`。基線：`8c9aa22ffd899e64fba44bb3b56045fae0b71863`。本分支 `feat/fs-pane-v2`。

## 1. 任務與驗收

將預設 pane 改為總覽與寵物兩頁。驗收：brief 嚴格解析與去重；卡住優先、無來源池固定一列；不重複數字或編造建議；底部僅有效按鍵；高度依內容且不補空白；正體中文；S1–S6；兩個指定驗證全綠。

## 2. 定位與原始行

- `register.tsx:264` 原為 `type PaneTab = 'pet' | 'usage' | 'fleet'`，265 行 `let paneTab: PaneTab = 'pet'`。
- `pane-view.ts:31` 原為 `pad: dock ? room - total : 0`。
- `register.tsx:801` 原為 `{Array.from({ length: win.pad }, (_, i) => <Text key={'pad' + i}> </Text>)}`。
- `register.tsx:807` 原為 `{win.overflow ? <Text key="spare"> </Text> : null}`。
- `register.tsx:337` 原為 `const r = await $.process.run(['id', '-u'])`。

精確的所有移除行與新增行以基線至本分支的 git diff 為準。

## 3–4. 逐項修改與驗證

每輪完整紀錄位於 `/tmp/forge-stats-pane-v2-evidence/`。

| 項目 | 紅測 | 修正後驗證 |
|---|---|---|
| brief 解析與去重 | `01-brief-red.log`：exit 1 | `01-brief-green.log`：exit 0，222 通過 |
| 總覽高度與排序 | `02-overview-red.log`：exit 1 | `02-overview-green.log`：exit 0，224 通過 |
| 預設頁、兩頁、無補白與捲動 | `03-integration-red.log`：exit 1 | `03-integration-green.log`：exit 0，179 通過 |
| S1–S6、共享操作列、像素透明列 | `04-shots-first.log`：exit 1（S6 空白列） | `04-shots-export.log`：exit 0，194 通過 |
| 指令回覆正體中文 | 既有英文回覆已定位 | `05-copy-locale.log`：exit 0，194 通過 |

數量下降來自移除舊頁籤與補白契約的畫面測試，解析與 band 斷言保留。逐項清單見 [測試遷移](test-migrations.md)。

## 5. 最終驗證

最終命令與 exit code 由 `verification.json` 記錄。候選版本已通過外掛測試與 check-all（exit 0）。
基線 check-all 是 exit 1：`NO_COLOR=1` 使 Rust 的 ANSI 輸出測試失敗；完整重跑以 `env -u NO_COLOR ./scripts/check-all.sh` 得到 exit 0，未改 Rust。

## 截圖

[160x40 深色](shots/S1-160x40-dark.txt)、[160x40 淺色](shots/S1-160x40-light.txt)、[120x30](shots/S2-120x30-dark.txt)、[80x24](shots/S3-80x24-dark.txt)、[少量資料](shots/S4-160x40-dark.txt)、[缺 brief](shots/S5-missing-160x40-dark.txt)、[舊 brief](shots/S5-stale-160x40-dark.txt)、[寵物](shots/S6-160x40-dark.txt)。全部 14 份與摘要雜湊見 [manifest](shots/manifest.json)。

重現：`UX_SHOTS_DIR=<輸出目錄> python3 plugins/forge-stats/tests/export-ux-shots.py`。不設目錄時只做斷言，不寫檔。
`ux-shots.test.tsx` 使用與 pane 相同的總覽規劃器及操作列；交辦只在 fixture 注入。S6 從實際掛載的 pane 樹解出 Raster 字元與語意色。
測試計算同一 subject 出現於幾列；同列的原始 next_step_zh 可以再提到自己，避免改寫來源原文。

## 實際宿主行為與限制

- [焦點探測](focus-api-probe.txt)：`ui.focus` 在本機測試宿主回報 `no implementation for ui.focus`；`ui.open` 的額外 `plugin` 參數未出現在送出的 payload。open 的 isPlaced 回覆是測試替身，不能證明真的聚焦其他 mod。因此正式 `d` 只顯示「輸入 /autopilot 開決策卡」。未在互動終端宣稱跨 mod 聚焦成功。
- [viewport 探測](viewport-probe.txt)：測試宿主傳 `viewport.rows=40`、`scroll.bodyRows=30`；樹可以只回內容。mod 沒有以空白填滿 30 列。真實 dock 外框是否縮矮仍須 PM 在互動終端檢視；此報告不把測試樹當成實體終端截圖。
- [測試 sandbox 能力](harness-capability-probe.txt)：沒有 process／Bun；node:fs 匯入被拒。因此測試發出 JSON receipt，外部匯出腳本依 UX_SHOTS_DIR 寫檔；mod 本身不啟動匯出行程。
- 原分支缺 UX gate 文件，從主工作目錄讀取並逐位元組補入 `doc/specs/forge-stats-ux-gate.md`；來源與補入檔雜湊一致。

## 未做與範圍外

沒有部署、合併或推送；PM 的 160x40 上線放行尚未要求。沒有改 band 畫法、Rust、autopilot repo 或使用者的 .claude；沒有讓 mod 啟動行程。operator-origin 正式來源尚未存在，正式交辦固定空陣列；速度來源未提供，因此不顯示 tok/s。未另外讀真實 fleet 資料。

逐檔變更原因見 `changed-files.md`。兩家 UX 評審的原始 JSON 與分數存於各輪檔案，最多三輪；本文不先宣稱評審已通過。

## 第一輪後的修正

Astra 第一輪 7/10，沒有 Critical；原始評審在 round-1-astra.json。

| 發現 | 處理 | 驗證 |
|---|---|---|
| M1：窄畫面會移除 u/a/i 綁定 | 依寬度切換完整／精簡鍵帽，保留每個有效動作；換頁清除 d 提示 | r2-navigation-red exit 1；r2-actions-verified 中此項通過；r2-navigation-green exit 0 |
| M2：無 need_you 的交辦縮為零筆 | 非空交辦至少保留一筆，逐一捲動可到全部交辦 | r2-navigation-red exit 1；r2-navigation-green exit 0，198 通過 |
| M3：主題只驗名稱 | 從已安裝宿主內建表取 light/dark 色盤，解析每段前景／背景；本文 ≥4.5、僅結構線 ≥2 的參考底色對比斷言；匯出時比對原宿主色盤 | r2-theme-red exit 1；r2-theme-green / r2-theme-export exit 0，199 通過 |
| m1：六列明細未說有省略 | 首列標「前 6/8」等實際模型數，仍最多六筆成本列 | r2-model-subset-red exit 1；r2-model-subset-green exit 0 |

色盤事實來源與解析器雜湊見 shots/host-theme-provenance.json；fixture 是宿主 2.1.326 的預設非 ANSI light/dark，黑／白是明示的參考畫布。使用者自訂主題、終端色彩夾制與實體對比仍未聲稱通過。
Carbon 初次工具讀檔遭 headless command 權限拒絕，其後大包 Read 達時限；這些未產生分數，不算已完成的評審。以 106388-byte 內嵌完整來源 diff、所有截圖（相同位元組以 SHA256 去重）重試，未提高工具權限。

## 閘門結果（2026-11-17）

| 輪次 | astra（gpt-6-astra） | carbon（gemini-4-carbon-high） |
|---|---|---|
| R1 | 6 | 6 |
| R2 | 7 | 8 |
| R3 | 7 | 9（過） |
| R4（PM 01M7M973，只重評 astra） | 8（過） | — |

- 原文：`round-N-{astra,carbon}.json`。
- astra 在 R3 唯一的 Critical：80x24 拿掉了「無來源」旗標。已在 R3 之後修正：放不下完整寫法時，改用短寫「無來源6」，並固定保留。測試是 `overview.test.tsx` R3-C1。
- 依評分表，R3 不過時由 PM 裁決。PM 01M7M973 裁定：astra 照同一份評分表完整重評第 4 輪；這一輪 astra 給 8 分、沒有 Critical，閘門通過。
