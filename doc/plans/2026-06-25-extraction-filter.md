# Plan — Extraction-stage 雜訊濾除 + 同類錯誤聚合計數

- **Date**: 2026-06-25
- **Size**: L（memory pipeline 上游 / 資料完整性 Risk Escalation）
- **執行模式**: /l5 orchestration — main 寫 plan + dispatch + depth-0 qc；實作/review 委派異質引擎
- **動的檔案（主角）**: `.claude/scripts/session-digest.js`（extraction source，非 installed copy）
- **下游關聯（唯讀理解，原則上不改）**: `src/dream/ingest_digests.rs::format_signal`、`src/dream/compile.rs`

---

## 1. 問題陳述

codeforge 的 dream pipeline 吃進大量低價值 `error-recovery` signal。實測 `~/.codeforge/signals/2026-06-25.jsonl` 16 個 SessionDigest signal 裡，大半是：

- **(A) 無上下文純 exit code**：`error="Exit code 2"` 之類，無任何 error 文字、無 fix 線索 → **真噪音**，連 compile LLM 都該跳過。
- **(B) 重複但有意義**：「File has not been read yet」×6、watch 逾時 ×2 → **不是噪音，是方法論頻率指標**。但現況 6 個獨立 signal 各送一次 compile LLM、dedup 成 1 條、**次數丟失、5 次 LLM 白燒**。

**架構洞察**：codeforge 目前只有「知識原子(lesson)」一種視角；錯誤頻率屬「flow/方法論 metric」，沒有獨立的家 → 被硬塞進 lesson 路徑、在 compile dedup 裡消失。

兩種雜訊**命運不同**：A 該在 extraction **濾掉**；B 不該濾，該**同 session 同類聚合 + 計數**，發一個帶次數的 signal（一次 LLM、頻率保留變方法論 lesson）。

---

## 2. 現況 pipeline（已讀 code 確認）

```
extractErrorRecoveries(messages)            session-digest.js:215-321
  └─ 偵測 error tool_result → 後 ERROR_WINDOW(5) 則 assistant 找同檔/同命令 recovery
  └─ recovered → push { type:'error-recovery', confidence:'high',
                        tool, error:truncate(errorText,300), file?, context? }
  ↓ signals = [...errorRecoveries, ...userCorrections, ...selfCorrections]   :975
  ↓ 寫 per-repo digest json                                                  :988-1013
ingest_from_dir → 只收 confidence=='high'                  ingest_digests.rs:216-240
  └─ format_signal(sig): 白名單欄位拼字串                                    :301-338
     白名單 = [tool, error, correction, context, assistant_context, file, description, text]
     → 「【錯誤修復】tool=… | error=… | file=…」；整段 <15 字 → None
compile_signal → L0→L1 LLM，dedup                          compile.rs
```

**關鍵約束**：`format_signal` 白名單**沒有 `count`**。純在 JS 端往 signal object 塞 `count` 欄位 → 下游 `format_signal` 不認得 → **count 被靜默丟棄，L0 content 不含次數**。聚合計數要生效，次數必須落在白名單既有欄位內（見 §4 決策）。

---

## 3. 改進 1 — Extraction 純 exit-code 噪音濾除（濾 A）

> **R1 修正（gpt-5.5）**：原設計用「剝樣板後字數 < 12」當主判準 → 會誤殺 `ENOENT`/`EACCES`/`Killed`/`not found`/`SIGTERM` 等**短但實質**的 error。字數門檻廢除。新判準：**只有當「剝掉所有已知 noise 樣板行後沒有任何剩餘非空內容」才視為純噪音濾掉**。有任何非樣板殘留文字 → 放行（含短 token）。

### 3.1 目標
`extractErrorRecoveries` 在 `push` 前，多一道「**這次失敗的 error 是否只剩 exit-code/noise 樣板**」判定。只濾掉「除樣板外什麼都沒有」者；任何實質 error 文字（含短 token、含 B 類「File has not been read yet」）一律放行。

