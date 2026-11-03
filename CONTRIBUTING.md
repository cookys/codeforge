# Contributing to CodeForge

Thanks for your interest! This document covers how to set up, propose changes, and stay aligned with project conventions.

## Project conventions

Read [CLAUDE.md](CLAUDE.md) first — it documents:
- Tech stack and crate versions
- Source layout and architecture
- Error handling patterns (`anyhow::Result` throughout, user-facing strings in 繁體中文)
- CJK-safe string truncation rule (`.chars().take(N).collect::<String>()`, never `&s[..N]`)
- SQLite PRAGMA setup (WAL + foreign_keys + busy_timeout)
- Phase roadmap and design references in `doc/specs/`

## Development setup

```bash
git clone https://github.com/cookys/codeforge
cd codeforge
cargo build
cargo test
```

## Pull request flow

1. Branch from `main`: `feature/<short-description>` for new features, `fix/<short-description>` for bug fixes.
2. Keep PRs focused — one logical change per PR.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages: `feat(area): ...`, `fix(area): ...`, `chore(area): ...`, `docs(area): ...`, `refactor(area): ...`, `test(area): ...`.
4. Run the quality gate locally before opening the PR (see below).
5. Open a PR with a clear title and description. Link any related issues.

## Quality gate — the merge gate

There is no hosted CI: `./scripts/check-all.sh` is the merge gate. A change merges only when it is
green on the merge result, run by whoever merges. One command runs every gate locally (on Windows it
also runs a Linux pass through WSL):

```bash
./scripts/check-all.sh                  # add --no-wsl to skip the Linux pass
./scripts/check-all.sh --install-hook   # make `git push` run it automatically (bypass: --no-verify)
```

What it runs, step by step:

```bash
./scripts/fmt.sh --check                    # pinned rustfmt, never a bare `cargo fmt`
cargo clippy --all-targets -- -D warnings
./scripts/test-flake-guard.sh               # cargo test; a flake is rerun once and logged
./scripts/check-cjk-safe.sh
python3 scripts/check-doc-drift.py
cargo deny check licenses
shellcheck install.sh
./scripts/package.sh x86_64-unknown-linux-gnu --toolchain <rust-version>   # MSRV release smoke
```

It needs `cargo-deny`, `shellcheck` and the MSRV toolchain (`rust-version` in Cargo.toml) installed.
A PR's author runs it before asking for review; the maintainer runs it again on the merge result.
Releases are built and verified by `./scripts/release-local.sh` (see
[`doc/specs/codeforge-release-pipeline.md`](doc/specs/codeforge-release-pipeline.md)).

## Reporting bugs

Open a [GitHub issue](https://github.com/cookys/codeforge/issues/new) with:
- What you expected to happen
- What actually happened
- A minimal reproduction (commands run, OS, Rust version)
- Relevant log output (with secrets redacted)

## Code of Conduct

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md) in all project interactions.

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
