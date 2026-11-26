# scripts/ inventory

`./scripts/check-all.sh` is the merge gate — there is no hosted CI, and every change
passes it before it lands. Formatting goes only through `./scripts/fmt.sh`; never call
bare `cargo fmt`.

Facts below come from each file's own header comment and from `scripts/check-all.sh`.
`Run by check-all?` is `yes` when the path appears in `scripts/check-all.sh` (executed
as a step or passed to its shellcheck run; verify with grep). `Test file` is the
matching `*.test.sh` / `*.test.tsx` (or a test harness script) that exercises it, else `—`.

## Top level

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/ansi2html.py | Turns a terminal capture with ANSI colours into one self-contained HTML page. | `python3 scripts/ansi2html.py [--light] <capture.ansi> > capture.html` | no | — |
| scripts/check-all.sh | The merge gate (no hosted CI): every check in this table marked `yes`, plus fmt, clippy, cargo test (flake guard), plugin tests, doc-drift, cargo deny, shellcheck and the MSRV release smoke; the step list is the script itself. | `./scripts/check-all.sh` (`--no-wsl` / `--install-hook`); env `CODEFORGE_TOOLCHAIN`, `CODEFORGE_WSL_DISTRO` | no | scripts/hook-env.test.sh |
| scripts/check-archive-hermetic.py | Fails when a release tarball carries anything from the build machine or the build moment (B43). | `python3 scripts/check-archive-hermetic.py <archive.tar.gz> --epoch <n> [--forbid <string>]...` | no | scripts/test-archive-hermetic.sh |
| scripts/check-cjk-safe.sh | Flags START-anchored integer-literal byte slices `[..N]` / `[..=N]` in `src/**/*.rs` (the never-byte-index-a-string HARD RULE; escape hatch `// cjk-ok: <reason>`). | `./scripts/check-cjk-safe.sh` (exit 0 clean, exit 1 violations printed) | yes | scripts/check-cjk-safe.test.sh |
| scripts/check-cjk-safe.test.sh | Behavioral test for check-cjk-safe.sh: eight controls, each in a fresh mktemp tree. | `./scripts/check-cjk-safe.test.sh` | yes | — |
| scripts/check-doc-drift.py | Deterministic doc↔code drift gate (links, fences, version-sync, cli-surface, roadmap-consistency). | `python3 scripts/check-doc-drift.py` | yes | — |
| scripts/check-private-leaks.sh | Privacy guard for the public tree: hashed denylist of private names (sha256 tokens) plus a non-allowlisted-email check. | `./scripts/check-private-leaks.sh`; test env `CODEFORGE_LEAKS_ROOT`, `CODEFORGE_LEAKS_DENYLIST`, `CODEFORGE_LEAKS_ALLOW` | yes | scripts/check-private-leaks.test.sh |
| scripts/check-private-leaks.test.sh | Tests check-private-leaks.sh in throwaway git repos with a synthetic denylist (the real one is never read). | `./scripts/check-private-leaks.test.sh` | yes | — |
| scripts/checkout-guard.test.sh | Tests for `.githooks/post-checkout` and install-checkout-guard.sh in a throwaway repo isolated from global git config; `HOOK=<file>` tests another hook. | `./scripts/checkout-guard.test.sh` (env `HOOK=<file>`) | yes | — |
| scripts/e2e-mnemos-chain.sh | Full-chain proof of the CodeForge ↔ Mnemos L1 loop against the live local daemon, using TEST-TAGGED append-only data only. | `scripts/e2e-mnemos-chain.sh`; env `MNEMOS_INGEST_URL` | no | — |
| scripts/fmt.sh | Runs the pinned rustfmt toolchain (the PIN variable inside the script) via `cargo +PIN fmt --all`; `--check` verifies without writing; installs the pinned toolchain with rustfmt first if it is missing; used by check-all. | `./scripts/fmt.sh [--check]` | yes | scripts/fmt.test.sh |
| scripts/fmt.test.sh | Tests for fmt.sh — the pinned formatter wrapper — with fake rustup/cargo binaries; never touches the real toolchain. | `./scripts/fmt.test.sh` | yes | — |
| scripts/hook-env.test.sh | Proves check-all.sh's git env cleanup line so its temp-dir tests never hit the real repo. | `./scripts/hook-env.test.sh` | yes | — |
| scripts/install-checkout-guard.sh | Installs `.githooks/post-checkout` as a copy in the common hooks dir shared by every worktree; refuses to replace a foreign hook. | `./scripts/install-checkout-guard.sh` | yes | scripts/checkout-guard.test.sh |
| scripts/install-release.sh | Installs codeforge from the reproducible release package (B64): packages HEAD through package.sh and installs the packed binary into <dest> with mode 0755 (temp name + mv, so a failure never touches the old binary); last line `install-release: <dest>/codeforge sha16 <16 hex> commit <sha>`, identical sha16 on every host for the same commit. | `./scripts/install-release.sh [--dest <dir>] [--target <triple>]`; env `INSTALL_RELEASE_PACKAGE` overrides the packaging step (test) | yes | scripts/install-release.test.sh |
| scripts/install-release.test.sh | Tests install-release.sh without a build: a fake package.sh (env `INSTALL_RELEASE_PACKAGE`) produces an archive with a fake binary; checks 0755 in --dest, the printed sha16, and that a failed package leaves the old dest file untouched. | `./scripts/install-release.test.sh` | yes | — |
| scripts/package.sh | Builds and packages one release target the way install.sh downloads it (reproducible, B43); prints the archive path on the last line. | `./scripts/package.sh <target> [--toolchain <tc>] [--out <dir>] [--version <v>] [--cross] [--max-glibc <x.y>]` | yes | — |
| scripts/private-denylist.sha256 | sha256 of lowercase private tokens matched by check-private-leaks.sh (one per line, `#` comments). | — | no | — |
| scripts/private-leaks.allow | Known-red allowance list for check-private-leaks.sh (`<path> <max matching lines>`; scheduled for removal). | — | no | — |
| scripts/push-ref-guard.sh | Pre-push guard: origin is a public repo, only `develop` and `main` may be pushed to it, unless a one-push override env is set (written to an audit log). | `push-ref-guard.sh <remote-name> <remote-url>` (pre-push stdin); `CODEFORGE_PUBLIC_PUSH_OVERRIDE="<PM message id or card id>: <reason>" git push ...` | yes | scripts/push-ref-guard.test.sh |
| scripts/push-ref-guard.test.sh | Red/green test for push-ref-guard.sh through real `git push`es (public URL rewritten by insteadOf to a local bare repo). | `./scripts/push-ref-guard.test.sh` | yes | — |
| scripts/release-local.sh | Local release: builds every release target through package.sh (MSRV toolchain, `cross`, glibc floor), verifies through install.sh, writes RELEASE.txt; `--publish` drafts a GitHub release. | `./scripts/release-local.sh [--out <dir>] [--publish]` | no | — |
| scripts/repro-check.sh | Proves a release tarball is reproducible (B43): builds HEAD twice from clean checkouts at two different paths and requires byte-identical archives; `--other-cargo-home` repeats the second build under a different, offline CARGO_HOME (B64). | `./scripts/repro-check.sh [--other-cargo-home] <target> [package.sh options, e.g. --cross --toolchain 1.85]` | yes | — |
| scripts/test-archive-hermetic.sh | Tests for lib/pack-tarball.sh + check-archive-hermetic.py: a green pack passes and byte-matches; a dozen archive mutations each fail. | `./scripts/test-archive-hermetic.sh` (env `KEEP` keeps the work dir) | yes | — |
| scripts/test-flake-guard.sh | `cargo test` with a flake guard: a failed test is rerun once and never hidden (flakes recorded to the flake log). | `./scripts/test-flake-guard.sh [cargo command...]` (default `cargo`); env `FLAKE_LOG` | yes | scripts/test-flake-guard.test.sh |
| scripts/test-flake-guard.test.sh | Tests test-flake-guard.sh with a fake cargo (modes clean / flake / real / build). | `./scripts/test-flake-guard.test.sh` | no | — |
| scripts/tux-cross.sh | Shared-TUI plan §4.3: pinned tux bytes and both sides' golden/contract tests against the sibling autopilot; sibling absent = skip in dev, failure in release. | `scripts/tux-cross.sh [--release]`; env `AUTOPILOT_DIR` | yes | scripts/tux-cross.test.sh |
| scripts/tux-cross.test.sh | Isolated sibling fixtures for tux-cross.sh: skips, release failures, byte/version drift and runner failures. | `./scripts/tux-cross.test.sh` | yes | — |
| scripts/tux-golden-data.py | Bridges tux JSON fixtures to the Claude hooks sandbox as a generated TS module; `--check` refuses stale checked-in data. | `python3 scripts/tux-golden-data.py` (`--check` or `<output path>`); env `TUX_GOLDEN_DIR` | no | scripts/tux-golden-data.test.sh |
| scripts/tux-golden-data.test.sh | Bridge-generator controls for tux-golden-data.py: fresh check, stale goldens, stale output, explicit output path. | `./scripts/tux-golden-data.test.sh` | yes | — |
| scripts/tux-lint.py | Keeps forge-stats' shared glyphs and rendered theme colors in the vendored tables (flags literal shared glyphs and direct theme colors in hooks). | `python3 scripts/tux-lint.py [--test]` | yes | — |
| scripts/tux-lock.sh | Verifies every pinned byte of the vendored tux tree and rejects unlisted files (including symlinks); also checks the golden-data bridge. | `./scripts/tux-lock.sh`; env `TUX_VENDOR_DIR` | yes | scripts/tux-lock.test.sh |
| scripts/tux-lock.test.sh | Negative controls for tux-lock.sh on isolated vendor copies; the committed vendor tree is never edited. | `./scripts/tux-lock.test.sh` | yes | — |
| scripts/tux-sync.sh | Pins a committed autopilot `mods/tux` tree into `plugins/forge-stats/vendor/tux`; never reads its working-tree files. | `AUTOPILOT_DIR=<autopilot-dir> scripts/tux-sync.sh <autopilot-sha>` | yes | scripts/tux-sync.test.sh |
| scripts/tux-sync.test.sh | Controls for tux-sync.sh against a fixture autopilot repo; the real repo tree is never touched. | `./scripts/tux-sync.test.sh` | yes | — |

