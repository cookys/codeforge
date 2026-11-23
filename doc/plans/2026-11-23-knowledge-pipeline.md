# 全機隊 transcript 知識管線（計畫初稿，走 plan loop，先不做）

> 操作員卡 d-8046739d（2026-11-23，答 A）願景：「從 transcript 提取知識，變成全機隊共用的技能、知識、SOP」。PM 單 01M836C68Y0EC65D15WWNKNMKJ。
> 狀態：初稿，等 plan loop（hetero 審）。本文件不授權任何讀取別席 transcript；讀取範圍見第 7 節，授權由 PM 另開卡。
> 盤點依據：`doc/plans/2026-11-23-knowledge-inventory.md`（每個零件的輸入、輸出、觸發、owner、缺口、讀取範圍，附證據路徑）。

## 1. 目標與不做的事

**目標**：某個 lane 踩過的坑，第二天別的 lane 在做同類工作時自動避開，不需要人記得去查。

**不做**：不另建第二套 transcript 掃描器；不把 raw transcript 離開原主機；不自動把內容寫進共用 skill（一律經審查）；不讀別席 transcript（授權前）；不碰 mnemos（任何 lane 都不得讀）。

## 2. OKR / KRs

量得出來的關鍵結果（KR 與觀察期只量測，不擋上線）：

- KR1: 去識別：P1 原型用含雷假資料（夾帶路徑、帳號別名、主機名各 3 種）做反向探針，共用倉寫入前被擋下 9/9；共用倉對 100 筆樣本掃描，原文逐字片段（≥20 字元）為 0 筆。
- KR2: 去重：同一教訓由兩個來源各記一次，共用倉只留 1 筆並合併證據指標（P1 以 5 組人工重複測試，5/5 合併）。
- KR3: 避坑示範：P2 示範劇本 1 個真坑：隔天別的 lane 開同類工作時，session 開頭出現該教訓，且該 session 沒有重踩（附召回記錄與 session id）。
- KR4: 退役：P3 切換後 14 天內舊路徑（codeforge L0→L1 寫入、autopilot learn 直接寫各處 knowledge）寫入次數為 0；回滾指令演練 1 次成功。
- KR5: 召回噪音：注入的教訓每次最多 3 則；抽 20 次召回，人工判定相關的比例 ≥ 70%。

## 2.5 Global Constraints

- 不另建第二套 transcript 掃描器；舊零件並行對照後退役，不長期雙跑。
- raw transcript 與逐字內容不離開原主機，不送任何雲端模型；跨席只流通去識別的候選教訓。
- 授權範圍：現有卡 d-dd69ee58、d-60077fea、d-5b8ecf51 不涵蓋本計畫；未取得新授權卡前，只用各席自己的 transcript 做原型。
- 不讀 mnemos repo；公開 repo 不寫真實主機名、組織、金額、帳號別名、本機絕對路徑。
- 自動產物不直接進共用 skill，一律經機器審查加人審。

## 3 現況：零件、重疊、缺口（摘自盤點）

| 零件 | 做到哪 | owner |
|---|---|---|
| autopilot `learn` | 手動記一筆教訓到 `.claude/knowledge/`（依 knowledge-routing 解析路徑） | autopilot |
| autopilot `distill` | 手動掃本機 transcript 的**頻率統計**（不輸出原文），產出可執行 SKILL.md，經人審 | autopilot |
| hippo records | 結構化記錄存放（lesson／decision／handoff 一筆一檔），**不讀 transcript** | hippo |
| codeforge L0→L1 | 手動 `learn`、SessionEnd 的 `dream` 把當前專案 memory 與 digest 編成 L1；`ship` 把 L1 與 git log 變 L2 日帳送 mnemos | codeforge |
| autopilot hooks | 沒有任何 hook 自動萃取教訓（`session-handoff`、`past-reference-recall` 只讀當次 transcript 做交接與召回） | autopilot |

**重疊**：autopilot `learn` 與 codeforge `learn`（都記單筆教訓，落在兩個不同位置）；`distill` 與 `dream`+`ship`（都把零散訊號壓成長期知識，產物形式不同）；兩邊都在 SessionEnd 讀本席 transcript，用途不同。
**缺口**：(a) 跨 lane／跨機器去重與升級成共用 skill；(b) 自動萃取（全靠手動）；(c) `ship` 尚未讀 session jsonl；(d) 統一的隱私與外洩審查關卡（autopilot 有 identifier-scan 加人審，codeforge L0 只有 mask_secrets，L1／L2 無審查）。


## 4 單一管線與單一 owner

**建議**：一條管線四段，**owner = autopilot**（機制：hook、腳本、檢查；操作員 d-8046739d 分工：autopilot 管背後的機制，codeforge 管操作員看得到的）。codeforge 只留「操作員看得到」的部分（pane 上看到近期教訓與採用狀態），不再維護自己的萃取。

```
 段 1 擷取（每席本機）   段 2 提煉（每席本機）    段 3 彙整審查（共用）       段 4 發佈與採用
 SessionEnd hook   →   候選教訓（去識別、   →   去重、歸類、人審        →   skill→autopilot plugin
 只讀本席 transcript    不含原文，帶證據指標）    （identifier-scan+人審）     知識/SOP→各 repo knowledge 或 hippo lesson
                                                                              採用回饋→下次 session 召回
```

