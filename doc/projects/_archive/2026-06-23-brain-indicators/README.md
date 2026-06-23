# Brain Connection Indicators

> Archived 2026-06-23 · merged to main `9d8bda3` · branch `feature/brain-indicators`

## Goal

statusline bottom border 寫死的 `memory ● active` → 兩顆真實狀態燈：本地腦（L1）+ 央腦 Mnemos（liveness/readiness）。加 `codeforge doctor` 診斷命令 + `codeforge mnemos-cli probe`。

## Shipped

- **雙軸健康模型**：probe=liveness / ship=readiness（防 OR 假綠）；ship 24h 新鮮度視窗；server 沒跑=中性灰 offline（非黃告警）。
- **per-machine 雙快取**（`$XDG_RUNTIME_DIR/codeforge/` liveness + ship，各單 writer、atomic temp+rename）；macOS CACHE_MAX_AGE 退路。
- **熱路徑零阻塞**：statusline 只讀快取 render；央腦 liveness 由 detached `mnemos-cli probe` 背景刷新（`process_group(0)` 隔離、stdio null、current_exe fail-soft）；O_EXCL rename-steal 鎖防 spawn herd；狀態相依 TTL + 指數 backoff。
- **渲染**：bottom_border 改「先量測再印」+ 6 級降級階梯；NO_COLOR 雙路徑（owo `if_supports_color`，外溢全 statusline 色塊）；CJK-safe vis()。
- **ship 順風車**：真 POST/flush 成功寫 readiness 快取（opt-in gated，早 return 不寫）。
- **`codeforge doctor`**：全維度診斷 + 即時前景 probe + 黃/灰態中文 next-step 建議。

## Process

- 設計：3 輪 adversarial review 收斂（R1 翻盤前提錯誤——誤信 stale doc 以為 Mnemos server 未蓋、實則 `/health` 已存在；R2 OR 假綠 + libc 編譯不過 + claim-race；R3 措辭/狀態機閉合）。spec `doc/specs/codeforge-brain-indicators.md`、plan `doc/plans/2026-06-23-brain-indicators.md`。
- 實作：6 phase（P0-P5）× per-task implementer + reviewer + fix subagent（subagent-driven-development）。每 phase spec-compliance + code-quality 雙 verdict。
- 收尾：whole-branch review (opus) 判定可 merge、無 Critical/Important；polish（probe timeout 4s、spec drift 對齊、BACKLOG B21-24）。

## Gates

817 tests、clippy `--all-targets -D warnings` 乾淨、fmt pinned、doc-drift 確定性 gate 5/5、零新 dep（全 std）。

## Cross-repo note

Mnemos 端 `/health` 已存在、無必做工作。選配（非阻塞、落 mnemos repo）：`/health` 升 JSON、修 `ARCHITECTURE.md:10` stale「Sprint 0 in design」標籤。

## Backlog spawned

B21（should_refresh dead-path）、B22（bottom_border panel_w<7 溢出）、B23（降級邊界寬度測試）、B24（count_active 熱路徑 parse 成本）。
