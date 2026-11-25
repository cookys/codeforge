# 全機隊 transcript 知識管線（計畫第 4 稿，第 1 代評審已併入；plan loop ≤2 代）

> 操作員卡 d-8046739d（2026-11-23，答 A）願景：「從 transcript 提取知識，變成全機隊共用的技能、知識、SOP」。PM 單 01M836C68Y0EC65D15WWNKNMKJ。
> 狀態：第 4 稿（11-23 起草，PM 裁定先做不依賴 autopilot 的部分；plan loop 11-26，≤2 代，第二席非 Claude）。本文件不授權任何讀取別席 transcript；讀取範圍見第 7 節，授權由 PM 另開卡。
> 盤點依據：`doc/plans/2026-11-23-knowledge-inventory.md`。第 1 代評審補出盤點漏列的零件：codeforge 的 `session-digest`（全域 SessionEnd／PreCompact 已在跑）、`ingest-digests`、`absorb`；本稿第 3 節已補。

## 1. 目標與不做的事

**目標**：某個 lane 踩過的坑，第二天別的 lane 在做同類工作時自動避開，不需要人記得去查。

**不做**：不另建第二套 transcript 掃描器；raw transcript 不離開原主機；不自動把內容寫進共用 skill（一律經審查）；不讀別席 transcript（授權前）；不碰 mnemos；**不改 `codeforge ship` 的輸入與契約**（第 5 節）。

## 2. OKR / KRs

KR 與觀察期只量測、不擋上線；但 KR1 的反向探針同時是第 4 節段 3 的**上線閘門**（探針不過，段 3 不開寫入），兩者分開記。

- KR1: 去識別：反向探針用含雷假資料（路徑、帳號別名、主機名、組織、金額各 3 種）嘗試寫入共用倉，被擋下 15/15；共用倉對 100 筆樣本做本機逐字比對，原文逐字片段（≥20 字元）為 0 筆。
- KR2: 去重：同一教訓由兩個來源各記一次，共用倉只留 1 筆並累加證據（P1 以 5 組人工重複測試，5/5 合併、證據筆數正確）。
- KR3: 避坑示範：P2 示範劇本 1 個真坑：隔天別的 lane 開同類工作時，session 開頭出現該教訓，且該 session 沒有重踩（附召回記錄與 session id）。
- KR4: 退役：P3 切換後 14 天內，被退役的**教訓寫入路徑**（autopilot `learn` 直接寫各處 knowledge、`distill` 獨立產 skill、codeforge `learn` 的教訓類落點；第 5 節清單）寫入次數為 0；回滾指令演練 1 次成功。codeforge 的 L0→L1→L2 本機鏈（`session-digest`、`dream`、`ingest-digests`、`absorb`、`ship`）**不退役，不在 KR4 內**（PM 裁定 2026-11-23：L1／L2 歸 codeforge，教訓管線歸 autopilot）。
- KR5: 召回噪音：自 P2 之後的真實召回日誌，隨機分層抽至少 20 次非空召回事件；以「每則被注入的教訓」為分母，每則獨立判定是否與該 session 的 repo／任務直接相關，相關率 ≥ 70%；判定人不是原提煉席。

## 2.5 Global Constraints

- 不另建第二套 transcript 掃描器；舊零件並行對照後退役，不長期雙跑。
- raw transcript 與逐字內容不離開原主機，**不送任何雲端模型，包含本機 CLI 轉送的外部模型**；跨席只流通去識別、無原文的候選教訓。
- 授權範圍：現有卡 d-dd69ee58、d-60077fea、d-5b8ecf51 不涵蓋本計畫；未取得新授權卡前，只用各席自己的 transcript，結果只寫本機沙箱倉，不進共用倉。
- 不讀 mnemos repo；公開 repo 不寫真實主機名、組織、金額、帳號別名、本機絕對路徑。
- 自動產物不直接進共用 skill，一律經機器審查加人審；人審發生在獨立的審查入口（PM pane 待決卡），不在 SessionEnd hook 裡（背景 hook 無法互動）。

