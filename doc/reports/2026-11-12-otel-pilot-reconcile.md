# OTEL pilot 對帳（2026-11-12）

PM 01M747D34N 開的 24 小時 pilot；PM 01M76T9R 裁定停止，並要求本報告進 repo。

## 範圍

- 主機：aimax395 的 team 帳號。期間：2026-11-11 10:37 到 2026-11-12 10:43（CST）。
- OTEL：Claude Code 的 `api_request` 事件，送到本機 receiver（OTLP/HTTP，port 4318），寫成 JSONL。
  只有在 team tmux 裡帶 OTEL 變數啟動的 session 會送，共 15 個 session。
- codeforge：`codeforge usage collect` 之後 `usage.db` 的 `requests`（`source = 'claude'`），用 request_id 對。

## 結果

| 項目 | 筆數 |
|---|---|
| OTEL `api_request` | 1741（無重複 request_id） |
| usage.db 同時段 claude 列 | 5483 |
| 兩邊都有 | 1529 |
| 四種 token（input、output、cache_read、cache_write = cw5m + cw1h）不一致 | **0** |
| 只在 OTEL | 212 |
| 只在 usage.db | 3954 |

**只在 OTEL 的 212 筆**：
- 208 筆是不寫進 transcript 的旁支呼叫（`query_source`）：prompt_suggestion 144、agent_summary 38、
  compact 10、generate_session_title 7、web_search_tool 5、web_fetch_apply 5。codeforge 只讀 transcript，
  結構上看不到這些。合計 output 77,234／904,639（約 8.5%），API 等值 $5.42／$84.23（約 6.4%）。
- 4 筆 `repl_main_thread` 是對帳那一分鐘內本 session 的主對話，在 collect 之後才產生，是時間差。

**只在 usage.db 的 3954 筆**：OTEL 只涵蓋 15 個 session；其他 lane、subagent、hetero worktree 沒帶 OTEL 變數。
usage.db 沒有 session 欄位，無法逐 session 切開，所以這一側不能用來判定 codeforge 多算或 OTEL 漏收。

**帳號歸屬**：OTEL 的帳號欄位整段期間都是同一個值，看不出 06:30 換成 setup-token，所以驗不了歸屬。
codeforge 這邊，對上的列在 06:30 前 624 筆記在 stranity；06:30 之後 377 筆仍記在 stranity、528 筆記在新 token 帳號。
06:30 之後記在 stranity 的那 377 筆，是 alias-pin 修正（eb4a546）之前的已知誤歸屬，依 PM 01M76FE9 不回填。

## 結論

- codeforge 從 transcript 收到的 token 數與 Claude Code 自己回報的完全一致。
- 已知偏差：旁支呼叫不在 transcript，codeforge 少算約 8.5% output、約 6.4% API 等值。已寫進
  `doc/specs/codeforge-usage.md`「成本估算」；capacity 的成本法燒速乘上係數 1.07。
- Pilot 已停：team tmux 的 5 個 OTEL 變數已 unset，receiver 已結束。已在跑的 session 行程裡仍有這些變數，
  會對已關閉的 port 匯出失敗（不影響運作），重開後就沒有。原始事件檔留在 `~/.codeforge-otel-pilot/`（0600，不進 repo）。
