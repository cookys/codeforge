# CodeForge Memory Contract — 共享 state 與跨工具接縫

> Created: 2026-06-16 · Status: living contract · Owner: cookys
> 背景:[`doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`](../proposals/2026-06-16-memory-recall-and-stolen-patterns.md)

improvement-queue / digest / L1 atoms 是被多個獨立血緣的工具(codeforge、autopilot、
未來其他)讀寫的**共享 state**。本文件是**明文契約**(不是隱性「剛好共用目錄」),
讓 producer / consumer 不漂移、不互蓋。設計原則:**codeforge 是知識 producer;
格式化/萃取是可插拔 consumer 的事**(見 dialectic 結論)。

## 1. codeforge 擁有 / 生產的 state

| Surface | 路徑 | 格式 | 誰寫 | 誰讀 |
|---------|------|------|------|------|
| **L0 signals** | `.codeforge/signals/YYYY-MM-DD.jsonl` | JSONL append | `learn` / session hooks | `dream` |
| **L1 atoms** | `.codeforge/store/{concepts,connections,qa}/*.md` | markdown + YAML frontmatter | `dream`(L0→L1) | `memory context`(本地 recall)、`ship`(→mnemos)、consumer(見 §4) |
| **L2 ledger** | POST `/v1/ingest/ledger` | envelope JSON | `ship` | Mnemos |
| **improvement-queue** | `~/.claude/improvement-queue.json`(global 共享) | JSON;item 帶 `project`(origin root)、`source` | `session-digest.js` | `check-improvements.js`(各專案以 `project===PROJECT_ROOT` scope) |

**File-first 原則**:L1 markdown 是 source of truth;SQLite FTS index 是可重建衍生
(`memory status` / 重掃)。刪 index 可由 .md 重建。

## 2. L1 atom frontmatter schema(consumer 讀這個)

```yaml
type: concept | connection | qa      # 結構類型
topic: <slug>                        # 穩定識別 + citation key（consumer 以此 pull）
created: 'YYYY-MM-DD'
updated: 'YYYY-MM-DD'
sources: [<L0 signal id>...]
links: ['[[topic]]'...]              # wikilink 關聯（≥2）
refs: <int>                          # 被引用次數
last_ref: <ts|null>
strength: <0.0..1.0>                 # ACT-R activation；recall ranking 的 importance 因子（score = importance × recency × citation，見 src/memory/recall.rs::score）
status: active | superseded | archived
# --- RESERVED（Phase B 由 dream 填,consumer 現在不可依賴) ---
# nature: procedural | declarative   # 是否「可複用 how-to/規則」→ skill 萃取候選
```

body:第一個 `# ` 行為 title;其後為內文。

> **`nature` 是 reserved**:dialectic 結論「codeforge 標 procedural atom、skill 萃取交
> consumer」。本欄位待 dream-compile 具備分類能力時(Phase B)填入;在那之前 consumer
> **不得假設其存在**。先在此契約佔位,避免日後 schema 驚訝。

## 3. READ-path 契約(本地 recall 注入)

- `codeforge memory context [--max-tokens N] [--hook]` 讀本地 active L1 → 統一 recall
  score 排序(importance × recency × citation,T2.3;importance = ACT-R strength)→
  budget 截斷 → **lean ranked index**(非 dump)。
- `--hook`:輸出 Claude Code SessionStart `hookSpecificOutput.additionalContext` JSON;
  無 active L1 → 不輸出(no-op)。裝在 **global settings.json** SessionStart(非 plugin
  hooks.json — GitHub #16538 後者注入不可靠)。
- 每條帶 citation `topic`;詳情走 `codeforge memory search <topic>`(progressive
  disclosure:index push + detail pull)。
- 中央版對應:`mnemos-cli context`(跨 source,需 mnemos opt-in)。

## 4. Consumer 接入(skill 萃取等)

skill 萃取(procedure→SKILL.md)**不在 codeforge**(dialectic:scope-routing 需 CC
session 拓樸,codeforge 看不到;且避免雙萃取器漂移)。consumer(autopilot:distill /
未來薄 distiller)依本契約讀 §2 的 L1 atoms(未來以 `nature` 選 extraction-ready),
自行決定 user/project-level skill 路由。codeforge 只保證 atom 的生產與 schema 穩定。

## 5. Versioning
schema 變更(新增/改名欄位)走本文件 + CHANGELOG;新增欄位一律 optional + back-compat
(serde default),不可破壞既有 .md 解析。consumer 對未知欄位寬容。
