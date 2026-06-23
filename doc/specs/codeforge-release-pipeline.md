# CodeForge Release Distribution Pipeline — Spec

**Status:** spec, ready to implement · **Target:** v0.0.2 first release ·
**Audience:** the next implementer.

## 0. Decision summary

| Question | Verdict | Reason |
|---|---|---|
| Windows in v1? | No | No demand yet; install.rs MVP already gates |
| macOS notarize/sign? | No | $99/yr + tooling tax; README documents `xattr -d` |
| Homebrew tap? | Defer | After install.sh + binstall have run for a month |
| In-binary self-update? | No | binstall handles Rust users; install.sh idempotent |
| Release automation | `cargo-release` | PR-driven; atomic bump+CHANGELOG+tag |
| MSRV | 1.75 via `rust-toolchain.toml` | Matches dep needs |
| Crates.io publish? | Manual later | Keep crate publishable but don't auto-publish |

## 1. The shared URL contract

CI workflow, `install.sh`, and `[package.metadata.binstall]` MUST agree:

```
https://github.com/cookys/codeforge/releases/download/v{VERSION}/codeforge-{VERSION}-{TARGET}.tar.gz
codeforge-{VERSION}-{TARGET}.tar.gz.sha256
```

Tarball layout (matches ripgrep/bat):

```
codeforge-{VERSION}-{TARGET}/
  codeforge          (executable, stripped)
  README.md
  LICENSE
```

Targets:

- `x86_64-unknown-linux-gnu` (ubuntu-22.04, glibc 2.35)
- `aarch64-unknown-linux-gnu` (cross-compile via `cross`)
- `x86_64-apple-darwin` (macos-13, Intel)
- `aarch64-apple-darwin` (macos-14, M-series native)

## 2. File inventory

### `.github/workflows/release.yml` — new

- Trigger: `on: push: tags: ['v*.*.*']` + `workflow_dispatch`
- Job `build` (matrix): per target, install Rust 1.75, build --release --locked, strip, tar+sha256, upload-artifact
- Job `publish` (needs all): `softprops/action-gh-release@v2` uploads all `*.tar.gz` + `*.sha256` to draft release
- Dry-run via `workflow_dispatch` input — skip `publish`
- Cache: `Swatinem/rust-cache@v2` per target

### `.github/workflows/release-smoke.yml` — new

Lighter PR-gate: builds linux-x86_64 only when `release.yml`,
`install.sh`, or `Cargo.toml` changes. Asserts tarball assembles.

### `install.sh` — new (repo root)

POSIX `sh`, shellchecked. Flow:

1. `need_cmd curl tar uname` + sha256 (prefer `sha256sum`, else `shasum -a 256`)
2. Detect OS/arch → target triple. Die on unsupported.
3. Resolve version: `$CODEFORGE_VERSION` env > `gh api releases/latest` (no jq)
4. Install dir: `$CODEFORGE_INSTALL_DIR` > `$CARGO_HOME/bin` > `~/.cargo/bin` > `~/.local/bin`
5. Download tarball + .sha256, verify, extract to tmp, `install -m 0755` to install dir
6. PATH check — print rc-append hint if missing
7. Print next steps: `codeforge install` + `codeforge init`
8. Existing-install detection — skip if same version unless `CODEFORGE_FORCE=1`

Hosting URL: `raw.githubusercontent.com/cookys/codeforge/main/install.sh`.
`codeforge.sh` domain deferred.

Error messages: each `die()` names cause + fix.

### `Cargo.toml` — modify

```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/codeforge-{ version }-{ target }.tar.gz"
pkg-fmt = "tgz"
bin-dir = "codeforge-{ version }-{ target }/{ bin }{ binary-ext }"

[profile.release]
strip = "symbols"
lto = "thin"
codegen-units = 1
```

Bump `version = "0.0.2"` as part of the release PR.

### MSRV declaration — `Cargo.toml` `rust-version`

Use `rust-version = "1.88"` in `[package]` instead of `rust-toolchain.toml`.
Cargo.lock v4 (current) requires Cargo 1.78+; 1.88 gives a comfortable
margin against deps. `rust-toolchain.toml` was rejected because it forces
every contributor + CI runner to download that exact version, slowing
everything down for no benefit over the declarative `rust-version`.

CI uses `dtolnay/rust-toolchain@stable` (existing in `ci.yml`); release
matrix uses `@1.88` for reproducibility.

### `CHANGELOG.md` — new

Keep-a-Changelog format. Sections `[Unreleased]`, `[0.0.2]`.
`cargo-release` moves `[Unreleased]` → versioned on bump.

### `release.toml` — new

```toml
pre-release-replacements = [
  { file="CHANGELOG.md", search="\\[Unreleased\\]", replace="[{{version}}] - {{date}}" },
]
tag-name = "v{{version}}"
publish = false
push = false   # flip to true once trusted
```

### `README.md` — modify §Install

Three-tier order:

```
Option 1 — curl installer (no Rust required)
  curl -sSL https://raw.githubusercontent.com/cookys/codeforge/main/install.sh | sh

Option 2 — cargo-binstall (Rust users, no compile)
  cargo binstall codeforge

Option 3 — cargo install (developers / source)
  cargo install --git https://github.com/cookys/codeforge --tag v0.0.2
```

Plus `codeforge install` wiring step + macOS Gatekeeper note
(`xattr -d com.apple.quarantine $(which codeforge)`) + §Uninstall.

## 3. Version-string verification

`codeforge --version` reads `env!("CARGO_PKG_VERSION")` via clap derive
(`src/cli/mod.rs:26`). No changes needed. Verify after bump.

## 4. Implementation sequencing

1. **PR 1**: `rust-toolchain.toml` + Cargo.toml additions (no version
   bump) + CHANGELOG skeleton + `release.toml`. CI stays green.
2. **PR 2**: `install.sh` + shellcheck CI job. Safe to merge; v0.0.2
   doesn't exist yet so no one runs it productively.
3. **PR 3**: `release.yml` + `release-smoke.yml`. `workflow_dispatch`
   dry-run verifies all four matrix builds.
4. **PR 4**: README §Install rewrite + §Uninstall + macOS note.
5. **Cut v0.0.2**: `cargo release patch --execute` → pushes tag →
   release.yml fires → assets land in draft GH Release.
6. **Validate** (manual):
   - `docker run ubuntu:24.04` — apt curl + run install.sh → `codeforge --version` = `0.0.2`
   - Same on linux/arm64 via qemu
   - `cargo binstall codeforge --dry-run`
   - macOS Intel + M1 manual download + xattr smoke

## 5. Failure modes handled

- install.sh on unsupported arch (e.g., armv7) → explicit die listing supported targets + binstall fallback
- GitHub API rate limit → `$CODEFORGE_VERSION` env var workaround
- SHA mismatch → die immediately, print both hashes
- CI matrix partial failure → `softprops/action-gh-release` `draft: true` keeps release hidden; implementer investigates before flipping live
- glibc too old (< 2.35) → documented limitation; musl target in v0.0.3 if reported

## 6. Out of scope (restated)

No Homebrew tap. No Windows. No macOS codesigning. No GPG/cosign
beyond SHA256. No in-binary updater. No npm shim. No auto-publish to
crates.io (leave publishable, manual `cargo publish` later).