## lib/

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/lib/pack-tarball.sh | Reproducible release tarball (B43): `pack_tarball` packs a stage dir with nothing from the build machine or moment; `release_epoch` = SOURCE_DATE_EPOCH or HEAD commit time. | `pack_tarball <stage-dir> <name> <archive-path> <epoch>` (sourced library, not executed) | no | scripts/test-archive-hermetic.sh |

## g40-real-data/

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/g40-real-data/check.sh | G40 real-data check: runs the statusline and forge-stats' panel/1 against one autopilot live dir (or a synthetic fixture) at five band widths; refuses a too-wide line, an `acct-` alias, or money. | `scripts/g40-real-data/check.sh [<live_dir>] [--negative]`; env `G40_STOP_AFTER_COPY=1`; needs cargo, python3, claude | yes | scripts/g40-real-data/check.test.sh |
| scripts/g40-real-data/check.test.sh | Controls for check.sh: a relative `<live_dir>` resolves against the caller's cwd, and `G40_STOP_AFTER_COPY=1` stops after the copy. | `./scripts/g40-real-data/check.test.sh` | yes | — |
| scripts/g40-real-data/input-module.py | Prints the TypeScript input module for real-data.test.tsx from a live dir (every regular file becomes a FILES entry keyed by its path under LIVE). | `input-module.py <live_dir> > real-data.input.ts` | no | — |
| scripts/g40-real-data/make-fixture.py | Writes a synthetic autopilot live dir for check.sh; `--negative` renames one account to an `acct-` alias (the negative control). | `make-fixture.py <live_dir> [--negative]` | no | — |
| scripts/g40-real-data/real-data.test.tsx | The G40 real-data check, forge-stats half: run by check.sh in a scratch plugin copy via `claude plugin test`; real data never enters the repo. | run by scripts/g40-real-data/check.sh (not run directly) | no | — |
| scripts/g40-real-data/scan-statusline.py | Scans statusline captures: every line must fit its width in terminal cells, never an `acct-` alias, never money (`$`). | `scan-statusline.py <out_dir> <width>...` | no | scripts/g40-real-data/scan-statusline.test.sh |
| scripts/g40-real-data/scan-statusline.test.sh | Negative controls for scan-statusline.py: a clean capture passes; a too-wide line, an `acct-` alias and money each fail on their own. | `./scripts/g40-real-data/scan-statusline.test.sh` | yes | — |

