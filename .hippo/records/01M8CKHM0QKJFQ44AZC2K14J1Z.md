---
id: 01M8CKHM0QKJFQ44AZC2K14J1Z
kind: issue
created: 2026-11-26T18:58:03.413Z
---
- **Since**: 2026-11-27
- **Problem**: v7.4 框（AUTOPILOT_LIVE_V5=0）在淺色主題下兩處對比不到 4.5：拿鍵盤（ctrl+x Tab）後的提示列用 claude 色，對白底 2.73；清單的 ▾ 用 subtle，2.38。v5 右 pane 已修到全列 ≥4.5（AP a9d642e51），v7.4 框沒跟上。
- **Impact**: 不到 200 欄的視窗（v5 不啟用）一律走 v7.4 框；淺色終端的操作員看不清提示列（按鍵說明）和收合符號。
- **Trigger**: ready
- **Pointer**: AP `mods/live/pane.tsx` 提示列與 ▾ 的色角色；量測腳本 scratchpad v5shot/panecontrast.py 的做法（逐列 fg/bg WCAG）；01M8CDY11V 交件串
