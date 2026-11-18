# CodeForge backlog

格式依 autopilot `references/backlog-entry.md`；一列一件，細節放在 Pointer 指的地方。自動派工從 Status=open 且 Trigger-Type=ready 的列取下一件。

### pane 80x24 艦隊頁、額度頁被裁切卻沒有溢位提示（第二批 M1、M2）
- **Id**: B49
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 80x24 時艦隊頁只列 3 條線，產線摘要和「離線 2 台主機」整段被裁掉；額度頁只畫 2 池，其餘 10 池和「o: 展開各席」被裁掉，兩頁都沒有「↓ 還有 N 項」。
- **Impact**: 操作員在小終端機上會以為主機都在線、額度都沒事，也點不到展開各席。
- **Trigger**: ready；第 1 批 pin 之後即可寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 閘門 b1 第 3 輪 carbon M1、M2；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B49 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ
- **Context**: 實作在 autopilot；本 lane 寫處置與紅測要求、交 autopilot、拍 80x24 截圖送閘門

### pane 待決頁提示「z 收合」卻沒有 z 的 Button（第二批 M3）
- **Id**: B50
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 待決頁的「其他待決（z 收合）」標題要人按 z，但底部操作列沒有綁 z 的 Button。
- **Impact**: 提示列有焦點時按 z 會打進對話框；用滑鼠也找不到可點的收合。
- **Trigger**: ready；第 1 批 pin 之後即可交 autopilot
- **Trigger-Type**: ready
- **Effort**: XS
- **Source**: 閘門 b1 第 3 輪 carbon M3；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B50 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ
- **Context**: 加一條測試：畫面上提到的每個鍵都有對應的可點 Button

### pane 艦隊頁展開的正常線被印成「閒置」（第二批 M4）
- **Id**: B51
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 摘要寫 2 條運轉中，展開其餘產線時沒有異常紀錄的線卻印成「閒置」，和摘要數字矛盾。
- **Impact**: 操作員會以為整個艦隊沒有線在跑。
- **Trigger**: ready；第 1 批 pin 之後即可交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 閘門 b1 第 3 輪 carbon M4；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B51 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ
- **Context**: 跟 R3「兩頁共用同一個狀態判定」同一個函式；沒有明細時寫「正常（無異常明細）」

### band 的過期原因和修復指令被截斷（第二批 M5）
- **Id**: B52
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: band 中間還有約 38 格空白，左側的過期原因和修復指令卻被截成「請執行：autopilot status run…」，結尾的 --watch 被吃掉。
- **Impact**: 違反「過期原因不截斷」（PM 01M7N33J）；照畫面打指令會跑錯。
- **Trigger**: ready；第 1 批 pin 之後即可交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 閘門 b1 第 3 輪 carbon M5；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B52 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ
- **Context**: band 仍須剛好 2 列（A9 矩陣）

### pane 關著時 band 第 2 列的計數不完整（第二批 M6）
- **Id**: B53
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: pane 關著時「額度」沒附任何異常計數；「艦隊(✗6)」把黃色閒置線也算成 ✗；9、0 頁籤消失；[+] 和 [-] 同列沒有標籤。
- **Impact**: 不開 pane 時操作員會以為額度全部正常，艦隊的嚴重度也被誇大。
- **Trigger**: ready；第 1 批 pin 之後即可交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 閘門 b1 第 3 輪 carbon M6；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B53 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ
- **Context**: 計數分紅（✗）和黃（▲/?）兩種；band 上的 Button 仍不綁數字 hotkey（A5'）

