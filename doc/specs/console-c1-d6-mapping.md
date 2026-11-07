# codeforge ↔ 操作台 C1／D6 對照(PM 定案，2026-11-07)

> 依據：主計畫 `docs/plans/2026-11-01-operator-console.md` C1(L308)、D6(L311);codeforge develop adf4454 的程式盤點(每一條都附 file:line)。
> 原則：codeforge 只提供資料，以有版本的契約交出去;畫面和互動歸操作台。D6 只吃 C2 的資料，不讓模型產生資料。
> **PM 定案(2026-11-07)**:(1) codeforge 交給 C2 的資料只有 `codeforge.usage-summary/1`;agent／session 鍵現在不加，等 D6 機制頁定了才決定。(2) 寵物 XP 不進 D6;寵物、codepower、D6 三套遊戲化在 D6 機制頁送操作員之前對齊一次，不准出現三套等級制度(PM 派給 D6 owner,codeforge 屆時提供寵物這邊的定義)。(3) codeforge 不做 C1 的任何工作;forge-stats 的 fleet 區塊只留計數。

## 1. 哪些 codeforge 項目可以當 D6 的資料來源

| codeforge 項目 | 契約 | 內容 | 是否由模型產生 | 給 D6 用 |
|---|---|---|---|---|
| 用量摘要 `usage/summary.json` | `codeforge.usage-summary/1`(`src/usage/summary.rs:18`) | 7 天，按日 × 帳號 × 模型：token 五欄、requests、`cost_usd`(API 等價估價，`price.rs`,codex 沒有定價) | 否 | **可以**，給「cost efficiency」用。已經能用 `codeforge usage push` 經 fleet upload 送出(`summary.rs:294`),C2 可以直接收 |
| 每帳號配額 `usage accounts --json` | `codeforge.usage-accounts/1`(`quota_view.rs:663`) | 5h／7d 比例、重置時間 | 否(來源是 autopilot 的鏡像) | 不需要：原始資料本來就是 autopilot 的，C2 直接讀鏡像，不要經過 codeforge 轉手 |
| 寵物 XP／等級 | SQLite `xp_events`、`pet`(`schema.sql:25,44`) | 人和 codebase 的活動:commit +20、session +10/+3、learn +5、search +1(`daemon/events.rs:16`、`learn.rs:39`) | 否 | **不建議**:量的是操作員本人的活動，不是 agent 的工作。D6 的等級來自「accepted tasks」,兩套語意不同，混在一起會互相干擾 |
| 徽章 | `badges`(`schema.sql:54`) | `award` 是 dead code(`pet/badges.rs:31`) | — | 不適用 |
| ship 帳本／教訓 | `LedgerEnvelope`(`cli/ship.rs:75`) | L2 教訓文字 | **是**(claude -p→agy→codex→Haiku,`llm.rs:191`) | **排除**:D6 規定資料不能由模型產生。只有「當天有沒有 ship」這種計數是確定值，價值很低 |
| dream L1、AI 旁白 | — | 文字 | **是**(`dream/compile.rs:312`、`commentary/dispatch.rs:9`) | **排除** |

缺口：summary 只有帳號和模型，沒有 agent／task id(專案在 summary 裡已經加總掉了)。D6 要算「每個 agent 的成本效率」,得靠 C2 用帳號和時間去對 fleet task events,不然就要 codeforge 在 summary 加一個 session 或 member 鍵。**建議等 D6 的機制頁定案、確定需要再加**,現在不先做。

## 2. 跟 C1 重疊的地方

C1 的 `members|tasks|decisions|pending-ops|project` 這五個子指令,codeforge 一個都沒有，也沒有任何地方讀 fleet 的 tasks、peers、task tree、decisions 或 audit trail(`src/` 全查過)。重疊只有三點：

| codeforge | 跟 C1 的關係 | 建議 |
|---|---|---|
| forge-stats 讀 `autopilot.fleet`,只有計數 `{needYou, failed, stuck, running, waiting}`(`model.ts:311`) | 和 `tasks --stuck` 是同一批資料的摘要 | forge-stats 只留計數，不長出逐條任務清單;要看明細走 C1 |
| `codeforge usage accounts --json`(有 schema、有 `--json`) | C1 沒有配額子指令;如果 `members` 想加配額欄，就會重疊 | C1 直接讀 autopilot 的鏡像，或叫這個 JSON,不要重寫一套鏡像解析 |
| `usage push` → fleet upload | 這是 C2 資料流的入口，不屬於 C1 | 列成 C2 的輸入之一(見 §1) |

## 3. 定案的三點(原本是提案，PM 已照這三點定案)

1. codeforge 交給 C2 的資料只有一份:`codeforge.usage-summary/1`(已經存在，也已經能推送)。要不要加 agent／session 鍵，等 D6 機制頁定了再決定。
2. 寵物 XP 不進 D6。現在已經有三套遊戲化:codeforge 寵物、codepower、D6。你之前裁定寵物分頁排期前要先和 codepower 對過，建議 D6 也一起對，避免三套等級制度。
3. codeforge 不做 C1 的任何工作;forge-stats 的 fleet 顯示只留計數。
