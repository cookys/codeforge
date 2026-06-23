# Knowledge Index — CodeForge

## Recent Learnings

| Date | Category | Title | File |
|------|----------|-------|------|
| 2026-06-23 | env | `install --all` 先 patch statusLine、衝突就 bail 在 hooks 之前 → 整個中止、hooks 沒裝；要 hooks 必落地就 fallback `install --hooks`（bootstrap step1 即此） | `environment.md` |
| 2026-06-23 | env | rustfmt drift → 一律走 pinned `scripts/fmt.sh`（單一版本來源、self-install、只 pin fmt 不動 build），永不裸跑 `cargo fmt`；CI fmt job + S/L/H gate 都跑 `--check` | `environment.md` |
| 2026-06-23 | gate | 寫 grep/awk 確定性 gate 三陷阱：比對原始行不 strip inline comment（`//`-in-string 漏抓）、allow-marker 加錨點（`cjk-ok:`）、精準優先於召回（誤報會被停用） | `gate-patterns.md` |
| 2026-06-22 | env | `git tag v*.*.*` 觸發 release.yml 發布 pipeline — 別為文件一致性打 tag（codeforge + autopilot 皆 release-on-tag） | `environment.md` |
| 2026-05-15 | env | Per-command git identity override（`-c user.email=...` 不寫 config）— HARD RULE 合規且支援多 repo 多 identity | `environment.md` |
| 2026-05-15 | env | `~/.cargo/bin` not on PATH for Claude Code spawned shells — silent statusline failure → `codeforge install` writes abs path | `environment.md` |
| 2026-05-15 | env | Hook scripts under `.claude/scripts/` split into global-safe (emit-session, session-digest) vs codeforge-repo-only (check-improvements, check-dev-flow) | `environment.md` |
| 2026-05-15 | build | Cargo.lock v4 requires Cargo ≥1.78 → MSRV declared as `rust-version = "1.85"` in Cargo.toml, not `rust-toolchain.toml` (avoids forcing toolchain download) | `rust-patterns.md` |
| 2026-05-05 | env | `gh repo create --push` 含 `.github/workflows/*` 需 token 帶 `workflow` scope（`gh auth refresh -s workflow`） | `environment.md` |
| 2026-05-05 | harness | Claude Code Write PreToolUse hook 擋 `.github/workflows/*.yml` — 用 Bash heredoc bypass | `harness-patterns.md` |

## Knowledge Files

| File | Category | Entries |
|------|----------|--------|
| `rust-patterns.md` | Rust 語言特性、常見陷阱 | 4 |
| `environment.md` | 環境設定、路徑、git config、GitHub auth、toolchain/fmt、install/hooks | 8 |
| `harness-patterns.md` | Claude Code / Anthropic API harness 行為 | 2 |
| `gate-patterns.md` | 確定性 gate script（grep/awk）寫法陷阱 | 3 |

## Usage

- **Session start**: skim Recent Learnings to avoid re-discovering known issues
- **After fixing**: invoke `autopilot:learn` to record
- **Keep top 10**: rotate older entries out of Recent Learnings (stay in category files)
