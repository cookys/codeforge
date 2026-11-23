# Rubric — 2026-11-23-knowledge-pipeline.md

> Source plan: doc/plans/2026-11-23-knowledge-pipeline.md

R1: 去識別：P1 原型用含雷假資料（夾帶路徑、帳號別名、主機名各 3 種）做反向探針，共用倉寫入前被擋下 9/9；共用倉對 100 筆樣本掃描，原文逐字片段（≥20 字元）為 0 筆。
R2: 去重：同一教訓由兩個來源各記一次，共用倉只留 1 筆並合併證據指標（P1 以 5 組人工重複測試，5/5 合併）。
R3: 避坑示範：P2 示範劇本 1 個真坑：隔天別的 lane 開同類工作時，session 開頭出現該教訓，且該 session 沒有重踩（附召回記錄與 session id）。
R4: 退役：P3 切換後 14 天內被退役的教訓寫入路徑（autopilot learn 直接寫各處 knowledge、distill 獨立產 skill、codeforge learn 的教訓類落點）寫入次數為 0；回滾指令演練 1 次成功。codeforge 的 L0→L1→L2 本機鏈不退役（PM 裁定 A，2026-11-23）。
R5: 召回噪音：注入的教訓每次最多 3 則；抽 20 次召回，人工判定相關的比例 ≥ 70%。
R6: 不另建第二套 transcript 掃描器；舊零件並行對照後退役，不長期雙跑。
R7: raw transcript 與逐字內容不離開原主機，不送任何雲端模型；跨席只流通去識別的候選教訓。
R8: 授權範圍：現有卡 d-dd69ee58、d-60077fea、d-5b8ecf51 不涵蓋本計畫；未取得新授權卡前，只用各席自己的 transcript 做原型。
R9: 不讀 mnemos repo；公開 repo 不寫真實主機名、組織、金額、帳號別名、本機絕對路徑。
R10: 自動產物不直接進共用 skill，一律經機器審查加人審。
R11: **去識別失敗**：候選教訓夾帶路徑或帳號別名。對策：codeforge 側機器閘（禁詞、別名、主機名、絕對路徑、組織金額；不改 identifier-scan）命中即留在候選區標紅，加人審在 commit／push 之前；上線前用含雷的假資料做反向探針（故意夾帶，確認被擋）。
R12: **owner 邊界**：本計畫把 owner 放 autopilot，codeforge 的 L0→L1 要退役，但 `ship` 是 production critical path（Mnemos 來源契約在 mnemos repo，不在此複製）。切換前需 mnemos 契約 owner 確認 L1 輸入換成共用倉不破契約；這點 codeforge 無法單方決定，要請 PM 轉。
R13: **召回噪音**：教訓太多會淹沒 session 開頭。對策：只注入與當前 repo／工具相符且已審、近期被採用的前 3 則。
R14: **未決（請 plan loop 挑戰）**：共用倉用 hippo lesson 還是 autopilot `.claude/knowledge/`？本稿選 hippo（git 版控、可重建索引、已有 CLI）；升級門檻 N 的值；段 1 在 codeforge 主機（無 autopilot hook 的主機）怎麼補。
