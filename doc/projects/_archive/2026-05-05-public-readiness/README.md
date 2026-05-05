# CodeForge Public-Readiness Cleanup

Started: 2026-05-05
Branch: `feature/public-readiness`
Plan: `doc/plans/2026-05-05-public-readiness.md`
Tracks: 公開 GitHub 前的 4 項 blockers + 9 項 hygiene recommendations（4-agent audit 結論）

## Project Goal

> **Final goal**: 把 CodeForge repo 從「private local」整理到「可在 public GitHub 上線」狀態 —
> 修完 4 項 blockers + 9 項 recommendations，最後在 user 明示確認後 push 到新建 public repo。
>
> **Success criteria**:
> 1. `LICENSE` 存在且為 Apache-2.0 全文（與 `Cargo.toml` 既有 `license` 一致）
> 2. `README.md` 存在，含：description / install / quickstart / Phase status / Anthropic disclaimer / Privacy / First-time setup
> 3. `.gitignore` 含全部 13 項 entry（env / `.codeforge/` 執行期 / `.claude/` local / IDE / OS / logs）
> 4. `.claude/scripts/check-{improvements,dev-flow}.js` 的 `PROJECT_ROOT` 由 `__filename` 推導，不再硬編碼 `/home/codepower/`
> 5. `.claude/knowledge/environment.md` 不再含 `cookys@example.com` / `cookys` git config 範例 / `twgs-dev`
> 6. `Cargo.toml` 補齊 `authors` / `repository` / `readme` / `keywords` / `categories`
> 7. `cargo check && cargo clippy --all-targets -- -D warnings && cargo test` 全綠（baseline 666 tests，不可退步）
> 8. `.github/workflows/ci.yml` 存在且 YAML syntax valid
> 9. `deny.toml` 存在，`cargo deny check licenses` 通過
> 10. `CONTRIBUTING.md` + `CODE_OF_CONDUCT.md` 存在
> 11. 4 條 stale `feature/*` branches 已刪除（`codeforge-phase1` / `phase3a-world-map` / `phase3e-crafting` / `phase3f-snapshot`）
> 12. `v0.1.0` git tag 建立
> 13. （Board pause 後）GitHub public repo 建立 + 初始 push 完成（含 tag）
>
> **Scope boundary**:
> - **Include**: B1-B4 blockers、R1-R9 recommendations、feature branch + project tracking、full quality-gate + code review、tag + push
> - **Exclude**: `src/clan/*` 修改（Board 決議 A — CodePower 也將公開）、商標申請、crates.io publish（Phase 6+）、`.claude/settings.json` template 化（BACKLOG）

## Design Decisions（user 拍板 2026-05-05）

1. **Board Decision A — CodePower 同步公開**：`src/clan/*` doc-comments 與 `doc/specs/nation-p2p-design.md` 維持原樣 ship；不 redact。理由：兩 repo 同 owner，互相導流比 decoupling 健康。
2. **Repo 名稱**：P3 USPTO + GitHub namespace 查完才決定（候選 `codeforge` / `codeforge-cli` / `codeforge-rs`）；binary 名 `codeforge` 永遠不變。
3. **`.claude/settings.json` 6 個絕對路徑**：先用 README「First-time setup」說明 + 進 BACKLOG 追 template 解法；不在本 project 動。理由：Claude Code 不支援相對路徑，正確解法需要 setup script，scope 過大。
4. **公開動作為 irreversible op**：P5 最末步 `gh repo create --public` 必須 Board 明示確認後才執行；CEO autonomous 不跨此線。

## Audit Findings 摘要（4-agent parallel）

| 類別 | Agent 結論 |
|------|----------|
| Secrets（working tree + 1356 historical objects） | **0 leaks**, high confidence |
| PII / 內部路徑 | 18 occurrences `/home/codepower/`（5 leaks 待修，13 legitimate hooks） |
| License / dependency | 18 直接 deps 全 MIT/Apache，無污染；缺 LICENSE 檔（BLOCKER） |
| Git hygiene | 167 commits conventional-style 乾淨；7.5MB `.git` 健康；最大 historical blob `Cargo.lock` 72KB |

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 LICENSE + .gitignore | ✅ done | `7b64baa` |
| P2 Path / personal-info sanitize | ✅ done | `9cd7703` |
| P3 README + Cargo.toml + Privacy + name decision (`codeforge` kept) | ✅ done | `23c88a6` |
| P4 deny.toml + CONTRIBUTING + CODE_OF_CONDUCT + CI workflow | ✅ done | `c66d21f` |
| QG (fmt + clippy + test 669/669 + secret scan) | ✅ done | `1f7af43`, `8522c77` |
| Review r1 | ✅ done (2 IMPORTANT + 2 MINOR) | — |
| Fix r1 findings (toolchain pin + path-gate portability) | ✅ done | `066a6c7` |
| Review r2 | ✅ CLEAN | — |
| Merge to main | ✅ done | `e5d984b` |
| Post-merge verify (key files in main + Cargo metadata) | ✅ done | — |
| Prune 4 stale `feature/*` branches | ✅ done | — |
| Version bump 0.1.0 → 0.0.1 (preview, per Board) | ✅ done | `9844b88` |
| Author email → GitHub noreply (linked profile for v0.0.1) | ✅ done | (config) |
| Lore / cosmology (CodeForge ↔ CodePower smithy framing) | ✅ done | `f42b62b` |
| Tag `v0.0.1` (re-tagged from v0.1.0 + at lore commit) | ✅ done | — |
| Board pause + `gh repo create --public` + push main + push tag | ✅ done | — |
| Archive | 🔄 in_progress | — |

## Outcome

**`https://github.com/cookys/codeforge` 公開上線 ✅** — `v0.0.1` released, CI 第一次 run 觸發中。

**Settings.json template + setup script** 進 BACKLOG B12（per Design Decision 3）。
