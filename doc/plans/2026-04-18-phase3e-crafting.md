# Phase 3e — Loot Crafting + Active Item Plan

**Branch**: `feature/phase3e-crafting`
**Spec**: `doc/specs/codeforge-mud-engine.md` §3.5 (合成系統) + §3.7 (主動 Item 使用)
**Depends on**: Phase 2b loot_inventory, Phase 3a game_world

## Goal

Ship the three §3.5 recipes + `codeforge craft/inventory/use` CLI + daemon Ghost Repellent consumer. Storage-only for ReduceDifficulty + SuppressDoppelgangerSplit; combat math + split-path consumers deferred to later phases (noted in commits).

## Phase Breakdown

- **P1** — Schema v9 `active_effects` + `craft::{Recipe, Effect, recipes()}` pure module
- **P2** — `codeforge inventory` + `codeforge craft [<name>]` CLI
- **P3** — `codeforge use <name>` CLI + `mob_scanner` ghost filter

## QG + Review + Merge

- `cargo check + clippy + test`
- `requesting-code-review` 2 rounds; Critical + Important fix
- Merge → main; archive.
