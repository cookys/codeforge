# Knowledge Index — CodeForge

## Recent Learnings

| Date | Category | Title | File |
|------|----------|-------|------|
| 2026-04-17 | env | `/tmp/claude-*-cwd` permission denied 是 harness 訊息，非指令失敗 | `environment.md` |
| 2026-04-14 | build | CJK 字串截斷用 `.chars().take(N).collect()` 不能用 `&s[..N]` | `rust-patterns.md` |
| 2026-04-14 | build | UUID `id[..8]` 雖然 ASCII 安全，但要跟 chars fix 保持一致 | `rust-patterns.md` |
| 2026-04-14 | env | codeforge init 需在目標 repo 目錄下執行才能建 `.codeforge/` | `environment.md` |
| 2026-04-14 | env | codeforge git repo 需獨立設 git config（user.email/name） | `environment.md` |

## Knowledge Files

| File | Category | Entries |
|------|----------|--------|
| `rust-patterns.md` | Rust 語言特性、常見陷阱 | 3 |
| `environment.md` | 環境設定、路徑、git config | 3 |

## Usage

- **Session start**: skim Recent Learnings to avoid re-discovering known issues
- **After fixing**: invoke `autopilot:learn` to record
- **Keep top 10**: rotate older entries out of Recent Learnings (stay in category files)
