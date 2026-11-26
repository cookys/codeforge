---
id: 01M8AYBSJRJJB6DWG66D0A4XF2
kind: issue
created: 2026-11-26T03:28:37.976Z
---
- **Since**: 2026-11-26
- **Problem**: T2 機器閘的 `source-overlap` 規則拿整個候選 JSON（含 `evidence.session_id`、`time_start`、`time_end`）去和來源 L0 訊號文字比對 ≥20 字元共同子串。如果 session-digest 的訊號文字裡本身帶 session id 或 ISO 時間戳，每一筆候選都會被這條規則丟掉，T3 的候選區永遠是空的。假資料測試沒有這種 L0，所以沒看出來。
- **Impact**: 01M83DFC1Q 上線後真實 L0 進來時，可能 0 筆候選，PM 要的 10 筆樣本拿不到，T4 判斷卡住（Major）。
- **Trigger**: ready；查 session-digest 寫進 L0 的 content 格式，若含 session id／時間戳，改成只拿五個列舉欄位比對（evidence 欄位不比），補一條「來源含同一 session id 仍保留」的測試
- **Pointer**: `src/lessons/gate.rs`（source-overlap）；`src/lessons/candidates.rs`（collect 傳 source）；計畫 doc/plans/2026-11-23-knowledge-pipeline.md 第 10.2 節 T2
- **Origin**: self（T2 交件時的未驗項）
- **Done-Check**: `cargo test lessons` 有新測試：source 含候選的 session_id 與 time_start 時候選保留；source 含候選 repo＋tool 拼成的 ≥20 字片段時仍丟棄
- **Artifact**: test
- **Needs**: none
- **Used-by**: e2e
