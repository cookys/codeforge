# P6a 草稿：forge-stats 從「畫 pane」改成「只出資料」

狀態：草稿。依據共用 TUI 計畫（`plan/shared-tui` 的 `doc/plans/2026-11-17-shared-tui-components.md`）§3.4 與 §8 的 P6a／P6b；PM 01M7RC5W 已同意 §3.4：v2 的內容改當 autopilot-live 的分頁，UX 閘門重跑，兩家都要 ≥8 且沒有 Critical。
P6a 的程式改動要等 P4b、P5，這份只先定兩件事：(1) panel/1 要補哪些結構化欄位；(2) 哪些 v1 pane 程式在 P6b 上線後要刪。

## 1. 現況

- `codeforge.panel/1` 唯一的寫入端是 forge-stats 的 `writePanel`（`plugins/forge-stats/hooks/register.tsx`，建構在 `hooks/panel.ts`、`hooks/side.ts`），Rust 側沒有。型別在 `plugins/forge-stats/types/index.d.ts` 的 `PanelState`。
- 目前的內容幾乎都是**依寬度排好的文字**：`usage.tiers`、`pet.tiers`、`side.*.tiers`。autopilot-live 拿到只能照貼，沒辦法重新排版。
- v2（`feat/fs-pane-v2-r2`）沒有待合併的 commit：develop 合過一次，因 PM 01M7M9J8 撤回（6753680）。程式還在分支上，早於 `pet-icon.ts`，要重用得先 rebase。

## 2. v2 每一區的資料歸屬

| v2 區塊 | 資料來源 | 歸誰出 |
|---|---|---|
| 待決、卡住、背景計數、資料年齡 | `fleet.brief/1` | **fleet.console/1**（autopilot 直接讀，§6），不進 panel/1 |
| 各池（方案、週窗、5h、浪費／不足警告、耗盡預估、備註、來源狀態） | `codeforge.fleet-quota/1` | **fleet.console/1** 的帳號區；codeforge 只維持 fleet-quota/1 這個輸入 |
| 交辦 | 目前沒有任何來源（等 fleet `feat/operator-origin`） | fleet，P6 不做 |
| 寵物分頁（名字、等級、XP、五項能力、策略、訊息、村莊、sprite、圖示） | `pet.json` | **panel/1**（下面 3.1） |
| 本機模型用量（前 6 名：tokens、請求數、花費、未定價） | `usage/summary.json` | **panel/1**（下面 3.2） |

原則：codeforge 只出它自己擁有的資料（寵物、本機用量）；fleet 的東西不經過 codeforge 轉手。

## 3. panel/1 新增欄位（全部是選填，schema 名稱不變）

舊讀取端看不到新欄位也照常運作；新讀取端缺欄位時退回現有的 `tiers` 文字。

### 3.1 `pet.stats`

```ts
pet.stats?: {
  name: string; level: number; xp: number; xp_to_next: number
  hp: number; atk: number; def: number; sup: number; ver: number
  strategy: string | null; message: string | null
  village: { id: string; display_name: string; language: string }
  written_at_ms: number            // pet.json 的 written_at，讀取端算資料年齡
}
```

### 3.2 `usage.models` 與 `usage.generated_at_ms`

```ts
usage.models?: { model: string; tokens: number; requests: number; cost_usd: number | null }[]  // 依 tokens 由大到小，最多 12 筆；null = 未定價
usage.total_usd?: number           // 已定價的合計；有未定價項時讀取端畫 `≥`
usage.generated_at_ms?: number     // summary.json 的 generated_at
```

金額只出現在 `usage`；`side` 維持不帶金額（`doc/specs/panel-band-contract.md` §2）。

### 3.3 不加的

- 警告／錯誤色調：維持計畫 F6，panel/1 不加，警示色走 fleet.console/1。
- 任何 fleet.brief 欄位。

### 3.4 契約測試

- 寫入端：`plugins/forge-stats/tests/quota-ui.test.tsx` 的 panel 區段加上 `pet.stats`、`usage.models` 的正常、缺欄位、毒值案例。
- 讀取端：autopilot `mods/live/panel-contract.test.js`。P4b 之後兩邊共用 `mods/tux/golden/panel-cases.json`（計畫 §4.1），任一邊改欄位名稱，兩邊都紅。

## 4. P6b 上線後要刪的 v1 pane 程式

**前提：autopilot-live 的寵物和本機用量分頁已經上線（P6b），而且 `/forge-stats` 改成開 autopilot 的分頁。** 在那之前不刪，否則操作員會沒有 pane。

- `hooks/register.tsx`：`PANE` 常數、`paneTab`／`paneScroll`／`paneTotal`、Pane 的 `ui.render` 與 `ui.scroll`、`readFleetQuota`、`fleetPresent`／`fleetBases`／`refreshFleetPresent`、sprite 動畫（`spriteFrame`、`stepSprite`、`lastCells`、`SPRITE_KEY`）、`rule()`。
- 整檔：`hooks/pane-view.ts`、`hooks/fleet-view.ts`。
- `hooks/model.ts`：`halfBlockCells`、`SPRITE_FRAME_MS`、`SPRITE_SIZE`、`KEY_W`／`padRight`／`padLeft`（先 grep 確認沒有其他呼叫者）。
- `hooks/strings.ts`：`tabPet`、`tabUsage`、`tabFleet`、全部 `fleet*`、`paneHint`、`paneMore`、`usage`（標題）、`title`。
- 測試：`tests/pane-chat.test.tsx`、`tests/fleet-tab.test.tsx` 整檔；`forge-stats.test.tsx`、`usage.test.tsx`、`quota-ui.test.tsx` 裡畫 Pane 的那些案例。
- **保留**（panel/1 或 band 還在用）：`petLines`、`usageLines`、`quotaPaneLines`、`parseSprite`、`parseUsage`、`countdown`、`displayWidth`、`side.ts` 全部、`panel.ts` 全部、band 與 `no-top-tabrow` 測試。

## 5. 順序

1. P4b（autopilot）合入後，codeforge 開 P5（改用 vendor）。
2. P6a：在 forge-stats 加 3.1、3.2 的欄位與寫入端測試（不刪任何東西）。
3. P6b（autopilot）：畫分頁、讀新欄位；UX 閘門重跑。
4. 上線並由操作員實際看過之後，codeforge 再開一個 PR 做第 4 節的刪除。
