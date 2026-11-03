# CodeForge Backlog

Last audited: 2026-11-01

Items confirmed low-priority or blocked by external dependencies. Each item has a trigger condition for when to pick it up.

---

## B1 — Signal Cursor Race Condition (Concurrent dream runs)

**Area**: `src/dream/compile.rs`, `src/db/schema.sql` (signal_cursors)
**Premise**: If two `codeforge dream` calls run simultaneously (e.g. SessionEnd hook + manual), the signal cursor may be read twice before either write completes, causing duplicate L1 entries.
**Trigger**: Pick up when Phase 2 daemon is implemented (daemon will own all write operations, eliminating concurrent CLI calls).
**Mitigation in place**: `PRAGMA busy_timeout=5000` ensures one waits; risk is low for Phase 1 single-user.
**Status 2026-04-17**: Phase 2a shipped. Daemon now owns derived-state writes (`pet_snapshot`, `combat_log`, `game_world`), but `signal_cursors` are still CLI-written by `dream compile` — this item remains open with unchanged scope.
**Status 2026-06-15**: ship-online 把 `codeforge dream` 移到 global SessionEnd（跑遍所有專案）。**預設 per-cwd `.codeforge` 不受影響**（各專案寫各自 store,無 cross race）；只有 `CODEFORGE_DIR` 共享 store 的使用者,多專案 session 同時結束時 concurrent-dream 機率上升 —— 仍由本 item 既有 scope 覆蓋,`busy_timeout` 緩解。

---

## B2 — CJK-Aware FTS5 Search Tokenizer

**Area**: `src/db/schema.sql` (memory_fts), `src/memory/fts.rs`
**Premise**: FTS5 uses `porter unicode61` tokenizer which doesn't handle CJK character segmentation. Searching for Chinese terms like "重複" may miss matches split across FTS tokens.
**Trigger**: Pick up when user reports CJK search misses, or when FTS corpus grows to >500 entries.
**Note**: ICU tokenizer or external jieba segmentation needed. External dependency — needs survey first.

---

## B3 — Badge System Implementation (Phase 1 stub)

**Area**: `src/pet/badges.rs`, `src/db/schema.sql` (badges table)
**Premise**: Badge table exists in schema and `badges.rs` has `award()` / `list()`, but nothing calls `award()` — no game event grants a badge (only `codeforge pet` lists them).
**Trigger**: Pick up when deciding which events grant badges (MOB kills, zone mastery, milestones); Phase 2b shipped without wiring it.

---

## B5 — Ingest Parser: ChatGPT Export Format

**Area**: `src/cli/ingest.rs`
**Premise**: `ingest --source chatgpt` is declared in CLI but parser may not handle all ChatGPT export variants (JSON vs markdown).
**Trigger**: Pick up if user requests ChatGPT migration or if a second user adopts CodeForge.

---

## B13 — 真實 production ship e2e（對 live Mnemos server）

**Area**: runtime（`codeforge ship --no-hook` SessionEnd + `~/.mnemos/data/mnemos.db`）
**Premise**: ship-online 專案的 e2e 只在隔離 test DB(`/tmp/ship-e2e`)驗過 → 200。真正對使用者 production Mnemos brain 的 session-end ship（global hook 鏈)尚未端到端跑過一次。已 opt-in（`~/.config/mnemos.env`），但驗證當下 Mnemos server 沒跑 → 本機 session-end ship 會 queue 到 `~/.codeforge/ship-failed/`。
**Trigger**: 下次 Mnemos server 在跑時 —— 確認一次真實 session-end 的 ledger 落進 production brain（查 `documents`/`atoms`），並 `codeforge ship --resend` 清掉累積的 ship-failed/ queue。
**Status 2026-06-15**: ship-online 上線後立即產生;設計上 ship --no-hook 失敗只 queue 不阻塞,故非 blocker。

---

## B15 — improvement-queue surfacing 一般化(project-scoped,非 codeforge-clone-only)

