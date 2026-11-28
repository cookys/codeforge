---
id: 01M8H3JV1MP5F0GXGK7E2RBM7V
kind: issue
created: 2026-11-28T12:55:18.324Z
---
hangar ask（fleet 01M8H113AK9HHK6PQ9M728PCB1，hangar 單 01M8H0N14J）：usage-summary 在沒有真實重置時間時把 resets_at 填成哨兵值 4102444800（2100-01-01），digest 原樣帶出、ThinkSmart 畫成 01-01（hangar ops log:1437）。實例：accounts[2] 所在 host 的 snapshot accounts[2].five_hour.resets_at（stranity-personal，stale 讀數）。hangar 已在 digest 加 >8d 防線；來源端改掉才算根治。

修法（summary.json contract 變更）：
- summary.rs `window()`（:163）：resets_at >= 4102444800（哨兵）時不輸出該欄，只留 pct（+seen_at_ms）。
- `WINDOW_KEYS`（:69）改成只有 pct；`resets_at` 移入 `WINDOW_OPTIONAL`；validate()/win() 容忍缺欄（缺＝不知何時重置，不是壞快照，不得走 DROP_REASONS[1]）。
- pane 端的 contract 複製檔要同步（summary.rs:116 contract() 註解：pane 有一份比對）；找齊所有 contract 比對測試再改。
- 讀側先確認容忍缺欄：quota_view.rs 用 `resets_at_local`（已是 Option）、statusline、pane 渲染；缺欄要畫「—」不能閃爍或 panic。
- 紅測：sentinel_resets_at_is_not_emitted（snap_with(alias, 5_000, 0.3, 4102444800) → v["accounts"][0]["five_hour"].get("resets_at").is_none()，validate(&v).unwrap() 仍過）。
驗收：紅測先紅後綠＋tip check-all EXIT=0＋併 develop 附 sha＋回 hangar（their >8d 防線可留可退役，由 hangar 決定）。
