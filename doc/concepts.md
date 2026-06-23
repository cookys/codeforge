# CodeForge 是怎麼運作的 — 概念與資料流

> 這份文件用白話講清楚 CodeForge 實際的運作方式：單機 vs 配合中央腦、`dream`/`ship` 記憶管線、養寵系統，以及跟 CodePower 的連結現況。
>
> 內容對照 source code 撰寫，明確區分「**今天真的有的**」與「**roadmap 規劃中**」。README 是快速上手、`doc/specs/*.md` 是技術規格，這份文件是中間那層 —— 給想理解「為什麼這樣設計」的人看。

---

## 1. 兩種運作模式：Solo vs Connected

CodeForge 的核心是**本機優先**（local-first）。所有東西在你自己的機器上就能跑，中央腦（Mnemos）是**選配**。

### Solo Smith（純單機，預設）

不需要任何外部服務。你 `learn`、`dream`、養寵、探索 codebase，全部寫進本機的 `.codeforge/`。

### Connected Smith（配合 Mnemos 腦）

額外把每天的知識 digest 成 L2 ledger，POST 到本機的 Mnemos daemon（中央 brain，匯集 coding / Slack / Email 等多來源）。這條路是 **opt-in** 的 —— 沒設定就完全不啟用，也不會留下任何 dead-letter 垃圾。

### 記憶迴圈的四格對照

記憶迴圈是 **absorb → distill → store → recall**。WRITE（寫入）與 READ（讀取）兩側，都遵循「**本機永遠跑 / 中央需 opt-in**」的對稱切分：

| | 本機（永遠，免 Mnemos） | 中央（opt-in，需 Mnemos） |
|---|---|---|
| **WRITE** | `dream`（L0 → L1 蒸餾） | `ship`（→ Mnemos L2 ledger） |
| **READ** | `codeforge memory context`（SessionStart 注入精簡 index） | `mnemos-cli context`（跨來源 atom recall） |

**關鍵保證**：一個只用 CodeForge、沒裝 Mnemos 的使用者，`dream` 照常蒸餾知識、養寵照常升級；`ship` 在 SessionEnd 是**乾淨的 no-op**（不 POST、不寫 dead-letter）。詳見 §3 的 opt-in gate。

---

## 2. 記憶的三層：L0 / L1 / L2

| 層 | 是什麼 | 存哪 | 由誰產生 |
|---|---|---|---|
| **L0** | raw signal（原始觀察） | `.codeforge/signals/YYYY-MM-DD.jsonl` | `learn` 指令、session hook、digest ingest |
| **L1** | compiled knowledge（蒸餾後的概念） | `.codeforge/store/{concepts,connections,qa}/*.md`（含 YAML frontmatter）+ SQLite FTS | `dream` |
| **L2** | daily ledger（每日帳本） | POST 到 Mnemos，不長存本機 | `ship` |

L0 是人類可讀、git 友善的純文字 append log；L1 是結構化、可全文檢索的知識；L2 是給中央腦的每日彙整。

---

## 3. `dream` — 本機蒸餾（L0 → L1）

`dream` 把累積的 raw signal 蒸餾成 compiled knowledge。**這是本機功能，不需要 Mnemos。**

### 觸發時機

- **自動**：全域 SessionEnd hook 在**每個專案**結束時跑 `codeforge dream --quiet`（`--quiet` 抑制 stdout，hook 必用）。hook 的 CWD = 專案根，所以蒸餾的是該專案自己的 `.codeforge`（per-cwd 記憶）。
- **手動**：`codeforge dream`。

### 讀什麼

L0 signals（含 `learn` 寫入的、session digest ingest 進來的）+ 既有 L1（FTS 去重對賬）。`compile` 本身不讀 SQLite metrics；signal cursor（已編譯標記）寫回 DB。

### LLM backend fallback 鏈（重要）

dream（與 ship）呼叫 LLM 時，走的是**三層 fallback**，不是單一把 key（AI commentary 是例外 —— 只有 2 層：有 `ANTHROPIC_API_KEY` 走 Haiku API、否則 rule-based，**不呼叫 claude -p**）：