- 段 1：沿用 autopilot SessionEnd 位置（`session-handoff` 已在讀當次 transcript；新增一個擷取 hook，不是新增掃描器）。只讀本席、只抽「摩擦與修正」訊號（失敗後換做法、使用者糾正、重複指令），輸出候選，不存原文。
- 段 2：把 `distill-scan` 的統計 atoms 與 codeforge 的 `dream compile` 合併成一個提煉步驟（保留 `dream` 的 L1 結構，因為 hippo 與 ship 已吃它）；產物統一成「候選教訓」一種格式，欄位：主題、坑、解法、適用範圍（repo／工具）、證據指標（session id 加時間範圍，不含內容）、去識別旗標。
- 段 3：共用倉（建議 hippo 的 `lesson` 記錄；hippo 本來就是 git 版控、一筆一檔、可重建索引）。去重靠主題加解法的正規化鍵；審查＝identifier-scan 機器關加一次人審（沿用 distill 的 AskUserQuestion 關卡），沒過不進共用。
- 段 4：升級規則：同一教訓被 ≥2 個不同 lane 各自記到，或被採用 ≥N 次，才升級成 skill 候選；skill 放 autopilot plugin，知識與 SOP 放各 repo 的 `knowledge/` 或 hippo。召回沿用現有 `past-reference-recall` 的 UserPromptSubmit 位置，把相關教訓在 lane 開工時注入。


## 5 舊零件怎麼退役（Migrate, don't run two systems）

1. codeforge `learn`／`dream compile` 的 L0→L1 萃取：段 2 上線並與 L1 輸出對得起來之後，`dream` 改為只讀共用倉，原寫入路徑停用；`ship` 的 L1 輸入改吃共用倉的已審記錄。
2. autopilot `learn`：保留為「人主動記一筆」的入口，但寫入目標改成共用倉的候選區（不再直接寫各處 `.claude/knowledge/`）。
3. `distill`：改成段 2 的一個提煉器，手動觸發保留；不再獨立產 skill，改走段 3 審查。
4. 退役順序：先並行唯讀對照一週（新舊產物比對，不寫舊路徑）→ 切換 → 舊路徑留 14 天唯讀 → 刪。每一步有回滾指令。


## 6. Risks + inversion

- **去識別失敗**：候選教訓夾帶路徑或帳號別名。對策：identifier-scan 機器關加人審，不過不進共用；上線前用含雷的假資料做反向探針（故意夾帶，確認被擋）。
- **owner 邊界**：本計畫把 owner 放 autopilot，codeforge 的 L0→L1 要退役，但 `ship` 是 production critical path（Mnemos 來源契約在 mnemos repo，不在此複製）。切換前需 mnemos 契約 owner 確認 L1 輸入換成共用倉不破契約；這點 codeforge 無法單方決定，要請 PM 轉。
- **召回噪音**：教訓太多會淹沒 session 開頭。對策：只注入與當前 repo／工具相符且已審、近期被採用的前 3 則。
- **未決（請 plan loop 挑戰）**：共用倉用 hippo lesson 還是 autopilot `.claude/knowledge/`？本稿選 hippo（git 版控、可重建索引、已有 CLI）；升級門檻 N 的值；段 1 在 codeforge 主機（無 autopilot hook 的主機）怎麼補。

## 7 隱私與授權（PM 拿這段去開授權卡）

- **現有授權不夠**：d-dd69ee58 只允許「抽操作員決定」；d-60077fea／d-5b8ecf51 只允許 SWE-v2 出題。都不涵蓋本計畫。
- **需要的讀取範圍**（建議，由 PM 開卡讓操作員決定）：
  1. 只限 Claude 席的 transcript（不含中國營運的模型席與其雲端）。
  2. **每席只在自己主機上處理**；raw transcript 與逐字內容不離開原主機，不送任何雲端模型；離開本機的只有「去識別的候選教訓」（無原文、無路徑、無帳號別名、無主機名）。
  3. 跨席彙整只看候選教訓，不看 transcript。
  4. 不讀 `~/.claude` 的 settings、hooks、plugins、credential 檔。
- **在授權之前**：只能用各席**自己的** transcript 做原型（本席自讀自寫，無跨席存取）。
- 提煉若要用 LLM：只用本機或同一台主機上的引擎，prompt 內不放原文（只放統計 atoms 與去識別片段）；這點要在授權卡裡明講。


## 8 驗收（要實際示範，不是測試綠）

**示範劇本**：選一個 lane 近期踩過的真坑（例：本席踩過的「claude 不在 PATH 導致 dispatch precondition_failed」或「committed 但 diff 只有 1–2 行等於沒進度」）。(1) 該坑經管線變成已審的共用教訓；(2) 隔天另一個 lane 開同類工作時，session 開頭自動出現這則教訓；(3) 該 lane 沒踩到同一個坑，附該 session 的召回記錄與沒有出錯的證據。一行證據：召回記錄的 session id 與教訓 id。

**不算驗收**：單元測試綠、管線跑通但沒有真實 lane 被避開。


## 9 里程碑、上線日、判定日（進 task_evidence）

| 階段 | 內容 | 上線日 | 判定日 |
|---|---|---|---|
| P0 | plan loop 通過（≤2 輪）；PM 開授權卡 | 11-25 | 11-26（卡有答覆） |
| P1 | 本席自己 transcript 的原型：段 1+2+3 單機跑通，一筆教訓進共用倉 | 授權卡前即可做，11-28 | 11-29（原型產物審查） |
| P2 | 兩個 lane 並行（授權後）；召回注入；示範劇本 | 授權後 +3 工作日 | 示範日 +1 |
| P3 | 退役舊路徑（第 5 節順序） | P2 通過後 | 切換後 14 天 |

日期是初稿估計，待 plan loop 與授權卡答覆後鎖定。
