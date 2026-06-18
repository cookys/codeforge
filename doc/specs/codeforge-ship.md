# CodeForge Ship — L2 Ledger Producer

> Status: **SHIPPED (v0.0.4)**，部分段落與實作有出入 — 見下方 ⚠️ · Owner: cookys · Created: 2026-05-15 · Expanded: 2026-06-10 · Audited: 2026-06-18
>
> 此 spec 是 `codeforge ship` subcommand 的實作依據。Payload schema 以 **Mnemos 端 `crates/mnemos/src/api.rs` ledger handler 實際解析的欄位為準**（contract truth，見 §4.1 對照表），不是憑空想像。
>
> Mnemos 對應 spec：[`cookys/mnemos/docs/specs/10-source-contract.md`](#) §5.1（`source: codeforge_ledger`）。

> ### ⚠️ As-shipped corrections (audit 2026-06-18)
>
> 此 spec 寫於實作前，部分段落與 `src/cli/ship.rs` / `src/mnemos/*` 實際行為有出入。canonical = code。逐項：
>
> **(a) STALE — LLM backend（§6 / §6.3）**：digest 主力已是 `claude -p` headless（預設 Opus，免 key），鏈為 `claude -p` → `ANTHROPIC_API_KEY`(Haiku) → rule-based。§6/§6.3 把 Haiku 當主、「無 key 即 passthrough」的描述已過時（§5.2 已自我修正）。`provenance.haiku_model` 永遠硬寫 `HAIKU_MODEL` 常數，不反映實際 backend/model。
>
> **(b) STALE — date window（§5 / §3.1）**：git log 實際用 `--since=<date> 00:00 --until=<date> 23:59`（同日 23:59 截止），非到次日 00:00 的半開區間。
>
> **(c) NOT YET IMPLEMENTED — session jsonl 輸入（§5 表格 + §6.1 [session 線索] block）**：ship **目前不掃 session jsonl**。`build_lessons` 只讀 L1 + git；`build_prompt` 無 session-hints 區塊；`SourceEvidence::session_jsonl` 是 `#[allow(dead_code)]`。設計目標、code 未跟上 → BACKLOG B18。
>
> **(d) NOT YET IMPLEMENTED — provenance 擴充欄位（§4 範例 + §4.3 表）**：`build_provenance` 實際只產 `{lesson_count, l1_concept_files, git_head_sha, git_branch, haiku_model}`。`raw_signal_count` / `source_jsonl_paths` / `digest_cost_usd` 皆未實作（`ctx` 未用於 query db metrics）→ BACKLOG B18。
>
> **(e) CODE LAGS DESIGN — ship-state（§7.6）**：spec 要「已 ship 過 → skip digest + POST」，但 code 先跑完整 `build_lessons`(digest) 才查 `already_shipped`，只 skip POST（浪費一次 LLM 呼叫）。設計較佳 → BACKLOG B18 修 code。

---

## 1. 動機

CodeForge 既有 memory pipeline 是 L0 raw signals → Haiku → L1 compiled markdown，留在 `.codeforge/store/` 本地用（見 `src/memory/l0.rs`、`src/memory/l1.rs`、`src/dream/compile.rs`）。

但 L1 是 per-session per-repo 的本地知識，沒有跨 repo aggregation 也沒有送進中央 brain。

**`codeforge ship`** 把 L1 + git log + 今日 session jsonl 再 digest 成 L2 daily ledger，POST 到 Mnemos（`POST /v1/ingest/ledger`），讓 Mnemos 端把 ledger 存成 1 個 Document、每個 lesson 變成 1 個 atom，進入跨 repo / 跨 session 的 active memory。

---

## 2. 角色定位

`codeforge ship` 不是玩具子命令，是 **CodeForge 作為 Mnemos source 的職責**。Day-1 就是 critical path（不是 nice-to-have）— SessionEnd hook 觸發、失敗有 retry policy、結果影響 Mnemos 資料完整性。

但實作上仍然只是 codeforge binary 的一個 subcommand（不另外開新 binary，依照 spec discussion 拍板的 single-binary 原則）。

---

## 3. CLI Surface

```
codeforge ship                    # 預設：digest 今日 (UTC)，POST 到 §6 解析出的 endpoint
codeforge ship --date 2026-05-15  # 指定日期 (re-ship 同一日不覆寫，Mnemos 端 dedup by ship_id)
codeforge ship --dry-run          # 印出完整 ledger envelope JSON 到 stdout，不送、不寫 ship-failed/、不寫 ship-state
codeforge ship --no-hook          # hook 模式：失敗時靜默寫 ship-failed/ 但不重試、不阻塞 SessionEnd
codeforge ship --resend           # 只重送 ~/.codeforge/ship-failed/*.json，不重新 digest 今日
```

### 3.1 Flag 語義精確定義

| Flag | 行為 |
|---|---|
| (無) | digest 今日 → 產 envelope → POST（含 §7 retry）→ 成功記 ship-state、失敗寫 ship-failed/。**且**先掃 ship-failed/ 補送（順帶 piggyback，見 §7.4） |
| `--date <YYYY-MM-DD>` | 改 digest 指定日（git log / jsonl 窗口改為該日 UTC 00:00–次日 00:00）。其餘同預設 |
| `--dry-run` | 走完整 digest，把 envelope JSON pretty-print 到 stdout 後 **exit 0**。不做任何網路或 disk side-effect。用於人工 review payload |
| `--no-hook` | 給 SessionEnd hook 用。POST 走 **單次嘗試**（不跑 §7 backoff，避免拖慢 session 結束）；失敗直接寫 ship-failed/ 後 exit 0（hook 永不因 ship 失敗而報錯）。**不**順帶補送 ship-failed/（避免 hook 啟動延遲） |
| `--resend` | 跳過今日 digest，只把 `~/.codeforge/ship-failed/*.json`（已是完整 envelope）依 `ship_at` 升序逐一重送。每筆走 §7 retry。成功則刪除該 failed 檔 |

`--dry-run` 與 `--resend` 互斥（resend 沒有「要 print 的新 digest」）；同時指定報 usage error。

---

## 4. 輸出 Schema（Ledger Envelope + Payload）

CodeForge 端 POST 的 body 是一個 **envelope**，內含 source-specific **payload**。

```json
{
  "ship_id": "01J0ZX9K8R7QABCDEF0123456",
  "source": "codeforge_ledger",
  "ship_at": "2026-05-15T08:23:01Z",
  "machine_id": "main-linux-blackwell",
  "payload": {
    "ledger_date": "2026-05-15",
    "repo": "codeforge",
    "lessons": [
      {
        "title": "Notify::notify_waiters 會丟 wakeup",
        "detail": "tokio shutdown 信號用 mpsc::channel(1) 而非 Notify::notify_waiters；後者在沒有 waiter 時靜默丟訊號，shutdown 偶發 hang。",
        "candidate_atom_kind": "lesson",
        "source_evidence": [
          {
            "kind": "session_jsonl",
            "value": "~/.claude/projects/-home-cookys-projects-codeforge/4e7c....jsonl",
            "locator": { "uuid": "4e7c...", "ts": "2026-05-15T05:41:12Z", "line": 318 }
          }
        ]
      }
    ],
    "provenance": {
      "raw_signal_count": 142,
      "source_jsonl_paths": [
        "~/.claude/projects/-home-cookys-projects-codeforge/4e7c....jsonl"
      ],
      "l1_concept_files": [
        ".codeforge/store/concepts/ipc-shutdown.md"
      ],
      "git_head_sha": "64a0f2c1d...",
      "git_branch": "main",
      "haiku_model": "claude-haiku-4-5-20251001",
      "digest_cost_usd": 0.013
    }
  }
}
```

### 4.1 欄位對照：CodeForge 產出 ⇄ Mnemos 實際解析（contract truth）

來源真相：`crates/mnemos/src/api.rs` 的 `LedgerEnvelope` / `LedgerPayload` / `LedgerLesson` structs（約 L249–L386）。Mnemos 用 plain `#[derive(Deserialize)]`（**未** `deny_unknown_fields`），所以**未列出的欄位 serde 會靜默丟棄、不報錯**。

#### Envelope 層

| 欄位 | Mnemos 是否解析 | Mnemos struct 欄位 | 說明 |
|---|---|---|---|
| `ship_id` | ✅ 解析 | `LedgerEnvelope.ship_id: String` | **必填**。必須是 valid ULID（Mnemos `Ulid::from_string` 驗，失敗回 400 `bad_request`）。冪等 key |
| `ship_at` | ✅ 解析（但忽略） | `LedgerEnvelope.ship_at: String`（`#[serde(default)]`） | Mnemos 讀進來後 `let _ = &env.ship_at;` — **解析但不使用**。CodeForge 仍要送（ISO 8601 UTC 打包時間），供 log / 未來用、也供 ship-failed/ 重送排序 |
| `source` | ❌ 不解析（丟棄） | — | 不在 struct。endpoint path `/v1/ingest/ledger` 已決定 source。**仍建議送** `"codeforge_ledger"` 以符合 spec 10 §3 envelope 慣例與人工除錯可讀性，但對 Mnemos 行為零影響 |
| `machine_id` | ❌ 不解析（丟棄） | — | 見 §4.2 結論 |
| `payload` | ✅ 解析 | `LedgerEnvelope.payload: LedgerPayload` | **必填** |

#### Payload 層

| 欄位 | Mnemos 是否解析 | Mnemos struct 欄位 | 說明 |
|---|---|---|---|
| `ledger_date` | ✅ 解析 | `LedgerPayload.ledger_date: String` | **必填**。空字串回 400 `payload_validation`。用作 `source_id` 一部分、`documents.created_at = <date>T00:00:00Z`、atom `when_hint` |
| `repo` | ✅ 解析 | `LedgerPayload.repo: String`（`#[serde(default)]`） | 用作 `source_id` 一部分；非空時當 atom `entities`。可空（空則 source_id 前綴為空、atom 無 entity） |
| `lessons` | ✅ 解析 | `LedgerPayload.lessons: Vec<LedgerLesson>`（`default`） | **ship 的核心 payload**。每筆非空 lesson → 1 lesson atom。見下方 lesson 層 |
| `provenance` | ✅ 解析（原樣保留） | `LedgerPayload.provenance: serde_json::Value`（`default`） | Mnemos 不解讀內部結構，整包塞進 document 的 `ref_locator_json.provenance`。**CodeForge 可放任意 JSON**（§4.3 建議內容） |

#### Lesson 層（`payload.lessons[]`）

| 欄位 | Mnemos 是否解析 | Mnemos struct 欄位 | 說明 |
|---|---|---|---|
| `title` | ✅ 解析 | `LedgerLesson.title: String`（`default`） | atom title。若空，Mnemos fallback 取 `detail` 前 40 字元 |
| `detail` | ✅ 解析 | `LedgerLesson.detail: String`（`default`） | atom body |
| `source_evidence` | ✅ 解析（原樣保留） | `LedgerLesson.source_evidence: serde_json::Value`（`default`） | **citation locator**。見 §4.4。Mnemos 收集所有非 null 的 `source_evidence` → `ref_locator_json.lessons_source_evidence[]`。api.rs 註明：缺它則 atom 無法 open 回原始 session，**違反 citations-mandatory** |
| `candidate_atom_kind` | ✅ 解析 | `LedgerLesson.candidate_atom_kind: Option<String>`（`default`） | atom kind 提示。Mnemos Sprint 0 用 `unwrap_or("lesson")`。CodeForge 預設送 `"lesson"` |

> **空 lesson 規則**：Mnemos 對 `title` 與 `detail` 皆空的 lesson 直接 `continue`（不建 atom）。CodeForge 不應送 title+detail 皆空的 lesson。

### 4.2 `machine_id` 去留結論

**結論：保留在 envelope，但標記為 cosmetic / forward-compat，不依賴它有任何效果。**

理由：
- Mnemos `LedgerEnvelope` struct **沒有** `machine_id` 欄位 → 今天送了也是 serde 丟棄、零行為影響。
- 但 Mnemos spec 10 §3 envelope 通用表把 `machine_id` 列為 `optional`（多機 dev 環境識別用），代表這是 envelope-level 的**保留欄位**，未來 Mnemos 可能加進 struct。
- CodeForge 端產生成本趨近於零（一次 hostname/config 讀取）。先送，等 Mnemos 真的解析時不必改 CodeForge。
- **不**把任何 CodeForge 行為（如 dedup、ship-failed 命名）綁在 machine_id 上 — 它純粹是 metadata。

值來源：`~/.config/mnemos.env` 的 `MACHINE_ID`（若有）→ fallback `hostname` → fallback `"unknown"`。

### 4.3 `provenance` 建議內容（Mnemos 原樣保留，不解讀）

> ⚠️ 部分欄位**未實作**（見頂部 banner (d)）：`build_provenance` 實際只產 `{lesson_count, l1_concept_files, git_head_sha, git_branch, haiku_model}`。`raw_signal_count` / `source_jsonl_paths` / `digest_cost_usd` 未實作；`haiku_model` 永遠硬寫 `HAIKU_MODEL` 常數（不反映實際 backend）→ BACKLOG B18。

`provenance` 整包進 `documents.ref_locator_json.provenance`，是 ledger 級的「這份 digest 是從哪些 raw 來源熬出來的」稽核資料。建議放：

| key | 值 | 用途 |
|---|---|---|
| `raw_signal_count` | int | 當日 L0 signal 數（從 `.codeforge/codeforge.db` 算） |
| `source_jsonl_paths` | string[] | 當日掃到的 session jsonl 路徑（去重） |
| `l1_concept_files` | string[] | 餵進 digest 的 L1 檔相對路徑 |
| `git_head_sha` | string | digest 當下 repo HEAD（取代舊 stub 的 top-level `git` 物件 — Mnemos 不解析 top-level `git`，放進 provenance 才會被保留） |
| `git_branch` | string | 同上 |
| `haiku_model` | string | digest 用的模型 id |
| `digest_cost_usd` | float | 本次 digest 的 token 成本 |

> 注意：舊 stub §4 的 `repo_path` / 頂層 `git` / `shipped` / `struggled` / `rabbit_holes` / `metrics` **全部不被 Mnemos 解析**。若仍想保留稽核價值，把需要的部分塞進 `provenance`（如上）；否則不送。本 spec 採「精簡 payload」原則：只送 Mnemos 會用的欄位 + provenance 稽核包，不送會被丟棄的裝飾欄位（`source`/`machine_id` 例外，理由見 §4.1/§4.2）。

### 4.4 `source_evidence` 結構（鎖死定義）

Mnemos 端把 `source_evidence` 當不透明 `serde_json::Value` 保留（只 filter null）。**結構由 CodeForge 鎖死**，因為它是 atom 日後 open 回原始 session 的唯一線索（citations-mandatory）。

**定義：`source_evidence` 是一個 array，每個 element 對齊 Mnemos RETRIEVAL_SPEC §2.1 的 ref shape（`kind` / `value` / `locator`）：**

```json
[
  {
    "kind": "session_jsonl",
    "value": "~/.claude/projects/-home-cookys-projects-codeforge/<uuid>.jsonl",
    "locator": { "uuid": "<uuid>", "ts": "2026-05-15T05:41:12Z", "line": 318 }
  }
]
```

| 欄位 | 必填 | 型別 | 說明 |
|---|---|---|---|
| `kind` | ✅ | string enum | 出處類型。Sprint 1 支援：`session_jsonl`（Claude session transcript）、`git_commit`（commit hash）、`l1_concept`（L1 markdown 檔）。未來可擴 |
| `value` | ✅ | string | 主識別。`session_jsonl`→jsonl 絕對/`~` 路徑；`git_commit`→full sha；`l1_concept`→相對 repo 路徑 |
| `locator` | optional | object | 精確定位。`session_jsonl`→`{uuid, ts, line}`（`ts` 是該 lesson 對應 message 的 ISO 8601；保留在這裡而非上拉到 document，對齊 spec 10 §5.1 rule 1）；`git_commit`→`{}` 或省略；`l1_concept`→`{topic}` |

規則：
- 每個 lesson **至少一個** `source_evidence` element（否則違反 citations-mandatory；digest 若無法歸因，該 lesson 應被丟棄而非送出空 evidence）。
- array 允許多 element（一個 lesson 由多個 raw 出處綜合而來，例如 session jsonl + 對應 commit）。
- `kind` 值與 RETRIEVAL_SPEC ref shape 對齊，未來 Mnemos 若把 `source_evidence` 升級成 typed 結構，CodeForge 端不必改。

---

## 5. Source 讀取（L1 Reading Strategy）

> ⚠️ 本節含**未實作的設計**（見頂部 banner (b)(c)(d)）：ship 實際只讀 **L1 + git log**。Session jsonl 掃描、`.codeforge/codeforge.db` metrics query、次日 00:00 半開窗口（實為同日 23:59 閉區間）皆未實作 → BACKLOG B18。下文表格保留為設計稿。

`codeforge ship` digest 的輸入來自四個本地來源。讀取窗口由 `ledger_date`（預設今日 UTC）決定：`[<date>T00:00:00Z, <date+1>T00:00:00Z)`。

| 來源 | 路徑 | 讀取方式 | 用途 |
|---|---|---|---|
| L1 concepts | `.codeforge/store/concepts/*.md` | `l1::scan_all()`（已存在），過濾 `frontmatter.updated == ledger_date` 或 `created == ledger_date` | **主要 lesson 來源**（已 Haiku-compiled，digest 階段做 paraphrase + 選材） |
| L1 connections | `.codeforge/store/connections/*.md` | 同上 | 關係型 lesson（`candidate_atom_kind` 仍送 `lesson`，Sprint 0 Mnemos 只有 lesson kind） |
| Git log | `git log --since="<date> 00:00" --until="<date+1> 00:00" --pretty=...` | 子程序 | 當日 commits → digest 的 shipped 上下文 + `source_evidence` 的 `git_commit` 候選 + `provenance.git_head_sha` |
| Session jsonl | `~/.claude/projects/<repo-slug>/*.jsonl`，mtime 落在窗口內 | 逐行掃 | digest 上下文 + `source_evidence` 的 `session_jsonl` locator（uuid/ts/line） |
| `.codeforge/codeforge.db` | 本地 SQLite | query | `provenance.raw_signal_count` 等 metrics |

### 5.1 L1 → lessons reduce 演算法

1. `scan_all(store_dir)` 取全部 L1 entries；過濾出 `updated` 或 `created == ledger_date` 的 active（`status == "active"`）entries。
2. 若當日 L1 entries 為空 → ledger 仍可送（`lessons: []`），但 §7.5 會記 warn「empty ledger」。預設行為：**空 lessons 不 POST**（避免在 Mnemos 製造無 atom 的空 document），除非 `--dry-run`。
3. 每個入選 L1 entry → 餵進 §6 Haiku digest，產出 0..N 個 lesson（一個密集的 concept 檔可能拆成多個 lesson；瑣碎的可能被丟）。
4. 每個產出 lesson 的 `source_evidence` 至少帶該 L1 檔的 `l1_concept` ref；digest 若能對應到 session jsonl 行或 commit，additional ref 一併帶上。
5. 去重：同 title 的 lesson 合併 detail（避免多 concept 檔講同件事重複成多 atom）。

### 5.2 來源邊界：ledger 只收第一人稱本 repo coding 經驗（2026-06-17 收嚴）

ledger 是中央 Mnemos 腦的 episodic 輸入,**只該收「這個 repo、第一人稱、真實 coding 經驗」**,不該夾帶別人/別專案的記憶。實作對齊大腦記憶機制(海馬只選擇性編碼第一人稱經驗、不回灌、不無差別倒灌；見 `mnemos/docs/research/2026-06-brain-memory-model/`)。

- **`origin` 純度標記**:L1 concept frontmatter 帶 `origin`(`session`=session-digest 萃取 / `dev`=手動 learn 等本 repo / `absorbed`=`dream absorb` 吸的跨專案 `~/.claude/projects/*/memory`)。`select_l1_for_date` **排除 `origin == "absorbed"`** → absorb 的跨專案 memory 不進 ledger(仍留 codeforge 自己 MUD 用)。
- **session-digest 接入(真料來源,A′ 落地 2026-06-17)**:SessionEnd/PreCompact hook(`session-digest.js`)從 transcript 萃的 high-confidence error-recovery/user-correction/self-correction,**落在 per-repo `<repo>/.codeforge/digests/<date>-<sid8>.json`**(從 cwd 往上找最近 `.codeforge`,像 git 找 `.git`;**找不到即 skip 不寫** → 非 codeforge 專案的 session 完全不在磁碟留明文,隱私根本解)。`dream` 第一個 step `ingest-digests` 掃本 repo 的 `digests/`(per-repo 天然隔離,**無需 cwd filter**)→ 以 `SignalSource::SessionDigest` 寫進 signals(compile 後 `origin="session"`,ship 收)→ **ingest 完刪 digest 檔**(明文不長存;刪失敗才退回標 `processed` 避免重收)。
  - **per-repo opt-out**:`<repo>/.codeforge/no-ship` 標記存在 → 此 repo dev signal 完全不進腦(ingest-digests 跳過 + `codeforge ship` 硬 no-op)。補 A′ 落點只擋「沒 init」的洞,讓「init 過但私人」的 repo 也不外送。gitignored runtime → 逐機各自設。
  - **過渡相容**:`ingest-digests` 過渡期同時掃舊全域 `~/.claude/session-digests/`(收嚴前 hook 寫的,全機共用 → 仍套 `cwd == 本 repo` filter),讀完即刪;別 repo 未處理的檔不動。30 天舊 digest 自然過期後可移除此段與全域目錄。
  - **`.gitignore`**:`**/.codeforge/digests/` 確保明文片段絕不 commit(consumer repo 亦然)。
- **day-1 backfill**:對既有歷史 transcript 跑 `session-digest.js`(吃 `transcript_path` + `cwd`,cwd 須為對應 repo root 才會落 per-repo `digests/`)產 digest → 下次 `dream` 的 `ingest-digests` 吸入。需先確認 transcript 保留期(見 `mnemos/docs/projects/HANDOFF-conversation-retention.md`)。
- **防回灌/防重複**:不靠路徑黑名單,靠 Mnemos 端既有離峰 `dedup --scan`(= 睡眠鞏固);相似 atom 合併,mnemos 自身記憶若漏入也在此收斂。
- **Cutover(收嚴前的舊資料)**:`origin` 欄是收嚴時新加。(a)**在途 signal**:收嚴前 `absorb` 標 `ClaudeCodeSession` 的未編譯 backlog,`origin_for` 已把 `ClaudeCodeSession` 一併判為 `absorbed`(該 variant 現已無其他生產者)→ 一編譯即被擋,免逐 repo 手清。(b)**已編譯舊 concept**(frontmatter 無 `origin` → 反序列化為 `""` → 不被 `!= "absorbed"` 擋)**不回溯**:各機上線前應清 per-machine 的 `<repo>/.codeforge/store/*`(gitignored runtime,重跑 `dream` 即以正確 origin 重建),否則首輪靠 Mnemos `dedup --scan` 收斂。
- **隱私**:`ingest-digests` 對 error/correction 等值套 `mask_secrets`(手刻 prefix + 高熵啟發式,遮 sk-/ghp_/AKIA/AIza/xox… 與 32+ 高熵字串;路徑不遮);session-digest content 屬第一人稱 dev = work-tier,另依賴 Mnemos 端 sensitivity 分層。
- **LLM backend(2026-06-17)**:digest 與 compile **優先走 `claude -p` headless**(codeforge 是 Claude Code 套件 → 免 API key、用訂閱、零 per-token 成本)。預設 model `opus`(env `CODEFORGE_DIGEST_MODEL`;bake-off 實測 ship digest prompt × haiku/sonnet/opus,**Opus 品質最高** —— title/detail/source_evidence 最正規,Haiku 會丟教訓+編假 evidence)。fallback 鏈:`claude -p` → `ANTHROPIC_API_KEY`(Haiku API)→ rule-based passthrough。`absorbed` 來源走 rule-based(ship 排除,不浪費 LLM)。模組 `src/llm.rs`;§6「Haiku Digest Pipeline」的直接 API 呼叫退為 fallback。

---

## 6. Haiku Digest Pipeline

> ⚠️ 標題與下文以 Haiku 為主述為**過時用詞**（見頂部 banner (a)）。實際 backend 三層：`claude -p`（預設 Opus，免 key，主力）→ `ANTHROPIC_API_KEY`(Haiku) → rule-based。「Haiku」一詞保留為歷史標題。

把「當日 L1 entries（已 compiled markdown）+ git log 摘要 + session jsonl 線索」reduce 成結構化 `lessons[]`。沿用既有 `src/dream/compile.rs` 的 Anthropic API 呼叫模式（`claude-haiku-4-5-20251001`、`x-api-key`、`anthropic-version: 2023-06-01`、抽 JSON）。

> Provider = Anthropic Claude（Haiku）。若 `ANTHROPIC_API_KEY` 未設，fallback 為 rule-based passthrough（見 §6.3），與 compile.rs 既有 fallback 哲學一致。

### 6.1 Prompt 草稿

System / user prompt（zh-TW，與既有 compile.rs 風格一致）：

```
你是 CodeForge 的 ledger digester。把今天這個 repo 的開發痕跡 reduce 成可長期保存的「技術教訓（lessons）」，準備送進中央記憶庫 Mnemos 變成 atom。

== 輸入 ==
repo: {repo}
日期: {ledger_date}
git HEAD: {git_head_sha} ({git_branch})

[當日 commits]
{git_log_oneline}   # 每行 "<short_sha> <subject>"

[當日 L1 知識條目]   # 已是 compile 過的 markdown，含 frontmatter topic / sources
{l1_entries_block}   # 每條: "### {topic}（檔: {rel_path}）\n{body 前 N 行}"

[session 線索]       # 用於歸因，不是內容來源
{session_hints}      # 每條: "{jsonl_rel_path} line {line} ts {ts}: <一句摘要>"

== 任務 ==
從上面萃取「值得跨 session / 跨 repo 記住」的技術教訓。每條教訓：
- title：一句話結論（≤ 40 字，可被未來的你一眼認出）
- detail：2–4 句，講清楚「為什麼」與「下次怎麼做」，不要只複述現象
- source_evidence：這條教訓的出處，array，每個 element 是 {kind, value, locator}
    - kind 用 "session_jsonl" | "git_commit" | "l1_concept"
    - 對得上 commit 就帶 git_commit；對得上 L1 檔就帶 l1_concept；對得上 session 行就帶 session_jsonl
    - 每條教訓至少一個 source_evidence；找不到任何出處的教訓請丟棄，不要硬湊

丟棄規則：流水帳、純情緒、無法歸因、和昨天重複的，一律不要。寧缺勿濫。

== 輸出 ==
嚴格 JSON，無多餘文字：
{
  "lessons": [
    {
      "title": "...",
      "detail": "...",
      "candidate_atom_kind": "lesson",
      "source_evidence": [
        { "kind": "l1_concept", "value": ".codeforge/store/concepts/xxx.md", "locator": { "topic": "xxx" } }
      ]
    }
  ]
}
若今天沒有任何值得保存的教訓，回傳 {"lessons": []}。
```

### 6.2 Prompt 設計要點

- **歸因強制**：prompt 明確要求每條 lesson 至少一個 `source_evidence`，且「找不到出處就丟棄」。這把 citations-mandatory 推到 digest 階段，而非寄望 Mnemos 補救（Mnemos 只 filter null，不會補）。
- **寧缺勿濫**：ledger 是長期記憶輸入，noise 成本高（每條變成永久 atom）。prompt 偏保守。
- **JSON 抽取**：沿用 compile.rs 的 `extract_json`（找首 `{` 末 `}`）。輸出 parse 失敗 → 視為 digest 失敗（§7.5），不送半截 payload。
- **多 lesson**：一個 prompt 一次出 0..N lesson（不是 per-concept 各呼叫一次，省 token、利於跨 concept 去重）。輸入過大時分批，結果再 merge + §5.1 去重。

### 6.3 No-API fallback

> ⚠️ **過時**（見頂部 banner (a)）：無 `ANTHROPIC_API_KEY` 並非直接 passthrough。若 `claude` CLI 可用，第一層 `claude -p`（Opus）仍會 digest；只有 `claude -p` 與 Haiku API 都不可用才 rule-based passthrough。

`ANTHROPIC_API_KEY` 未設時：不做 LLM digest，直接把當日 active L1 entries **一條一 lesson** passthrough（title=L1 title、detail=body 摘要、source_evidence=該 L1 `l1_concept` ref）。品質較低但仍 citable，且不阻塞 ship。記 warn。

---

## 7. Retry / Queue / Failure Policy

對齊 Mnemos spec 10 §4.3（client-side retry）+ §9.1。核心原則：**同一份 payload 永遠用同一個 `ship_id`，retry 不換 ULID**（否則 Mnemos dedup 失效，重複建 atom）。

### 7.1 ship_id 冪等

- 每次「新 digest」產生**一個** ULID 當 `ship_id`，寫進 envelope。
- 此 ULID 一旦寫進 ship-failed/ 檔，重送時沿用，**不重新產生**。
- re-ship 同一日（`--date` 重跑）會產生**新的** ship_id → Mnemos 視為新 ship。但因 Mnemos document 的 `source_id = <repo>:<date>:<ship_id>` 含 ship_id，兩次 re-ship 會建兩個 document。**避免重複 ship 同一日**；真要重送請用 ship-failed/ 機制（沿用原 ship_id）而非 `--date` 重跑。

### 7.2 Backoff schedule（預設模式 / `--resend`）

```
attempt 1 → 5xx / network error → wait 1s  → retry
attempt 2 → fail                → wait 5s  → retry
attempt 3 → fail                → wait 30s → retry
attempt 4 → fail                → 寫 ~/.codeforge/ship-failed/<ship_id>.json
```

- **4xx 不 retry**（payload 不對，重送也 4xx）：直接記 error、**不**寫 ship-failed/（壞 payload 重送無意義），exit 非 0。例外：`409`/`200 duplicate` 視為成功（Mnemos dedup 命中）。
- `200 { "status": "accepted" | "duplicate" }` → 成功。

### 7.3 `--no-hook` 模式

單次嘗試，無 backoff。失敗（5xx/network）→ 寫 ship-failed/ 後 exit 0（永不阻塞 SessionEnd）。4xx → 記 error、不寫 ship-failed/、exit 0。

### 7.4 ship-failed/ queue + piggyback resend

- 失敗檔：`~/.codeforge/ship-failed/<ship_id>.json`，內容是**完整 envelope**（含原 ship_id），可直接重 POST。
- 預設 `codeforge ship`（非 `--no-hook`、非 `--dry-run`）開頭先掃 `ship-failed/`，依 `ship_at` 升序逐一重送（走 §7.2 retry）；成功則刪檔。再 digest 今日。
- `--resend`：只做上述補送，不 digest 今日。
- `--no-hook`：跳過 piggyback（避免拖慢 hook）。靠下次互動式 `codeforge ship` 或排程 `--resend` 清 queue。
- flush 時遇 **4xx**（payload 被拒）：不刪檔，移到 `~/.codeforge/ship-rejected/` 隔離（silent-data-loss 紅線防護），繼續 flush 其餘。
- flush 時遇 **持續不可達（Exhausted）**：break 停止 flush 剩餘 queue（留待下次），不刪檔。

### 7.5 Digest 失敗（POST 之前）

- L1 讀取失敗 / Haiku digest 失敗 / JSON parse 失敗：記 error，**不送、不寫 ship-failed/**（沒有有效 payload）。exit 非 0（`--no-hook` 下 exit 0 + warn）。
- 空 lessons：依 §5.1 rule 2，預設不送。

### 7.6 ship-state（避免重複 ship 同一日）

> ⚠️ **CODE LAGS DESIGN**（見頂部 banner (e)）：下文「已 ship 過 → skip digest + POST」是設計順序；實作先跑完整 digest 才查 already_shipped，只 skip POST（digest 已浪費）→ BACKLOG B18。

- 成功 ship 後記 `~/.codeforge/ship-state.json`：`{ "<repo>": { "<ledger_date>": "<ship_id>", ... } }`。
- 預設模式 ship 今日前先查 state：若今日已成功 ship 過 → skip digest + POST（記 info「already shipped today」），但仍跑 ship-failed/ piggyback。`--date` 重跑同樣會被 state 擋（要強制請先手動刪 state entry）。`--dry-run` 不寫也不查 state。

---

## 8. Endpoint 解析

```
1. 讀 ~/.config/mnemos.env（若存在）：
     MNEMOS_INGEST_URL=http://127.0.0.1:8845   # base，可只給 host:port 或完整 base url
     MACHINE_ID=main-linux-blackwell           # 供 envelope.machine_id（§4.2）
     MNEMOS_TOKEN=...                           # 若 Mnemos 啟用 auth（Sprint 1 預設 localhost 無 auth）
2. ledger endpoint = <base>/v1/ingest/ledger
3. fallback（mnemos.env 缺 MNEMOS_INGEST_URL）：http://127.0.0.1:8845/v1/ingest/ledger
```

- 預設 local-first，bind `127.0.0.1`，對齊 Mnemos `:8845`（spec 10 §3.2 / CLAUDE.md §3.2）。
- `MNEMOS_TOKEN` 若有 → `Authorization: Bearer <token>`（Sprint 1 localhost 多半不需要，保留鉤子）。

---

## 9. Cite Subcommand（協作關係）

`codeforge mnemos-cli context|cite|cite-detect` 是另一個 subcommand。**已實作（v0.0.4）**；未拆出獨立 spec 檔（原規劃的 `codeforge-mnemos-cli.md` 從未建立）—— 行為見 CHANGELOG `[0.0.4]` 與 [`../concepts.md`](../concepts.md) §4。

> ⚠️ **NOT YET IMPLEMENTED — auto-cite-on-ship**：以下「ship 結束順便偵測 + 回寫」是**設計目標，尚未接線**。`ship` 不呼叫 cite，SessionEnd hook 鏈也不含 cite-detect（見 §10）。目前 citation 回寫只能**手動**跑 `codeforge mnemos-cli cite-detect <transcript>`。→ BACKLOG B18。

`ship` 與 `mnemos-cli cite` 的**設計**協作（待實作）：
- ship 結束時順便偵測「本 session transcript 是否引用任何 Mnemos atom」。
- 偵測到 → 對每個 atom 呼叫 `mnemos-cli cite <atom_id>`（write-back → Mnemos `atom.citation_count++`）。
- Sprint 1 用 fulltext_match heuristic；Sprint 5+ 改 Haiku。
- **confidence**：cite-detect（自動偵測）用 `0.5`；手動 `mnemos-cli cite <atom_id>` 用 `0.7`。

> 本 spec 只負責 `ship`（L2 ledger 產出）。cite write-back 的精確 contract 見 CHANGELOG `[0.0.4]` 的 `mnemos-cli cite` 條目與 Mnemos `10-source-contract.md` §11。

---

## 10. Hook 設定

`~/.claude/settings.json`：

```json
{
  "hooks": {
    "SessionEnd": [
      "codeforge dream --quiet",
      "codeforge ship --no-hook"
    ]
  }
}
```

`dream` 先跑（產出 / refresh L1），`ship --no-hook` 接著（讀當日 L1 → digest → 單次 POST，失敗靜默落 ship-failed/，永不阻塞 session 結束）。互動式 `codeforge ship` 或排程 `--resend` 負責清 ship-failed/ queue。

---

## 11. 端到端流程（Sprint 1 e2e 對齊 mnemos 20-sprint-0-2.md）

```
codeforge dream             # L0 → L1（既有）
codeforge ship --no-hook    # 當日 L1 + git + jsonl → Haiku digest → lessons[] → envelope
  → POST /v1/ingest/ledger (127.0.0.1:8845)
  → Mnemos：1 Document（source_id=<repo>:<date>:<ship_id>, ref_locator_json 帶 provenance + lessons_source_evidence）
            + 每 lesson 1 lesson-atom（evidence_refs = doc 三元組）
  → SessionStart hook 後續可 surface 這些 atom
  → codeforge mnemos-cli cite <atom_id> → atom.citation_count++
```

驗收（對齊 CLAUDE.md §5 Gen-2 e2e）：codeforge ship → mnemos ingest → atom extract → SessionStart 浮現 atom → cite write-back → citation_count++。

> **e2e 已實證**（2026-06-15,隔離 test DB）:`ship` → POST `/v1/ingest/ledger` → **200**,Mnemos `documents` 存入 ledger（`source=codeforge_ledger`, `source_id=<repo>:<date>:<ship_id>`）+ `atoms` 萃取 1 lesson-atom;`mnemos-cli cite <atom_id>` → **200**,`citation_count` 0→1、`last_cited_at` 寫入。精簡 payload（§4）無 contract mismatch。

### 上線（SessionEnd hook 鏈）

dream→ship 鏈安裝在 **global** `~/.claude/settings.json` SessionEnd（`codeforge install --hooks`/`--all`),跨所有專案 per-project 萃取（hook CWD=專案 root）。`ship --no-hook` 以 `MnemosConfig::opted_in`(`~/.config/mnemos.env` 存在或 `MNEMOS_INGEST_URL` 設定)gate:無 Mnemos 的 codeforge-only 使用者照常 dream 萃取、ship 乾淨 no-op。

---

## 12. 待實作清單（✅ 已完成 — 2026-06-15）

- ✅ `src/cli/ship.rs`：flag parsing（§3）、endpoint 解析（§8）、retry/queue（§7）。
- ✅ L1 reading（§5）：複用 `l1::scan_all` + 日期過濾 + git log / jsonl 窗口掃描。
- ✅ Haiku digest（§6）：複用 compile.rs API 呼叫 + JSON 抽取 + §6.1 prompt;無 key 時 rule-based passthrough。
- ✅ ship-failed/ + ship-state IO。
- ✅ `source_evidence` builder（§4.4 結構）。
- ✅ locales/{en,zh-TW}.yaml strings。
- ✅ unit tests：envelope 序列化、source_evidence 結構、retry backoff、dedup ship_id 不換。
- ✅ integration（手動 e2e）：真 server `/v1/ingest/ledger` → 200 accepted（見上 e2e 已實證）。
- ✅ SessionEnd hook 鏈上線（global） + `ship --no-hook` opt-in gate（本次新增,§11「上線」）。
- CLAUDE.md update（CodeForge 新增 ship 角色 + production critical path 註明）。
```
