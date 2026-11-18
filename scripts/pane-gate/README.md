# pane-gate — 真終端機截圖與 UX 閘門（autopilot-live pane）

codeforge 主導 pane 搬到共用 TUI（PM 01M7N307），實作在 autopilot；這裡是 codeforge 驗收用的工具。

- `gate-shots/run.sh <autopilot-dir> <label>`：tmux 裡跑 Claude Code，假 fleet 資料（`gate-shots/bin/fleet` → `gen.py`），拍 1–4 頁 160x40／120x30／80x24 → `gate-shots/out/<label>/`。
- `gate-shots/keyprobe.sh <autopilot-dir> <tag>`：pane 開著時縮窄、按數字鍵，記錄頁首和提示列（B56）。`START`/`STARTH` 開啟尺寸，`RW`/`RH` 縮成的尺寸，`KEYS` 要按的鍵。
- `run-gate-b2.sh <autopilot-worktree>`：拍照 → keyprobe → `build-b2.py` 組評審包 → carbon 和 astra 並行評審。用 Bash 的 run_in_background 跑。
- `pet-shots/`：寵物分頁截圖。

截圖 session 的 cwd 都會放 `.fleet-no-register`（`gate-shots/lib.sh`、`pet-shots/shot*.sh`），fleet 6353fad2 起就不會把它們登記到巡查名單（PM 01M7PMF7）。截圖只用假資料，`build-b2.py` 會檢查評審包裡沒有真實主機名或組織名。