## 3. 現況：零件、重疊、缺口（摘自盤點，含第 1 代補漏）

| 零件 | 做到哪 | owner |
|---|---|---|
| autopilot `learn` | 手動記一筆教訓到 `.claude/knowledge/` | autopilot |
| autopilot `distill` | 手動掃本機 transcript 的頻率統計（不輸出原文），產出 SKILL.md，經人審 | autopilot |
| hippo records | 一筆一檔結構化記錄（lesson／decision／handoff），不讀 transcript；`hippo new --key` 命中既有鍵時不寫入、忽略新參數；provenance 為單值欄位（frozen v1） | hippo |
| codeforge `session-digest` | 全域 SessionEnd／PreCompact 已在每台主機掃本席 transcript，抽摩擦與修正訊號，寫 L0 | codeforge |
| codeforge `dream`／`ingest-digests`／`absorb` | L0→L1 編譯本專案記憶；`ship` 把 L1 與 git log 變 L2 日帳送 mnemos（production critical path） | codeforge |
| autopilot hooks | 沒有 hook 自動萃取教訓；`past-reference-recall` 只在使用者輸入抱怨詞時觸發，只讀固定一個 `.hippo/records`，不留召回日誌，最多 5 筆 | autopilot |

**重疊**：autopilot `learn` 與 codeforge `learn`（單筆教訓落兩處）；`distill` 與 `dream`（都把訊號壓成長期知識）；`session-digest` 與 autopilot 想新增的擷取 hook（同一種訊號）。
**缺口**：(a) 跨 lane／跨機器去重與升級；(b) 自動召回與採用回饋；(c) 統一的隱私與外洩審查（identifier-scan 不涵蓋帳號別名與裸主機名）；(d) 無 autopilot 的主機沒有萃取器。

## 4. 單一管線與單一 owner

**owner = autopilot**（機制：hook、腳本、檢查；d-8046739d 分工）。codeforge 管「操作員看得到的」：pane 上顯示近期教訓與採用狀態。

```
 段 1 擷取（每席本機）  段 2 提煉（每席本機）   段 3 彙整審查（共用）          段 4 發佈與召回
 沿用 codeforge     →  規則式、去識別、    →  去重、閘門、審查入口      →  skill→私有 fleet skill-pack repo
 session-digest        無原文的候選教訓       共用倉＝專用私密 git repo      教訓本體留共用倉（不寫各 repo knowledge）
                                                                           SessionStart 召回＋採用回饋
```

**第 1 代評審後定案的三項（原「未決」）**：
1. **共用倉**：一個專用私密 git repo（internal，不是公開的 autopilot `.claude/knowledge/`）。各席有本機 checkout；**唯一的寫入者是段 3 的寫入器**（人審通過的教訓、證據，以及段 4 本機回饋的批次匯入，見段 4「採用回饋」）；召回 hook 只 pull、只讀，從不寫共用倉。
2. **升級門檻 N = 3**：同一教訓在 ≥3 個不同 session、跨 ≥2 個 lane 被採用或各自重記，才升級成 skill 候選。lane 以去識別的 `lane_hash`（席位位址的 sha256 前 12 碼）計，寫在每一筆證據與回饋裡；共用倉不存原始位址，段 3 機器閘把原始位址樣式當禁詞擋。
3. **無 autopilot 的主機**：段 1 的萃取由 codeforge 側提供，不依賴 autopilot：在 codeforge 的 `session-digest` 之上新增一個唯讀子命令 `codeforge lessons candidates --json`，輸出本席 L0 中的候選訊號（只輸出模板欄位與指標，不輸出原文）。autopilot 的段 2 消費這個 JSON；沒有 autopilot 的主機只產出候選、不進段 3（本階段不納入共用倉寫入，只在 P2 試點的兩個 lane 上線）。

