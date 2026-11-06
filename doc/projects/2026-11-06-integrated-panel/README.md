# 整合面板 O4（band 寬度分級）

autopilot × codeforge 的整合面板：操作員 d-f0bcb1f2 選定 O3 為底＋G3 進度列，PM 2026-11-06 定案依 band 的
`bodyColumns` 分 80／120／160／200／240 五級（O4）。autopilot 畫整個 band（外殼與 reader），codeforge 的
forge-stats 寫 `codeforge.panel/1` 的資料（含本專案新增的選填 `side`）。

- 介面約定：[`doc/specs/panel-band-contract.md`](../../specs/panel-band-contract.md)（aad858d，autopilot 已確認）
- 規範：[`doc/specs/tui-guideline.md`](../../specs/tui-guideline.md) §3.2（✦ 登記）、§14（疊層與焦點）
- 設計與真機座標：`ui-preview/codeforge/ux-v7/design.md`（ui-preview 不進 git）
- 交接：[`HANDOFF.md`](HANDOFF.md)

## 成功條件

1. forge-stats 寫出 `side`（usage 30/40/50、usage_full 40、memory 30/40/50），每列不超過欄寬、不含 `$`。
2. autopilot 的 band 依 `bodyColumns` 選級，五級下題目、選項、反白、快速列座標相同，移動次數 0／0／0。
3. 五級真機截圖（只裁到面板）＋兩邊 sha 交給 PM 之後，兩邊各自 `--no-ff` 合併。