```
1. claude -p headless     ← 主力。借用 Claude Code CLI 登入，免 API key，
                            吃訂閱額度，Opus 預設、品質最高
        ↓ 不可用時
2. ANTHROPIC_API_KEY      ← 備援。直接打 Haiku API，按 token 計費
        ↓ 不可用時
3. rule-based passthrough ← 最後保險。純規則、不呼叫任何 LLM、零外送
```

**所以 `ANTHROPIC_API_KEY` 是 optional，不是必要的** —— 只要你機器上有 `claude` CLI，第一層就夠用了。每一層降級都會印 warning 讓品質下降可見。

> 例外：`origin == "absorbed"` 的 signal（跨專案吸收的 `AbsorbedMemory`、過渡期 `ClaudeCodeSession` backlog）**跳過整條 LLM 鏈、直接走 rule-based** —— 它們是二手知識、不值得花 LLM，且不會被 ship。

> 設計理由：2026-06-17 的 bake-off 顯示 `claude -p`（Opus）品質明顯優於 Haiku（Haiku 會漏教訓、捏造 evidence），且免 per-token 費用，故列為主力。

### 寫什麼

L1 store 的 `.md` 檔（frontmatter 帶 `origin` 欄位：`session` / `absorbed` / `dev`）+ SQLite 的 `dream_runs` 紀錄 + signal cursor（標記已編譯，避免重複處理）。

---

## 4. `ship` — 配合中央腦（L1 → L2 → Mnemos）

`ship` 把當天的知識 digest 成 L2 ledger 送進 Mnemos。**這是 opt-in 的中央功能。** 它是 production critical path（每次 SessionEnd 跑、有 retry policy、影響 Mnemos 資料完整性），不是玩具。

### 觸發時機

全域 SessionEnd hook 在 `dream --quiet` 之後，緊接著跑 `codeforge ship --no-hook`（同一條鏈）。

### 讀什麼

當天的 active L1 concepts（`origin != "absorbed"`，只送本 repo 第一手 coding 經驗）+ git log（當天 commit，當作 commit evidence）。provenance 僅由 lessons + git head/branch 組成（`lesson_count` / `l1_concept_files` / `git_head_sha` / `git_branch` / `haiku_model`）—— 不 query SQLite db metrics。

> 註：spec `codeforge-ship.md` 另有「掃 session jsonl 取 source_evidence locator」的設計，但**目前實作尚未做**（`SourceEvidence::session_jsonl` 是 dead_code、digest prompt 無 session 線索 block）。ship 實際只吃 L1 + git。

### 流程

```
當天 L1 + git log
        ↓  LLM digest（同 §3 的 fallback 鏈：claude -p → Haiku → rule-based）
  lessons[]（每條需 ≥1 source_evidence，無 evidence 者丟棄）
        ↓  provenance.haiku_model 永遠硬寫 HAIKU_MODEL 常數（不反映實際 backend，即使走 claude -p/Opus）
        ↓  POST
  <base>/v1/ingest/ledger     （base 預設 http://127.0.0.1:8845，本機 Mnemos）
```

### opt-in gate（`MnemosConfig::opted_in`）

`ship --no-hook`（SessionEnd 路徑）動工前先檢查是否 opt-in：

- **opt-in 條件**：存在 `~/.config/mnemos.env`，**或**環境變數 `MNEMOS_INGEST_URL` 有設。
- **沒 opt-in**：`--no-hook` 直接 return（乾淨 no-op，不 POST、不寫 dead-letter）。所以 codeforge-only 使用者跑全域 SessionEnd 鏈時，dream 照常蒸餾、ship 安靜略過。
- **互動式 `codeforge ship`（無 `--no-hook`）**：忽略 gate —— 這是使用者主動動作，一律嘗試。

### retry policy

| 模式 | 行為 |
|---|---|
| **互動式**（預設） | 失敗 backoff 重試：1s → 5s → 30s，共 4 次 |
| **`--no-hook`**（SessionEnd） | 單次嘗試；失敗寫 `~/.codeforge/ship-failed/<ship_id>.json`，**永遠 exit 0**（絕不卡住 SessionEnd） |

`ship-failed/` 是 dead-letter queue：下次互動式 `ship` 會先 flush 它（成功即刪），`--resend` 只 flush 不 digest。4xx payload 被拒的會移到 `ship-rejected/`（保留供排查）。另有 `ship-state.json` 防同一天重複 ship。

### per-repo opt-out：`<repo>/.codeforge/no-ship`

