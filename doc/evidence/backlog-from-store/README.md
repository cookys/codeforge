# doc/BACKLOG.md 改由 hippo 紀錄庫產生（codeforge）

- 建庫：`scripts/onboard-repo.sh`（BACKLOG_PATH=doc/BACKLOG.md）先在 clone 演練，43 列 43 筆紀錄、0 筆略過；外洩掃描（fleet deny 清單）0 筆命中。
- 匯入前改過一處來源檔：B74（11-24 追溯與 band 併後的 Minor）原本沒有 Id 行，補上 `B74`，這是唯一的來源改動。
- 匯入器為此認得 `blocked`、`done（…）`、`superseded（…）` 三種寫法（B67、B70、B71、B45 原本是 unknown-status）。
- `diff-before.txt`：第一次產生結果與原檔的差異。88 行 missed（匯入器本來就丟掉的欄位如 Artifact、Done-Check、Needs、Used-by、Queued-By、ms，以及 Status／Origin 的原文）、format 2 行。
- 補進 store（只補不刪）：`backlog-restore.py backfill --apply`。`diff-after.txt`：missed 0、extra 0、format 15（只剩欄位順序，產生器用固定順序）。
- 產生檔與原檔逐行比對（多重集合）：只差標頭兩行（加上「由 scripts/backlog-view.sh 產生，不要手改」與 next-id 註解），以及 B74 原本重複的第二行 `Pointer`（同一列有兩行 Pointer，匯入器只留第一行）。
- 守門：`scripts/backlog-view.test.sh` 在 `check-all.sh` 內；手改 `doc/BACKLOG.md` 即紅（red-check：檔尾加一行 → FAIL；把 check 改成永遠通過 → 暫存副本內的手改測試 FAIL；還原後綠）。
- 日常：用 `hippo new/edit/set/close` 改紀錄，再跑 `scripts/backlog-view.sh`，同一個 commit 帶 `.hippo/` 和 `doc/BACKLOG.md`。
