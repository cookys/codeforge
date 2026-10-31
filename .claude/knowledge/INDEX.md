# Knowledge Index — CodeForge

## Recent Learnings

| Date | Category | Title | File |
|------|----------|-------|------|
| 2026-10-31 | arch | AbovePrompt band layout / statusline heartbeat detection：band 一實例、上層不呼叫 next 就輪不到；mod 心跳檔讓 statusline 縮成 2 行 | `harness-patterns.md` |
| 2026-10-31 | api | pixel sprite format / half-block raster animation / terminal pixel art palette：調色盤+字元列、`▀` fg=上 bg=下、xterm-256 cube 色、`$.clock.every(500)`+`$.ui.blit`（幀放模組變數）、Raster 用 `e.surface==='terminal'` 判斷 | `harness-patterns.md` |
| 2026-10-31 | build | mod 測試不能 import JSON／外掛資料夾外的檔（用 `.ts` 鏡像 + Rust 比對）；`$.clock.now()` 未模擬會讓整個 session.start 被 skip；AbovePrompt 讓出要有帶 matcher 的下層替身 | `harness-patterns.md` |
| 2026-10-31 | env | mod 放專案 `.claude/skills/<name>/` 才每次載入；session mods 資料夾 `--resume` 後失效；`CLAUDE_CODE_PLUGIN_DIRS` 不讀專案 settings | `harness-patterns.md` |
| 2026-10-31 | build | Rust 字串續行（行尾 `\`）吃掉下一行前導空白 → ASCII art 默默歪掉；改 `concat!` 一行一個字面字串 + 測試釘縮排 | `rust-patterns.md` |
| 2026-06-23 | env | 加 dep 會撞 `cargo deny check licenses`（CI-only、不在本機 gate）→ push 前先本機跑；webpki-roots=CDLA-Permissive-2.0 等單一非標準 license 才會卡 | `environment.md` |
| 2026-06-23 | env | `install --all` 先 patch statusLine、衝突就 bail 在 hooks 之前 → 整個中止、hooks 沒裝；要 hooks 必落地就 fallback `install --hooks`（bootstrap step1 即此） | `environment.md` |
| 2026-06-23 | env | rustfmt drift → 一律走 pinned `scripts/fmt.sh`（單一版本來源、self-install、只 pin fmt 不動 build），永不裸跑 `cargo fmt`；CI fmt job + S/L/H gate 都跑 `--check` | `environment.md` |
| 2026-06-23 | gate | 寫 grep/awk 確定性 gate 三陷阱：比對原始行不 strip inline comment（`//`-in-string 漏抓）、allow-marker 加錨點（`cjk-ok:`）、精準優先於召回（誤報會被停用） | `gate-patterns.md` |
| 2026-06-22 | env | `git tag v*.*.*` 觸發 release.yml 發布 pipeline — 別為文件一致性打 tag（codeforge + autopilot 皆 release-on-tag） | `environment.md` |

## Knowledge Files

| File | Category | Entries |
|------|----------|--------|
| `rust-patterns.md` | Rust 語言特性、常見陷阱 | 5 |
| `environment.md` | 環境設定、路徑、git config、GitHub auth、toolchain/fmt、install/hooks、cargo-deny | 9 |
| `harness-patterns.md` | Claude Code / Anthropic API harness 行為、mod（Raster 像素畫、載入位置、測試替身、band 與心跳） | 6 |
| `gate-patterns.md` | 確定性 gate script（grep/awk）寫法陷阱 | 3 |

## Usage

- **Session start**: skim Recent Learnings to avoid re-discovering known issues
- **After fixing**: invoke `autopilot:learn` to record
- **Keep top 10**: rotate older entries out of Recent Learnings (stay in category files)