**Area**: `.claude/scripts/check-improvements.js`、`codeforge install`（hook 安裝佈局）
**Premise**: improvement-queue 已做 project 歸屬(session-digest 寫項標 `project: cwd`,2026-06-15 fix `a0169cc`),但**surface 它的 `check-improvements.js` 仍是 codeforge-clone-only**(只裝在 codeforge clone 的 `--project-hooks`,`PROJECT_ROOT` 由 `__filename` 推導恆為 codeforge 根)。後果:
- CodeForge 只 surface 自己的項(已修,正確)。
- **其他專案(CodePower 等)寫進共享 queue 的項,沒有任何地方會在該專案 surface** —— 它們帶了 `project` tag 卻無 surfacing hook 消費。
參考 dream/ship 的做法:把 surfacing 從 clone-only 改成可跨專案(global 或可裝進任一專案的 SessionStart hook),用 hook CWD/`CLAUDE_PROJECT_DIR` 當 `PROJECT_ROOT`,各專案 SessionStart 只報屬於自己(`project === <該專案根>`)的 pending 項。
**Audit 2026-11-01**: 併入 B31 —— 納編後 hooks 改由 autopilot 的 opt-in 清單註冊，跨專案 surfacing 跟著那次一起設計，不單獨做。
**Trigger**: 當 (1) 想讓 CodePower(或其他專案)session 也能在 SessionStart 看到自己的 pending improvements,或 (2) 把 check-improvements 納入 global hook 安裝鏈(類似 ship-online 把 dream/ship 移 global)時。需處理 `PROJECT_ROOT` 來源從 `__filename` 改成 hook 提供的 project dir。
**Status 2026-06-15**: project 歸屬(寫端)+ CodeForge 自身 scoping(讀端)已完成;跨專案 surfacing 一般化未做,記為 enhancement。


---

## B16 — memory-recall Phase B / C(偷好偷滿的下一輪)

**Area**: `src/dream/compile.rs`、`src/memory/recall.rs`、SessionEnd hook、L1 schema
**Premise**: Phase A(本地 recall 注入器)已上線(2026-06-16,`local-recall` 專案)。design spec `doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md` 的後續 tier:
- **Phase B(Tier 2)**:T2.2 mem0 式 ADD/UPDATE/DELETE/NOOP 對賬 + T2.3 統一 recency×importance×relevance 排序 → **2026-06-17 開做**(plan `doc/plans/2026-06-17-memory-recall-phase-b.md`)。**T2.1 async fire-and-forget worker 留此 backlog**(見下)。**procedural-atom `nature` 欄位**:只在 frontmatter 留位,分類邏輯歸 Phase C。
- **Phase C(Tier 3,評估後選)**:本地語意 recall(FastEmbed+HNSW,取代/增補 FTS5 keyword)、失敗/卡關為一等記憶、typed observation/relation schema、矛盾偵測 + 兩段式信心衰減。
**Trigger**:
- **T2.1(async worker)**:當 dream/ship 在 SessionEnd 開始有感卡頓時開新 L。設計已收斂(留檔在 phase-b plan「OUT OF SCOPE」段):conditional enqueue + 同步 fallback —— 有 live daemon 才 `dream --background` emit `dream_scheduled` event 快速返回,無 daemon inline 同步跑(維持「永遠會 distill」、不回歸 ship-online)。2026-06-17 user decision defer:觸發條件未成立 + 紅利範圍窄(多數專案無 daemon)+ 動剛上線熱路徑風險不划算。
- **Phase C**:當 keyword recall 品質證實不足(語意)、或想捕捉失敗 pattern 時。**Audit 2026-11-01**:語意檢索依 pm 分工(B33)歸 mnemos,本地 FastEmbed+HNSW 開做前先確認不和 mnemos 重做。
**Status 2026-06-17**:Phase A done;Phase B 的 T2.2+T2.3 開做;T2.1 + Phase C 記為 enhancement,有 design spec 母本與 credits。

---

## B17 — session-digest A′ 落地的 review 殘留(低優先)

**Area**: `.claude/scripts/session-digest.js`、`src/dream/ingest_digests.rs`
**Premise**: A′ 落地(2026-06-17,`443e31d`)§6 獨立 review 留下幾個低優先項,已判定接受/駁回,記為 enhancement:
- **mtime-guard 殘留亞秒競態**:ingest 刪檔前比對 mtime,但 coarse-mtime(秒級)FS 上「讀後同一 mtime-tick 內被改寫」仍可能刪到新檔。已配 atomic write 大幅收窄;若要全閉,改 rename-to-staging(`X.json`→`X.json.ingesting` 後處理)+ orphan 回收。實務機率極低(09:00 離峰 cron、session 多不活躍),故未做。
- **`findCodeforgeRoot` 吞 EACCES**:`statSync` 把權限錯一律當「非 repo」→ skip 不寫,fail-safe 正確但不可診斷。可加「非 ENOENT errno 時 log」便於排查。
- **improvement-queue 非原子寫**(pre-existing,本次 out of scope):`session-digest.js` 寫 `improvement-queue.json` 用 `writeFileSync` 但 doc-comment 宣稱 atomically。可比照 digest 改 temp+rename。
**Trigger**: (1) 真的觀察到 worktree/權限情境的靜默 capture gap;或 (2) 多 dream 並行/高頻 PreCompact 環境下出現 digest 競態丟失;或 (3) 順手清 improvement-queue 寫入時。
**Status 2026-06-17**: review 已處置(接受現狀);記為 enhancement。