### pane 第二批小項：焦點順序、無作用游標、折行縮排、過期寫法（N1–N4）
- **Id**: B54
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 現況頁預告的最舊卡在待決頁不是預設焦點（N1）；現況頁有沒作用的「▸」，兩頁排序相反（N2）；保留完整原因時孤字折到第 0 欄（N3）；過期列重複寫「資料已過期」且主機舊值被抹掉（N4）。
- **Impact**: 掃讀時對不上卡片、看錯焦點線、誤以為多一筆項目、規則前後不一。
- **Trigger**: ready；第 1 批 pin 之後，可以跟 B49–B53 一起送閘門
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 閘門 b1 第 3 輪 carbon N1–N4；PM 01M7P1HY 排第二批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B54 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7P72ATZJR6P0PSH2GRPD6AJ

### pane 第 1 批補 astra 評審（等 codex 重新登入）
- **Id**: B55
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 閘門 b1 第 3 輪 astra 的 runner 失敗（team 帳號 codex token 失效），第 1 批只有 carbon 的判定。
- **Impact**: 閘門要求兩家都 ≥8；第二批送審時要補 astra。
- **Trigger**: 操作員完成 team 帳號 codex 登入（PM 開卡）
- **Trigger-Type**: event
- **Effort**: S
- **Source**: PM 01M7P1HY：astra 補評等登入恢復後放到第二批
- **Pointer**: doc/backlog/pane-batch2.md

### pane 開著時終端機縮窄，數字熱鍵改打進提示列（第二批 B56）
- **Id**: B56
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: pane 在寬終端機（側邊）開著，終端機縮到 80 寬後 pane 改疊在提示列上方，之後按 1–9、0 一律打進提示列，分頁不切換；一開始就是 80 寬、或只縮高度（160x24）都正常。
- **Impact**: 操作員按數字想切頁，結果在對話框打出數字，可能直接送出錯字（PM 01M7PMAN：會打錯字的真 bug）。
- **Trigger**: ready；隨第 2 批一起交
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: codeforge keyprobe 11-18（Claude Code 2.1.327、已安裝的 autopilot 083cf2e9）；PM 01M7PMAN 列入第 2 批
- **Pointer**: doc/backlog/pane-batch2.md
- **Done-Check**: `grep -q "^B56 pass" doc/design/ux-gate/pane-batch2/verdicts.md`
- **Artifact**: run
- **Needs**: none
- **ms**: pane-batch2
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7PMANY4HGR5N2J6BNAWFD6J
- **Context**: 實作在 autopilot；可能是 host 換位置時沒重新登記 hotkey，要 autopilot 查清楚是自己的還是 host 的，host 的就做繞道並回報

### pane 寵物分頁正式上線（第 3 批）
- **Id**: B57
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 9 寵物分頁目前只畫佔位文字「寵物分頁還沒上線」。
- **Impact**: 操作員按 9 看不到寵物；寵物只能在這一頁畫（statusline 永不畫寵物）。
- **Trigger**: 第 2 批 pin 之後
- **Trigger-Type**: event
- **Effort**: M
- **Source**: PM 01M7PMAN：寵物頁排第 3 批，codeforge 主導、autopilot 實作
- **Pointer**: doc/specs/codeforge-mod-ui.md
- **Needs**: B56
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7PMANY4HGR5N2J6BNAWFD6J
- **Context**: 資料走 pet.json 契約；閘門照 forge-stats-ux-gate


