# 逐次驗證索引

每個 receipt 記錄子命令 exit code；同名 log 保留測試與失敗輸出（省去 UX JSON 與行尾空白）。`green` 是當時嘗試名稱，是否成功以 exit code 為準。原始輸出的 SHA256 包含未提交的大量 UX JSON；畫面由 shots/manifest.json 獨立記錄。

| 執行 | exit code | 日誌 |
|---|---|---|
| [01-c2-green](01-c2-green.json) | 0 | [01-c2-green.log](01-c2-green.log) |
| [01-c2-red](01-c2-red.json) | 1 | [01-c2-red.log](01-c2-red.log) |
| [02-c1-green](02-c1-green.json) | 1 | [02-c1-green.log](02-c1-green.log) |
| [02-c1-red](02-c1-red.json) | 1 | [02-c1-red.log](02-c1-red.log) |
| [02-c1-verified](02-c1-verified.json) | 0 | [02-c1-verified.log](02-c1-verified.log) |
| [03-m1-green](03-m1-green.json) | 0 | [03-m1-green.log](03-m1-green.log) |
| [03-m1-red](03-m1-red.json) | 1 | [03-m1-red.log](03-m1-red.log) |
| [04-k1-green](04-k1-green.json) | 1 | [04-k1-green.log](04-k1-green.log) |
| [04-k1-red](04-k1-red.json) | 1 | [04-k1-red.log](04-k1-red.log) |
| [04-k1-verified](04-k1-verified.json) | 0 | [04-k1-verified.log](04-k1-verified.log) |
| [05-m2-green](05-m2-green.json) | 0 | [05-m2-green.log](05-m2-green.log) |
| [05-m2-red-isolated](05-m2-red-isolated.json) | 1 | [05-m2-red-isolated.log](05-m2-red-isolated.log) |
| [05-m2-red](05-m2-red.json) | 1 | [05-m2-red.log](05-m2-red.log) |
| [06-m3-green](06-m3-green.json) | 0 | [06-m3-green.log](06-m3-green.log) |
| [06-m3-red](06-m3-red.json) | 1 | [06-m3-red.log](06-m3-red.log) |
| [07-m4-green](07-m4-green.json) | 0 | [07-m4-green.log](07-m4-green.log) |
| [07-m4-red](07-m4-red.json) | 1 | [07-m4-red.log](07-m4-red.log) |
| [08-k2-green](08-k2-green.json) | 0 | [08-k2-green.log](08-k2-green.log) |
| [08-k2-red](08-k2-red.json) | 1 | [08-k2-red.log](08-k2-red.log) |
| [09-k3-green](09-k3-green.json) | 1 | [09-k3-green.log](09-k3-green.log) |
| [09-k3-red](09-k3-red.json) | 1 | [09-k3-red.log](09-k3-red.log) |
| [09-k3-verified](09-k3-verified.json) | 0 | [09-k3-verified.log](09-k3-verified.log) |
| [10-k4-green](10-k4-green.json) | 0 | [10-k4-green.log](10-k4-green.log) |
| [10-k4-red](10-k4-red.json) | 1 | [10-k4-red.log](10-k4-red.log) |
| [11-shots](11-shots.json) | 0 | [11-shots.log](11-shots.log) |
| [12-m2-alignment-green](12-m2-alignment-green.json) | 0 | [12-m2-alignment-green.log](12-m2-alignment-green.log) |
| [12-m2-alignment-red](12-m2-alignment-red.json) | 1 | [12-m2-alignment-red.log](12-m2-alignment-red.log) |
| [13-c1-overflow-green](13-c1-overflow-green.json) | 1 | [13-c1-overflow-green.log](13-c1-overflow-green.log) |
| [13-c1-overflow-red-exact](13-c1-overflow-red-exact.json) | 1 | [13-c1-overflow-red-exact.log](13-c1-overflow-red-exact.log) |
| [13-c1-overflow-red](13-c1-overflow-red.json) | 1 | [13-c1-overflow-red.log](13-c1-overflow-red.log) |
| [13-c1-overflow-verified](13-c1-overflow-verified.json) | 0 | [13-c1-overflow-verified.log](13-c1-overflow-verified.log) |
| [14-c1-single-pool-green](14-c1-single-pool-green.json) | 0 | [14-c1-single-pool-green.log](14-c1-single-pool-green.log) |
| [14-c1-single-pool-red](14-c1-single-pool-red.json) | 1 | [14-c1-single-pool-red.log](14-c1-single-pool-red.log) |
| [15-row-padding-green](15-row-padding-green.json) | 0 | [15-row-padding-green.log](15-row-padding-green.log) |
| [15-row-padding-red](15-row-padding-red.json) | 1 | [15-row-padding-red.log](15-row-padding-red.log) |
| [final-check-all](final-check-all.json) | 0 | [final-check-all.log](final-check-all.log) |
| [final-doc-drift](final-doc-drift.json) | 0 | [final-doc-drift.log](final-doc-drift.log) |
| [final-plugin](final-plugin.json) | 0 | [final-plugin.log](final-plugin.log) |
| [final-shots](final-shots.json) | 0 | [final-shots.log](final-shots.log) |

[範圍與程式雜湊](scope.json)；[最終驗證彙整](../round-1-verification.json)。