**各段規則**：
- **段 1**：只有一個擷取器：codeforge `session-digest`（已全域在跑，已有 `mask_secrets`）。**`distill-scan` 不再當擷取器**：它降為段 2 的離線提煉工具，輸入改為段 1 的候選 JSON，不再自己讀 transcript（解掉兩個掃描器重複讀 transcript 的問題）。補一個本機設定的**禁詞清單**（帳號別名、主機名、組織名，各主機自己維護，不進任何 repo）。**席位與 session 隔離（fail-closed）**：uid、家目錄與本機 session 清單只能證明「同一使用者」，在共用家目錄的主機上分不出同機別席，所以**不當隔離條件**。段 1 只處理 `seat` 欄位等於目前執行席位址、且 `model` 在本機模型白名單（只列 Claude 模型）內的 L0；`seat` 為空或不符的 L0 一律丟棄，不進候選。`seat` 欄位的來源是 fleet 單 01M83DFC1Q；它上線前，T1–T3 只跑含雷假資料與合成 fixture，不讀任何真實 L0。
- **段 2（規則式、無外送）**：以規則與模板組出候選，欄位是**受限詞彙**：主題（從固定分類表選）、坑類型（固定列舉：PATH／權限／相依／競態／格式…）、解法類型（固定列舉）、適用範圍（repo、工具名）、證據指標（session id 加時間範圍）。**不含自由文字欄位**，所以沒有逐字原文可外洩，段 3 的逐字比對閘只需對來源 L0 的「訊號文字」做比對，不會與「受限詞彙」互相卡死。`distill-scan` 的 atoms 要有測試證明不含 ≥20 字元的逐字片段（目前文件稱「不含」但沒有測試，P1 補測試後才可依賴）。LLM 路徑預設**關閉**；開啟需另一次授權，且只能是本機引擎、輸入只有受限詞彙欄位，不得經 `claude -p` 或其他雲端路徑。
- **段 3（閘門 + 審查）**：候選先進**本機候選區**（本機 spool 目錄，不在任何 git 追蹤內）。順序固定，**人審在 commit／push 之前**：(i) 機器閘（放在 codeforge 側的閘腳本，**不改 autopilot 的 identifier-scan**，因板級 ADR-0001 禁止 deny-list）：本機禁詞清單、帳號別名、裸主機名、原始席位位址、**所有本機絕對路徑**（含 /home、/Users、/tmp、/run、`~/` 與磁碟代號開頭）、組織與金額樣式，**命中的候選不進共用倉流程、留在候選區並標紅，由人決定改寫或丟棄**；(ii) 逐字比對閘：候選的每一個欄位與來源 L0 訊號文字做 ≥20 字元共同子串比對，命中同樣標紅、不得進共用倉。手動 `learn --share` 也只能填受限詞彙欄位（每個值必須在固定列舉內，機器檢查），沒有自由文字，所以沒有來源對話時 (ii) 由「值都在列舉內」這條代替，不會被跳過；(iii) **人審**：由審查入口（`autopilot` 新增 `knowledge-review`，PM pane 待決卡）一次審一批；未通過人審的候選永遠不 commit；人審時改寫過的候選**重跑 (i)(ii)**，通過才算審過；(iv) 人審通過後才寫入：先 `git pull --rebase`，以**正規化鍵＝主題＋坑類型＋解法類型＋適用工具**查共用倉去重（四個都是固定列舉；不同 repo、不同工具、不同坑類型的教訓不會被併掉；適用 repo 不入鍵，記在證據裡）。命中則不呼叫 `hippo new`，只新增一個證據檔 `evidence/<key>/<lane_hash>-<session_hash>.json`；未命中才新增教訓檔。**每筆證據一個新檔，不追加共用檔**，所以兩席併發寫不同證據不會互相衝突。commit 後 push；被拒或 rebase 衝突（兩席同時新建同一個鍵）時：`git rebase --abort`，把本機這個未推送的 commit 丟掉（候選仍在候選區，可重做，tier 0），重新 pull、重查鍵（這次會命中，只寫證據檔）再 push 一次；仍失敗就留在候選區下次再送。反向探針（KR1）驗證的是 (i)(ii) 兩道硬擋＋(iii) 未審不得進倉。
- **段 4（升級、發佈、召回）**：
  - 升級：達 N=3 才產生 skill 候選；生成的 SKILL.md 另過一道機器審查（格式、禁詞、洩漏掃描，含組織、金額、非 /home 絕對路徑）加人審，才進 **autopilot 既有的私有 fleet skill-pack repo**（PM 裁定 2026-11-23；是否公開另案，不在本計畫）。教訓本體留在私密共用倉。
  - 召回：新增 **SessionStart 召回 hook**，**同時取代** codeforge 全域 SessionStart 的兩個召回注入器（`memory context --hook`、`mnemos-cli context --hook`）中屬於教訓類的部分，避免同一 session 開頭注入 8 塊以上；合併後總注入上限 3 則教訓加原有 mnemos 區塊，不另外疊加。（不沿用 `past-reference-recall`：它只在抱怨詞觸發、只讀固定 `.hippo`、不留日誌。）讀本機共用倉 checkout；排程：每台機器一個 systemd user timer，每 30 分鐘 `git pull --rebase`，失敗時用上次快取並在召回日誌記「快取過期」。過濾：type=lesson、已審、與當前 repo／工具範圍相符；每次召回寫本機召回日誌（session id、教訓 id、時間）。
  - 採用回饋：**不用「沒有新候選」當正訊號**（廣範圍教訓會憑空得到採用分）。改成：召回後該 session 的候選或指令中**明確引用該教訓 id**（`learn --useful <id>` 或 `--noisy <id>`），才記採用或噪音；沒有標記的召回只計「曝光」，不計採用。回饋先寫本機回饋 spool（教訓 id、`lane_hash`、`session_hash`、標記），由段 3 的寫入器每天批次匯入共用倉 `feedback/<key>/<lane_hash>-<session_hash>.json`（每筆一檔，同 (iv) 的衝突處理）；召回 hook 本身不寫共用倉。
  - 召回資格與排序：**資格**＝已審、範圍相符、且噪音標記數不超過採用標記數＋1（被標噪音較多的教訓不再注入）。**排序**：3 個名額中 2 個給「近 30 天採用數」最高者，1 個保留給**審過不滿 14 天、曝光少於 5 次**的新教訓（冷啟動名額，沒有符合者時回給採用排序）；曝光多但無標記的教訓排序下降。

