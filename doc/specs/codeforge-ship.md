# CodeForge Ship — L2 Ledger Producer

> Status: **STUB** · Owner: cookys · Created: 2026-05-15
> Sprint 1 啟動時 expand 完整 schema + Haiku digest prompt + L1 reading strategy + retry/queue 設計。
>
> This stub exists to unblock Mnemos Sprint 0-2 (`cookys/mnemos/docs/specs/20-sprint-0-2.md` D1.1) — the full spec must be written before Sprint 1 implementation starts.

---

## 1. 動機

CodeForge 既有 memory pipeline 是 L0 raw signals → Haiku → L1 compiled markdown，留在 `.codeforge/store/` 本地用。

但 L1 是 per-session per-repo 的本地知識，沒有跨 repo aggregation 也沒有送進中央 brain。

**`codeforge ship`** 把 L1 + git log + 今日 session jsonl 再 digest 成 L2 daily ledger，POST 到 Mnemos（`POST /v1/ingest/ledger`），讓 Mnemos 端可以萃取 atom 進入跨 repo / 跨 session 的 active memory。

---

## 2. 角色定位

`codeforge ship` 不是玩具子命令，是 **CodeForge 作為 Mnemos source 的職責**。Day-1 就是 critical path（不是 nice-to-have）— SessionEnd hook 觸發、失敗有 retry policy、結果影響 Mnemos 資料完整性。

但實作上仍然只是 codeforge binary 的一個 subcommand（不另外開新 binary，依照 spec discussion 拍板的 single-binary 原則）。

---

## 3. CLI Surface

```
codeforge ship                  # 預設：digest 今日 (UTC)，POST 到 ~/.config/mnemos.env 指定的 endpoint
codeforge ship --date 2026-05-15  # 指定日期 (re-ship 不覆寫，Mnemos dedup by ship_id)
codeforge ship --dry-run        # 印 ledger JSON 不送
codeforge ship --no-hook        # 抑制 retry 寫 ship-failed/（hook 用）
codeforge ship --resend         # 從 ~/.codeforge/ship-failed/ 補送
```

預設 endpoint: `http://127.0.0.1:8845/v1/ingest/ledger`
預設 ship_id 產生: ULID per call

---

## 4. 輸出 Schema

對齊 [`mnemos/docs/specs/10-source-contract.md`](#) §5.1 `source: codeforge_ledger`。

完整 envelope + payload 在 source-contract 那邊定義；本 spec 只負責「CodeForge 端如何產出這個 payload」。

關鍵欄位（partial reminder，full schema 看 source-contract）：

```json
{
  "ship_id": "<ULID>",
  "source": "codeforge_ledger",
  "ship_at": "<ISO 8601>",
  "machine_id": "<machine identifier>",
  "payload": {
    "ledger_date": "...",
    "repo": "...",
    "repo_path": "...",
    "git": { "branch": "...", "head_sha": "..." },
    "shipped": [...],
    "struggled": [...],
    "lessons": [{ "title": "...", "detail": "...", "candidate_atom_kind": "lesson",
                  "source_evidence": [{ "kind": "session_jsonl", "value": "...", "locator": {...} }] }],
    "rabbit_holes": [...],
    "metrics": { "commits": N, "tests_added": N, "session_hours": F, "haiku_cost_usd": F },
    "provenance": { "raw_signal_count": N, "source_jsonl_paths": [...], "l1_concept_files": [...] }
  }
}
```

---

## 5. Source 讀取（待 Sprint 1 完整定義）

`codeforge ship` 讀取的本地資料：

| 來源 | 路徑 | 用途 |
|---|---|---|
| L1 concepts | `.codeforge/store/concepts/*.md` | 主要 lesson 來源（已 Haiku-compiled，再 paraphrase 即可） |
| L1 connections | `.codeforge/store/connections/*.md` | 關係 atom（未來 atom 多類型時） |
| Git log | `git log --since=<ledger_date> --until=<+1d>` | shipped commits |
| Session jsonl | `~/.claude/projects/<slug>/<uuid>.jsonl` (今日新增) | struggled / rabbit_holes 偵測 + source_evidence pointer |
| `.codeforge/codeforge.db` | 本地 SQLite | metrics (session_hours, xp_gained 等) |

---

## 6. Haiku Digest Pipeline（待 Sprint 1 完整定義）

```
Sprint 1 啟動時定義:
- prompt template (input: L1 + git diff + session 摘要 → output: structured ledger payload)
- 多 lesson 抽取的 prompt strategy
- struggled 偵測 heuristic (transcript 重複 prompt / cargo test 失敗 / file 短窗連改)
- rabbit_holes 偵測 (git reset / abandoned branch / git revert)
```

---

## 7. Cite Subcommand

`codeforge mnemos-cli context|cite` 是另一個 subcommand，spec 在 [`codeforge-mnemos-cli.md`](codeforge-mnemos-cli.md)（也是 stub，待 Sprint 1 寫）。

`ship` 跟 `mnemos-cli cite` 的協作:
- ship 結束時順便偵測「本 session transcript 是否引用任何 Mnemos atom」
- 偵測到 → 對每個 atom 呼叫 `mnemos-cli cite <atom_id>`
- Sprint 1 用 fulltext_match heuristic；Sprint 5+ 改 Haiku

---

## 8. Retry & Failure Policy

依照 source-contract §9.1:

```
1st attempt → fail (5xx) → wait 1s → retry
2nd attempt → fail → wait 5s → retry
3rd attempt → fail → wait 30s → retry
4th attempt → fail → 寫 ~/.codeforge/ship-failed/<ship_id>.json，下次 ship 自動 re-attempt
```

`--no-hook` 模式下不寫 ship-failed/，避免 hook 啟動延遲。

---

## 9. Hook 設定

`~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionEnd": [
      "codeforge dream --quiet",
      "codeforge ship"
    ]
  }
}
```

`dream` 先跑（產出 / refresh L1），`ship` 接著（讀 L1 digest L2）。

---

## 10. 待 Sprint 1 expand 的事項

- Haiku digest prompt 完整版
- L1 → ledger payload 的 reduce 演算法（多 concept 文件如何 merge / dedup）
- struggled / rabbit_holes 偵測 heuristic 的精確規則
- `mnemos-cli context` 的 topic 推導邏輯
- error log + structured logging
- unit + integration tests
- locales/{en,zh-TW}.yaml strings
- CLAUDE.md update（CodeForge 新增 ship 角色 + production critical path 註明）