### 3.2 前置步驟（實作必做，不可省）：建真實格式 fixture
plan 不臆測 Bash tool_result 的 error 措辭。**實作第一步**：從真實 transcript 取樣，蒐集 Claude Code Bash/工具 error 的實際格式集合，建成 fixture（`.claude/scripts/__fixtures__/exit-code-noise.txt` 之類）。已知須涵蓋的變體（取樣後補全，勿視為窮盡）：
- `Exit code 2` / `exit status 127`
- `Command failed with exit code 2`
- `Process exited with code 1`
- `Bash command failed with exit code ...`
- 可能帶前綴標籤 / 括號 / `Error:` 包裝

取樣來源：`~/.claude/projects/*/*.jsonl` 內 `is_error:true` 的 `tool_result`。實作者把真實樣本貼進 fixture，regex 對 fixture 驗證。

### 3.3 設計：`isPureExitStatusNoise(errorText)`（命名採 R1 建議，語義限定「純 exit code 噪音」，不擴大成泛用低價值分類器）

```
// NOISE_LINE_PATTERNS：逐行比對，命中=該行純屬 exit-code/noise 樣板（無學習價值）
const NOISE_LINE_PATTERNS = [
  /^\s*exit\s+(code|status)\s*:?\s*\d+\s*$/i,
  /^\s*(command|process|bash\s+command)\s+(failed\s+with\s+exit\s+code|exited\s+with\s+code)\s*:?\s*\d+\.?\s*$/i,
  // …取樣 fixture 後補全真實變體
];

function isPureExitStatusNoise(errorText) {
  if (!errorText || !errorText.trim()) return true;   // 全空 = 無實質 = 噪音
  const residual = errorText
    .split('\n')
    .filter(line => line.trim() && !NOISE_LINE_PATTERNS.some(p => p.test(line)))
    .join('\n')
    .trim();
  return residual.length === 0;   // 剝樣板後無殘留 → 純噪音
}
```

- 套用點：`extractErrorRecoveries` line 308 `push` 前、`recovered` 成立之後：`if (isPureExitStatusNoise(errorText)) continue;`
- **無字數門檻** → 短實質 error 零誤殺。
- **B 類保護**：「File has not been read yet」非任何 noise 樣板 → residual 非空 → 放行（交給改進 2 聚合）。
- multiline（exit-code 行 + 真 error 行）→ 只剝 exit-code 行、真 error 殘留 → 放行。

### 3.4 邊界 / 風險
- 判定**只看 `errorText`**，不看 `context`/`file`。理由：噪音判準是「這次失敗本身有沒有可學的 error」。
- 不複用下游 `format_signal` 的 <15 字門檻 —— 那是整段 content（含 label/tool/file path）計字，純 exit code + 長檔名仍可 >15 字過關，攔不住 A。本層在 extraction、只看 error 是否純樣板，是正確攔截層。
- regex 若漏某真實變體 → 該噪音漏網（fail-open，不誤殺），可後續補 pattern；比 fail-closed 誤殺實質 error 安全。

---

## 4. 改進 2 — 同 session 同類錯誤聚合計數（B）

### 4.1 目標
同一 session 內，同類 error-recovery signal **dedup 成一條、帶 `count`**。例：6 個「File has not been read yet」→ 1 條 `error="File has not been read yet"`、次數 6。

### 4.2 聚合 key（signature）

> **R1 修正（gpt-5.5）**：(1) 廢「取前 80 字前綴」—— 區分性常在後段，前綴相同會 false-merge（如 `failed to run custom build command for ...`、`Command failed:` 後才接關鍵 stderr）。改用**完整 normalized 字串**當 key。(2) normalize **不再全域刪數字 / 刪所有 quoted literal** —— 那會誤併不同 error code / HTTP status / port / version / enum variant。只移除**明確可變部分**（路徑、行列號、臨時檔名），並**保留錯誤碼類 token**（`E\d+`、HTTP status、compiler diagnostic code）。

> **R2 修正（gpt-5.5）**：(1) signature 必須用 **raw `errorText`（truncate 前）**計算 —— 現況 signal 的 `error` 是 `truncate(errorText,300)`，用它當 key 等於只比前 300 字 prefix，前 300 字同、尾段異仍 false-merge。(2) 原 path regex `/(?:\/[^\s:]+)+/` 太粗暴：會吃掉語義 slash phrase（`read/write permission` → `read<path>`，誤併），且對 relative path 不完整（`src/foo.rs` 只換 `/foo.rs` 留 `src<path>`）。改用**精確 path detector**：只匹配「絕對 / `~` / `./` / `../` 開頭」或「含副檔名的多段路徑」，**不**匹配純 `word/word` slash phrase。

