---
id: 01M8AJ7WV3H1R1GQCC6MVB08DK
kind: issue
created: 2026-11-18T00:00:00.000Z
---
- **Since**: 2026-11-18
- **Problem**: 9 寵物分頁目前只畫佔位文字「寵物分頁還沒上線」。
- **Impact**: 操作員按 9 看不到寵物；寵物只能在這一頁畫（statusline 永不畫寵物）。
- **Trigger**: 第 2 批 pin 之後
- **Source**: PM 01M7PMAN：寵物頁排第 3 批，codeforge 主導、autopilot 實作
- **Pointer**: doc/specs/codeforge-mod-ui.md
- **Status**: dropped 2026-11-27 (舊 forge-stats pane 批次已由 v7.4 框／console v5 取代；PM 01M8CTQD2Y0NDKZE295K16MHNB)
- **Context**: 資料走 pet.json 契約；閘門照 forge-stats-ux-gate
- **Needs**: B56
- **ms**: pane-batch3
- **Used-by**: user
- **Queued-By**: cookys/host-console/pm--claude 01M7PMANY4HGR5N2J6BNAWFD6J
