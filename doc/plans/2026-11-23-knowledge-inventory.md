# Transcript 知識管線現有零件盤點

> 唯讀盤點，供「transcript 知識管線」計畫初稿使用。只記錄已驗證事實與 repo 相對路徑。

## 1. autopilot `distill` 與 `learn` skill（含 routing）

- **輸入**：`distill` 掃 `~/.claude/projects/<encoded-cwd>/*.jsonl`（`scripts/distill-scan.js:30`）或情節模式回顧剛結束的長 session（`skills/distill/SKILL.md:55-70`）；`learn` 讀當前對話與 `.claude/knowledge/*.md`（`skills/learn/SKILL.md:92-102`）。
- **輸出**：`distill` 寫至 `~/.claude/skills/autopilot-distill-skills/skills/<slug>/SKILL.md`（全域）或 `<project>/.claude/skills/<slug>/SKILL.md`（專案），游標存 `~/.autopilot/distill/scan-state.json`（`skills/distill/SKILL.md:21-24,40`）；`learn` 依 `scripts/resolve-knowledge-routing.sh:37-40` 寫入 `memory_dir`、`.claude/knowledge/`（需 `git add -f` + commit）或 `discipline_target`。
- **觸發**：手動（`/distill`、`/learn` 或關鍵詞）與 `finish` 節點評估（`skills/distill/SKILL.md:6-8,65`、`skills/learn/SKILL.md:6-10,147`）。
- **Owner**：`autopilot`。
- **不做**：`distill` 不寫進 autopilot 自身 repo、不自動寫入未經人審 `AskUserQuestion` 的 skill（`skills/distill/SKILL.md:16,102-106`）；`learn` 不從歷史 transcript 批次萃取流程（`skills/learn/SKILL.md:10`）；`resolve-knowledge-routing.sh` 只解析路徑，不負責寫入（`scripts/resolve-knowledge-routing.sh:9-10`）。
- **證據**：`skills/distill/SKILL.md:1-150`、`skills/learn/SKILL.md:1-150`、`references/knowledge-routing.md:1-150`、`scripts/resolve-knowledge-routing.sh:1-150`、`scripts/distill-scan.js:1-50`。

## 2. hippo records

- **輸入**：CLI 命令輸入（`hippo new <kind> --title ...`）或匯入舊來源 `hippo import --from autopilot-backlog|adr-dir|handoff|pm-ledger`（`README.md:12,17`）。
- **輸出**：git 版控的 `.hippo/` store（一筆一檔、op log 為唯一權威、可重建本機索引），支援 `hippo export --json`（`README.md:3,11,16`、`CLAUDE.md:36-38`）。
- **觸發**：由團隊 agent / `autopilot`（`scripts/project-record.js`）在寫入 backlog、handoff、decision、lesson 時透過 CLI 主動呼叫（`CLAUDE.md:5-7`）。
- **Owner**：`hippo`。
- **不做**：不直接掃描或解析 session transcript（`README.md:10-18`、`CLAUDE.md:5-10`）；跨專案彙整與語意搜尋交由 mnemos（`CLAUDE.md:5`）。
- **證據**：`README.md:1-21`、`CLAUDE.md:1-13,36-40`。

## 3. codeforge L0→L1 記憶管線與 L2 `codeforge ship` 日帳

