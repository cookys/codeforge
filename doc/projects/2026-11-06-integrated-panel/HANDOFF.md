## 目標
codeforge lane(team 帳號)的交接。O4 整合面板、G40 真資料檢查腳本、B34 和 statusline／install 的一串修正都已經合進 develop;這條線現在停車，剩下的工作全部在等外部條件。里程碑仍是 G40:用真的本機鏡像資料顯示配額。

## 現況
- 寫於 2026-11-08,取代 11-07 那份(那份列的 worktree 和分支都已經刪掉了)。session 要重開(操作員卡 d-b7c4f225:autoCompactWindow 233000 和新版 plugin 生效)。
- develop 是 835ecd9,已推，主 clone 乾淨，沒有其他 worktree,沒有在跑的背景工作。
- 已合進 develop 的(每一支都有 PM go,審查都是 seat_gap,只有 claude-native 一席，codex 那一席欠著):
  - 90bfa22:O4 side 欄(forge-stats `codeforge.panel/1` 的 `side`)、band 契約、ux-v4..v7 設計文件、C1／D6 對照(PM 定案)
  - cc6d946:G40 真資料檢查 `scripts/g40-real-data/check.sh [<live_dir>]`(五級寬度、`acct-`、`$`;沙盒執行，真資料不進 repo)
  - ab00b14:B34,心跳加 `band_seen`,statusline 只在 band 真的畫過時才縮;外加 roster override
  - bb07c3d:side.memory 永遠不會是空的(兩個燈號都 hidden 時寫「記憶 —」)
  - 22cf4a3:用量摘要一開機就讀得到(readUsage 會掃所有候選目錄)
  - cdc6ea0:一筆壞的配額快照只丟那一筆，記在 `dropped_accounts`
  - 76cac4e:`codeforge usage` 表格照顯示寬度對齊
  - 653ff01 + cbec1e9:install 寫 `statusLine.refreshInterval: 10`(卡 d-f6e451d2),改動前先備份 settings.json.bak(權限沿用原檔、不寫穿 symlink、唯讀的也沒問題)
  - 835ecd9:沒有標記、但 command 剛好是「codeforge＋statusline」兩個詞的 statusLine,install 會認領並補標記
- 真機截圖(兩個真外掛、五級 × 四畫面 × 有沒有鏡像兩種資料)和偏離結論在 /var/tmp/codeforge-band-shots/index.md(不進 git)。PM 已經讓 O4 結案。
- PM 停車單 01M6VYXA6S 已 accepted,最晚 11-10 18:00 TW 回頭確認(這不是交件時間)。

## 已決事項(不重議)
- 只認 courier-attested from 是 PM 地址的 go;別人轉述的不算(全域 CLAUDE.md)。
- seat_gap 做法:codex／agy 沒登入時，用 claude-native 單席審查，merge 訊息寫明「codex 欠著」,進 main 或 release 前要補 — PM。
- codeforge 交給 C2 的資料只有 usage-summary/1;寵物 XP 不進 D6;codeforge 不做 C1 — PM 定案(doc/specs/console-c1-d6-mapping.md)。
- 用量和寵物分頁是另案(backlog):band 合併後先做用量，寵物排期前要先和 codepower 對過 — PM。
- 心跳的 `layout` 刻意不跟著畫面變:autopilot 靠它決定要不要把一列交給 forge-stats(doc/specs/codeforge-mod-ui.md)。
- fleet 用量／quota 一張表:quota 的正本是 autopilot 鏡像，用量的正本是各台的 usage-summary/1,由 C2 收;表格畫面歸操作台 — 已提案給 PM,還沒開工。

## 下一步
1. 讀 fleet 新訊息，看有沒有 PM 的 go。沒有的話就繼續停著，不要找事做(PM 的停車單)。
2. codex 登入後(操作員在 team 帳號跑 team-engine-login.sh,卡 d-56aea4dc;先用 `timeout 20 codex login status` 確認),等 PM 給 go,再一次補 codex 那一席：範圍是 develop 的 adf4454..835ecd9,diff 大的話用 `hetero-review-loop.js --packets` 切開。
3. 卡 d-2a19fdb5 好了之後(有真的配額鏡像):`PATH="$HOME/.cargo/bin:$PATH" ./scripts/g40-real-data/check.sh <live_dir>`。live_dir 依序是 $AUTOPILOT_LIVE_DIR、/run/user/1001/autopilot、/dev/shm/autopilot-1001、~/.autopilot,要有 quota/ 才有用。
4. autopilot 的 scripts/lib/quota-contract.json 出現之後:做 B45 的同步測試(見 doc/BACKLOG.md B45)。
5. backlog 裡 PM 還沒排的：其他引擎的 transcript 收集器、`usage push` 排程、alias 對齊提示(見上面的 fleet 一張表提案)。

## 驗證方式
- `claude plugin test plugins/forge-stats`:175 pass、0 fail。
- `PATH="$HOME/.cargo/bin:$PATH" ./scripts/check-all.sh`:最後一行是 `check-all: all gates green`(cargo test 1073+ pass)。
- `scripts/g40-real-data/check.sh` 不帶參數是 exit 0;`--negative` 是 exit 1。

## Read-order
1. ~/projects/codeforge/CLAUDE.md:Goal、規則、主 clone 永遠停在 develop。
2. ~/.claude/projects/-home-team-projects-codeforge/memory/MEMORY.md:team 帳號的工具缺口、lane 狀態、「gate 單獨跑、看完 exit code 再 commit」。
3. ~/projects/codeforge/doc/specs/panel-band-contract.md:和 autopilot 的分工、side 格式。
4. ~/projects/codeforge/doc/specs/codeforge-mod-ui.md 的 band 版面段落:心跳、band_seen、refreshInterval、.bak。
5. ~/projects/codeforge/doc/BACKLOG.md:B41、B45 的剩餘項。

## 陷阱
- team 帳號：cargo 要加 `PATH="$HOME/.cargo/bin:$PATH"`;讀不到 /home/cookys;fleet 的 send／reply 不能帶 attachments(先 upload,再送 ref)— 見記憶 team-account-toolchain。
- gate(check-all、doc-drift)要單獨跑，看完 exit code 才 commit;這次踩過兩次 — 見記憶 gate-before-commit。
- 外掛測試只能 import 相對路徑和 "claude-code";render 裡不能讀 `$.clock`(用 flag 在 timer 裡換算);mock.clock 不會改 `Date.now()`。
- 新版 autopilot 的 roster 規則會拒絕沒放在 override 區塊裡的座席欄位(codeforge 已改成 override,until 2026-12-07,到期要續)。
- statusline 在閒置時只靠 refreshInterval 重跑;測試 statusline 時要確認 Claude Code 真的有重跑。
- 輸入框裡可能有操作員打了還沒送出的字，不要自己送出，也不要清掉。
- 審查帳本在 ~/.codeforge-backups/*-review-ledger/(不進 git)。