---

## B18 — spec 設計目標 code 未跟上（doc-code-drift audit 2026-06-18）

**Area**: `src/cli/ship.rs`、`src/mnemos/*`、`src/daemon/combat.rs`、`src/pet/ability.rs`
**Premise**: 2026-06-18 的 doc↔code drift audit（6 領域 + 對抗驗證，48 confirmed findings）發現幾處是「spec 寫了設計、code 尚未實作」（非 doc 寫錯）。已在各 spec 頂部標 `⚠️ NOT YET IMPLEMENTED / CODE LAGS DESIGN`，code 補上時清這些標記：
- **ship 掃 session jsonl**：`codeforge-ship.md` §5/§6.1 設計 ship 讀 `~/.claude/projects/<repo>/*.jsonl` 取 source_evidence locator（uuid/ts/line）+ prompt [session 線索] block。實作只讀 L1 + git；`SourceEvidence::session_jsonl` 是 dead_code。
- **provenance 擴充欄位**：`codeforge-ship.md` §4.3 設計 `raw_signal_count`(從 db 算)/`source_jsonl_paths`/`digest_cost_usd`。`build_provenance` 只產 5 個基本欄位。
- **ship-state 省 digest**：§7.6 設計「已 ship 過 → skip digest + POST」，code 先 digest 才查 already_shipped，只 skip POST（浪費一次 LLM 呼叫）。
- **Pet Ability 戰鬥效果**：`codeforge-mud-engine.md` §2.5 的 Quick Eye/Focus Strike/Tome Sense 效果未實裝；`ability.rs` 只是 display-only catalog，`combat.rs` 無 ability 邏輯。
- ~~**auto-cite-on-ship**~~ ✅ **Done 2026-07-02**（fuchikoma P1.1）：`ship` 結束（成功/已 ship/空 ledger 三分支）自動掃當日本 repo session transcript → 比對候選 atom 標題 → 回寫 cite。實作 `src/mnemos/autocite.rs` + `ship.rs::maybe_auto_cite`（best-effort、gated on opt-in、confidence 0.5 + session_jsonl provenance）。e2e:`scripts/e2e-mnemos-chain.sh` 步驟 5。§9 的 ⚠️ 標記已清。**餘下未做**:ship digest 端仍不掃 jsonl 取 source_evidence(見上「ship 掃 session jsonl」子項,digest 證據那條仍 open)。
- **Void Creature MOB + Lv50 legendary 村莊**（full sweep 2026-06-18 補）：`codeforge-mud-engine.md` §2 的 Void（missing test coverage→drain DEF）MOB 未實作（scanner 只產 Zombie/Boss/Elite/Ghost 4 種）；§2.5 Lv50 legendary 表列的 ML / 開源基金會 村莊不存在（實際 5 村莊，且 javascript 無 legendary 條目）。
**Trigger**: 各子項獨立 —— 想讓 ledger 帶 session 證據 / 想要 cost telemetry / 想省重複 digest / 想做 ability 戰鬥深度 / 想自動 cite / 想加 MOB 種類或村莊時，分別開 S~L 實作並清掉對應 spec 的 ⚠️ 標記。
**Status 2026-06-18**: audit 已把 doc 側全部更新成現實 + 標記設計目標;code 側未動,記為 enhancement。純 stale/missing 的 drift（首輪 48 條 + scoped 4 條 + full sweep 22 條）已分別修正落 doc。

---

## B21 — `should_refresh` 的 `!fresh_enough` dead path revisit

