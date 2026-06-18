# Doc-Sync — CodeForge Config
# Consumed by autopilot:doc-sync. Defines this repo's doc↔code domains so the
# drift audit is sharp + repeatable. See project-config-template/doc-drift-config.md
# in autopilot for the full schema.

## Domains

### memory-pipeline
docs:  doc/concepts.md (§1-4), doc/specs/codeforge-ship.md, doc/specs/codeforge-memory-contract.md, README.md + CLAUDE.md (dream/ship sections)
code:  src/dream/, src/cli/ship.rs, src/cli/mnemos_cli.rs, src/mnemos/, src/llm.rs, src/db/mod.rs
focus: LLM fallback chain (claude -p → Haiku → rule-based), endpoints, opted_in gate, retry schedule, ship-failed/ship-state, no-ship opt-out, what ship actually reads (L1+git, NOT session jsonl)

### pet-daemon-combat
docs:  doc/concepts.md (§5-6), doc/specs/codeforge-mud-engine.md, README.md + CLAUDE.md (pet/daemon sections)
code:  src/pet/, src/power/, src/daemon/, src/clan/
focus: XP values per event, leveling math, 4 stats, village count, daemon-write/CLI-overlay, MOB scan rate (every 10 ticks), elite branch threshold (20), ability effects (catalog-only, not wired), clan skeleton (not wired)

### install-hooks-statusline
docs:  README.md (hook setup), doc/specs/codeforge-install-subcommand.md, doc/getting-started.md, CLAUDE.md (Hook Path Note / Statusline), .env.example
code:  src/cli/install.rs, src/cli/statusline.rs, .claude/scripts/*.js, src/main.rs (env vars)
focus: 3 global hook types (SessionStart recall + SessionEnd dream→ship + PreCompact), install flags shipped (not roadmap), uninstall is a top-level subcommand, statusline 5-line, CODEFORGE_LOCALE default (system→en)

### cli-surface
docs:  README.md quickstart, doc/getting-started.md, doc/concepts.md, CLAUDE.md Development Commands
code:  src/main.rs (clap), src/cli/
focus: every documented command/flag exists; implemented commands not omitted (emit/daemon/tui/attach/strategy/inventory/craft/use/uninstall, memory status|context, mnemos-cli cite-detect, dream --only)

### phase-status
docs:  README.md + CLAUDE.md Phase Roadmap tables, doc/concepts.md §6
code:  doc/projects/INDEX.md archive + presence of modules (src/daemon, src/tui, src/world, src/craft, src/snapshot, src/commentary)
focus: Phase 2a-3f shipped (not planned); Phase 4/5 genuinely absent; nation-p2p is Phase 5 roadmap

### changelog-version-nation
docs:  CHANGELOG.md, Cargo.toml, doc/specs/nation-p2p-design.md
code:  git log, src/ (grep ed25519/nation/P2P → confirm unimplemented)
focus: Cargo.toml version sync; shipped features present in CHANGELOG [Unreleased]; nation P2P consistently Phase-5 roadmap

## Preferred auditor
# See .claude/dispatch-config.md → ## Doc Drift Audit. CC fast-path = the two
# Workflow scripts in .claude/workflows/; native is the portable fallback.

## Staleness threshold
staleness_days: 30
# Last full sweep tracked in .claude/doc-audit-state.json (last_full_audit).
