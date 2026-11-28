---
id: 01M8H0GQ5Z90XDGQ4HAMTSVKNZ
kind: issue
created: 2026-11-28T12:01:43.103Z
---
pane-gate 實拍後的人工彙總（跑 contrast256.py、cjk-width.py、貼數字）每次都重做一遍。改為 capture.py 收尾時自動對 out-dir 跑兩個分析器，結果寫進 out-dir/report.txt 並在 shots.json 記錄；toolchain 控制用現成 fixture 驗證 report 產出。吃掉的操作：B80/B83 兩輪實拍後的人工彙總。
