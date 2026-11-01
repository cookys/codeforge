# Rubric — 2026-11-01-install-stable-paths.md

> Source plan: doc/plans/2026-11-01-install-stable-paths.md

R1: 寫進 global settings.json 的 hook `command` 不得含絕對路徑、`$HOME`、`~` 或版號；一律是 `codeforge <subcommand> …`，與既有 `codeforge dream --quiet 2>/dev/null || true` 同樣依賴 PATH。
R2: marker 值固定為 `codeforge@1`（install 契約版本，不是套件版本）；判斷是否為 codeforge 的規則維持 `starts_with("codeforge@")`，舊的 `codeforge@0.0.x` entry 照樣會被 sweep。
R3: `--project-hooks` 的行為與輸出完全不變（已經用 `${CLAUDE_PROJECT_DIR}`）。
R4: 不新增 crate 依賴。
R5: 不在作者的開發機上安裝 codeforge hooks；e2e 一律用 `CODEFORGE_CLAUDE_SETTINGS` 指向 scratch 的 temp 檔。
R6: 使用者看得到的訊息用正體中文；CJK 截斷用 `.chars().take(N)`。
R7: 格式化只用 `./scripts/fmt.sh`。
R8: settings.json 是 symlink 時，寫入後連結保持不變、內容寫在最終 target；dangling 連結回錯且不覆蓋連結本身。
R9: 共用檔上，hook 與 statusLine 的 command 在每台機器、每個版本寫出的內容相同；舊版（`codeforge@0.0.x`、`hooks/<ver>/` node 路徑）entry 在重跑 install 後不留下、不重複觸發。
R10: 共用檔在沒有 `--cleanup-period-days` 時不新增、不修改、不刪除 `cleanupPeriodDays`；單機檔維持現行預設 3650。
R11: `codeforge hook` 原樣轉發 stdin、argv 與腳本的 exit code；轉發器自身的錯誤絕不回傳 2。
