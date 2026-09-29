# CodeForge on Windows

Verified end-to-end on Windows 11 (x86_64, MSVC toolchain), 2026-09-29: build → install →
Claude Code wiring → `init` / `learn` / `adopt` / `dream` / `memory search` / `statusline` /
`doctor` / `daemon start|status|stop`. The full test suite passes locally on Windows (no Windows CI job).

> **Not yet available on Windows:** the `curl … | sh` installer and `cargo binstall` (no Windows
> release asset is published — `release.yml` builds Linux/macOS only). Build from source as below.

## 1. Prerequisites

| Need | Why | Install |
|---|---|---|
| Rust stable ≥ 1.88, **MSVC host** | build | <https://rustup.rs> (default host `x86_64-pc-windows-msvc`) |
| Visual Studio Build Tools 2022 — "Desktop development with C++" | `rusqlite` compiles bundled SQLite (C), and MSVC needs `link.exe` | `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` (or the VS Installer GUI) |
| Git for Windows | clone; **Claude Code runs hooks through its Git Bash** | `winget install Git.Git` |
| Node ≥ 20 | the JS hook scripts (`emit-session.js`, `session-digest.js`) | `winget install OpenJS.NodeJS.LTS` |
| Claude Code (`claude` on PATH) | `dream`/`ship` compile via `claude -p` | <https://claude.com/claude-code> |

Open a **new terminal** after installing anything so PATH is refreshed.

### Pitfall: GNU vs MSVC toolchain

If `rustup show` says the active toolchain is `stable-x86_64-pc-windows-gnu`, the build needs
`gcc` (for bundled SQLite) and fails without it. Either switch the default…

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

…or leave your default alone and pass `+stable-x86_64-pc-windows-msvc` to every cargo command
below.

## 2. Build and install

Use **PowerShell** (or a Developer PowerShell). From Git Bash, `C:\Program Files\Git\usr\bin\link.exe`
(the coreutils `link`) shadows MSVC's linker and breaks the build.

```powershell
git clone https://github.com/cookys/codeforge C:\git\codeforge
cd C:\git\codeforge
cargo +stable-x86_64-pc-windows-msvc install --path . --locked   # ~1 min; drop the +toolchain if MSVC is your default
codeforge --version
```

The binary lands in `%USERPROFILE%\.cargo\bin\codeforge.exe`.

## 3. Wire it into Claude Code

```powershell
codeforge install --all      # statusLine + global hooks (idempotent; --dry-run to preview)
codeforge bootstrap          # optional: install --all + pinned rustfmt check + Mnemos status
```

This edits `%USERPROFILE%\.claude\settings.json` (other keys are preserved; back it up first if you
like). Written commands use **forward slashes** (`C:/Users/you/.cargo/bin/codeforge.exe statusline`)
because Claude Code hands them to Git Bash, where a backslash is an escape character; paths with
spaces are quoted automatically. `--all` also sets `cleanupPeriodDays = 3650` so session transcripts
survive until `dream` distils them.

Then per project:

```powershell
cd C:\path\to\your\project
codeforge init
codeforge learn "tokio::select! preserves cancellation across branches"
"3`ny`n" | codeforge adopt         # adopt is interactive; pipe "village number, y" to script it
codeforge dream                    # L0 → L1 via `claude -p`
codeforge memory search "tokio"
codeforge doctor
```

Restart Claude Code (or `/clear`) to see the statusline.

## 4. What behaves differently on Windows

| Area | Windows behaviour |
|---|---|
| `codeforge daemon stop` | No SIGTERM: the daemon is terminated hard, so its pidfile stays behind. `daemon status` shows `(stale)`; the next `daemon start` reclaims it. Ctrl-C and Ctrl-Break handlers are installed for a clean foreground shutdown (not exercised in the verification above). |
| Live context files (`codeforge statusline`) | No tmpfs, so they always go to `%USERPROFILE%\.autopilot\context\` (disk-backed) — silently, without the once-per-process warning Linux prints. |
| `dream` / `ship` LLM calls | `claude -p` runs directly at BELOW_NORMAL priority with an in-process 180 s watchdog (Unix uses `nice` + `timeout`). The load-average throttle is Linux-only and is skipped. `agy` / `codex` npm shims (`.cmd`) are untested. |
| Statusline session-version chip | Read from `claude --version`; the Linux `/proc` ancestor walk has no Windows equivalent. |
| Hook shell | `codeforge dream --quiet 2>/dev/null \|\| true` etc. rely on Git Bash being what Claude Code uses on Windows (the default when Git for Windows is installed). |

## 5. Verifying changes locally (no GitHub CI)

```bash
./scripts/check-all.sh    # from Git Bash: Windows-native pass + Linux pass through WSL
```

It picks the MSVC toolchain automatically on Windows, uses a separate cargo target dir inside WSL
(`~/.cache/codeforge-check-target`) and needs a Rust ≥ 1.88 + gcc in the WSL distro. Add
`--install-hook` to run it before every `git push`. macOS cannot be covered locally.

## 6. Where Windows-specific code lives

All OS differences are isolated in [`src/platform.rs`](../src/platform.rs); the rest of the code
calls its functions and contains no `cfg(windows)` / `cfg(unix)`:

- process alive / terminate / identity → the cross-platform [`sysinfo`](https://crates.io/crates/sysinfo) crate
- detached spawn, stop signals, low-priority + timeout wrapper, shell-safe path rendering
- `scripts/*.sh` are forced to LF by `.gitattributes` (Windows `core.autocrlf` otherwise turns
  them into CRLF and non-Git-Bash shells reject them); `bootstrap` runs `fmt.sh` through Git for
  Windows' `bash`

`live.rs` is the one deliberate exception: Unix file modes (0700/0600), `getuid` and the tmpfs probe
have no Windows analogue, so it keeps its own `cfg` (plus `cfg(unix)` on the `findmnt` test fixtures).

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `error: linker 'link.exe' not found` / `link: extra operand` | Build Tools missing, or building from Git Bash | Install the C++ workload; build from PowerShell |
| `failed to run custom build command for libsqlite3-sys` / `gcc not found` | GNU toolchain active | Use the MSVC toolchain (see above) |
| Hook errors `node: command not found` | Node missing or terminal opened before install | Install Node, open a new terminal, restart Claude Code |
| `scripts/fmt.sh: set: pipefail: invalid option name` | CRLF checkout on a pre-`.gitattributes` clone | Re-clone (the `.gitattributes` now forces LF), or run `git pull` and re-checkout the scripts |
| `codeforge dream` says `claude -p 不可用` | `claude` not on the PATH of the shell that ran it | Fix PATH; `where.exe claude` must resolve |