**Area**: `src/mnemos/health.rs:should_refresh`
**Premise**: `should_refresh` 的 `cache_stale = !fresh_enough(l, now)` 分支在 `PROBE_TTL_MAX(600s) < CACHE_MAX_AGE(3600s)` 的當前常數下永遠被 `ttl_expired` 先短路（dead path，保留作 belt-and-suspenders）。代碼已加 comment 說明。若未來 `PROBE_TTL_MAX` 調高至 > `CACHE_MAX_AGE`，此 OR 分支將重新生效，語意正確性需 revisit（或可移除）。
**Trigger**: 調整 `PROBE_TTL_MAX` 常數（往 >3600s 方向）時，或決定清理 belt-and-suspenders 時。

---

## B24 — `l1::count_active` 熱路徑逐檔 parse frontmatter 潛在成本

**Area**: `src/memory/l1.rs:count_active`、`src/cli/statusline.rs`（呼叫點）
**Premise**: `count_active` 對 `store/concepts/` 下每個 `.md` 逐一 parse frontmatter，per-render 呼叫、未快取。概念數通常 <100 故成本可忽略，但 concepts 目錄成長至數百筆後，每次 statusline render 均有 O(n) 讀檔+解析開銷。
**Trigger**: concepts 目錄 >200 筆，或 statusline render 延遲有感變慢時。改法選一：(a) 在 SQLite DB 加 `l1_count` 欄位、dream 寫入後更新，statusline 走 DB 查詢；(b) 快取到 runtime_dir（與 liveness cache 同源）並由 dream 觸碰失效。

## B25 — fleet authed-readiness probe（央腦燈即時反映「真能 ship」）

**Area**: `src/mnemos/health.rs`（probe）、`central_light` readiness 軸；協調 Mnemos repo `fleet-ingest-rollout`
**Premise**: 央腦燈是 probe(liveness, `/health`) + ship(readiness) 雙軸。`/health` 結構性豁免 auth → 綠燈只證「腦活著」，**不證 token 配對正確**。fleet 機若 `MNEMOS_TOKEN` 錯/缺 → `/health` 綠、但 ship/context 吃 401。現況靠 ship 結果（401→`last_ship_ok=false`→燈翻黃）已能抓到，**但延遲到 SessionEnd（一 session 一次）**。
**改法**: 背景 probe 在「有 token」時多打一條 **authed liveness**，餵進**既有 readiness 軸**（即時取代延遲的 ship 訊號），維持單一健康軸（綠/黃/灰）。不做成第三條獨立燈。frequency 放低（liveness 30s / readiness ~5min，因 authed 端點較重）。打 **`GET /v1/whoami`** 當 authed liveness（只過 Bearer middleware、零腦負載），比打 `/v1/atoms/context` 省。
**Dependency（已就緒，code 已驗）**: `GET /v1/whoami` 端點 **Mnemos 已實作並 merge 進 main**（commit `70fa2d0`；origin/main `Cargo.toml` v0.3.5；落 spec 10 §8 / §3.2）。code 在 `crates/mnemos/src/api.rs`（route line 42 + handler line 67 + 3 tests：auth關→200、auth開缺token→401、auth開對token→200）。回 `{"ok":true,"version":"<crate>","auth":"enabled|disabled"}`：auth 關→恆 200 當 liveness；auth 開→缺/錯 token 401、對 token 200 → readiness 即時。不碰 store/embedder（比 `/v1/atoms/context` 省一輪 hybrid 檢索）。**caveat**: staging :8846 仍舊 binary 無此端點，flip-prod 一起換。runtime **已本機實證**（2026-06-24 經 SSH 隧道打 prod `/v1/whoami` 回 `{"auth":"disabled","ok":true,"version":"0.3.5"}`，與 origin/main `Cargo.toml` 對齊 → prod 腦確跑帶 whoami 的 binary）。
**Trigger**: Mnemos flip-prod 開 auth（設 `MNEMOS_API_TOKEN`）/ fleet 開始多機收錄。現在不做（auth 關、單機，零價值）。端點雖已 live，auth 開前它只是 liveness、無 readiness 意義。
**Cross-repo**: 與 `cookys/mnemos:docs/projects/fleet-ingest-rollout` 對齊；`/v1/whoami` 端點落 Mnemos repo（spec 10 §8）。

## B26 — digest 引擎 fallback 鏈（claude -p → agy → codex → Haiku → rule-based）✅ 實作 2026-06-24

