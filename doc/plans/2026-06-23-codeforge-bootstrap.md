# Plan — `codeforge bootstrap` 一鍵多機部署 (B14)

**Created**: 2026-06-23
**Size**: L (new CLI command)
**Backlog**: B14 — 其他機器部署（new binary + install --hooks + mnemos.env + fmt toolchain）

## Goal

> **Final goal**: 一個 idempotent 的 `codeforge bootstrap` 命令，把「在一台新機器讓 codeforge
> 完整就緒」的多步流程收斂成一鍵 —— 包含 fmt pin toolchain，讓 B19 的 pinned rustfmt 在
> 每台開發機自動對齊。
> **Success criteria**:
> - (a) `codeforge bootstrap --dry-run` 印出將執行的三步計畫、零寫入（`cargo run -- bootstrap --dry-run` 退出 0）。
> - (b) `codeforge bootstrap` 在 codeforge clone 內：完成 `install --all` 等效 wiring + `scripts/fmt.sh --check` 通過（self-install pinned toolchain）+ 報告 Mnemos opt-in 狀態（驗證：summary 三段皆出現）。
> - (c) 非 clone 機器：fmt 步驟 skip 並註記，其餘照常（單元測試覆蓋 clone 偵測）。
> - (d) `cargo test` 綠、`./scripts/fmt.sh --check` 綠、`cargo clippy -D warnings` 綠。
> **Scope boundary**:
> - INCLUDE: 新 `bootstrap` 子命令（薄 orchestrator）、clone 偵測、Mnemos opt-in 報告（report-only）、docs、tests。
> - EXCLUDE: 自動下載/安裝 codeforge binary 本身（你要先有 binary 才能跑 bootstrap）；自動建 `~/.config/mnemos.env`（opt-in 刻意手動，只 hint）；遠端對其他機器執行（CLI 只能在本機跑，runbook 指引在各機器執行）。

## Design

`codeforge bootstrap [--dry-run] [--quiet]`，順序執行、每步報告、全程 idempotent：

1. **Claude Code wiring** — 複用 `install::run(InstallOpts{ all: true, dry_run, quiet, .. })`
   = statusline + global hooks（emit-session / session-digest / SessionStart recall /
   SessionEnd dream→ship / cleanupPeriodDays）。
2. **fmt toolchain（dev 機）** — 從 CWD 往上找 `scripts/fmt.sh`（codeforge clone 標記）。
   找到 → 跑 `scripts/fmt.sh --check`（self-install pinned 1.94）；`--dry-run` 只報告不跑。
   找不到 → skip + 註記「非 codeforge clone，fmt pin 不適用」。
3. **Mnemos opt-in（report-only）** — `MnemosConfig::opted_in()`：已 opt-in → 報告來源；
   未 opt-in → 印出 opt-in 方法（建 `~/.config/mnemos.env` 或設 `MNEMOS_INGEST_URL`），**不自動建檔**。
4. **Summary** — 條列這台機器現在擁有什麼 + 後續手動步驟（如各機器要 `git pull`）。

## Phases

- **P1**: `src/cli/bootstrap.rs` — `BootstrapOpts` + `run()` orchestrator + 純 helper
  （`find_fmt_script` clone 偵測、summary 組裝）+ 單元測試（偵測 / opt-in 報告分支）。
- **P2**: wire `Bootstrap { dry_run, quiet }` 進 `src/cli/mod.rs` Commands enum + dispatch。
- **P3**: docs — CLAUDE.md Development Commands + 「其他機器部署」一節；BACKLOG B14 → done + runbook。

## Quality Gate

`./scripts/fmt.sh --check` + `cargo check` + `cargo clippy -- -D warnings` + `cargo test`
+ `superpowers:requesting-code-review`（src/cli/ 區域）。