某個 repo 放了 `.codeforge/no-ship` 檔，就硬性 opt-out：`dream` 的 digest ingest 略過、`ship` no-op。適合用 codeforge 初始化但不想 ship 的私有 repo。

### `cite` — 自然引用回寫

`codeforge mnemos-cli cite-detect <transcript>` 會掃 transcript、對命中的 Mnemos atom title POST 到 `/v1/atoms/<atom_id>/cite`，讓 Mnemos 累加 citation、驅動 active-memory ranking。confidence：自動偵測（cite-detect）用 0.5、手動 `mnemos-cli cite <atom_id>` 用 0.7；Sprint 5+ 換 Haiku 偵測。

> ⚠️ **目前 cite-detect 是手動子命令，尚未接入自動流程**：`ship` 不呼叫 cite，SessionEnd hook 鏈（emit-session / session-digest / dream / ship）也不含 cite-detect。所以正常 session 結束**不會**自動回寫 citation —— 需手動跑 `cite-detect`。自動化是 roadmap（見 `codeforge-ship.md` §9）。

---

## 5. 兩顆腦的連線燈號

`codeforge statusline` 的底框右側現在有兩顆燈，讓你一眼看出記憶系統的健康狀態：

### 本地腦（local memory）— 永遠本機

只要這個專案有 `.codeforge/` 歷史，左邊的 **`memory`** 燈就會出現：
- `●` 綠：active L1 概念 > 0，記憶正常運作
- `◌` 灰：store 目錄存在但 active = 0（「記憶怎麼不見了」是值得知道的訊號，不靜默）
- 不顯示：全新專案，從未 dream 過

這顆燈每次 render 即算（讀已開的 DB conn），成本為零。

### 央腦（central brain, Mnemos）— opt-in 選配

只有啟用 Mnemos opt-in（`~/.config/mnemos.env` 或 `MNEMOS_INGEST_URL`）時，右邊的 **`mnemos`** 燈才出現：
- `●` 綠 `ok`：liveness probe 通、ingest 健康
- `◐` 黃 `degraded`：server 活著，但最近 ingest 失敗或 queue 積壓
- `○` 灰 `offline`：server 沒在跑（**中性，不是告警**，去把 server 開起來就好）
- `◌` 灰 `pending`：opt-in 但從未成功連線

黃/灰態底框會出現 `→ doctor` 提示，引導你去執行 `codeforge doctor` 查詳情。

### 燈號的資料來源

- **liveness** (`mnemos-liveness.json`)：由 `statusline` 排程的 detached 背景進程（`codeforge mnemos-cli probe`）每隔 30 秒–10 分鐘（指數 backoff）探一次 `GET /health`，結果寫到 **per-machine** 暫存目錄（`$XDG_RUNTIME_DIR/codeforge/`）。statusline 熱路徑**只讀快取**，從不阻塞網路。
- **readiness** (`mnemos-ship.json`)：每次 `codeforge ship` 真正 POST ledger 之後（或 flush 成功後）寫入，反映 Mnemos 的真實 ingest 能力。

兩軸**獨立**、不 OR 合一（避免「server 活著但 ingest 500」或「probe 死但 ship 三天前綠」被洗成假綠）。

### 無色終端 / 色盲

設定 `NO_COLOR=1` 時，燈退化為文字形式：`memory:active`、`mnemos:offline` 等完整詞（無縮寫），保持可讀性。

### 深度診斷

`codeforge doctor` 提供全維度：
- 本地 L1 active count 與 store 歷史
- Mnemos opt-in 狀態
- **即時** probe 結果（一次前景 `GET /health`，~2s）
- 上次快取 probe 時間與結果
- 上次 ship 時間與成敗
- 待重送 queue 深度（筆數 + 最舊 age）
- base_url 設定
- 黃/灰態的 next-step 操作建議

`codeforge mnemos-cli probe [--verbose]` 可以手動探 `/health`（`--verbose` 印 stderr 詳情、不寫快取，適合 debug）。

---

## 6. 養寵系統

codebase 是世界地圖、技術債是怪物、pet 是你的角色。

### PetState 持有什麼

