## 目標
codeforge lane(team 帳號，地址用 fleet directory 查)有兩件工作：(A) O4 整合面板的 codeforge 側，交五級截圖後合併;(B) G40 真資料 preflight 腳本(PM go,S)。

## 現況
- 2026-11-07 約 21:00Z,取代 2026-11-06 那份交接(舊 session e6514bd9,cookys 帳號)。
- **(A) `feature/panel-side`**,worktree `~/projects/codeforge-wt-panel2`,tip f8c131c,已推，乾淨。03fba21 之後只有文件 commit:ccd3a44(契約 §1.3 panel_open_at_ms、§2 欄位與 reader 行為)、6cb6061(ux-v7 進 repo)、366b416＋7928759(ux-v4..v6 進 repo)、c4294d1＋f8c131c(用量／寵物分頁標成另案 backlog)。
  - 驗證:check-all 10 步在 03fba21 全綠(team 帳號要加 PATH,見陷阱);forge-stats 171 pass。
  - review:g1 已 finalize;g2 因 team 帳號沒有 codex 記成 aborted(seat_gap)。**下一次是 g3**。ledger 在 `~/.codeforge-backups/side-review-ledger/`(PM 從舊帳號傳過來的)。
  - autopilot 的外殼：**已進 autopilot develop** c63987f2cd4d(Merge band;lane ab679f7bb500)。五級 × 四畫面截圖在 `/var/tmp/autopilot-band-shots/v7/`(index.md),我看過，沒有問題;交件前在真機重拍一次(重載 plugin)。
- **(B) 已完成**:`feature/g40-real-data-preflight` 6146c80(已推，從 feature/panel-side 開的),worktree `~/projects/codeforge-wt-g40pre`。停車單 01M6T847PY 已回 done。要等 O4 合併之後，再請 PM 給這支的合併 go。
- C1／D6:PM 已定案，對照頁和定案結論都在 `doc/specs/console-c1-d6-mapping.md`(這個分支)。之後 D6 owner 來要寵物定義時，從這份提供。

## 已決事項(不重議)
- 只認 courier-attested from 是 PM 地址的 go(全域 CLAUDE.md)。已有的 go:(A) g2/g3＋finalize＋receipt;(A) 的合併由 PM 看過截圖＋兩邊 sha 之後才給;(B) S,完成回 sha。
- g3 不能用跨帳號(cookys)代跑繞過去 — PM。
- 契約裁定(已回 autopilot):panel_open_at_ms 每 15 秒寫一次、寫入間隔小於 60 秒;`$` 只丟整份 side,usage／pet 照畫;外殼只在分頁打開時出現;進度列在主視窗框內;沒有圓桌分頁 → 9 換到派工;左框標題列卡片時寫分頁名，列分頁清單時寫「選單」。
- 用量／寵物分頁:另案 backlog。band 合併後先做用量;寵物排期前要先和 codepower 對過 — PM 直接確認。
- (B) 的做法：外掛測試只能 import 相對路徑和 "claude-code",不能用 node:fs,也拿不到 process.env(已探測)。所以腳本把 `plugins/forge-stats` 複製到暫存目錄，把鏡像目錄的檔案寫成一個相對路徑的 input 模組，再放一份 real-data 測試進去，跑 `claude plugin test <暫存目錄>`。真資料不進 repo。Rust 端用 `AUTOPILOT_LIVE_DIR=<dir>`＋`COLUMNS=80/120/160/200/240` 餵 `tests/fixtures/statusline-p0.json` 給 `codeforge statusline`。

## 下一步
1. 先查 `timeout 20 codex login status`。登入了的話，跑 g3:
   `node ~/projects/autopilot/scripts/hetero-review-loop.js collect --repo-root ~/projects/codeforge-wt-panel2 --ledger ~/.codeforge-backups/side-review-ledger/ledger --phase full --generation 3 --branch feature/panel-side --seats 'gpt-6-astra/high@codex' --spec-file ~/.codeforge-backups/side-review-ledger/spec.md --timeout 25m`(run_in_background,輸出存檔、看真的 exit code)。收斂後 finalize g3,resolutions 帶 g1 那條 7fa75441…(fixed in 03fba21);再跑 `check-phase-review-receipt.js --ledger … --phase full --branch feature/panel-side --phase-base 0ae0c34e0086690e711ff6dfea2b2869a0aecb7b --repo-root ~/projects/codeforge-wt-panel2`。
2. (B) O4 合進 develop 之後:feature/g40-real-data-preflight rebase 到 develop,跑 check-all,向 PM 要合併 go。真資料到了(d-2a19fdb5)就跑 `scripts/g40-real-data/check.sh <live_dir>`。
3. autopilot 這邊的 sha 已經有了(c63987f)。g3 receipt 綠了之後:在 panel tip 上用 `PATH="$HOME/.cargo/bin:$PATH" ./scripts/check-all.sh` 重跑 → 把五級截圖、兩邊 sha、receipt 交 PM → PM go 後 `--no-ff` 合 develop(merge 訊息先寫檔再 `-F`,最後一段放 `QC-Verdict: PASS (reviewer codex/gpt-6-astra, <日期>)` 和 Co-Authored-By)→ push → 刪 worktree 和分支。
4. PM 指示新 session 接 (B) G40 preflight;(A) 等 codex 登入和 autopilot 推送，有動靜再處理。

## 驗證方式
- (A) `claude plugin test plugins/forge-stats` → 171 pass;check-all 最後一行 `check-all: all gates green`;`check-phase-review-receipt.js` exit 0。
- (B) `scripts/g40-real-data/check.sh` 用 fixture 跑 exit 0;`--negative` exit ≠ 0;check-all 綠。

## Read-order
1. ~/projects/codeforge-wt-panel2/doc/specs/panel-band-contract.md — 分工、side 格式、本次補的 §1.3／§2。
2. ~/projects/codeforge-wt-panel2/doc/projects/2026-11-06-integrated-panel/ux-v7/design.md — O4 版面和座標(截圖驗收用)。
3. ~/projects/codeforge-wt-g40pre/plugins/forge-stats/tests/quota-ui.test.tsx — L22-70 fixture、L385-462 假 fs 的 `mirror()`、L688-750 怎麼讀 `forge-stats.panel`。
4. ~/projects/codeforge-wt-g40pre/src/usage/quota_mirror_read.rs — live_dir 的解析順序。
5. /var/tmp/autopilot-band-shots/v7/index.md — autopilot 截圖和它和 v7 的差異清單。

## 陷阱
- team 帳號的工具缺口(cargo 要加 PATH、codex 已裝但沒登入、讀不到舊帳號的家目錄):見記憶 `team-account-toolchain`。g2 abort 之後下一次就是 g3,不要再用 2。
- fleet 的 send／reply 不能帶 attachments(`attachments_unsupported`):先 `upload`,再把 ref 用文字送出去。
- zsh／bash-footgun-guard:glob 一律加引號(`'*.ts'`);不要用 bare `ls`,用 `command ls`。
- 不要 commit 在還沒看 doc-drift 結果之前(這次就踩到一次，用 7928759 修掉了):先存 exit code 看過再 commit。
- 從其他帳號收進來的檔案(PM 上傳的):放到空目錄、先 `tar tzf` 看路徑，本機路徑換成佔位字再進 repo。