```
// 精確 path detector：避免吃普通英文 word/word；涵蓋帶 line:col 尾綴
// 順序重要（alternation 從左優先）：最具體「含副檔名多段路徑」在前。
const PATH_RE = new RegExp([
  // (R3) 含副檔名的多段路徑，相對或絕對皆涵蓋（最後段須有 .副檔名 → 不吃 read/write）：
  //   src/foo.rs:1:1、lib/foo.rs:9:9、/a/foo.rs:12:3、./x.ts
  '(?:(?:~|\\.{1,2})?\\/)?(?:[\\w.\\-]+\\/)+[\\w.\\-]+\\.\\w+(?::\\d+(?::\\d+)?)?',
  '(?:~|\\.{1,2})\\/[\\w.\\-\\/]+',                            // ~/ ./ ../ 開頭路徑（無副檔名也算）
  '\\/(?:[\\w.\\-]+\\/)+[\\w.\\-]+',                           // 絕對多段無副檔名 /usr/local/bin
].join('|'), 'g');

function recoverySignature(toolName, rawErrorText) {
  const norm = (rawErrorText || '')
    .toLowerCase()
    .replace(PATH_RE, '<path>')              // 精確路徑 → 佔位（不吃 read/write）
    .replace(/:\d+:\d+\b/g, ':<lc>')          // 殘留 line:col
    .replace(/\bline\s+\d+\b/gi, 'line <n>')  // "line 42"
    .replace(/[.\-][0-9a-f]{6,}\b/gi, '<tmp>')// 臨時檔 hash / -XXXXXX
    .replace(/\s+/g, ' ')
    .trim();
  // 完整 normalized 字串即指紋（保留 E\d+ / HTTP status / diag code 等區分性 token；不 slice 前綴）
  return `${toolName}::${norm}`;
}
```

- signature 算在 **truncate 前**：見 §4.3，push 時暫存 `_rawError`，聚合用它，聚合後移除（不進 digest）。
- 同 `toolName` + **完整 raw normalized error 相同** → 同類。
- **path detector 設計約束**（實作對 fixture 調校）：MUST normalize `src/foo.rs`、`/a/foo.rs:12:3`、`~/x`、`../y.ts`；MUST NOT 吃 `read/write`、`allow/deny`（無副檔名、無路徑前綴的純 word/word）。
- **刻意取捨（R3）**：無副檔名且無 `~/`/`./`/`../` 前綴的 relative path（如 `a/b`）與無副檔名單段絕對路徑（`/tmp`、`/var`）**刻意不 normalize** —— 保守策略，寧 under-normalize（少數同錯不合併）也不 over-normalize（吃掉 slash phrase 致誤併）。
- **file 不納入 signature**：B 類核心場景「Read-before-Edit 違反 ×6」正是發生在 **6 個不同檔**；把 file 併進 key 會拆開、摧毀聚合目的。跨檔資訊改在 §4.4 以 metadata 帶出。

### 4.3 聚合流程（在 `extractErrorRecoveries` 內，回傳前）

**前置（push 點，line 308）**：push signal 時多帶一個**非輸出**欄位 `_rawError`（未經 `truncate` 的原始 `errorText`）供聚合算 signature；聚合完移除，不進 digest：

```
signals.push({
  type: 'error-recovery', confidence: 'high', tool: toolName,
  error: truncate(errorText, 300),
  file: errorFile || undefined, context: context || undefined,
  _rawError: errorText,            // R2：raw（未截斷）供 signature；下方聚合後 delete
});
```

聚合（回傳前）：

```
const grouped = new Map();   // signature -> { signal, count, files:Set }
for (const s of signals) {
  const key = recoverySignature(s.tool, s._rawError);   // R2：用 raw、非 truncate 後的 s.error
  const g = grouped.get(key);
  if (g) {
    g.count += 1;
    if (s.file) g.files.add(s.file);
  } else {
    grouped.set(key, { signal: s, count: 1, files: new Set(s.file ? [s.file] : []) });
  }
}
return [...grouped.values()].map(({ signal, count, files }) => {
  const out = count > 1 ? withRepeatMeta(signal, count, files.size) : signal;
  delete out._rawError;            // R2：不洩漏 raw 進 digest（避免明文/未遮罩外洩 + 體積）
  return out;
});
```