- **等級與經驗**：`level` / `xp` / `xp_to_next`
- **生命**：`hp`（由 daemon 的 ECS 管理，升級不直接加 HP）
- **四維 stats**（`CharacterStats`）：`atk`（Fluency 流暢度）/ `def`（Integrity 可靠性）/ `sup`（Reach 跨域影響力）/ `ver`（Breadth 技術廣度）
- **身分**：`village`（目前 5 個 hardcoded：rust / python / typescript / go / javascript）+ 衍生的 `name`

### XP 從哪來

寵物的 XP 來自**任何專案**的活動（不只 codeforge 這個 repo），透過全域 hook 餵入：

| 事件 | XP |
|---|---|
| `git_commit` | +20 |
| `session_end` | +10 |
| `session_start` | +3 |
| `file_saved` | +1 |

升級時四維 stats 各 +1、`xp_to_next` 約 ×1.5。**daemon 權威 leveling**（`check_levelup`）每升一級另外 `hp_max += 10`、`hp` 補滿到 `hp_max`，`xp_to_next = xp_to_next + xp_to_next/2`（`.max(100)`）。**CLI live-overlay**（`PetState::add_xp`）只算 `*1.5`（夾 `.min(10M)` 防 u32 overflow），不動 hp。

### daemon 寫 / CLI 讀（write ownership）

- **daemon 擁有** `pet_snapshot`（權威遊戲狀態）：每 tick drain `event_inbox`、算 XP、寫回 snapshot。
- **CLI append** `event_inbox`（append-only，WAL 安全，無鎖）。
- **CLI 讀取走 live-overlay**：讀 `pet_snapshot`（無則 fallback 到 Phase 1 的 `pet` 表）+ 即時疊加尚未被 daemon 看到的 `event_inbox`，當場跑升級 cascade 回傳。所以**即使 daemon 沒在跑，statusline 也能即時反映 XP**。

---

## 7. 養寵連結 CodePower —— 今天 vs 規劃

CodePower（鬥技場，公開競技）與 CodeForge（鍛造間，私人累積）是搭配使用的。「Power 提供能量，Forge 塑形」。關於「把養好的寵物帶進 CodePower」這件事，現況如下：

### ✅ 今天真的有的

- **跨專案共用 statusline**：`codeforge statusline` 被所有 Claude Code session 呼叫（全域 hook），同一隻寵物面板在每個專案都看得到。
- **XP 來自任一專案**：上面 §5 的事件來自任何 repo 的活動，寵物在你所有工作中持續成長。
- **Clan content skeleton**：`src/clan/`（`ClanContentProvider` trait、`HttpClanContentProvider` 讀 `~/.codeforge/config.toml` 的 `[codepower]` 設定）已有骨架（`mod.rs` 的 re-export 用 `#[allow(unused_imports)]` 抑制），**尚未接線**到 `src/pet/village.rs`（仍是 5 個 hardcoded village）。

### 📋 Phase 5 roadmap（尚未實作）

- **Nation P2P**：玩家 ed25519 keypair 身分、簽章的 nation credential（player_id / level / badges / pet）、credential 驗證、`nations.toml` 社群清單。
- **Connected Smith 參戰**：把 forged-self 帶進一個 CodePower nation 打團戰。
- **Multi-Nation Pilgrim**：透過 Nation P2P 同時連多個 nation。

設計細節見 [`doc/specs/nation-p2p-design.md`](specs/nation-p2p-design.md)。**目前只有 Solo Smith（純單機）是完整可用的**；Connected / Pilgrim 是 Phase 5 願景。

---

## 相關文件

- [`README.md`](../README.md) — 安裝與快速上手
- [`doc/specs/codeforge-mud-engine.md`](specs/codeforge-mud-engine.md) — MUD 引擎技術規格（daemon / combat / TUI）
- [`doc/specs/codeforge-ship.md`](specs/codeforge-ship.md) — ship 的 Mnemos source 規格
- [`doc/specs/codeforge-brain-indicators.md`](specs/codeforge-brain-indicators.md) — 雙燈健康模型規格（local + central，probe/ship 雙軸，liveness 快取，doctor）
- [`doc/specs/codeforge-memory-contract.md`](specs/codeforge-memory-contract.md) — 記憶共享狀態與 atom schema 契約
- [`doc/specs/nation-p2p-design.md`](specs/nation-p2p-design.md) — Phase 5 Nation P2P 設計
- [`CLAUDE.md`](../CLAUDE.md) — Claude Code 工作指南（含生態系互動規則）
