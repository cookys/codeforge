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

## Quality gate

Before pushing or opening a PR, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check licenses    # if cargo-deny is installed
```

CI runs the same checks on every PR. PRs cannot be merged with failing CI.

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