- 保留**首次** signal 的 `error`/`file`/`context` 作代表性樣本，`error` body **不被污染**（見 §4.4）。
- `files.size` 帶出「跨幾個檔」—— metadata 明示跨 N 檔的反覆模式，而非單檔單次。
- `_rawError` 是聚合內部狀態：**務必聚合後 `delete`**，否則 raw（可能含未遮罩 secret）會進 L0。§6 加 case 驗證 digest signal 不含 `_rawError`。
- **硬約束（R3）**：在 `delete _rawError` 之前**不得**對 `signals` 做任何 debug dump / `JSON.stringify` / 寫檔，避免 raw 在移除前外洩。

### 4.4 決策：`count` 如何抵達 L0 content（**plan 的核心拍板點**）

> **R1 修正（gpt-5.5）**：原推薦「count 編進 `error` 字串」會**語義污染** —— compile LLM 把「重複 N 次」當成原始錯誤文字一部分；且不同 count → 不同 error body → 破壞未來去重。改為：**count 以穩定 ASCII marker 編入 `context` 欄位**（白名單既有欄位），`error` body 保持乾淨原文。

```
function withRepeatMeta(signal, count, fileCount) {
  const marker = `[repeat_count=${count} same_session=true${fileCount > 1 ? ` files=${fileCount}` : ''}]`;
  return { ...signal, context: signal.context ? `${marker} ${signal.context}` : marker };
}
```

仍維持「**只動 JS、不碰 Rust `format_signal`**」（context 已在白名單）。三選一比較：

| 選項 | 做法 | blast radius | 取捨 |
|---|---|---|---|
| **(a′) ASCII marker 進 `context`**（**採用**） | `[repeat_count=N same_session=true files=M]` 前綴進 context | **僅 JS 一檔** | error body 乾淨、marker 機器可辨識可正則回收、不碰 critical Rust；context 是 metadata 的天然位置 |
| (a) marker 進 `error` body | 併進 error 字串 | 僅 JS | R1 否決：污染 error 語義 / 破壞去重 |
| (b) Rust `format_signal` 加結構化 `count` 欄 | 跨 JS+Rust | 動 critical pipeline | 結構化最佳但超範圍；歸 flow-metrics BACKLOG |

**採用 (a′)**：兼顧 R1 的「機器可辨識穩定 marker」與「不污染 error body」，且仍只動一檔。結構化 count（選項 b）歸第三層 flow-metrics。

- marker 為純 ASCII → 不受 `mask_secrets` 影響（無高熵 token、無 secret 前綴）。
- `same_session=true` 明示這是單 session 內聚合，與未來跨 session 累計區隔。

### 4.5 邊界
- 聚合**僅限同一 digest（同 session）內**。跨 session 累計屬第三層 flow-metrics。
- 只聚合 `error-recovery`；`user-correction`/`self-correction` 不動。
- 聚合在 `extractErrorRecoveries` 內完成，不污染 `main()` 的 `[...errorRecoveries, ...]` 組裝。
- **§3 濾除先於 §4 聚合**：純噪音先在 §3 被 `continue` 濾掉，不進聚合 Map（避免噪音被計數）。

---

## 5. 範圍（三層，前兩個本 plan 做、第三個記 BACKLOG）

| 改進 | 規模 | 動哪 | 本 plan |
|---|---|---|---|
| 1. 實質內容門檻（濾純 exit code） | 小 | `session-digest.js` | ✅ |
| 2. 同 session 同類聚合計數（帶次數） | 中 | `session-digest.js` | ✅ |
| 3. flow-metrics 獨立 surface（per-session 反覆踩坑報告 + 結構化 count + 跨 session 累計） | 大 | 新功能 | ⛔ → BACKLOG |

**Out of scope**：改 `format_signal`（除非 reviewer 推翻 §4.4 決策）、改 compile、改 ingest 過濾、跨 session 聚合、新 CLI surface。

---

## 6. 測試計畫

