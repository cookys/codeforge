---
id: 01M8GZX2B6XHZM51QA21E85AZR
kind: issue
created: 2026-11-28T11:50:59.174Z
---
## Problem
scripts/pane-gate/capture.py 宣告了 SZ=<WxH> 環境參數（第 21 行）但從未使用——初始視窗尺寸由 harness 的 launch(cols=209, rows=49) 硬編。文件與代碼不一致：使用依 SZ 開單尺寸Capture 會静默得到 209x49（B80 實拍時踩到）。

## Done-Check
1. SZ 接進 R.launch 的 cols/rows（預值維持 209x49），或乾脆移除該參數並更新開頭註解——擇一，代碼與註解一致。
2. toolchain.test.sh 加控制：--check-env（或 capture.py --print-plan）能讀到生效尺寸。
3. 用 SZ=120x40 跑一張 smoke 實拍，display-message 讀回 window_width=120（沙箱，驗後清除）。

## Boundary
只動 scripts/pane-gate/capture.py（必要時 toolchain.test.sh）；不改 v74-keys harness 的預設值。
