## 目標
codeforge 成員從 cookys 帳號搬到 team 帳號（hangar 計畫 §6，PM go 2026-11-06）；新 session 接手整合面板 O4 的 codeforge 這一側，和 autopilot 一起交五級真機截圖後合併。

## 現況
- 寫於 2026-11-06T05:15Z，舊地址是 cookys 帳號下的 codeforge 成員（session e6514bd9）。舊 session 交完這份檔後停住，不再做事；session 和檔案都不會刪。
- develop：0ae0c34（Merge docs/tui-guideline-overlay）＋本交接檔的合併。主 clone 停在 develop（規則見根目錄 CLAUDE.md）。
- **進行中：`feature/panel-side`**（已推上 origin，tip 03fba21），worktree `~/projects/codeforge-wt-panel2`。**不要先合**：PM 指示要等 autopilot 的 band 外殼做好，一起拍五級截圖再合。
  - aad858d：介面約定 `doc/specs/panel-band-contract.md`（autopilot 已確認定案）＋ tui-guideline §3.2 登記 `✦`。
  - 5598013：forge-stats 寫 `side`（`plugins/forge-stats/hooks/side.ts`、`tests/side.test.ts`、`quota-ui.test.tsx` 的整合斷言）。
  - 03fba21：codex review g1 的發現（模型名稱要用顯示寬度裁）已修，附中文名稱回歸測試，紅燈驗證過。
  - 驗證：forge-stats 171 pass、check-all 全綠。
- **review 狀態**：hetero code review g1 已 finalize（1 個 Major，verified 並修在 03fba21，open 1 條待 g2 關）。ledger 備份在 `~/.codeforge-backups/side-review-ledger/`（`ledger/`、`spec.md`、`g1-disp.json`）。**g2 還沒跑。**
- 已完成並合併（不用再碰）：額度快照只增不減（33b64e7）、statusline 單行（23a487f）＋版本放最右（cdbda5c）、tui-guideline §14（0ae0c34）、checkout guard（a8072b4）。
- 已交件給 PM：ux-v5、ux-v6、ux-v7 比較頁（`ui-preview/codeforge/ux-v{5,6,7}/`，不進 git）。

## 已決事項(不重議)
- O4 版面：O3 為底＋G3 進度列，選項數字留在題目下面，清單不編號 — 操作員 d-f0bcb1f2。
- 選級門檻寫在 `bodyColumns`：≥235/195/155/115 → 240/200/160/120，其他 80；`maxRows` < 12 退回現有版面 — contract §1.1，autopilot 審過。
- 分工：autopilot 畫整個 band＋reader（排在他們的 leadexam 之後）；codeforge 只寫 `side` — contract §1。
- `side` 加在 `codeforge.panel/1`，不改 schema 字串；usage_full 只有 `'40'`、模型取前 3 — autopilot 審查意見。
- 新訊息 `✦`（預設前景色）、子選單目前項 `◐`（共用 chart.here）、`▸` 只表示收合、框線走 PALETTE here/idle/structure — contract §3。
- band 的寬度用 `bodyColumns`，statusline 照舊用 `CODEFORGE_WIDTH → COLUMNS−4`，兩邊不需要同一個數字 — PM。
- 疊層（§14.1 三條規範）可用；這一版用不到，用到時在當時的 Claude Code 版本重測 — PM。
- **讀 tool-results 溢出檔已允許**：操作員 d-522f32f4，所有 agent 都能唯讀 `~/.claude/projects`（tool-results、transcripts）；寫入、settings、hooks、憑證仍要操作員開卡 — 不用再等。
- **autopilot 側邊窗格不得佔 band 寬度**（面板開著時）：PM 已裁示，autopilot 今天 18:00 TW 前交做法和截圖 — 不用再等。

## 下一步
1. 跑 g2（範圍 5598013..03fba21 的 delta）：
   `node ~/projects/autopilot/scripts/hetero-review-loop.js collect --repo-root ~/projects/codeforge-wt-panel2 --ledger ~/.codeforge-backups/side-review-ledger/ledger --phase full --generation 2 --branch feature/panel-side --seats 'gpt-6-astra/high@codex' --spec-file ~/.codeforge-backups/side-review-ledger/spec.md --timeout 25m`（run_in_background），收斂後 finalize g2，resolutions 帶 g1 那條（id 7fa75441…，fixed in 03fba21），再跑 `check-phase-review-receipt.js --ledger … --phase full --branch feature/panel-side --phase-base 0ae0c34e0086690e711ff6dfea2b2869a0aecb7b --repo-root …`。
2. 等 autopilot 通知 band 外殼和 reader 好了（聯絡 autopilot lane；地址用 fleet directory 查）。
3. 一起在真機拍五級截圖：每級四個畫面（主選單、子選單、答題、送出後），只裁到面板（截圖流程見 `ui-preview/codeforge/ux-v4/BRIEF.md`；截圖 session 只在 codeforge 開，帶 `--strict-mcp-config --mcp-config '{"mcpServers":{}}'`，tmux 拍完 kill；不能有 `$` 金額或 `acct-`）。
4. 把五級截圖＋兩邊 sha 交 PM（地址用 fleet directory 查），PM 同意後 `--no-ff` 合 feature/panel-side 進 develop，merge 訊息最後一段帶 `QC-Verdict: PASS (reviewer codex/gpt-6-astra, <日期>)` 和 Co-Authored-By，push，刪 worktree 和分支。

## 驗證方式
- `cd ~/projects/codeforge-wt-panel2 && claude plugin test plugins/forge-stats` → 171 pass、0 fail。
- `./scripts/check-all.sh` → `check-all: all gates green`。
- receipt：`check-phase-review-receipt.js` exit 0。
- 真機：autopilot band 讀到 `forge-stats.panel.side`，120 級以上右欄出現用量（5h／7d 量表＋比例），240 級第三欄出現「記憶 L1 N 條／大腦 …」。

## Read-order
1. ~/projects/codeforge-wt-panel2/doc/specs/panel-band-contract.md（在 feature/panel-side 分支） — 兩邊的分工、`side` 格式、各級用法、測試分工。
2. ~/projects/codeforge-wt-panel2/plugins/forge-stats/hooks/side.ts — 寫入端（`sideState`）。
3. ~/projects/codeforge/ui-preview/codeforge/ux-v7/design.md — O4 每一級的版面和真機座標。
4. ~/projects/codeforge/doc/specs/tui-guideline.md §3、§14 — 符號、顏色、疊層與焦點規範。

## 陷阱
- 主 clone 永遠停在 develop；改檔一律開 worktree（`git worktree add ../<dir> -b <branch> develop`）。
- `git merge -F -` 讀不到 stdin：merge 訊息先寫檔再 `-F <file>`。QC-Verdict 和 Co-Authored-By 要在同一個最後段落。
- 不要把 commit 串在「經過 grep 的測試輸出」後面：exit code 是 grep 的。先存 log、看真的 exit code 再 commit。
- zsh：glob 一律加引號；bash-footgun-guard 也會擋含 `&&` 鏈的 hetero-review 指令，finalize／collect 分開下。
- 截圖：autopilot 外掛的側邊窗格在 viewport ≥144 時會自動開、吃掉 band 寬度並露出真實金額；截圖 session 用 `--settings` 暫時關掉該外掛（不寫任何設定檔）。
- 原先 BRIEF 寫的「band 沒有疊層」是錯的（`Box position="absolute"` 可疊），已在 tui-guideline §14.1 更正。
