# CodeForge: Dream Cycle

Use this skill to run the dream cycle and process your memory signals.

## Instructions

Run: `codeforge dream`

This executes 6 operations in sequence:
1. **compile** — Convert L0 signals to L1 knowledge entries (uses Claude API if ANTHROPIC_API_KEY is set, otherwise rule-based)
2. **lint** — Detect orphaned entries and broken wikilinks
3. **dedup** — Mark near-duplicate entries as superseded
4. **absorb** — Import new entries from `.claude/memory/` (Claude Code's native memory)
5. **decay** — Update ACT-R activation strength based on age and reference count
6. **track** — Update skill confidence metrics in `~/.codeforge/brain/skills/`

To run only one operation: `codeforge dream --only compile`

Your pet gains **+10 XP** each time you run a full dream cycle.