> **R1 修正**：測試須以 helper **export + 獨立 node test script**（如 `.claude/scripts/session-digest.test.js`）跑，**不得**用 `require.main` self-test 塞進 production 執行路徑（避免影響正常 digest 生成）。`session-digest.js` 須 export `isPureExitStatusNoise` / `recoverySignature` / `withRepeatMeta`（或經一個薄 testable 入口）供測試 require；正常 hook 執行路徑不變。不得引入外部 dep（對齊檔頭「zero external dependencies」），用 node 內建 `assert`。離線跑、餵造的資料，不依賴真 transcript。

必涵蓋 case（粗體為 R1 點名的關鍵 regression）：
1. **純 exit code 被濾**：`error="Exit code 2"` 的 recovery → 不進 signals。
2. **真實格式變體全濾**：對 §3.2 fixture 每一條（`Command failed with exit code 2` / `Process exited with code 1` / …）→ `isPureExitStatusNoise` 回 true。
3. **短實質 error 零誤殺（R1 #1）**：`ENOENT` / `EACCES` / `Killed` / `not found` / `SIGTERM` → 放行（residual 非空）。
4. **multiline 只剝樣板**：`"Exit code 1\nTypeError: foo is undefined"` → 真 error 殘留 → 放行。
5. **B 類跨檔聚合**：同 session「File has not been read yet」在 6 個**不同檔** → 1 條、`context` 含 `[repeat_count=6 same_session=true files=6]`，`error` body 保持原文無污染。
6. **count==1 無 marker**：單次 → 無 `repeat_count` marker、context 不被加料。
7. **長共同前綴不同錯誤不誤併（R1 #3）**：兩個 error 前 80+ 字相同但尾段不同（如 `failed to run custom build command for crate-A` vs `…for crate-B`）→ signature 不同 → 不合併。
8. **錯誤碼類 token 保留（R1 #4）**：`HTTP 500` vs `HTTP 404`、`E0277` vs `E0308` → 不同 signature → 不誤併。
9. **同錯誤不同路徑/行號應合併**：`error at /a/foo.rs:12:3` vs `error at /b/foo.rs:88:1`（同錯誤語句）→ 路徑/行列號被 normalize → 同 signature → 合併計數。
10. **不同 tool 不誤併**：`Read` 的 X 錯 vs `Bash` 的 X 錯 → signature 不同。
11. **濾除先於聚合**：純噪音不進聚合 Map（不被計數）。
12. **signature 用 raw 非 truncate（R2 #1）**：兩 error 前 300 字完全相同、第 301+ 字不同 → signature 不同 → **不合併**（證明用的是 raw `_rawError` 而非 truncate 後 `error`）。
13. **path detector 不吃 slash phrase（R2 #2）**：`read/write permission denied` vs `read/execute permission denied` → `read/write`、`read/execute` **不**被當路徑 → signature 不同 → 不誤併。
14. **relative path 正常 normalize（R2 #2）**：`src/foo.rs:1:1 error X` vs `lib/foo.rs:9:9 error X`（同錯誤語句、不同檔/行） → 路徑+行列號被 normalize → 同 signature → **合併**（且 `files=2`）。
15. **`_rawError` 不洩漏**：聚合後輸出的每個 digest signal 物件**不含** `_rawError` 欄位（防 raw 未遮罩 secret 進 L0）。

---

## 7. 部署（實作完成後）

1. `node --check .claude/scripts/session-digest.js`（語法）+ 跑獨立 test script。
2. 改的是 **source**（`.claude/scripts/session-digest.js`），installed copy 在 `~/.local/share/codeforge/hooks/<ver>/`。
3. `codeforge install --hooks --dry-run` 預覽 → `codeforge install --hooks` 重裝 installed copy。
4. **install 後驗收（R1 補）**：`diff` source 與 installed copy（或比 checksum）確認真的同步，避免 dry-run 顯示更新但實際 hook copy 未變：
   `diff .claude/scripts/session-digest.js ~/.local/share/codeforge/hooks/<ver>/session-digest.js && echo SYNCED`
5. （binary 本身未改 → 不需 rebuild/部署 binary；本 plan 不動 Rust。）

---

## 8. 驗收標準（quantifiable）

