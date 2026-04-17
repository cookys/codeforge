# Phase 3f — `codeforge snapshot`

> Zero-friction shareable ASCII monthly report card. Spec §3.6.

**Plan**: [../../plans/2026-04-18-phase3f-snapshot.md](../../plans/2026-04-18-phase3f-snapshot.md)
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.6
**Branch**: `feature/phase3f-snapshot`
**Started**: 2026-04-18
**Completed**: 2026-04-18

## Progress

| Phase | Status | Commit |
|-------|--------|--------|
| P1 Data aggregation | ✅ done | `e3eb472` |
| P2 ASCII card renderer | ✅ done | `e3eb472` |
| P3 CLI wiring + polish | ✅ done | `af40184` |
| QG (495/495 tests, clippy clean) | ✅ done | `943756a` |
| Review r1 → fix (1 finding) | ✅ done | `ac192cc` |
| Review r2 → clean | ✅ done | — |
| Merge + archive | ✅ done | `(merge commit)` |

## Outcome

- `codeforge snapshot [--days N]` prints a box-drawn ASCII card to stdout (CARD_WIDTH=58 visible cols).
- Sections: pet name + date header, 2×2 zone grid + 5th-village row, kill breakdown (Boss / Elite / Zombie / Ghost), longest consecutive-same-zone-same-kind streak (suppressed when < 2), identity + tagline quote footer.
- `--days` default 30; 0 / negative rejected with `--days 必須為正整數`.
- Pure data → string: `render(&SnapshotData) -> String` — no I/O, offline-testable. `collect(conn, days)` is the only DB touch and is read-only (respects daemon-owns-derived-writes).
- CJK-safe: `pad_visible` + `clip_visible` via unicode-width. Width invariant verified at 9999-count stress test.
- `village_short_name` uses a per-village place-noun mapping (Forge-Ruins / Scriptorium / Garrison / Dockside / Strata) instead of first-word stripping, so "The Forge-Ruins" no longer renders as "Rust The".

## Test Delta

466 baseline (post-3a merge) → 495 total. +29 tests:
- `snapshot::tests` — 9 aggregation (empty DB, window param, kind grouping, streak same/different zone/kind, window respect, seeded end-to-end)
- `snapshot::render::tests` — 15 (pet name/date, kill counts, streak shown/suppressed, card-width invariant at 4-digit counts, missing-pet fallback, locked-zone `???`, veteran glyph, custom days, pad/clip CJK, village short name, zero-kill rendering) — includes 2 regression tests from r1 fix
- `cli::snapshot::tests` — 4 (zero/negative days rejected, ensure_initialized required, end-to-end collect → render on seeded DB)
- Plus 1 from running full suite (existing pre-existing test was retro-counted)

## Review Findings (r1)

1. **IMPORTANT** — `clip_visible` always appended U+2026 even when input already fit the budget. `pad_visible` protected production via its strict `>` gate, but the standalone contract ("truncate so width ≤ max_cols") was wrong. Fixed with exact-fit fast path + 2 regression tests.

Two other r1 findings were correctly rejected:
- Streak ordering comment (low confidence, not a defect)
- Width-invariant test extraction (reviewer mis-read — `chars().count()` correctly strips two ASCII border chars regardless of CJK content)

Round 2: clean, no new findings.

## Deferred to Backlog

- `--clipboard` flag (arboard/cli-clipboard crate)
- Legendary commits progress (depends on Phase 5a Nation)
- Loot crafting count (depends on Phase 3e)
- Image/PNG export (Phase 4 Zoa)
