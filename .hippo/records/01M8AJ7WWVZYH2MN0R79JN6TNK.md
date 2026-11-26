---
id: 01M8AJ7WWVZYH2MN0R79JN6TNK
kind: issue
created: 2026-11-19T00:00:00.000Z
---
- **Since**: 2026-11-19
- **Problem**: 推版走 `cargo install`，沒加 `--remap-path-prefix`，binary 帶了 clone 路徑和 `$HOME`，同一 commit 各機 sha16 不同，只能看 HEAD 加功能字串驗收。`package.sh` 已可重現（B43），缺推版改走它，以及跨 `$HOME`、跨使用者的 repro 驗證。
- **Impact**: 推版不能直接比 sha16，驗收得多一步功能檢查，也證明不了各席跑的是同一份 binary。（Minor）
- **Trigger**: ready；做法是推版改從 `package.sh` 的封存檔安裝（或套用同一組 RUSTFLAGS），再補一個跨 `$HOME`、跨使用者的 repro 檢查；推版腳本那一側歸 hangar
- **Source**: 01M7QPEX 推版結果（14/14 7bd44069）與 hangar 01M7QQ2Y 的比對
- **Pointer**: none
- **Context**: 同一 commit、同一 rustc，建出來應該一樣；做好後推版要求可以改成「各席 sha16 等於 package.sh 封存檔裡的那支」
- **Done-Check**: `bash scripts/repro-check.sh x86_64-unknown-linux-gnu`
- **Artifact**: code
- **Needs**: none
- **ms**: none
- **Used-by**: e2e
- **Queued-By**: cookys/host-console/pm--claude 01M7QPZ6
