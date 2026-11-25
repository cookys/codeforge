# 第 0 輪處置表（審的是 sha 388470f474b0）

| 席 | # | 等級 | 處置 | 改在哪裡 |
|---|---|---|---|---|
| codex | 1 | blocking | 修：加 2-L2，等你動手的長步驟在回覆區用 ↑↓ 捲，對話區不蓋 | PLAN §3.1、§3.2、§3.3 |
| codex | 2 | blocking | 修：加 5 步互動畫面（2-I 附註輸入、2-P 預覽、2-Pc 取消、2-R 成功、2-F 失敗） | PLAN §3.2 |
| codex | 3 | blocking | 修：殘留頁回覆區改唯讀加處理指引，不畫處置鍵 | PLAN §3.1 |
| codex | 4 | blocking | 修：待辦卡的問題放進回覆區 | PLAN §3.1 |
| codex | 5 | note | 修：選項鍵只有 a–c | INVENTORY 更正節 |
| codex | 6 | note | 修：y 複製標為新增能力，Q2 交 PM | PLAN §3.1、§3.4 |
| codex | 7 | note | 修：tok/s 來源補上 agentsColumn（speedview.ts:118），註明要接線 | PLAN §3.1、INVENTORY |
| codex | 8 | note | 採納：全文不蓋對話區，在原框內捲；文字區只寫列數，不寫鍵 | PLAN §3.3 |
| codex | 9 | note | 修：補每個來源的缺源、過時覆蓋表，輸出 coverage.json | PLAN §3.2.1 |
| codex | 10 | note | 修：註明「可捲動」只代表有截斷提示 | INVENTORY 更正節 |
| agy | 1 | note | 修：補 f 鍵 | INVENTORY 更正節 |
| agy | 2 | note | 同 codex 3 | PLAN §3.1 |
| agy | 3 | note | 修：待辦頁沒有 m 鍵，附註只顯示 | PLAN §3.1 |
| agy | 4 | note | 修：快取降級畫在 5-S | PLAN §3.2.1 |
| agy | 5 | Q1 | 採納，同 codex 8 | PLAN §3.3 |

## PM 裁定
- Q1：全文不蓋對話區（PM 01M89QHM）。
- Q2：複製網址改用 **g**（PM 01M89QHM）。
  - PM 建議 o，但 o 在非卡片頁是「展開焦點列」（keys.ts:44）。PM 指示「被佔用就換」，所以換成 keys.ts 裡沒有任何情境用到的 g。
  - y 保留給確認送出；c 會撞到選項鍵。

## 第二代（審的是 sha 154f99f4feb2）：codex FAIL（2 blocking），agy PASS
| 席 | # | 等級 | 處置 | 改在哪裡 |
|---|---|---|---|---|
| codex | 1 | blocking | 修：回覆區溢出通則（↑↓ 捲、「下面還有 N 列」、還有選項沒看到的提示）；8-L 改畫 80x24 長問題加長選項 | PLAN §3.1、§3.2、§3.3 |
| codex | 2 | blocking | 修：拿掉「額度池照常」；5-X 是 quota 失敗，另加 5-X2 是額度池、今日用量缺源；coverage.json 逐來源記錄 | PLAN §3.2.1 |
| codex | 3 | note | 修：§3.2.1 改成逐個獨立來源列出；加 1-X2、6-X2、7-X2；沒有過時概念的來源標明 | PLAN §3.2.1 |
| codex | 4 | note | 修：鍵分工統一，↑↓ 捲回覆區、PgUp/PgDn 捲來龍去脈；2-L2 畫兩者同時存在 | PLAN §3.1、§3.3 |
| codex | 5 | note | 修：原列直接改 a–c，補 f | inv-1-3、inv-7-9 |
| codex | 6 | note | 修：退回原因沒有來源，盤點更正；設計稿標「新增資料」 | inv-7-9、PLAN §3.1 |
| codex | 7 | note | 已採納（§3.3） | — |
| codex | 8 | note | 確認，無需處置 | — |
| agy | 1 | note | 採納：C2 在 80x24 沿用上緣收起規則 | 畫面輪處理 |
| agy | 2–3 | note | 確認，無需處置 | — |