## 5. 舊零件怎麼退役（Migrate, don't run two systems）

**owner 邊界（PM 裁定 2026-11-23）**：L1／L2（`session-digest`、`dream`、`ingest-digests`、`absorb`、`ship`）歸 codeforge，**不退役、不改輸入、不改 mnemos 契約**；教訓管線（候選、共用倉、審查、召回、升級）歸 autopilot。新管線只「消費 L0 候選、產共用教訓」，兩者產物不同，不重疊，所以不算雙系統。真正退役的只有重複的**教訓寫入路徑**：

1. autopilot `learn`：保留人主動記一筆的入口。切換後預設寫本機私密筆記（照 knowledge-routing 的 memory_dir）；加 `--share` 才進本機候選區走段 3；不再直接寫各 repo 的 `.claude/knowledge/`。
2. `distill`：改成段 2 的提煉器之一；不再獨立產 skill，輸出走段 3 審查。
3. codeforge `learn`：**本機寫入完全不變**（它是 L0→L1→`ship` 的輸入，PM 裁定 A 不動），所以對 `ship` 輸入的影響為零，不需要 mnemos 契約 owner 確認。只新增 `--share`：在原本的本機寫入之外，**另外**產生一筆受限詞彙候選放進候選區。它被退役的只有「教訓直接寫到本機 store 以外的共用位置」這條（現況若沒有就是 0，KR4 照量）。
4. 順序：並行唯讀對照一週（新舊產物比對；舊路徑照常寫）→ PM 確認 → 切換教訓寫入路徑 → 舊教訓寫入路徑留 14 天唯讀 → 刪。每一步有回滾指令：新增設定鍵 `knowledge.learn_target`（`legacy`＝切換前的行為：直接寫各 repo `.claude/knowledge/`、`distill` 獨立產 skill；`pipeline`＝本節 1、2 的新行為；列入 config-keys.json 並過 check-config-docs），autopilot `learn` 與 `distill` **都**只在讀這個鍵的同一個函式裡決定寫入路徑，回滾＝改回 `legacy`。codeforge `learn` 不讀這個鍵（本機寫入不變，見 3）。

