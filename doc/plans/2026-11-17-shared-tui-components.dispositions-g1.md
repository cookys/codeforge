# 第一代審查處置（plan@54db9f7 → g1）

自 `2026-11-17-shared-tui-components.md` §11 原文搬出（防膨脹上限）。

指紋為 g1b 輸出的前 8 碼；「AP」= autopilot 擁有者審查（plan@54db9f7）。全部採納，沒有不採納項目。

| 來源 | 發現 | 處置 |
|---|---|---|
| astra 4ed92112（R9） | 11-18 上線要求 tux_version 相同，但 VERSION 到 P4b 才有 | 採納：P3 只要求兩個 tag 與 sha16；相等門檻自首次 P5 之後的上線起算（§7、§8） |
| astra 4c586d87（R2） | 沒有 panel 契約案例與貼底位置測試 | 採納：`panel-cases.json` 兩側都跑、`layout-bottom-*` golden、`tests/no-top-tabrow.test.tsx`（§4.1、§4.2） |
| astra 57591e5d（R7） | F3、F6–F8 沒有工作項 | 採納：F3→tokens thresholds、F6 DROP（附理由）、F7→`contracts/panel.ts`、F8→`tiers.ts` 兩張表，並排進 P4a/P4b/P5（§1.1、§8） |
| astra 43c0dfeb（R8） | 第一次遷移沒有可用的回滾目標 | 採納：回滾 = `checkout 47a2cf2` + 前一個 binary 的 `.bak`（§7） |
| carbon 011d3f7c（R1） | 路徑不明確、缺 panel 契約、符號重複 | 採納：§2.1 逐項唯一路徑；glyph 只在 `glyphs.ts`，`tokens.json` 去掉 glyphs；`VENDOR.lock` 路徑固定，`tux-lock` 也抓多餘檔（§2） |
| carbon caa19319（R2） | 單行 `strip(fixture)` golden 抓不到順序、無 panel 案例 | 採納：golden 改呼叫 live `barStrip`（含 console 模式）；panel 案例兩側跑（§4.1） |
| carbon 76c96979（R3） | develop 上的 v1 pane 頂部分頁列在 11-18 仍可達 | 採納：新增 P1b，已存的 `layout: pane` 遷成 `band`；11-18 只有 band 版面；死碼 P6a 刪（§3.1、§8） |
| carbon 11101d3b（R5） | reader 只驗 2 格、emoji 例外外溢、L19 未排程 | 採納：reader 白名單＋負向控制、例外只給 `pet.icons` 且有獨立斷言、P2 做 `panel.ts` 搬家與反向檢查（§6） |
| carbon dae0d4b6（R6） | 沒寫哪些不檢查、fleet 只釘 sha、缺角色 | 採納：§5 列出機械／非機械兩份清單、fleet 加一對一測試、缺角色在審查前提出（§5、P7） |
| carbon a8ac9b60（R7） | F3/F6/F7/F8/F9 未對應、P5 缺具體遷移、tiers 混在一起 | 採納：§8 P5 逐項列 F1–F5、F8–F10；F8 分兩張表；F6 DROP（§1.1、§8） |
| carbon c5d59793（R8） | §2 與 §7 對版本一致的規則互相矛盾，缺 autopilot tag 與 hangar 路徑 | 採納：sync 不必同步上線、門檻只看 live tag 且自 P5 起算；autopilot tag `live-20261118-1`；hangar 路徑由 hangar 提供（§2.3、§7） |
| carbon 1f5975e7（R9） | P2 與 P4b 寫同檔卻無依賴；P6 雙擁有者 | 採納：P4b 依賴 P2 已合；P6 拆成 P6a／P6b 各一擁有者並對映 `usageRows` 遷移（§8） |
| AP R1 | 缺 panel 型別與 tiers；`pct`/`floorPercent` 兩份；PALETTE 與 roles 兩份；VERSION 鎖無機制；行號 | 採納：§2.1、F2（`pctInt`＋`pct`、`floorPercent` 改呼叫）、`palette.ts` 由 `roles` 產生、VERSION.manifest＋負向控制、行號改 `ui.ts:87-88` |
| AP R2 | golden 沒案例；tab 順序在 live；`tux-cross` 自打架；runner；兄弟不在 | 採納：§4.1–§4.3（`barStrip`、版號規則、runner 為 `mods/live/run-tests.sh` ＋ `claude plugin test`、release 硬失敗） |
| AP R3 | pet 內容貼底、後備位置、`petTabFits`、forge 與 pet 的關係 | 採納：§6（`flex-end`、`petTabFits`＋`tabrow-80`）、§3.3 最後一列、§3.5 共存 |
| AP R5 | 欄寬 < 80 後備、白名單擋資料、`verified` 格式、獨立斷言、`shell.ts` 的 L19 衝突、檔名清單 | 採納：§6 逐項 |
| AP R6 | `terms` 與 locales 兩份；門檻不在 tokens | 採納：locale 由 `terms` 產生並 `--check`；`thresholds` 進 tokens（§5） |
| AP R7 | F3/F5/F6/F8 無列；`cutProse` 無使用證據 | 採納：§8 列出；F6 DROP；`cutProse` 不當獨立共用項，只隨 `strip()` 當內部相依（FS 實際用的是 `fitCells` `side.ts:61` 與 `clipCells` `pane-view.ts:52`，行為不同，本計畫不替換，P5 若要換須另附等價證據） |
| AP R8 | tux_version 與 11-18 上線矛盾 | 採納：同 astra 4ed92112 |
| AP R9＋附帶 | P4b 依賴 P2；ui-lint 的 `except`／L3 import 要改指 tux | 採納：§8 P4b；§2.1 說明 `modSources()` 本來就掃全部，工作是改 `except` 與 import |