- **輸入**：L0 接收手動筆記 `codeforge learn`（`src/cli/learn.rs:33-35`）、`dream absorb` 收編當前專案 `~/.claude/projects/<slug>/memory/*.md`（`src/dream/absorb.rs:33-37`）與 `ingest-digests` 吸入 session digest（`src/dream/runner.rs:30-37`）；L2 `codeforge ship` 讀 `.codeforge/store/concepts/*.md` 與當日 `git log`（`doc/specs/codeforge-ship.md:17`、`src/mnemos/CLAUDE.md:16-18`），`autocite` 掃當日 `~/.claude/projects/<repo-slug>/*.jsonl`（`src/mnemos/CLAUDE.md:24`）。
- **輸出**：L0 寫入 `.codeforge/store/signals/<date>.jsonl`（經 `mask_secrets`）（`src/memory/l0.rs:70-83`）；`dream compile` 寫入 `.codeforge/store/{concepts,connections,qa}/*.md`（`src/memory/l1.rs:15-21`、`src/dream/compile.rs:1-4`）；`ship` POST L2 ledger 至 Mnemos `/v1/ingest/ledger`，失敗寫 `~/.codeforge/ship-failed/`（`src/mnemos/CLAUDE.md:20,28`）。
- **觸發**：`codeforge learn` 手動；`dream --quiet` 接 `ship --no-hook` 掛於全域 `~/.claude/settings.json` 的 `SessionEnd` hook（`src/mnemos/CLAUDE.md:16,30`）。
- **Owner**：`codeforge`。
- **不做**：`ship` 目前**不掃** session jsonl 產 lesson（`SourceEvidence::session_jsonl` 僅為設計目標、`#[allow(dead_code)]`）（`doc/specs/codeforge-ship.md:17`、`src/mnemos/CLAUDE.md:20`）；`dream absorb` 不跨專案讀取別的 `<slug>/memory`（`src/dream/absorb.rs:33-37`）。
- **證據**：`src/cli/learn.rs:1-45`、`src/memory/l0.rs:1-100`、`src/memory/l1.rs:1-42`、`src/dream/runner.rs:15-50`、`src/dream/absorb.rs:1-40`、`doc/specs/codeforge-ship.md:9-22,27-31`、`src/mnemos/CLAUDE.md:14-31`。

## 4. autopilot 自動從 transcript 抓教訓的 hook

- **輸入**：`hooks/hooks.json` 查無自動從 transcript 萃取「教訓／knowledge／lesson」的 hook；最接近者為 `session-handoff.js`（讀當次 `transcript_path` 產生交接摘要）與 `past-reference-recall.js`（讀 `transcript_path` 與 hippo records 做關鍵詞召回）。
- **輸出**：`session-handoff.js` 寫一次性交接狀態至 `~/.autopilot/handoff/`（`hooks/session-handoff.js:14-15,38-39`）；`past-reference-recall.js` 僅注入 stdout 提示（`hooks/past-reference-recall.js:5-7,54`）。
- **觸發**：`SessionEnd`（`session-handoff.js`、`hooks/hooks.json:65`）與 `UserPromptSubmit`（`past-reference-recall.js`、`hooks/hooks.json:99`）。
- **Owner**：`autopilot`。
- **不做**：不將 transcript 內容自動寫入 `.claude/knowledge/`、`SKILL.md` 或 hippo `lesson`（未找到任何寫入教訓的 hook）。
- **證據**：`hooks/hooks.json:59-128`、`hooks/opt-in-manifest.json:1-27`、`hooks/session-handoff.js:1-40`、`hooks/past-reference-recall.js:1-18`。

---

## 重疊表

| 重疊項目 | 零件 A | 零件 B | 說明 |
|---|---|---|---|
| **手動記一筆教訓／筆記** | autopilot `learn` skill（`skills/learn/SKILL.md:3-10`） | codeforge `codeforge learn`（`src/cli/learn.rs:33-42`） | 兩者皆由人／agent 主動觸發記錄單筆經驗，分別寫入 `.claude/knowledge/`（或 `memory_dir`）與 `.codeforge/store/signals/`。 |
| **從歷史／當日記憶產出彙整知識** | autopilot `distill`（`skills/distill/SKILL.md:3-6`） | codeforge `dream compile` + `ship`（`src/dream/compile.rs:4`、`doc/specs/codeforge-ship.md:31`） | 兩者皆把零散訊號壓縮為長期知識，但 `distill` 產出可執行 `SKILL.md`，`dream`/`ship` 產出 L1 markdown 與 L2 ledger。 |
| **SessionEnd 讀取當次 transcript** | autopilot `session-handoff.js`（`hooks/session-handoff.js:2-18`） | codeforge `ship` autocite（`src/mnemos/CLAUDE.md:24`） | 兩者皆在 SessionEnd 讀本席 `.jsonl` transcript，但用途不同（交接摘要 vs. atom 引用計數）。 |