## 6. Risks + inversion

- **去識別失敗**：候選夾帶路徑、帳號別名、組織、金額。對策：段 3 兩道機器閘加人審；上線前反向探針 15/15 才開寫入。
- **逐字外洩**：提煉器或 LLM 帶出原文。對策：規則式、不外送、逐字比對閘；KR1 抽樣驗證。
- **洩漏到私有 skill-pack 以外**：升級後的 skill 只進私有 fleet skill-pack repo（另掃描加人審）；教訓留私密共用倉；不寫任何公開 repo。
- **契約風險**：`ship` 是 production critical path，本計畫不改它的輸入；若日後要改，另案走 mnemos 契約 owner。
- **召回噪音**：最多 3 則，只放已審且範圍相符；KR5 以每則為分母量測。
- **共用倉衝突**：多機同時 push 同一檔。對策：每筆教訓一檔、sidecar 證據為追加檔（append-only），push 前 `pull --rebase`，衝突時重試一次，仍失敗留候選區下次再送。

## 7. 隱私與授權（PM 拿這段去開授權卡）

- **現有授權不夠**：d-dd69ee58 只允許「抽操作員決定」；d-60077fea／d-5b8ecf51 只允許 SWE-v2 出題。都不涵蓋本計畫。
- **需要的讀取範圍**（建議，由 PM 開卡讓操作員決定）：
  1. 只限 Claude 席的 transcript；**排除中國營運模型席與其雲端**。實作方式：段 1 只處理該機器上 `session-digest` 已產出的 L0（它只針對本機 Claude Code session），且段 2 以席位白名單（本機設定，列出允許的席位位址）過濾；不在白名單的席位 L0 不進候選。共用家目錄下的別席檔案不讀。
  2. 每席只在自己主機上處理；離開本機的只有通過段 3 閘門的候選教訓（無原文、無路徑、無帳號別名、無主機名）。
  3. 跨席彙整只看候選教訓，不看 transcript。
  4. 不讀 `~/.claude` 的 settings、hooks、plugins、credential 檔。
- **在授權之前（P1）**：只讀本席自己的 session；產物只寫**本機沙箱倉**（與共用倉分開、不 push），不寫共用倉。授權後才開共用倉寫入。
- 提煉若要用 LLM：只用本機引擎、prompt 內只放統計 atoms 與模板欄位；授權卡要明講。

## 8. 驗收（要實際示範，不是測試綠）

**示範劇本**：選一個 lane 近期踩過的真坑（例：「claude 不在 PATH 導致 dispatch precondition_failed」）。(1) 該坑經管線變成已審的共用教訓；(2) 隔天另一個 lane 開同類工作時，SessionStart 召回注入這則教訓；(3) 該 lane 沒踩到同一個坑，附召回日誌（session id、教訓 id）與沒有出錯的證據。

**不算驗收**：單元測試綠、管線跑通但沒有真實 lane 被避開。

## 9. 里程碑、上線日、判定日（進 task_evidence）