## trinity-import/

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/trinity-import/codeforge-trinity-import.service | systemd oneshot unit for the trinity quota import; pins `AUTOPILOT_LIVE_DIR` to the same live base the Claude sessions use. | — | no | — |
| scripts/trinity-import/codeforge-trinity-import.timer | systemd timer: runs the trinity import every 15 minutes, 3 minutes after boot, persistent. | — | no | — |
| scripts/trinity-import/trinity-import.sh | Imports trinity's self-reported quota from this host's fleet-cli inbox (courier-attested sender + `trinity-usage/1` line); alerts once per failure or staleness. | run by codeforge-trinity-import.timer; env `CODEFORGE_TRINITY_SENDERS`, `CODEFORGE_TRINITY_NOTIFY`, `CODEFORGE_TRINITY_ENV`, `CODEFORGE_TRINITY_STATE`, `CODEFORGE_TRINITY_STALE_H` | yes | scripts/trinity-import/trinity-import.test.sh |
| scripts/trinity-import/trinity-import.test.sh | Tests trinity-import.sh against a fake fleet and the real codeforge binary; the live dir is a fresh tmpfs dir under /dev/shm. | `./scripts/trinity-import/trinity-import.test.sh` | yes | — |

## pane-gate/

Real-terminal screenshots and the UX gate for the shared autopilot-live pane TUI; all data is fake. See `scripts/pane-gate/README.md` for the workflow.

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/pane-gate/.gitignore | (no header comment) — ignores generated gate-shots/ and pet-shots/ output (out/, cwd/, home/, mirror/, rooms/, scenario, logs, .ans). | — | no | — |
| scripts/pane-gate/README.md | Documents the pane-gate tooling and workflow: shot scripts, keyprobe, owncard, run-gate-b2 rounds. | — | no | — |
| scripts/pane-gate/build-b2.py | Assembles gate-b2.md (batch 2 full gate packet) from the r3 packet head + gate-b2-insert.md + fresh shots; gates the packet against real names. | `build-b2.py <sha> <shots-label> [<nojump-label>]`; env `GATE_ROUND`, `IMPL_NOTE` | no | — |
| scripts/pane-gate/gate-b1-r3.md | Batch 1 round 3 UX gate review packet: scoring table, must-pass items, known host limits (in Traditional Chinese). | — | no | — |
| scripts/pane-gate/gate-b2-insert.md | Batch 1 r3 findings and the batch 2 dispositions the reviewer must judge item by item (in Traditional Chinese). | — | no | — |
| scripts/pane-gate/nojump-check.py | The no-jump gate: the tab strip, prompt and header rows must stay at identical row indexes across 30 captured frames. | `nojump-check.py <frames-dir>`; env `NOJUMP_HEADER_RE` | no | scripts/pane-gate/nojump-check.test.sh |
| scripts/pane-gate/nojump-check.test.sh | Controls for nojump-check.py: fixture frame dirs, exit codes and output. | `./scripts/pane-gate/nojump-check.test.sh` | yes | — |
| scripts/pane-gate/run-gate-b2.sh | Batch 2 UX gate end to end: shots → own-card → keyprobe → packet → carbon + astra reviews in parallel. | `run-gate-b2.sh <autopilot-worktree-at-batch2-sha>`; env `NO_REVIEW`, `FS_DIR`, `AUTOPILOT_DIR`, `GATE_ROUND` | no | — |

