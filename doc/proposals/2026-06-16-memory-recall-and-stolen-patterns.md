# Design Spec — CodeForge 記憶 recall 補全 + Stolen Patterns

> Created: 2026-06-16 · Status: **待 approve(brainstorm gate)** · Owner: cookys
> 源頭:本 session brainstorm → think-tank-dialectic → 外部 prior-art 研究(6 路)。
> 這是 design spec(feed plan),不是 plan 本身。

## 1. 問題

完整價值迴圈 = **吸收 → distill → 存 → recall(用回來)**。READ 側目前不對稱:

| | 本地(always,免 mnemos) | 中央(opt-in,需 mnemos) |
|---|---|---|
| **WRITE** | `dream`(L0→L1)✅ | `ship`(→mnemos)✅ |
| **READ** | **缺口** —— `projection`(L1→~/.claude memory)是死碼且機制錯 | `mnemos-cli context`✅ |

standalone(無 mnemos)使用者:知識 distill 進 `.codeforge/store/` 後,除非手動 `memory search`,**不會自動回到 session**。

## 2. 已定案的架構決策(本 session 收斂)

1. **注入機制 = SessionStart hook 回傳 `hookSpecificOutput.additionalContext`**(VERIFIED 官方,10K 上限,session 開始前載入,免 server,互蓋極小)。業界標準(claude-mem / claude-self-reflect / SuperBrain 全用),且 `mnemos-cli context` 已是此機制。
2. **READ 側補本地注入器**:`codeforge` 一個本地 context 命令(讀本地 top-N L1 → lean index markdown),由 SessionStart hook 注入。對稱於 `mnemos-cli context`(opt-in 中央版)。
3. **注入 hook 放 settings.json(codeforge install),不放 plugin**:GitHub #16538 —— plugin hooks.json 的 additionalContext 有時不被注入,settings.json 才可靠。與「不急著 plugin 化、保留 cargo 自足 + marker」一致。
4. **`projection/mod.rs` 退役**:寫進 ~/.claude memory 目錄機制錯(該目錄 Claude 自有,只 `MEMORY.md` 前 200 行保證載入,丟入檔不可靠載入)。改為 §2 的本地注入器。
5. **skill-extraction 維持 defer**(think-tank-dialectic 結論):codeforge 守「知識/記憶」車道,當 procedural-atom 生產者;skill 格式化交可插拔 consumer(autopilot:distill)+ 明文契約。codeforge 標 procedural atom、出 seam 契約。
6. **codeforge / autopilot 互不依賴**;universal-dev-skills 殘留退役;靠 mnemos + 明文契約鬆耦合。

## 3. Stolen Patterns(分 tier;偷好偷滿但排序)

### Tier 1 — 現在偷(便宜、高價值、正中缺口)
- **T1.1 Lean budgeted index 注入**(claude-mem v3→v4 / SuperBrain / Anthropic auto-memory):SessionStart 注入 **~1,200–1,500 token 的 ranked précis,不 dump**。claude-mem v3 dump 全部 → context pollution → v4 撤退。這是鐵律。
- **T1.2 Progressive disclosure(index push + detail pull)**:注入 index;詳情靠既有 `codeforge memory search`(FTS5)on-demand pull。pull 側已存在,只補 push。
- **T1.3 Citation-by-ID**:注入的每條帶 atom id/topic,供 pull 詳情 + 對齊 mnemos cite。
- **T1.4 File-first / DB-as-rebuildable-index**(basic-memory):L1 markdown 是 source of truth、FTS 是可重建衍生索引 —— codeforge 已如此,spec 明文化(可 `rebuild` FTS)。
- **T1.5 「storage solved, injection isn't」**(SuperBrain):把工程重心放在**排序/浮現**,不是儲存。