| 階段 | 內容 | 上線日 | 判定日 |
|---|---|---|---|
| P0 | plan loop 通過（≤2 代）；PM 開授權卡 | 11-25 | 11-26（卡有答覆） |
| P1（縮小，PM 裁定 2026-11-23） | 只做兩件事並附修好前後實測數字：(1) `distill-scan` 與 `dream compile` 收斂成摘要、不送逐字原文；(2) L0 補欄位（session id、時間範圍、席位位址、模型來源），digest 改成下游確認收到才刪。只動 codeforge 自己的資料路徑，不讀他席 transcript。第 4 稿等 P1 事實出來才寫，另跑 plan loop。 | 範圍確定後重估，回 PM | 重估後定 |
| P1.5 | 切換前對照：新舊教訓產物並行比對一週（舊路徑照寫） | T1–T3 上線後（要有新管線產物才能比） | +7 天 |
| P2 | 授權後開共用倉；兩個 lane 並行；SessionStart 召回與採用回饋；示範劇本。任務表見第 10 節 | 授權後 +3 工作日 | 示範日 +1 |
| P3 | 退役舊教訓寫入路徑（第 5 節順序，含 mnemos 契約 owner 確認） | P2 通過後 | 切換後 14 天 |

日期是估計，待 plan loop 與授權卡答覆後鎖定。

## 10. P1 事實與 P2 任務表（第 4 稿新增）

### 10.1 P1 已出的事實（sandbox canary 與 rollout，只動 codeforge 自己的資料路徑）
- **範圍**：codeforge develop `ac0d9a6` 合併：`src/dream/egress.rs`、`src/dream/compile.rs`、`src/memory/l0.rs`、`src/dream/ingest_digests.rs`、`.claude/scripts/session-digest.js`。已由 hangar 在 12 台主機驗證（rollout 回報在 fleet 單 01M836C68Y）。
- **逐字外送（R7）**：canary 中 dream 送給 LLM 的輸入與來源 L0 訊號文字的最長共同子串為 10 字元（門檻 20，修好前最長可到 200–300 字元）。
- **L0 欄位（R14／R8）**：canary 的 4 筆 L0 訊號都帶 `session_id`、`time_start`、`time_end`、`model`；`seat` 欄位為 0（環境變數未設，來源待 fleet 單 01M83DFC1Q）。段 1 的隔離以 `seat` 為準、fail-closed（第 4 節段 1），所以 01M83DFC1Q 上線前 T1–T3 只跑假資料，真實 L0 等它上線。
- **digest 殘留（R14）**：下游確認收到才刪；canary 成功後 digests 目錄殘留 0 筆。
- **沒做的事**：沒讀任何他席 transcript；P1 不需要授權卡。

### 10.2 P2 任務表（授權卡答覆之後才開工；之前只準備不上線）
每列各用一個 worktree、一個 fleet 單，能並行的並行。「授權」指 PM 開的卡 d-xxxxxxxx 答覆；未答覆前只做不讀他席 transcript 的列。

| # | 任務 | owner／實作 | 依賴 | 驗收（要實際示範） |
|---|---|---|---|---|
| T1 | `codeforge lessons candidates --json`：只讀 `seat`＝本席且 model 在白名單內的 L0，輸出受限詞彙欄位加證據指標，不輸出原文 | codeforge（便宜席實作，本席審） | P1（真實 L0：01M83DFC1Q） | 反向探針：把含帳號別名、≥20 字元逐字片段的假 L0 餵進去，輸出為空或標紅；輸出 schema 有測試 |
| T1b | `distill-scan` 改吃 T1 的 JSON、不再讀 transcript；補「atoms 不含 ≥20 字元逐字片段」的測試 | autopilot | T1 | 停讀 transcript 的測試（餵 transcript 路徑被拒）；≥20 字元測試先紅後綠 |
| T2 | 本機禁詞清單與機器閘腳本（codeforge 側，不改 identifier-scan） | codeforge | T1 | 反向探針 15/15（路徑、帳號別名、主機名、組織、金額各 3 種，路徑含 /home 與 /Users）在 live 上全擋 |
| T3 | 候選區（本機 spool，不進 git）與 `knowledge-review` 審查入口（PM pane 待決卡） | autopilot 機制；pane 畫面 codeforge | T1、T2 | 人審前不得 commit：未審候選嘗試 push 被擋的實測 |
| T4 | 共用倉建立與寫入器（pull --rebase、四欄正規化鍵去重、每筆證據一檔、衝突時丟本機 commit 重做一次）；回饋 spool 的每日批次匯入 | autopilot | T2 的 15/15 探針、T3、01M83DFC1Q（`seat` 欄位）、授權 | 兩個 lane 併發寫同一教訓只產生一筆教訓檔加兩個證據檔；KR1 的 100 筆樣本在各來源主機本機做逐字比對，命中 0 |
| T5 | SessionStart 召回 hook：取代 codeforge 兩個全域召回注入器中屬教訓的部分，總量上限 3 則；每 30 分鐘 timer pull | autopilot hook；codeforge 移除舊注入 | T4、mnemos 契約 owner 確認（見 R12，P2 就要問，不等 P3） | session 開頭注入不超過上限的實測，附召回日誌 |
| T6 | `learn --useful／--noisy <id>` 採用回饋：寫本機回饋 spool，由 T4 的寫入器匯入 | autopilot（機制）；pane 顯示採用狀態 codeforge | T5 | 無標記的召回只計曝光的測試與日誌 |
| T7 | 示範劇本（第 8 節）與 KR5 噪音抽樣 | 本席主持，兩個 lane 配合 | T1–T6、T1b | 召回日誌加沒踩坑證據；判定人不是提煉席 |

