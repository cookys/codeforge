# Plan — CodeForge Public-Readiness Cleanup

Date: 2026-05-05
Branch: `feature/public-readiness`
Project: `doc/projects/2026-05-05-public-readiness/`

## Context

四個 audit agents 平行掃過 CodeForge repo（167 commits / 1356 historical objects），結論為 **GO WITH FIXES** — 沒有 secret leak、沒有 license 污染、commit history conventional-style 乾淨；但缺 LICENSE / README 兩個基本檔，`.gitignore` 嚴重不足（4 行），`/home/codepower/` 硬編碼洩漏在兩個 hook 腳本與 `.claude/knowledge/environment.md`。

Board Decision: **A** — CodePower 將同步公開，`src/clan/*` 與 `doc/specs/nation-p2p-design.md` 維持原樣 ship，不需 redact。

## Phases

### P1 — Foundation files
- 加 `LICENSE`（Apache-2.0 全文）對齊 `Cargo.toml` 既有 `license = "Apache-2.0"`
- 擴充 `.gitignore`：`.env`、`.env.*.local`（保留 `.env.example` tracked）、`/.codeforge/codeforge.db*`、`/.codeforge/signals/`、`/.codeforge/store/`、`/.codeforge/projections/`、`.claude/settings.local.json`、`**/CLAUDE.local.md`、`.vscode/`、`.idea/`、`*.swp`、`*.swo`、`.DS_Store`、`Thumbs.db`、`*.log`、`**/*.rs.bk`

### P2 — Path / Personal-info sanitization
- `.claude/scripts/check-improvements.js` — `PROJECT_ROOT` 改由 `path.dirname(path.dirname(path.dirname(__filename)))` 推導
- `.claude/scripts/check-dev-flow.js` — 同上
- `.claude/knowledge/environment.md` — 移除 `cookys@example.com` / `cookys` git config 範例 + `twgs-dev` 他人 username + 環境特定 `/tmp/claude-XXXX-cwd` 條目；改用 placeholder
- `.claude/settings.json` 6 個 hook 絕對路徑：暫不動（Claude Code 不支援相對路徑），用 README「First-time setup」+ BACKLOG 追 template 解法

### P3 — Public-facing content
- **R8 pre-flight**：WebSearch USPTO TESS for "CODEFORGE" + GitHub `codeforge` namespace 查衝突 → 決 repo 名稱（候選：`codeforge` / `codeforge-cli` / `codeforge-rs`；binary 名 `codeforge` 不變）
- 寫 `README.md`：title / one-liner / install / quickstart（learn → dream → pet → statusline）/ Phase status / Anthropic affiliation disclaimer / Data & Privacy 區塊 / First-time setup（hook 路徑須改）/ Contributing 連結 / License
- `Cargo.toml` 補：`authors = ["cookys"]`、`repository`、`readme = "README.md"`、`keywords`、`categories`

### P4 — Community + CI infrastructure
- `deny.toml`：`[licenses]` allow-list（Apache-2.0, MIT, BSD-2/3-Clause, ISC, Unicode-DFS-2016, Unicode-3.0, Zlib, MPL-2.0）
- `CONTRIBUTING.md`：Conventional Commits + CLAUDE.md 連結 + quality gate 說明
- `CODE_OF_CONDUCT.md`：Contributor Covenant 2.1
- `.github/workflows/ci.yml`：Rust stable + `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` + `cargo deny check licenses`

### P5 — Repo cleanup + tag + push
- Quality gate：`cargo check && cargo clippy --all-targets -- -D warnings && cargo test`（baseline 666 tests，不可退步）
- Code review：`superpowers:requesting-code-review`（最多 3 輪）
- Fix findings → review round 2（CLEAN 才能進 merge）
- Merge `feature/public-readiness` → `main`
- 刪除 4 條 stale `feature/*`：`codeforge-phase1`、`phase3a-world-map`、`phase3e-crafting`、`phase3f-snapshot`
- 建 `v0.1.0` tag
- **Board pause**：`gh repo create --public` 為 irreversible op，CEO 不跨此線；明示確認後執行
- `gh repo create cookys/<name> --public --source=. --remote=origin --push`
- `git push --tags`

## Risks

| Risk | Mitigation |
|------|-----------|
| Hook scripts 改寫後失效（PreToolUse warnings 噴錯） | P2 後在本機重跑一次 hook 驗證；保留 fallback `process.cwd()` 邏輯 |
| README 寫太長變維護負擔 | 控制在 200 行內，連結指向 `doc/specs/`、`doc/projects/_archive/` |
| Cargo.toml metadata 改動觸發整批重編 | 預期且可接受（純 metadata 不影響 binary） |
| USPTO 搜尋發現衝突 → 改 repo 名 | 接受改名；binary 名 `codeforge` 不變，只動 GitHub repo 與 `Cargo.toml repository` 連結 |
| 公開 repo 後發現遺漏 secret | push 前再跑一次 `git log --all -p -G 'sk-ant-\|sk-[a-zA-Z0-9]{30}\|api[_-]?key\|token' -i` 為 belt-and-suspenders |
| 4 條 stale branches 中有未合進 main 的 work | P5 刪前先跑 `git log main..feature/<name>` 確認；發現未合的 commit 暫停討論 |

## Out of scope（明示 exclude）
- `src/clan/*` 移除或 redact（Board: A）
- `doc/specs/nation-p2p-design.md` 改寫（Board: A）
- 商標申請（公司層級事務）
- crates.io publish（Phase 6+ 才做）
- `.claude/settings.json` template + setup script（複雜度較大，進 BACKLOG）
