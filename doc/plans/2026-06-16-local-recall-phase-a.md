# Plan — 本地 recall 注入器(Phase A)

> Created: 2026-06-16 · Owner: cookys · Size: L · Branch: `feature/local-recall`
> Design spec(母本):[`doc/proposals/2026-06-16-memory-recall-and-stolen-patterns.md`](../proposals/2026-06-16-memory-recall-and-stolen-patterns.md)

## 背景

READ 側缺口:standalone(無 mnemos)使用者 distill 出的 L1 知識,除非手動 `memory search`,不會自動回 session。Phase A 補上**本地自動 recall**,對稱於 `mnemos-cli context`(中央版)。機制已由研究定案:**SessionStart hook → `additionalContext`**(官方、10K 上限、免 server),放 settings.json(plugin hooks.json 有 #16538 不可靠)。

## Final Goal

> codeforge 在每個 session 開始,自動把本地 top-N L1 知識(lean ranked index,~1.5K token)注入 context;不需 mnemos、不需 autopilot。

## Success Criteria(可量化)

| # | 條件 | 驗證 |
|---|------|------|
| 1 | `codeforge memory context`(本地)輸出 ranked L1 index markdown,**≤ ~1,500 token / 可設上限**,每條帶 citation(topic/id) | 單元測試斷言 budget cap + citation 欄位;手動跑看輸出 |
| 2 | `--hook` 模式輸出合法 SessionStart JSON(`hookSpecificOutput.additionalContext`) | 單元測試斷言 JSON 形狀;`echo '{}' | codeforge memory context --hook` 實跑 |
| 3 | 排序 = strength 為主(recency 加權),取 top-N 至 budget | 單元測試:高 strength/新近項優先 |
| 4 | `codeforge install` 把本地注入器寫進 global SessionStart | install 單元測試斷言 hook entry |
| 5 | `projection/mod.rs` 死碼退役(移除或明確重導向) | grep 無殘留 dead_code projection;build 綠 |
| 6 | 全綠 | `cargo check + clippy + test` 零失敗 |
| 7 | live 驗證 | 實裝後 SessionStart 注入可見(或 `--hook` dry 輸出 JSON 正確) |

## Scope Boundary

- **IN**:本地 `memory context` 命令(lean index + budget + citation + ranking)、SessionStart hook 接線(settings.json)、projection 退役、seam 契約一頁文件、procedural-atom 標記(便宜、dialectic 推薦)、doc sync。
- **OUT**(後續 phase / defer):async worker(Tier 2/Phase B)、mem0 對賬(Phase B)、本地語意 embedding recall(Tier 3,預設不做)、companion skill-distiller(defer)、skill-extraction(dialectic defer)。

## 命名(naming-before-specs)
- 本地命令:**`codeforge memory context`**(在既有 `memory` subcommand 下,與 `memory search` 並列;對稱於中央的 `mnemos-cli context`)。
- hook 模式 flag:`--hook`(輸出 SessionStart JSON);非 hook 模式輸出純 markdown(供人看 / 管線)。

## Phases

- **A0** — `codeforge memory context`:`scan_l1` → rank(strength + recency 加權)→ budget 截斷(預設 ~1500 token,可 `--max-tokens`)→ lean markdown index(每條 topic + citation id)。`--hook` 模式包成 SessionStart JSON。+ 單元測試。
- **A1** — install 接線:`install.rs` global SessionStart 加 `codeforge memory context --hook`(settings.json,marker)。+ install 單元測試。注意與既有 emit-session/dream/ship 鏈共存順序。
- **A2** — projection 退役 + seam 契約文件(一頁,atom→consumer 契約 + `project` scope + procedural-atom tag schema)+ L1 frontmatter 加 procedural/corrective 標記(延伸 `candidate_atom_kind`)。
- **A3** — doc sync(CLAUDE.md READ/WRITE 對稱表、CHANGELOG、ship/memory spec)+ build/install 重部署 + live 驗證。

## Quality Gate
`cargo check + clippy + test` → `superpowers:requesting-code-review`(`src/cli/` + `src/memory/` 命中)→ 修 CRITICAL/IMPORTANT → round 2 → merge(finish-flow)。

## Anti-patterns 守則(研究 VERIFIED,直接約束實作)
- 注入**精簡 ranked index,不 dump**(claude-mem v3→v4 context pollution 教訓)。
- 不 clobber CC auto-memory 的 MEMORY.md(走 additionalContext,不寫 memory 目錄)。
- budget 硬上限,超出截斷(防 context rot)。
