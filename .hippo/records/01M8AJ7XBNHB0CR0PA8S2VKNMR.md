---
id: 01M8AJ7XBNHB0CR0PA8S2VKNMR
kind: issue
created: 2026-11-23T00:00:00.000Z
---
- **Pointer**: doc/specs/codeforge-usage.md
- **Status**: done（併 AP develop 27e9e5d67；真終端幾何由測試鎖住，實拍隨下次 pin 後驗收）
- **Since**: 2026-11-23
- **Problem**: (1) pm／decide feed 讀取失敗時資料清成 null，標題卻顯示 last ok 年紀，沒保留上次成功值（v74/feeds.ts 約 98–102、131–135）；(2) model.ts 的 qc_reason、readQcReason 無人寫入或呼叫，qc 另有 QcView.at 與 detail.qc_at 兩條來源只有 updated_at 時不一致；(3) pane.tsx 約 322 行有狀態列時無條件丟 body 最後一列；pages/fleet.ts 讀 errors.pm 但寫入端 key 是 errors.now；(4) band 的提醒與待決計數同為黃色、▸ 兼作提醒與清單游標與收合語彙（codex r6 兩條 Minor）。
- **Impact**: 邊角顯示與命名不一致，不擋使用；(1) 會讓失敗時的「年紀」誤導。
- **Trigger**: ready；pin 換到 7f7b5b95b 並完成真 pane 實拍後即可做
- **Source**: autopilot 審查；doc/design/ux-gate/band-layout/codex-r6.md
- **Context**: 範圍 autopilot mods/live；走 AP 分支交 autopilot 審併
- **Origin**: autopilot 審併單 01M848TE5GS4PS3AMY5C2TJ62A 與 01M8408BYV4NWMG7CPVK8X92FN