### pane 現況頁 80x24 計數錯、120x30 漏掉本席花費項
- **Id**: B58
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 80x24 現況頁寫「資料過期 0 分」（160x40 同時有 3 池過期）、「另有 25 項（艦隊 0 · 額度浪費 ▲2）」（艦隊實為 產線 52 ⊘7，0+2 也湊不到 25）；120x30 的「另有 8 項」沒算進 160x40 第 32 列的「花費 ✗ seats --quota 沒有本席這一列」，該項在 120x30 整個看不到。
- **Impact**: 操作員在小終端機上讀到錯的數字，也看不到本席的花費錯誤。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### pane 資料過期年紀顯示成「- 分未更新」
- **Id**: B59
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 額度池沒有年紀時，現況頁和額度頁都畫「✗ 額度 … - 分未更新（>10 分）」或「✗ — 分未更新」，數字是破折號、單位還在。
- **Impact**: 讀起來像壞掉；看不出到底多久沒更新、是沒資料還是過期。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: XS
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### pane 名稱截到分不出：額度池名、艦隊卡住線的 peer
- **Id**: B60
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 額度頁 160x40 池名也截在約 11 字，「agyclaude」和「agyclaude(alias 2)」畫成一樣，別名的序號被截掉；艦隊頁卡住線的 peer 截成「Claude (主機名前幾…」「Claude (c…」，120x30 兩列分不出是哪台。
- **Impact**: 操作員沒辦法從畫面判斷是哪個池、哪台機器出事。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### pane 沒有專案的 session：派工、審查、殘留頁只剩錯誤和不對的修復指令
- **Id**: B61
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 不在 autopilot 專案層級的一般 session 開 pane，派工、審查、殘留三頁只畫「✗ 本席…資料不是最新：沒有專案 · 請執行：autopilot status runs --watch」和「資料不足，無法判定」；照做也不會有專案。標題列每頁都帶「✗ 槽位 無來源」，沒說要做什麼。
- **Impact**: 一般 session 是常態；三頁永遠是空的錯誤頁，修復指令誤導，紅色 ✗ 天天都在、失去警示作用。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### pane 花費頁沒有任何花費數字
- **Id**: B62
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 花費頁只畫 5h／7d 額度百分比和「✗ seats --quota 沒有本席這一列」，判定「資料不足，無法判定」；160x40 帳號列還露出英文原始欄位「primary_owner」。80x24 連本席錯誤列都被擠掉。
- **Impact**: 頁名是花費卻看不到花費，操作員不知道該看哪裡。（Major）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: S
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### pane 120x30 分頁列換成兩列、「0: 鍛」截字
- **Id**: B63
- **Status**: open
- **Since**: 2026-11-18
- **Origin**: self
- **Problem**: 120x30 dock 寬 70 欄，分頁列換成兩列（1–6／7–0），每種尺寸都把「鍛造」截成「鍛」；純文字截圖也看不出選中哪一頁，只能靠標題列。
- **Impact**: 多佔一列正文；標籤不一致。（Minor）
- **Trigger**: ready；寫處置交 autopilot
- **Trigger-Type**: ready
- **Effort**: XS
- **Source**: 真機走查 team 主機，autopilot 9f9dcf292（第 2 批上線版），三種尺寸 10 頁 30 張，PM 01M7QF7Z 第 (2) 項
- **Pointer**: none
- **Needs**: none
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7QF7Z95YNS694VNC6ERSDT8
- **Context**: 實作在 autopilot；截圖含真實主機名，只留在本機 scratchpad，不入庫

### 兩個 dream 同時跑時 signal cursor 競態
- **Id**: B1
- **Status**: open
- **Since**: 2026-04-16
- **Origin**: self
- **Problem**: 兩個 dream 同時執行時，signal cursor 可能在任一方寫入前被讀兩次，造成重複的 L1 條目。
- **Impact**: 偶爾出現重複的 L1 知識條目；目前單人使用風險低。
- **Trigger**: Phase 2 daemon 接手所有寫入之後；或多專案共用 CODEFORGE_DIR 時出現重複條目
- **Trigger-Type**: event
- **Effort**: S
- **Source**: 早期設計審查；2026-06-15 global SessionEnd 上線後重新評估
- **Pointer**: none
- **Context**: 範圍 dream compile 與 signal_cursors；已由 busy_timeout=5000 緩解；daemon 尚未接管 signal_cursors（2026-04-17 確認）

### FTS5 搜尋支援中日韓斷詞
- **Id**: B2
- **Status**: open
- **Since**: 2026-04-16
- **Origin**: self
- **Problem**: 全文搜尋的分詞器不會切中文，搜「重複」這類詞可能漏掉結果。
- **Impact**: 中文記憶搜尋有漏檢，條目變多後更明顯。
- **Trigger**: 使用者回報中文搜尋漏檢，或 FTS 語料超過 500 筆
- **Trigger-Type**: event
- **Effort**: M
- **Source**: 早期設計審查
- **Pointer**: none
- **Context**: 需 ICU 或 jieba 外部相依，要先 survey；範圍 memory_fts 與 memory/fts.rs

### 徽章系統沒有任何事件會授予
- **Id**: B3
- **Status**: open
- **Since**: 2026-04-16
- **Origin**: self
- **Problem**: 徽章表和授予函式都在，但沒有任何遊戲事件會去授予徽章，寵物頁只會列出空的清單。
- **Impact**: 徽章功能形同不存在，玩家看不到成就回饋。
- **Trigger**: 決定哪些事件（擊殺、區域精通、里程碑）授予徽章時
- **Trigger-Type**: owner
- **Effort**: M
- **Source**: Phase 2b 出貨時留下
- **Pointer**: doc/specs/codeforge-mud-engine.md
- **Context**: 範圍 pet/badges.rs 與 badges 表

### ingest 的 ChatGPT 匯出格式解析
- **Id**: B5
- **Status**: open
- **Since**: 2026-04-16
- **Origin**: self
- **Problem**: ingest 指令宣告支援 ChatGPT 匯出，但解析器可能處理不了所有格式變體（JSON 與 markdown）。
- **Impact**: 匯入 ChatGPT 對話時可能失敗或漏資料。
- **Trigger**: 有人要求 ChatGPT 遷移，或第二位使用者採用 CodeForge
- **Trigger-Type**: event
- **Effort**: M
- **Source**: 早期設計審查
- **Pointer**: none
- **Context**: 範圍 cli/ingest.rs

### 真實 production ship 對 live Mnemos 端到端驗證
- **Id**: B13
- **Status**: open
- **Since**: 2026-06-15
- **Origin**: self
- **Problem**: ship 的端到端只在隔離測試庫驗過；真實 session 結束後對正式 Mnemos 的整條鏈還沒跑過一次。
- **Impact**: 正式環境的 ledger 是否真的落地沒有證據，且失敗佇列會持續累積。
- **Trigger**: 下次 Mnemos server 在跑時，確認真實 session-end 的 ledger 進了正式腦
- **Trigger-Type**: event
- **Effort**: S
- **Source**: ship-online 專案上線後的收尾缺口
- **Pointer**: doc/specs/codeforge-ship.md
- **Context**: 驗完後用 ship --resend 清掉 ship-failed 佇列；失敗只排隊不阻塞，非 blocker

### improvement-queue 提示一般化成跨專案
- **Id**: B15
- **Status**: open
- **Since**: 2026-06-15
- **Origin**: self
- **Problem**: 其他專案寫進共用改善佇列的項目，沒有任何地方會在該專案顯示，因為顯示用的 hook 只裝在 codeforge 自己的 clone。
- **Impact**: 其他專案的待改善項目寫了卻沒人看到。
- **Trigger**: 想讓其他專案 session 看到自己的待改善項，或把該 hook 納入全域安裝鏈時
- **Trigger-Type**: event
- **Effort**: M
- **Source**: 2026-06-15 改善佇列歸屬修正後留下的增強項
- **Pointer**: doc/plans/2026-11-02-autopilot-hook-bridge.md
- **Context**: 2026-11-01 併入 B31（hooks 改由 autopilot 清單註冊），不單獨做；需讓專案根改由 hook 提供

### memory-recall 的 T2.1 非同步 worker 與 Phase C
- **Id**: B16
- **Status**: open
- **Since**: 2026-06-16
- **Origin**: self
- **Problem**: recall 第二階段只做了對帳與排序；非同步背景 worker 和第三階段（本機語意檢索、失敗記憶、矛盾偵測）都還沒做。
- **Impact**: SessionEnd 變慢時沒有現成解法；關鍵字檢索品質不足時沒有升級路。
- **Trigger**: dream/ship 在 SessionEnd 開始有感卡頓；或關鍵字 recall 品質證實不足
- **Trigger-Type**: event
- **Effort**: L
- **Source**: 2026-06-17 使用者決定延後（觸發條件未成立）
- **Pointer**: doc/plans/2026-06-17-memory-recall-phase-b.md
- **Context**: 語意檢索依分工歸 mnemos（B33），開做前先確認不重複；設計母本 doc/proposals 下的 memory-recall 提案

### session-digest 落地審查的低優先殘留
- **Id**: B17
- **Status**: open
- **Since**: 2026-06-17
- **Origin**: self
- **Problem**: digest 落地審查留下三個已接受的小問題：ingest 刪檔前有極小的時間競態、權限錯誤被靜默吞掉、改善佇列不是原子寫入。
- **Impact**: 極少數情況下 digest 被誤刪或漏記，且難以診斷。
- **Trigger**: 觀察到靜默漏記或 digest 競態丟失，或順手整理改善佇列寫入時
- **Trigger-Type**: event
- **Effort**: S
- **Source**: 2026-06-17 獨立審查
- **Pointer**: none
- **Context**: 範圍 session-digest.js 與 dream/ingest_digests.rs；2026-06-17 審查處置為接受現狀

### spec 寫了設計但程式還沒跟上（ship 與 MUD）
- **Id**: B18
- **Status**: open
- **Since**: 2026-06-18
- **Origin**: self
- **Problem**: 文件審查發現幾處設計尚未實作：ship 不掃 session 紀錄取證據、provenance 欄位不全、已 ship 仍先做 digest、寵物技能無戰鬥效果、缺少 Void 怪與傳說村莊。
- **Impact**: ledger 缺 session 證據，浪費重複的 LLM 呼叫，MUD 深度不足。
- **Trigger**: 各子項獨立：想要 ledger 帶證據、成本統計、省重複 digest、技能戰鬥或新怪物村莊時
- **Trigger-Type**: event
- **Effort**: L
- **Source**: 2026-06-18 doc-code-drift 審查
- **Pointer**: doc/specs/codeforge-ship.md
- **Context**: spec 頂部有未實作標記，補程式時一併清；auto-cite 已於 2026-07-02 完成，不在此列

### should_refresh 的 cache_stale 分支是死路徑
- **Id**: B21
- **Status**: open
- **Since**: 2026-06-23
- **Origin**: self
- **Problem**: 健康檢查的重新探測判斷中，快取過期那一支在現有常數下永遠被先判斷的 TTL 條件蓋住，不會被執行。
- **Impact**: 日後調高常數時語意可能出錯；目前無影響。
- **Trigger**: 調高 PROBE_TTL_MAX 超過 3600 秒，或決定清掉這道保險分支時
- **Trigger-Type**: event
- **Effort**: XS
- **Source**: 程式碼審查
- **Pointer**: none
- **Context**: 範圍 mnemos/health.rs 的 should_refresh；程式碼已加註解

### statusline 每次繪製逐檔解析 L1 概念數
- **Id**: B24
- **Status**: open
- **Since**: 2026-06-23
- **Origin**: self
- **Problem**: 計算有效概念數時每次繪製都逐檔解析前置資料，沒有快取；概念變多後每次繪製都變慢。
- **Impact**: 概念數成長到數百筆後 statusline 會有感變慢。
- **Trigger**: concepts 目錄超過 200 筆，或 statusline 繪製延遲有感變慢
- **Trigger-Type**: event
- **Effort**: S
- **Source**: 效能審查
- **Pointer**: none
- **Context**: 改法二選一：存進 DB 計數欄位由 dream 更新，或快取到 runtime 目錄由 dream 失效；範圍 memory/l1.rs

### 央腦燈即時反映能否真正 ship（authed 探測）
- **Id**: B25
- **Status**: open
- **Since**: 2026-06-24
- **Origin**: self
- **Problem**: 健康探測的 liveness 端點不驗 token，token 錯或缺時燈號仍是綠的，要等到 session 結束 ship 失敗才翻黃。
- **Impact**: 艦隊機器 token 設錯時，燈號整個 session 都是假綠。
- **Trigger**: Mnemos 正式環境開啟驗證，或艦隊開始多機收錄時
- **Trigger-Type**: event
- **Effort**: M
- **Source**: Mnemos 協作討論，2026-06-24
- **Pointer**: doc/specs/codeforge-mnemos-runtime.md
- **Context**: 背景探測改打 Mnemos 的 whoami 端點（已上線）餵進既有 readiness 軸，不做第三盞燈；與 Mnemos 的 fleet-ingest-rollout 對齊

### statusline 中段預留給其他狀態燈
- **Id**: B27
- **Status**: open
- **Since**: 2026-06-25
- **Origin**: self
- **Problem**: statusline 底邊中段留了空位給未來的即時狀態燈（跨 agent 連線、autopilot 模式），但目前沒有狀態來源也沒有語意。
- **Impact**: 沒有，純預留；做早了反而讓窄終端更擠。
- **Trigger**: 有 indicator 具備明確語意與狀態來源時；且須先決定是否由 autopilot band 取代
- **Trigger-Type**: event
- **Effort**: S
- **Source**: statusline 版號右對齊改動時預留
- **Pointer**: doc/specs/codeforge-mod-ui.md
- **Context**: 2026-11-01：autopilot 狀態已由它自己的 band 畫，隨 B31 決定留不留；擴充步驟寫在 statusline 程式註解內

### 小圖行數規格三處說法不一
- **Id**: B29
- **Status**: open
- **Since**: 2026-10-31
- **Origin**: self
- **Problem**: 繪圖指南說小圖最多 4 行，程式註解寫 6 行，實際五個村落都是 6 行、畫面放 5 行，新增村落時不知該畫幾行。
- **Impact**: 新村落圖案可能畫錯行數而被截掉。
- **Trigger**: 下次新增或重畫村落圖案，或動 statusline 的圖案欄
- **Trigger-Type**: event
- **Effort**: XS
- **Source**: 2026-10-31 pet-view-snapshot 收尾的文件掃描
- **Pointer**: none
- **Context**: 先量實際能放幾列，再統一指南與註解並在測試釘住上限；只記錄，未改行數規格

### 其他四個村落的像素 sprite
- **Id**: B30
- **Status**: open
- **Since**: 2026-10-31
- **Origin**: self
- **Problem**: 目前只有 Rust 村的 Ferris 有像素圖，其他四個村落的寵物在面板上退回字元圖。
- **Impact**: 非 Rust 的寵物沒有像素圖，視覺不一致。
- **Trigger**: 使用者領養非 Rust 村落的寵物，或要求其他村落也有像素圖
- **Trigger-Type**: event
- **Effort**: M
- **Source**: pixel-pet 專案（2026-10-31）刻意縮小範圍
- **Pointer**: doc/plans/2026-10-31-pixel-pet.md
- **Context**: 每村一組 24x24 兩幀，驗證函式會自動涵蓋；畫法要補進繪圖 skill

### codeforge 成為 autopilot 的 extension
- **Id**: B31
- **Status**: open
- **Since**: 2026-10-31
- **Origin**: owner
- **Problem**: 決定 codeforge 走 autopilot 的 extension 契約，hooks、分頁、狀態片段、設定、執行檔需求分段接上，使用者只裝 autopilot。
- **Impact**: 不做的話使用者要分別安裝兩套，hooks 仍寫進全域設定。
- **Trigger**: autopilot v3.0.0-alpha.7 重整發布後；開工前先有 extension 機制計畫並過審查
- **Trigger-Type**: event
- **Effort**: XL
- **Source**: 操作員決定 2026-11-01（卡 d-0e2e6e0d）
- **Pointer**: doc/plans/2026-11-02-autopilot-hook-bridge.md
- **Context**: Rust 本體不搬、須能單獨降級執行；hooks 段只有契約沒有程式；alpha.7 搬家期間不定掛載點

### 專案範圍知識的正本改放 pm
- **Id**: B33
- **Status**: open
- **Since**: 2026-11-01
- **Origin**: self
- **Problem**: 本機知識庫整個被 gitignore，多機開發彼此看不到；專案範圍的 lesson 計畫改由 pm CLI 寫入並跟著 repo 走。
- **Impact**: 不做的話專案知識無法跨機器共享。
- **Trigger**: 操作員對 pm 計畫拍板、fleet 通知 codeforge 寫遷移計畫；之後先開決策卡，不先動程式
- **Trigger-Type**: owner
- **Effort**: L
- **Source**: fleet 討論 2026-11-01，codeforge 已提出六項限制
- **Pointer**: doc/specs/codeforge-memory-contract.md
- **Context**: 需新增 atom 的 scope 欄位與來源 repo 欄位；遷移計畫與程式全部未開始

### 分身偵測：續行裡的 block comment 含結束符會提前結束
- **Id**: B37
- **Status**: open
- **Since**: 2026-11-02
- **Origin**: self
- **Problem**: 同一行同時有跨行續行與未閉合的區塊註解時，續行狀態蓋掉註解狀態；註解本文若含結束符，續行提前結束，可能生出假的分身怪。
- **Impact**: 只會多生假怪，不影響資料或 tick。
- **Trigger**: 下次動 dup_detect.rs 的行分類，或實際看到這種假分身怪
- **Trigger-Type**: event
- **Effort**: S
- **Source**: B10 hetero code review 第 3 輪延後項（finding d06d04b2）
- **Pointer**: none
- **Context**: 加續行內註解狀態並補兩檔負例測試；範圍 daemon/dup_detect.rs 的 classify

### G40 配額顯示的後續
- **Id**: B45
- **Status**: open
- **Since**: 2026-11-04
- **Origin**: self
- **Problem**: 配額顯示端已完成，但還等 autopilot 的契約檔與對照向量、真實 mirror 樣本與帳號卡，才能用真資料驗證。
- **Impact**: G40 里程碑（真資料顯示正確）無法驗收。
- **Trigger**: autopilot 宣布 P3 與 N37 落地，或拿到真實 mirror 樣本（卡 d-2a19fdb5），或下次動配額顯示
- **Trigger-Type**: event
- **Effort**: M
- **Source**: G40 顯示端完成時（develop，2026-11-04）
- **Pointer**: doc/specs/codeforge-usage.md
- **Context**: 拿到樣本後跑 scripts/g40-real-data/check.sh；另有 QUOTA_CONTRACT 同步測試與 fs.read 無 no-follow 選項兩項

### G38 本機 release 流程 review 的後續
- **Id**: B42
- **Status**: open
- **Since**: 2026-11-03
- **Origin**: self
- **Problem**: release 流程審查列為可延後的項目：自訂下載來源無提示、aarch64 strip 與 smoke 驗不到、清理函式未共用、輸出目錄整個被換掉、GitHub 來源辨識太嚴。
- **Impact**: 都是操作摩擦或驗證缺口，不擋出貨。
- **Trigger**: 下次改 install.sh 或 release 流程，或艦隊有了 aarch64 主機或 qemu
- **Trigger-Type**: event
- **Effort**: M
- **Source**: autopilot 四席審查 G38 第 1 至 3 輪
- **Pointer**: none
- **Context**: 範圍 install.sh、package.sh、release-local.sh；細節在審查 finding（3bd8512b、d5e5ae1f、b83770c7 等）

### G36 第 2 至 5 階段 review 的後續
- **Id**: B41
- **Status**: open
- **Since**: 2026-11-03
- **Origin**: self
- **Problem**: 用量收集器審查留下小項：Codex 基準修剪後重讀、非 unix 測試、快照時間超出範圍整份驗證失敗、中文表頭對不齊。
- **Impact**: 邊界情況下收集失敗或表格錯位，不影響主流程。
- **Trigger**: 下次動用量收集器，或非 unix 建置測試；快照驗證失敗時處理 8f1b90b2
- **Trigger-Type**: event
- **Effort**: M
- **Source**: autopilot 四席審查 G36 第 2、3 輪
- **Pointer**: doc/specs/codeforge-usage.md
- **Context**: 範圍 usage/collect.rs 與 store 測試；詳見 lane/g36rest 第 2、3 輪 finding

### 其他改行程環境變數的測試（B40 同類）
- **Id**: B46
- **Status**: open
- **Since**: 2026-11-05
- **Origin**: self
- **Problem**: 數個測試仍用 set_var 改行程環境，只有模組內的鎖，與其他讀同一變數的測試平行跑時仍可能互撞。
- **Impact**: 偶發的測試 flaky，要靠重跑才過。
- **Trigger**: flaky-tests.log 再出現落在這些模組的 FLAKY 紀錄，或下次動這些測試
- **Trigger-Type**: event
- **Effort**: S
- **Source**: B40 修完 live.rs 後盤點
- **Pointer**: none
- **Context**: 改成把環境當參數傳入；涉及 statusline、subagent_statusline、llm、mnemos/health、commentary/budget

### forge-stats 的 plugin validate 提醒
- **Id**: B47
- **Status**: open
- **Since**: 2026-11-05
- **Origin**: self
- **Problem**: plugin 驗證通過但有三個上架外觀提醒：缺 author、缺 icon、README 有非 https 連結被丟掉。
- **Impact**: 不影響載入；上架目錄時外觀不完整。
- **Trigger**: 下次動 forge-stats 的 manifest 或 README，或要上架 plugin 目錄
- **Trigger-Type**: event
- **Effort**: XS
- **Source**: 0.0.9 升版驗證；PM 說不擋任何事
- **Pointer**: none
- **Context**: icon 需 512 至 2048 px 方形 PNG

### MUD TUI 戰鬥紀錄改成聊天室式
- **Id**: B48
- **Status**: open
- **Since**: 2026-11-05
- **Origin**: self
- **Problem**: forge-stats pane 已改成操作區在底、內容往上長，但 MUD TUI 的戰鬥紀錄仍是最新在最上面，與準則相反。
- **Impact**: 兩個介面的閱讀方向不一致。
- **Trigger**: Goal 把 MUD 後段放回目前目標，下次動 MUD TUI 時
- **Trigger-Type**: event
- **Effort**: S
- **Source**: PM 決定 MUD TUI 這次不改（2026-11-05）
- **Pointer**: doc/specs/tui-guideline.md
- **Context**: 範圍 tui/panels/combat_log.rs 與 tui/layout.rs

### G36 第 1 階段 review 的後續（statusline 配額列）
- **Id**: B39
- **Status**: open
- **Since**: 2026-11-03
- **Origin**: self
- **Problem**: 配額列審查留下小項：權限檢查字面與計畫不一致、非 unix 不擋 symlink、黃色計算看資料不看畫面、提示被擠掉、窄 pane 寵物屬性列被截斷。
- **Impact**: 都是邊角的顯示或強健性問題，不擋使用。
- **Trigger**: 下次動 statusline 用量列或 forge-stats 摘要列、寵物分頁
- **Trigger-Type**: event
- **Effort**: M
- **Source**: G36 第 1 階段四席審查（g2 SHIP-AS-IS，f6baef59）
- **Pointer**: doc/specs/codeforge-usage.md
- **Context**: 範圍 usage/quota.rs 與 cli/statusline.rs；glyph 寬度與 parseFleet 空錯誤兩項為 G33 Suggestion