### Tier 2 — 接著偷(真價值、中成本)
- **T2.1 Async fire-and-forget worker**(claude-mem <20ms 返回 / memex commit-gated):dream/ship 目前**同步**跑在 SessionEnd hook,量大會卡 session 結束。改 hook 快速 enqueue、distill 丟背景。⚠️ 與本 session 剛上線的 dream→ship SessionEnd 鏈直接相關。
- **T2.2 mem0 式 ADD/UPDATE/DELETE/NOOP 對賬**:dream-compile 時新事實和現有 L1 對賬,而非 append-only → 殺掉 stale/矛盾累積。(目前是 append。)
- **T2.3 排序 = recency × importance × relevance**(Generative Agents):codeforge 已有 ACT-R strength + mnemos citation-count;整成統一 ranking 餵 T1.1 的 index 選取。

### Tier 3 — 之後偷 / 先評估(高價值但較大或需驗證)
- **T3.1 本地語意 recall**(claude-self-reflect v8:FastEmbed 384d + HNSW,單 binary,免 API):codeforge 目前 FTS5 keyword-only;語意 > keyword。評估嵌入式本地 embedding 升級 recall(較大 lift)。
- **T3.2 失敗/卡關為一等記憶**(claude-self-reflect「Ralph loop」/ ReasoningBank / ExpeL):捕捉 iteration/failure pattern,不只結果。
- **T3.3 Typed observation/relation schema**(basic-memory:`[category]` facts + typed wikilinks):L1 已有 `links`;可加 typed relation + 分類 observation。
- **T3.4 矛盾偵測 + 兩段式信心衰減**(wisdomGraph CONTRADICTS / memex bitemporal:validated ~139d、unvalidated ~30d half-life):L1 已有 strength;加 validated-vs-unvalidated 區分 + 矛盾標記。

## 4. 必守的 Anti-patterns(研究 VERIFIED)
- 別 bloat eager 檔(>200 行傷 adherence);別 dump(context rot:Lost-in-the-Middle 2023 / Chroma Context Rot 2025 全 frontier model 隨長度退化)。
- 別把注入文字當強制力(硬規則走 PreToolUse hook,如既有 check-dev-flow)。
- 別 clobber 手寫檔(issue #1723:`#` quick-memory 把 800 行 CLAUDE.md 砍到 8 行)。
- 別開常駐 MCP(tool-schema tax:issue #29971 schema 吃掉 2/3 window)。

## 5. Open Questions(只有你能答)
1. **standalone(無 mnemos / 無 autopilot)客群多大?** → 決定是否現在做 companion skill-distiller(dialectic 的延伸,目前傾向 defer)。
2. **T3.1 本地語意 recall 要不要做?** keyword(便宜、已有)夠用,還是要 embedding 語意(大 lift、recall 品質躍升)?
3. **identity 守窄(memory/pet smithy)還是寬(完整學習探針)?** 影響 Tier 3 要吃多少。

## 6. 建議分期
- **Phase A(= Tier 1)**:本地 SessionStart 注入器(lean index + citation + FTS pull)、projection 退役、settings.json hook、seam 契約一頁文件。← 直接補 recall 缺口,自足。
- **Phase B(= Tier 2)**:async worker decouple、dream-compile 對賬、統一 ranking。
- **Phase C(= Tier 3,評估後選)**:本地語意 recall、失敗記憶、typed schema、矛盾/衰減。

## 7. Credits / Inspired By(誠實標註來源)
- **claude-mem**(thedotmack):lean index 注入 + async worker + citation-by-ID + progressive disclosure — 最近的架構雙胞胎。
- **claude-self-reflect**(ramakay):單 Rust binary + SQLite 驗證;本地語意 recall;失敗/卡關偵測;time-decay ranking。
- **basic-memory**(basicmachines-co):file-first / DB-rebuildable;typed observation/relation schema。
- **memex**(STiFLeR7):commit-gated synthesis;bitemporal 兩段式信心衰減;token-budgeted context。
- **mem0**:ADD/UPDATE/DELETE/NOOP 對賬。
- **SuperBrain / A. El Khoury**:「storage solved, injection isn't」;~1.2K token précis 注入。
- **wisdomGraph**(cklam12345):DIKW + CONTRADICTS/REINFORCES 矛盾偵測。
- 研究來源 URL 見 session 研究輸出(code.claude.com/docs/en/hooks、memory;各 GitHub repo)。