- [ ] **AC1**：餵造的 16-signal transcript（含 6× File-not-read 跨 6 檔 + ≥2 純 exit code），extraction 輸出從 16 降到去重後類別數（純 exit code 全濾、6× 併 1），且 File-not-read 那條 `context` 含 `[repeat_count=6 same_session=true files=6]`、`error` body 保持原文。以 test assert 驗。
- [ ] **AC2**：`node --check` pass + 獨立 test script 全綠（§6 case 1–15）。
- [ ] **AC3**：**短實質 error 零誤殺**（R1 #1）—— §6 case 3/4 全綠。
- [ ] **AC4**：**不誤併**（R1 #3/#4）—— §6 case 7/8/9/10 全綠：長共同前綴不同錯誤不合併、錯誤碼類 token 保留、同錯誤不同路徑/行號合併。
- [ ] **AC5**：`format_signal`（Rust）**未改**（§4.4 採 (a′)，只動 JS）。Rust 端不在本改動範圍 —— `cargo test` 僅作整體 smoke（若紅且與 JS 無關，不算本 plan failure）。
- [ ] **AC6**：`codeforge install --hooks --dry-run` 顯示將更新 session-digest.js、無 dual-fire / orphan；install 後 §7.4 `diff` 顯示 SYNCED。

---

## 9. /l5 orchestration 劇本（user 指定）

1. **本 plan** → `/l5` **gpt-5.5 / xhigh** loop review 這份 plan，改到通過。
2. plan OK → `/l5` **agy flash 3.5 / high** 實作。
3. 實作 → `/l5` **gpt-5.5 / xhigh** review 實作。
4. main 全程只 dispatch + 收 depth-0 qc verdict，不下海實作/review。

## 10. BACKLOG 追加（第三層）

> **flow-metrics 獨立 surface**：把「錯誤頻率」從 lesson 路徑分出，建立 per-session 反覆踩坑報告（結構化 `count`、跨 session 累計、方法論視角）。觸發：本 plan 改進 2 落地、使用者要看「我反覆踩哪些坑」時。屆時一併做 §4.4 選項 (b)（Rust `format_signal` 結構化 count）。

---

## 11. As-built 偏離記錄（實作後回填，2026-06-25）

實作 = agy Gemini 3.5 Flash High（commit `4cd8596`）+ main 依 gpt-5.5 impl review 修正（`3c9f59e`）。對 plan 文字的偏離（**實作 code 為最終 SoT**）：

- **§3.3 `NOISE_LINE_PATTERNS` 實際 3 條**（plan 寫 2 條 + 「取樣補全」）：實作補齊了 `Error:` / `[error]` / 括號包裝變體（對齊 R3 non-blocking 的真實樣本要求），且加了一條獨立 `error: exit code N` pattern。fixture 內聯在 test case 2 的 `noiseVariants`，未另建 `__fixtures__/` 檔（更輕量、同效）。
- **§4.2 移除泛化 tmp-hash rule**：plan §4.2 的 `recoverySignature` 原列 `.replace(/[.\-][0-9a-f]{6,}\b/, '<tmp>')`。gpt-5.5 impl review 指出它會誤吃 `crate-abcdef12` / `commit-<hash>` → 不同 build error 誤併；路徑內臨時檔已由 `PATH_RE` 處理。**已移除該行**（實作 code + 註解說明）。
- **§3.3 / §4.2 型別防護**：兩 helper 開頭加 `const text = typeof x === 'string' ? x : String(x ?? '')`（export 公開 API 的 robustness；對既有 string 輸入零行為改變）。
- **§6 測試 case 16–18 新增**（plan 列 1–15）：16 = 主流程聚合確用 `_rawError`（前 300 字同尾段異不誤併）；17 = `count===1` 的 `_rawError` delete 路徑；18 = 同檔重複 marker 省略 `files=` 段（固定 plan §4.4 刻意設計）。**本機實跑 18/18 綠**。
- **§4.4 files= 省略為刻意設計**：gpt-5.5 曾建議永遠輸出 `files=M`；經裁決維持 plan 原設計（fileCount>1 才顯示），case 18 固定之。
- **未動**：Rust `format_signal`、其他 extractor、parse 層（transcript shape 不在範圍）。
