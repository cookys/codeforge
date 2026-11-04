# doc/ — roadmaps (split from the root CLAUDE.md)

Loaded by Claude Code when a file under `doc/` is touched. `scripts/check-doc-drift.py`
(roadmap-consistency) compares the Phase Roadmap table below with README.md.

## Phase Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1 | Memory CLI + Common Pet | ✅ done |
| 2a | Daemon framework + IPC socket + tick loop | ✅ done |
| 2b | MOB generation + auto-combat + loot | ✅ done |
| 2c | TUI rendering + Local Map | ✅ done |
| 3a | World Map + Zone unlock | ✅ done |
| 3b | Strategy Mode | ✅ done |
| 3c | AI Commentary (Haiku, 1/hour max, opt-in) | ✅ done |
| 3d | Stickiness: welcome-back, mood decay, zone mastery, milestones | ✅ done |
| 3e | Loot crafting + active item use | ✅ done |
| 3f | `codeforge snapshot` (shareable ASCII monthly report) | ✅ done |
| 4 | Zoa 3D pet animation | planned |
| 5a | Nation Plugin + credential verify (ed25519) | planned |
| 5b | Organizer cross-Nation events | planned |
| 5c | Nation Statusline Theme (tiered unlock) | planned |

## Mnemos Integration Roadmap

Orthogonal to MUD Phase Roadmap above; aligned with Mnemos sprint cadence (`cookys/mnemos:docs/specs/20-sprint-0-2.md`).

| Sprint | Deliverable | Status |
|--------|-------------|--------|
| Mnemos Sprint 0 | Mnemos Rust foundation (no codeforge work) | blocked on Mnemos |
| Mnemos Sprint 1 | `codeforge ship` + `codeforge mnemos-cli cite` end-to-end; SessionEnd hook chain `dream → ship` | ✅ shipped (v0.0.4) — see `doc/specs/codeforge-ship.md` |
| Mnemos Sprint 2 | `mnemos-cli context` for SessionStart hook (cross-source atom recall) | ✅ shipped (v0.0.4, `mnemos-cli context`) |
| Mnemos Sprint 5+ | Replace fulltext_match cite heuristic with Haiku-based detection | backlog |
