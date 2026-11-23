# 知識管線計畫：plan loop 紀錄（至 2026-11-23）

| 輪 | 評審 | 結果 |
|---|---|---|
| 初稿 g1 | carbon（google）＋grok（xai） | grok 402 額度用盡，兩次嘗試都失敗，transport_exhausted；carbon 提 13 條阻擋。第 2 稿已逐條吸收（commit e486cbb）。 |
| 第 2 稿 r2-g1 | carbon 單席新 session（grok 席無法使用，topology 內只剩 grok 一家 hetero 備選） | CONDITIONAL，depth_0_adjudication_required：11 條阻擋、2 條非阻擋，next_generation=2。 |

## r2-g1 阻擋意見的處置方向（depth-0 提案，待 PM 裁定設計取捨後再寫正式處置檔）

可直接折進計畫（accept-and-fold）：
- R8 席位隔離：P1 與授權後的過濾要靠機器可檢查的條件（transcript 路徑屬於本席 uid／session id），不靠「席位白名單」文字。
- R14 無 autopilot 主機：session-digest 的消費端要在 codeforge 側有一個不依賴 autopilot 的最小萃取入口，或明講這類主機本階段不納入。
- R1 規則式提煉與逐字比對閘的死結：逐字比對只對「候選欄位」做，坑的描述欄位用受限詞彙加指標，不放自由文字。
- R7：distill-scan atoms 的「不含原文」宣稱要改成可測條件並加測試；LLM 路徑預設關閉。
- R11：identifier-scan 的 ADR 禁止自動拒絕，改成「命中即擋下並進人審」，不是自動拒絕。
- R2：commit 前先 pull --rebase，去重鍵比對在 pull 後。
- R5：採用回饋不能用「沒有新候選」當正訊號；改成需明確標記，或以召回後該 session 有引用才算。
- R13：不同時留兩套 SessionStart 召回：新召回取代 codeforge 的兩個全域召回，或合併成單一注入點並總量上限。
- R6：distill-scan 與 session-digest 只留一個擷取器；dream 的去留見下。

需要 PM 裁定的設計取捨（R4、R12 與 R6 交疊）：
1. 是否退役 codeforge 的 L0→L1（`dream`／`ingest-digests`／`absorb`）。第 2 稿選「不動，因為 ship 是 production critical path」；評審 rubric（R4、R12）要求退役並明寫 owner 邊界。選項 A：不動 L0→L1，KR4 改寫成只量教訓寫入路徑，owner 邊界寫清楚「L1/L2 屬 codeforge，教訓管線屬 autopilot」（建議）；選項 B：退役 L0→L1，ship 改吃共用倉，要 mnemos 契約 owner 確認，風險高。
2. R9：升級後的 skill 進公開 autopilot plugin repo，或進 autopilot 現有的私有 fleet skill-pack repo。建議後者，公開與否另案。
3. hetero 評審只剩單家：grok 額度要補，或 PM 同意以 carbon 單席（google 家族）滿足跨家族規則收尾。

## 第 3 稿的評審（2026-11-23 稍晚）

| 輪 | 評審 | 結果 |
|---|---|---|
| r2-g2（第 2 代，上限） | carbon | CONDITIONAL，`generation_cap_requires_depth_0_adjudication`，13 條阻擋 |
| r3-g1（第二家，不同家族） | codex（openai） | CONDITIONAL（STOP），2 條阻擋（R4、R11）、1 條非阻擋（R13） |

PM 裁定（2026-11-23）：L0→L1 不動（A）；skill 放私有 fleet skill-pack repo；第二家改非 Claude 有額度的引擎；跑完收尾。已達兩代上限，依規則由 PM 裁決。

### 兩家共同指出、查有實據的問題（第 3 稿沒有解決）
1. **R11 順序**：段 3 先 commit／push，再人審。兩家都指出，必改：人審通過才 commit。另外 identifier-scan 的板級 ADR 禁止 deny-list，本機禁詞清單要放在 codeforge 側的閘，不改 identifier-scan。
2. **R7 事實錯誤**：`distill-scan` 會輸出最長 160 字元逐字原文；`dream compile` 會把 L0 內 200–300 字元逐字內容經 `claude -p` 送出。第 3 稿宣稱「不含原文」不成立；P1 第一步要先量測並修（或關掉）這兩條外送路徑，才談新管線。
3. **R14／R8 事實缺口**：現行 L0 沒有保存 session id 與時間範圍、沒有席位位址與模型來源；`dream ingest-digests` 在 SessionEnd 會當場刪 digest。段 1 的「機器可檢查隔離」與唯讀子命令在現況下做不到，要先改 `session-digest` 的輸出。
4. **R4 事實錯誤**：autopilot 沒有單一 learn 寫入腳本（`distill`、`project-record.js` 各自寫），codeforge `learn` 不讀設定鍵；`knowledge.learn_target` 單鍵回滾不成立。
5. **R5／R2／R13／R12**：共用倉唯讀 checkout 與寫 sidecar 衝突；併發新增同一教訓會產生兩筆重複；SessionStart 召回無法只取代「教訓類子集」；mnemos 契約確認排在 P3 但 P2 已動全域 hook。

### depth-0 判斷
這份計畫一次想同時做「新擷取、隔離、去識別、共用倉、召回、升級」，而現行零件有多處事實缺口（上列 2、3、4）。兩代評審後仍有 13 條阻擋，其中多數是「現況做不到」而不是文字問題。建議 PM 裁決：**縮小為 P1 一件事——先把現行外送與缺欄位修掉並量測（`distill-scan` 160 字元、`dream compile` 外送、L0 缺 session id／席位、digest 被刪）**，把修好後的事實當依據，再重寫「候選→審查→共用倉」的計畫，另走一次 plan loop。R4（rubric 與 PM 裁定 A 衝突）以 PM 裁定為準，rubric 該項改寫。
