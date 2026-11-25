# Pane 歷史

開 `index.html`：原則 → 現行 R6 → 設計稿 → 各版。`versions.json`、`principles.json` 是生成器的資料來源；準則仍以 [R1–R7](../pane-design-language.md) 為準。

```sh
python3 doc/design/pane-history/tools/build.py
python3 doc/design/pane-history/tools/verify.py
bash doc/design/pane-history/tools/append.test.sh
doc/design/pane-history/tools/append.sh <autopilot_commit>
python3 doc/design/pane-history/tools/shoot.py <autopilot_commit> --theme light
```

`append.sh` 拍深淺、取 commit 的完整訊息、追加未知原則 `?`，再重建；重複 sha 不追加。`--no-shoot` 供離線資料追加及合成 sha 測試，須自行補圖。`PANE_HISTORY_ROOT` 可指向測試資料目錄；`PANE_HISTORY_AP_REPO` 可指定追加工具讀取的 autopilot clone。

拍攝工具唯讀匯出 autopilot 版本，用私有 tmux、假 fleet、獨立 HOME 和設定複本，每張限制 240 秒。既有深色畫面按 sha 收錄；缺圖逐版拍攝，失敗留在 `shots/notes.log`，不冒用其他版。歷史工具不支援開啟指令與缺少 commit 的情況保留缺圖。

公開樹中的 sandbox HOME 名稱以等寬代字匿名化；pane 內容、欄位與格數不變，PNG 原樣複製。ANSI 預覽會提升不足的字色對比，下載檔保留原 ANSI 顏色。`drafts/r1-rev5/` 的舊生成器是歷史材料，現行工具在 `tools/`。

`?theme=dark|light&measure=1` 顯示格寬與行高量測。深淺切換同時選擇對應實拍；沒有圖片時明列缺圖。點縮圖放大、Escape 關閉。

上架檢查：

```sh
~/projects/lab/bin/lab preview serve doc/design/pane-history --bind lo --report --ttl 2m
```

缺稿、缺操作員原話及日期未記錄之處均保留標示，沒有重畫或猜測。自動 pin 掛勾、跨 repo 搬移、live 部署與 push 不在本次範圍。