### pane-gate/gate-shots/

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/pane-gate/gate-shots/bin/fleet | (no header comment) — `#!/bin/sh` wrapper that execs gen.py; stands in for the fleet CLI on the shot sessions' PATH. | placed first on PATH as the fake `fleet` (fleet CLI args) | no | — |
| scripts/pane-gate/gate-shots/bin/gen.py | Fake fleet backend: all data invented (h1–h6, seats, pool names); the scenario is the first word of `$GATE_DIR/scenario`. | invoked via the bin/fleet wrapper | no | — |
| scripts/pane-gate/gate-shots/focus-probe.sh | Opens the pane, tries focus methods, and reports the prompt line and tab strip after each. | `focus-probe.sh <ap> <cols> <rows>` | no | — |
| scripts/pane-gate/gate-shots/keyprobe-installed.sh | At 80x24, presses digits and logs the pane header + composer after each (does a digit land in the prompt?). | `keyprobe.sh <autopilot-dir> <tag> [forge-stats-dir]`; env `START`/`STARTH`, `RW`/`RH`, `KEYS`, `PRE` | no | — |
| scripts/pane-gate/gate-shots/keyprobe.sh | Same probe, but the selected tab comes from the strip's colours via seltab.py; each line ends `ok` or `BAD`. | `keyprobe.sh <autopilot-dir> <tag> [forge-stats-dir]`; env `START`/`STARTH`, `RW`/`RH`, `KEYS`, `PRE` | no | — |
| scripts/pane-gate/gate-shots/lib.sh | Sourced tmux helpers for the shot scripts: `cap`/`click`/`key`/`composer`/`dig`, plus the scratch cwd setup (`.fleet-no-register`). | sourced by the gate-shots/*.sh scripts | no | — |
| scripts/pane-gate/gate-shots/nojump-console.sh | Console-mode variant of nojump.sh: 30 scripted frames (clicks and digits) over a console session. | `nojump-console.sh <autopilot-dir>` | no | — |
| scripts/pane-gate/gate-shots/nojump-extra.py | (no header comment) — summarizes nojump frames: expanded vs band-only, title and legend rows, row counts. | `python3 nojump-extra.py <frames-dir>` | no | — |
| scripts/pane-gate/gate-shots/nojump.sh | 30 frames (capture-pane, 1 s apart) over a scripted session; then runs nojump-check.py. | `nojump.sh <autopilot-dir>` | no | — |
| scripts/pane-gate/gate-shots/owncard.sh | Shoots page 2 with a card this seat can answer (sandbox HOME, pinned session id, decision cards planted in the scratch repo). | `owncard.sh <autopilot-dir> <label>` (run after run.sh, never beside it) | no | — |
| scripts/pane-gate/gate-shots/pet-r4.sh | Real-terminal shots of the pane's pet page (9) with fake fleet data and a sandbox pet, at 160x40 / 120x30 / 80x24. | `pet-r4.sh <autopilot-dir> <forge-stats-dir> [label]` | no | — |
| scripts/pane-gate/gate-shots/run-console.sh | Console-mode variant of run.sh: shots of pages 1–4 with fake fleet data where console mode is ON. | `run-console.sh <autopilot-dir> [label]` | no | — |
| scripts/pane-gate/gate-shots/run.sh | Real-terminal shots of pages 1–4 with fake fleet data (console mode OFF); also 80x20/80x16, the forge page, the closed-pane band, stale data, light theme, and pane probe logs. | `run.sh <autopilot-dir> [label]`; env `NOPANE=1` (closed-pane band) | no | — |
| scripts/pane-gate/gate-shots/seltab.py | Prints the selected tab of the shared tab strip from `capture-pane -e` output (by label colour, theme-independent). | `seltab.py < capture-pane-output` | no | — |

### pane-gate/pet-shots/

| Path | What it does (one line, from its header comment) | Usage (args/flags as the script documents them) | Run by check-all? | Test file |
|---|---|---|---|---|
| scripts/pane-gate/pet-shots/shot-open.sh | Like shot.sh, then clicks the pet tab and captures the open tab at 160x40 and 80x24. | `shot-open.sh <ap> <fs> <icon> <out>`; env `CODEFORGE_PET_ICON`, `EXTRA_ENV` | no | — |
| scripts/pane-gate/pet-shots/shot.sh | Real-terminal shots of the pet tab (closed), at 160x40 and 80x24; plain text and ANSI captures. | `shot.sh <autopilot-dir> <forge-stats-dir> <icon-id> <out-dir>`; env `CODEFORGE_PET_ICON`, `EXTRA_ENV` | no | — |
