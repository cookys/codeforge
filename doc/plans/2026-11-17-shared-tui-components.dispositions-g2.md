# 第二代審查處置（plan@7259493 → g2）

第二代（封頂）：凍結檢查在 7259493 通過（check-phase-review-receipt exit 0），以下為 depth-0 裁決後折入。

指紋為前 8 碼。全部採納，沒有不採納項目。

| 指紋 | 席位 | rubric | 阻擋 | 處置摘要 | 折入章節 |
|---|---|---|---|---|---|
| e9bbd4fd | astra | R5 | 是 | `readPet()` 整個 icon tuple 比對單一白名單條目；< 80 標籤從條目推出；兩級寬度檢查繪出標籤；負向測試（79 欄） | §6 |
| d9ad3bfa | astra | R2 | 否 | 由 7361ffb8 的改法涵蓋：`tux-cross` 執行 AP 測試；示範改 AP 分頁順序 → codeforge 紅 | §4.3 |
| a3c62ad9 | astra | R9 | 是 | P3 專用 `pet-cross`（擁有者 P2/P1b），sibling 不在就紅；hangar live pin 與 release-local.sh 都跑；P5 後由 `tux-cross` 取代 | §4.3、§8 P2/P3/P5 |
| 7361ffb8 | carbon | R2 | 是 | `tux-cross` 在 `$AUTOPILOT_DIR` 跑 `node --test`、停用遞迴、核對 VENDOR.lock；`tabrow-*` 只在 AP 側並從 codeforge golden 清單移除 | §4.1、§4.2、§4.3、§8 P5 |
| 3f1bdfdf | carbon | R5 | 是 | L19 同時擋字面 `codeforge.panel` 與 `panel.ts` import／re-export；`readPet()` 永久例外、`readLegacyUsage()` 過渡白名單（P6b 刪） | §6、§8 P2/P6b |
| ab40900c | carbon | R5 | 是 | 同 e9bbd4fd（tuple 比對、fallback 從白名單推出、兩級寬度檢查） | §6 |
| 25eb3217 | carbon | R9 | 是 | P4a 只寫 tokens.json 與獨立 tokens.test.js（子集、thresholds 相等），不接 run-tests.sh；嚴格相等、接線、gen-locales/locales.ts 移到 P4b | §2.1、§5、§8 P4a/P4b |
| 7751883f | carbon | R1 | 否 | `gen-tux.js --check` 也產 `THRESHOLDS` 到 `palette.ts`（入 manifest）；F3 註明 `pctInt(ratio) >= THRESHOLDS.warn` | §1.1 F3、§2.1 |
| 43a3db3e | carbon | R6 | 否 | roles 每值須為 CC 八主題鍵；fleet 補的角色名與對應鍵於 P4a 定版前記進 §5（待 fleet 提供） | §5、§9 |
| ac4c03e3 | carbon | R8 | 否 | hangar live pin 自 P3 拒絕非 tag ref；AP tux VERSION>1 的 live tag 等 codeforge P5；回滾列出 AP 前一 tag 的 checkout | §7 |