**起算與日期**：T1、T2 不依賴授權，plan loop 通過（11-26）當天開工；T1–T3 全在本機，01M83DFC1Q 上線前只跑假資料，上線後只讀 `seat`＝本席的 L0，不需要卡；只有 T4（寫入共用倉，候選教訓第一次離開本機）起要等授權卡答覆與 01M83DFC1Q；P2 上線日＝授權答覆日 +3 工作日（沿第 9 節），判定日＝示範日 +1。fleet 單號：T1–T2 開單時寫回第 9 節表。

### 10.2.1 授權的實際範圍（一句話）
需要授權的只有 T4 起的一件事：**各席把通過閘門與人審、無原文的候選教訓，推送到全隊共用的私密 git repo**（資料第一次離開產生它的主機）。T1–T3 各席只讀自己席的 session（沿用現有「讀自己席 session 檔」的例外），原始 transcript 不離開本機、不讀他席。現有 d-dd69ee58 不涵蓋：它是 PM 派出的 agent 唯讀全隊席、**只抽操作員決定**；本計畫沒有任何 agent 讀他席 transcript，也不是抽決定，而是把本席教訓的衍生物（受限詞彙欄位加證據指標）發佈到共用倉。排除中國營運模型席與其雲端的條件（第 7 節 1）照舊。

### 10.3 第 4 稿相對第 3 稿的變更
- P2 一行展成 7 列任務表，每列有依賴與驗收（不再是「授權後做完」）。
- T1、T2 與授權脫鉤，可先做，解掉「全部卡在授權卡」的空等。
- mnemos 契約 owner 確認從 P3 提前到 T5 之前（R12）。
- `seat` 欄位缺來源明講並不擋開工。

### 10.4 第 4 稿第 1 代評審併入（11-26，carbon＋codex）
- 席位隔離改 fail-closed：以 `seat` 欄位為準，uid／家目錄不當隔離條件；01M83DFC1Q 前只跑假資料，T4 依賴它。
- 機器閘擋所有本機絕對路徑（含 /home、/Users）；人審改寫後重跑閘；`learn --share` 只收列舉值。
- 去重鍵改四欄；每筆證據、回饋各一檔；衝突時丟本機未推送 commit 重做。
- codeforge `learn` 本機寫入不變（`ship` 輸入不變）；回饋改本機 spool 由段 3 寫入器匯入，召回端唯讀；`lane_hash` 讓 N=3 的跨 lane 可判定。
- 召回加資格條件與冷啟動名額；設定鍵改 `legacy`／`pipeline`，涵蓋 `learn` 與 `distill`；新增 T1b（distill-scan 改接 T1）；P1.5 移到 T1–T3 之後；架構圖改私有 skill-pack。