## 缺口表

| 缺口項目 | 現況說明 | 證據 |
|---|---|---|
| **跨 lane／跨機器知識去重與升級** | `distill` 僅靠 slug 正規化收斂（`skills/distill/SKILL.md:143-148`），codeforge L1 去重限單一 `.codeforge/store/`（`src/dream/compile.rs:17-20`）；無人自動把多 lane 重複教訓升級成共用 skill。 | `skills/distill/SKILL.md:143-148`、`src/dream/compile.rs:17-20` |
| **autopilot 自動從 transcript 抓教訓的 hook** | `hooks/hooks.json` 無任何自動萃取 lesson／knowledge 並落盤的 hook，全靠手動 `/learn` 或 `/distill`。 | `hooks/hooks.json:59-128`、`hooks/opt-in-manifest.json:1-27` |
| **`codeforge ship` 尚未讀 session jsonl 產 lesson** | `SourceEvidence::session_jsonl` 仍為 `#[allow(dead_code)]`，`ship` 只讀 L1 與 `git log`。 | `doc/specs/codeforge-ship.md:17`、`src/mnemos/CLAUDE.md:20` |
| **統一的隱私／外洩審查關卡** | autopilot 有 `identifier-scan.js` + `AskUserQuestion` 人審（`skills/distill/SKILL.md:102-136`）；codeforge L0 僅做 `mask_secrets`（`src/memory/l0.rs:82`），L1/L2 無人審關卡。 | `skills/distill/SKILL.md:102-136`、`src/memory/l0.rs:82` |

## 讀取範圍表

| 零件 | 實際讀取哪些 transcript | 內容或只有統計／摘要 | 授權依據（程式或文件明載） |
|---|---|---|---|
| **autopilot `distill-scan.js`** | 本機 `~/.claude/projects/<encoded-cwd>/*.jsonl`（`--real-only` 過濾 `/tmp`）（`scripts/distill-scan.js:30-33`） | 僅頻率統計 atoms（指令 n-gram、tool／slash 次數、雙語摩擦詞命中），不輸出原文（`scripts/distill-scan.js:5-7`） | `skills/distill/SKILL.md:102-106`（人審關卡）；未找到獨立授權卡 id。 |
| **autopilot `session-handoff.js`** | 僅 hook 傳入的當次 `transcript_path`（`hooks/session-handoff.js:71-80`） | 摘要文字與最後動作，寫入 `~/.autopilot/handoff/` 後即焚（`hooks/session-handoff.js:14-15,36`） | 未找到授權卡 id。 |
| **autopilot `past-reference-recall.js`** | 僅 hook 傳入的當次 `transcript_path`（明確註明不用 `transcript-reader-lib.js` 掃 `~/.claude`）（`hooks/past-reference-recall.js:12-14`） | 僅 user／assistant 純文字關鍵詞命中片段（不含 tool results）（`hooks/past-reference-recall.js:12-13`） | 未找到授權卡 id。 |
| **codeforge `dream absorb`** | **不讀 transcript**；僅讀當前專案 `~/.claude/projects/<slug>/memory/*.md`（`src/dream/absorb.rs:33-37`） | Markdown 記憶全文轉 L0 signal | `src/dream/absorb.rs:33` 註明 `PM 01M7QZ9Y`（限制只讀當前專案，避免跨專案污染）。 |
| **codeforge `ship`（含 `autocite`）** | `ship` 本體不讀 transcript；`autocite` 掃當日 `~/.claude/projects/<repo-slug>/*.jsonl`（`src/mnemos/CLAUDE.md:20,24`） | 全文比對 candidate atom 標題以回報引用（`src/mnemos/CLAUDE.md:24`） | `src/mnemos/CLAUDE.md:24,30`（`B18`、Mnemos opt-in gate）；未找到其他授權卡 id。 |
| **hippo** | 不讀任何 transcript | — | — |