**Area**: `src/llm.rs`（`headless_digest` + `agy_p`/`codex_exec`）、caller `src/cli/ship.rs`、`src/dream/compile.rs`（reconcile + compile_signal）
**Premise（品質懸崖）**: dream/ship digest 走 `claude -p`(Opus)。但 `claude -p` 大 prompt 可能空輸出(#7263) → 無 `ANTHROPIC_API_KEY` 的機器**直接掉 rule-based passthrough**（最差），中間無 LLM 級檔位。ship 是 production critical path，品質下限不該是 passthrough。
**Bake-off（2026-06-24，真實 ship-shape prompt）**: 三家 headless 引擎都產 valid JSON、3+ lessons、evidence 歸因正確。claude(Opus) 40s/nuance 最全、title 最像結論；agy(Gemini Flash) 16s/最快/夠好但丟部分 nuance；codex(0.142.0) 中速/最徹底(抓 4 條)。三家 decorrelated（Anthropic/Google/OpenAI），且都走訂閱（claude/ChatGPT）或近免費（agy）→ 免 per-token key。
**改法（已實作）**: 抽 `llm::run_cli` 共用 headless 執行器；`headless_digest` 串 `claude_p → agy_p → codex_exec`，回第一個非空輸出；三家全失敗才 Err，caller 再接既有 Haiku(key)→rule-based 尾。主力仍 Opus（`CODEFORGE_DIGEST_MODEL`，bake-off 最佳），agy/codex 是免 key 中間檔。env：`CODEFORGE_AGY_MODEL`（預設 `Gemini 3.5 Flash (Medium)`）。
**已知限制 / 後續**:
- (a) **parse-fail 不換引擎**：某引擎回「非空但無法 parse 的 JSON」→ caller 走 passthrough，不會 fall through 到下一引擎（維持原 claude parse-fail 語義）。要更穩可讓 `headless_digest` 收 validator closure，逐引擎驗 parseable。低優先（Opus 回垃圾罕見）。
- (b) **codex 輸出格式**：codex exec 是 agent，stdout 理論上可能夾雜非 JSON 前後文；bake-off 實測乾淨，但它是第三 fallback、罕觸發。若實戰發現髒輸出，parse 端已有 `extract_json` 容忍。
- (c) **cron PATH**：agy/codex 常在 `~/.local/bin` 或 nvm bin，cron 下 ship 要確保在 PATH 內（`scripts/codeforge_ship.sh`），否則該層 spawn 失敗沿鏈降級。

## B27 — statusline 中段 indicator（hangar-bridge / autopilot / …）

**Area**: `src/cli/statusline.rs` `bottom_border`（擴充點註解已就位，central segment 之後）
**Premise**: 版號已右對齊（commit `ff51004`），bottom border 變成 `╰─ memory… mnemos ● ok ──[中段 fill]── ⬆ v… ──╯`。中段 fill 是預留空間，未來新功能的即時狀態燈（hangar-bridge 連線、autopilot 模式指示等）就接在這 —— central(mnemos) 之後、版號之前，當作「左側 content」的額外 segment，fill 自動縮短吸收。
**接法（code 內已寫詳細步驟）**: 對齊 central segment 的 pattern：(1) 狀態來源 enum + glyph/顏色 match；(2) `make_<name>_seg_{colored,glyph_only_colored,nocolor}` 三變體；(3) degradation ladder 每 level `total += seg_vis + gap` 並 `content.push_str` 接在 central 後；(4) 窄寬度於較低 level 優先丟（如 hint 的處理）。width 自動正確（ladder 用 vis 量、assemble fill = panel_w - overhead - left_vis - right_vis）。
**候選 indicator**:
- **hangar-bridge**：跨 agent 通道連線狀態（此概念目前 repo 內不存在，需先定義；見本 session 接腦討論）。
- **autopilot**：autopilot 模式 / DOA 狀態指示。
**Trigger**: 當 hangar-bridge 或 autopilot indicator 有了明確語義 + 狀態來源時。現在不做（無狀態來源、純預留）。
**Audit 2026-11-01**: autopilot 的狀態已由 autopilot 自己畫在 band（分頁列），B31 的 status 片段契約定案時一併決定這個擴充點留不留；在那之前不加 indicator。
**注意**: 每加一個 indicator 中段 fill 變窄；窄終端要靠 ladder 的 drop 順序保證不溢出（已有 `bottom_border_narrow_no_overflow` 測試守著）。

## B29 — ascii-pet-artist 指南寫小圖「最多 4 行」，實際五村都是 6 行

**Area**: `.claude/skills/ascii-pet-artist.md`（硬性約束段、新村莊 checklist）、`src/pet/village.rs` struct 註解（「6 行 × ≤10 字」）、`src/cli/statusline.rs` `render_full` 的 art 欄
**Premise**: 指南說 `ascii_small` 最多 4 行（對應 render_full rows 0-3），但 struct 註解寫 6 行、現有五個村落的小圖都是 6 行，statusline 實際畫出 5 行。三處說法不一，新增村落時不知道該畫幾行。（來源：2026-10-31 pet-view-snapshot 收尾的 doc-sync 掃描。）
**Proposed**: 先量 `render_full` 實際能放幾列 art，再把指南、struct 註解統一成那個數字，並在 `village.rs` 測試釘住上限。
**Trigger**: 下次新增或重畫村落圖案，或動 statusline 的 art 欄。
**Not done**: 只記錄落差；2026-10-31 只修了縮排、加了 `concat!` 寫法說明，沒有動行數規格。

## B30 — 其他四個村落的像素 sprite（目前只有 Ferris）

**Area**: `src/pet/sprite.rs`（`for_village`、新 sprite 常數）、`.claude/skills/ascii-pet-artist.md`（補 sprite 畫法）
**Premise**: pixel-pet（2026-10-31）只做了 Rust/Ferris 的 24×24 兩幀 sprite；python/typescript/go/javascript 的 `village.sprite` 是 `null`，forge-stats 面板對這些寵物退回 ASCII。sprite 格式與驗證已定（`validate()`：24×24、`.` 透明、xterm-256 cube 色、≥2 幀、500 ms）。
**Proposed**: 每村畫一組 24×24 兩幀 sprite 加進 `for_village`；`validate` 測試自動涵蓋。畫法說明寫進 ascii-pet-artist skill。
**Trigger**: owner 領養非 Rust 村落的寵物，或要求其他村落也有像素圖。
**Not done**: 範圍刻意只做 Ferris（plan `doc/plans/2026-10-31-pixel-pet.md` §7）。

## B31 — codeforge 成為 autopilot 的 extension（autopilot 3.0.0-alpha.8 起）

**Area**: 跨專案架構（autopilot extension 契約、codeforge 的 hooks / forge-stats mod / statusline 接法）；Rust 本體不搬
**Decision**: owner 2026-11-01（卡 d-0e2e6e0d）：codeforge 走 autopilot 的 extension 契約、不併進 autopilot repo，從 autopilot
3.0.0-alpha.8 起分段做（band 分頁 → hooks → status 片段 → 設定鍵 → binary 需求），在 3.0.0 正式版前完成，使用者只裝 autopilot。
兩邊共識（fleet msg `01M69B3CY03VMC41QQJ1CEGWQ1`、`01M69B89Z5GRJE5Z9NHBDM19N4`）：
- Rust 本體（記憶、ship、MUD）留在 codeforge repo；autopilot 是 Node、跨 harness、已公開，不帶進 Rust build 與 SQLite schema。
- band 契約只剩一件事：forge 分頁點開時，autopilot 往下問一次展開區，codeforge 回 Ferris、寵物、記憶與大腦。
  原本的分頁標籤契約（keyed Box，main `c5165cf`）隨速度一起退場（B32 已完成，見 CHANGELOG Unreleased 的 Removed）。
- hooks 改由 autopilot 的 opt-in 清單註冊、不再寫全域 settings。ship 失敗責任：autopilot 只負責有呼叫、記結束碼、失敗看得到；
  重試與補送仍是 codeforge 的。
- 降級：每一項都要能沒有 autopilot 單獨跑（forge-stats 沒有 host 時照現在自己畫；Solo Smith 定位不變）。
- 取代原 B31（表現層由誰統一畫）：答案是 autopilot 當 host，各 extension 照契約交內容；fleet 分頁也走這條：owner 決定由 autopilot 畫（卡 d-d99e2d99 已關），autopilot 登記為 BL-0358，alpha.7 之後做、不放核准按鈕。
**Trigger**: autopilot `v3.0.0-alpha.7`（repo 重整 P1–P5）發布之後；開工前 autopilot 先寫 extension 機制 plan 並過 plan review，
codeforge 參與審查（掛載點、manifest、binary 需求、ship 失敗責任這四塊）。
**hooks 段**: 2026-11-02 提前定好介面契約（7840hs 請求；owner 卡 d-0ce83828 選 C：只定契約）：`codeforge hook on <session-start|session-end|pre-compact>`，autopilot 用通用提供者清單 `hooks/hook-providers.json` 呼叫（autopilot 卡 d-92a2231d），codeforge install 依五項條件偵測、共用檔不自動清。契約：[`doc/plans/2026-11-02-autopilot-hook-bridge.md`](plans/2026-11-02-autopilot-hook-bridge.md)；專案：`doc/projects/2026-11-02-autopilot-hook-bridge/`。兩邊實作都等 alpha.7 之後，開工時再跑一次兩席 plan review。
**Not done**: hooks 段只有契約、沒有 code；其他段的契約與實作都還沒開始；alpha.7 搬家期間不定掛載點（路徑會變好幾次）。

## B33 — 專案範圍知識的正本改放 pm（跟著 repo 走、跨機器同步）

**Area**: 記憶管線（`src/dream/`、`src/memory/`、`memory context` recall、`ship` 選條目）；L1 atom schema（`doc/specs/codeforge-memory-contract.md`）
**Premise**: `.codeforge/` 整個被 gitignore，L1 只存在本機，多機開發彼此看不到。fleet 規劃中的 pm CLI（Rust，單一專案的 backlog/決策/ADR 鏈/專案事實，正本是 commit 進 repo 的 JSONL、sqlite 只當本機索引、id 用 ULID）要收專案範圍的 lesson/gotcha；codeforge 的 learn/dream 繼續從對話萃取，專案範圍產出改經 pm CLI 寫入，跨專案、L2 ledger、語意檢索照舊歸 codeforge/mnemos。
**codeforge 開出的限制**（fleet msg `01M6BASVM2RS9CY36Y2V61FFR9`，fleet 回覆會原樣寫進正式計畫）：(a) dream 產出先標 candidate，經 identifier-scan + 人工把關才升正式；(b) dream 只寫本機，由 finish 決定何時匯出並 commit；(c) 一條目一檔或 union merge；(d) refs/last_ref/strength 等使用統計留本機不進 commit；(e) pm 不在時降級寫本機 store，SessionEnd 不可失敗，recall 統一讀 pm＋本機；(f) ship 以 ULID 去重。另需新增 atom `scope: project | cross` 與來源 repo 欄位（現在只有存放位置與 `origin` 可粗分）。
**Trigger**: operator 對 pm 計畫拍板、fleet 通知 codeforge 寫遷移計畫；計畫寫好後開決策卡給 owner，不先動 code。
**Not done**: 遷移計畫、schema 增欄、寫入與 recall 路徑改動，全部未開始。

## B34 — 上層 plugin 吃掉 band 時，statusline 仍縮成精簡版、寵物哪裡都不顯示

**Area**: `.claude/skills/forge-stats/hooks/register.tsx`（心跳每 2 秒無條件寫）、`src/cli/statusline.rs` `read_mod_heartbeat`
**Premise**: statusline 只看心跳的 `layout: band` 就縮（owner 2026-10-31 定案，拿掉 `pet_shown`）。心跳由計時器寫，不知道 AbovePrompt 的 render hook 有沒有輪到 forge-stats：若鏈上有 plugin 自己畫 band、不呼叫 `next(e)`，forge-stats 的 band 不出現，statusline 卻照樣縮，寵物兩邊都看不到。現在 autopilot 的分頁列用 `await next(e)` 疊在上面（BL-0352），不會發生。
**Proposed**: 心跳加「最近 N 秒內 AbovePrompt hook 有被呼叫」的條件（只看有沒有輪到，不看 blit 是否成功，不回到舊的 `pet_shown` 規則）。
**Trigger**: autopilot 新增會拿掉 forge 分頁或不呼叫下層的模式，或使用者裝了別的會畫 band 的 plugin。
**Not done**: 只記錄；2026-11-01 清理時順手發現，不在那次範圍。

## B37 — 分身偵測：邊界續行裡的 block comment 本文含結束符時，續行提前結束

**Area**: `src/daemon/dup_detect.rs` `classify`（`State::Boundary` 與 `State::BlockComment` 只能擇一）
**Premise**: 同一行同時開了跨行 import／屬性續行與未閉合的 `/*`（例：`pub(crate) use crate::{ /*`）時，以續行狀態為準、註解狀態被丟掉。若註解本文剛好有 `};`、`]`、`)` 這類續行結束符，續行就提前回到 Normal，之後到 `*/` 為止的註解本文會進入重複索引，兩檔都這樣寫就可能生出假的分身怪。只會多生怪，不影響資料或 tick。（來源：B10 hetero code review g3，grok-4.7 finding `d06d04b2`，審查已達 3 輪上限而延後。）
**Proposed**: 加一個「續行內的註解」狀態：遇到含 `*/` 的行之前一律略過、不檢查結束符；`*/` 之後若續行還沒結束就回到續行，直到真正的結束行。加兩檔各寫一段這種結構的負例測試。
**Trigger**: 下次動 `dup_detect.rs` 的行分類，或實際觀察到這種假分身怪。
**Not done**: 只記錄；B10 已修掉 g1、g2 的兩個同類問題（`c2679ad`、`bd59ca5`）。

## B38 — MOB scanner 會掃進 `.claude/worktrees` 等 worktree 副本

**Area**: `src/daemon/mob_scanner.rs` `should_skip_dir`
**Premise**: skip 清單只有 `target`、`node_modules`、`.git`、`dist`、`build` 等；有 git worktree 放在 repo 內（例如 `.claude/worktrees/<lane>/`）時，scanner 會把 worktree 裡的副本當成獨立檔案。B10 實測 autopilot repo：3 隻分身怪全是 worktree 副本與主檔之間的重複；殭屍、Boss 等既有怪也會重複出現。
**Proposed**: skip `.claude/worktrees`，或更一般地跳過含 `.git` 檔案（worktree 標記）的子目錄；加 tempdir 測試。
**Trigger**: 在有 repo 內 worktree 的專案上跑 daemon，或下次動 `should_skip_dir`。
**Not done**: 只記錄；B10 verify（2026-11-02）時發現，不在 B10 範圍。

## B41 — G36 第 2–5 階段 review 列為後續的項目

**Area**: `src/usage/collect.rs`、`src/usage/store.rs` 測試
**Premise**: autopilot 四席審查第 2 輪（範圍 `d754c4c..22e90c4`）判為後續、不擋合併的項目：659de784、383eb39a（Codex 基準被修剪後從頭重讀的邊界情況）、6635affb（bucket 溢位的檔案每次 collect 都會重試、每次都被略過）、247999e5（store 測試用到 unix-only API 的部分要加 `#[cfg(unix)]`）、38b0450a（`MAX_LINE`／`MAX_COUNT` 等上限的調整）。
**Trigger**: 下次動 `codeforge usage` 的收集器，或在非 unix 平台建置測試。
**Not done**: 只記錄；詳情見 autopilot lane/g36rest 第 2 輪的 finding。

## B39 — G36 第 1 階段 review 的後續（statusline 配額列）

**Area**: `src/usage/quota.rs`、`src/cli/statusline.rs`（`others_segment`、`own_bars_yellow`、`render_full`）
**Premise**: G36 第 1 階段四席審查（g2 SHIP-AS-IS，f6baef59）留下的小項：
- `wider_than_0600` 用 `mode & 0o077`：0700 的快照不會被改回 0600（只多 owner execute，不外洩，審查方駁回為 Major），但字面上和計畫書「0600」不一致，可改成 `(mode & !0o600) != 0`。
- 非 unix 平台沒有 O_NOFOLLOW，`read_bounded` 不擋 symlink；可補 `symlink_metadata` 的 pre-check。
- `own_bars_yellow` 看資料不看畫面：ctx 或倒數因寬度被截掉時仍算一個黃，其他帳號可能少拿到黃色。
- 其他帳號的資訊可能把「→ codeforge adopt」提示擠掉（Minor a0aa9d06）。
- 16 檔上限套在過濾之前；0.795 顯示成 80% 但顏色門檻用原值；`render_full` 沒有測試骨架。
- glyph ▲●⊘ 屬 East Asian Ambiguous 寬度（G33 Suggestion），`parseFleet` 的 `error: ''` 被當成沒有錯誤（G33 Suggestion）。
- forge-stats 寵物分頁在窄 pane（約 37 欄、sprite 並排）時，「屬性」那列每個 Text 各自被截成 `…AT……`：`petLines` 依 `cols - sprite.width - 2` 排版，五項屬性放不下時應改折行或少放幾項（G36 第 4 階段截圖 cfcc6f80 看到，既有問題，非 G36 引入）。
**Trigger**: 下次動 statusline 用量列或 forge-stats 摘要列／寵物分頁。
**Not done**: 只記錄；G36 第 1 階段與 G33 皆已合併。
