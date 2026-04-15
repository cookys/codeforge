# CodeForge RPG Engine — Architecture Spec

> Phase 1 decision log. Captures all CEO-mode architectural decisions made during the MUD-like idle RPG survey session.
> **Do not implement** until Phase 2 daemon work begins. This doc prevents architectural drift between sessions.

## Core Model: Daemon Owns All Writes

```
┌─────────────────────────────────────────────────────┐
│  DAEMON (long-running)                              │
│  - tick game loop (1 tick = 60s)                    │
│  - all SQLite writes                                │
│  - XP, HP, combat, level-ups, item drops            │
└────────────────────────┬────────────────────────────┘
                         │ SQLite WAL (read-only from CLI)
┌────────────────────────▼────────────────────────────┐
│  CLI (codeforge statusline)                         │
│  - SELECT from pet_snapshot                         │
│  - NEVER writes game state                          │
│  - NEVER advances time                              │
└─────────────────────────────────────────────────────┘
```

**Two-writer rule (hard constraint):**
- ONLY daemon writes game state tables
- ONLY CLI writes user preference tables (locale, display settings)
- These are separate tables with zero overlap
- Rationale: eliminates all SQLite write contention; no IPC needed

## Tick System

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Tick interval | 60 seconds | Fine enough for meaningful idle, coarse enough to avoid CPU |
| Catch-up cap | 240 ticks (4 hours) | Prevents 2-week idle → instant max level |
| Burst pacing | 1 tick per 100ms | Avoids CPU spike on startup catch-up |

**Catch-up formula** for gaps > 240 ticks:
```
effective_ticks = 240 + sqrt(missed_ticks - 240)
```
Diminishing returns beyond 4h. A 2-week gap yields ~252 effective ticks, not 20,000.

**On daemon start:**
1. Read `last_tick_at` from DB
2. Compute `missed = (now - last_tick_at) / 60s`
3. If `missed > 240`: apply compression formula
4. Process ticks in 100ms bursts
5. Write `last_tick_at = now` after batch completes

## ECS: `hecs` Crate

Entity = one pet instance per user. All game state as components.

**Components:**
```rust
struct PetStats { hp: u32, hp_max: u32, xp: u32, xp_to_next: u32, level: u32, atk: u32, def: u32, sup: u32 }
struct VillageId { id: String }
struct StatusEffect { kind: EffectKind, remaining_ticks: u32 }
struct LastMessage { text: String }
```

**Systems (run per tick):**
- `xp_system`: process activity signals → XP gain
- `combat_system`: tick mob encounters → HP damage/rewards
- `regen_system`: HP regeneration per tick
- `levelup_system`: check xp_to_next → level up, recalculate stats
- `message_system`: generate speech bubble text based on recent events

**Serialization:** after each tick batch → serialize ECS world → `pet_snapshot` table in SQLite.

## Daemon ↔ CLI Contract

**Shared read path:**
```sql
SELECT village_id, level, hp, hp_max, xp, xp_to_next, atk, def, sup, last_message, updated_at
FROM pet_snapshot
WHERE id = 1
```
Single row, updated by daemon each tick. CLI reads on every `statusline` render.

**SQLite WAL mode:** allows concurrent reader (CLI) + writer (daemon) without locking.

**No IPC required:** polling via SQLite read is sufficient for the statusline update cadence (renders on every Claude Code prompt).

## Mob Roster (per village)

| Village | Weak | Medium | Boss (24h respawn) |
|---------|------|--------|---------------------|
| The Forge-Ruins (Rust) | Borrow Checker Golem | Lifetime Wraith | The Borrow Checker |
| Scriptorium Vast (Python) | Indent Demon | None-Walker | The GIL |
| Border Garrison (TypeScript) | Type Guard | any Phantom | The Compiler |
| Dockside Workshop (Go) | Goroutine Ghost | Nil Pointer Imp | The Race Condition |
| Strata Bazaar (JavaScript) | NaN Sprite | Undefined Poltergeist | The Event Loop |

## Daemon Deployment

- Linux: systemd user unit (`~/.config/systemd/user/codeforge-daemon.service`)
- macOS: launchd plist (`~/Library/LaunchAgents/dev.codeforge.daemon.plist`)
- Startup trigger: `codeforge daemon start` (CLI subcommand, Phase 2)
- Auto-start on login via systemd `WantedBy=default.target`

## Phase Boundary

| Feature | Phase |
|---------|-------|
| Display layer (statusline reads Village struct) | ✅ Phase 1 |
| i18n wiring (rust-i18n YAML) | ✅ Phase 1 |
| Daemon process + tick loop | Phase 2 |
| SQLite game tables + migrations | Phase 2 |
| hecs ECS integration | Phase 2 |
| Combat / mob system | Phase 2 |
| Daemon ↔ CLI pet_snapshot contract | Phase 2 |
| systemd/launchd deployment | Phase 2 |

## Decision Log

| Decision | Rationale |
|----------|-----------|
| No IPC (polling SQLite) | Zero dependencies, works with existing SQLite infra |
| hecs over bevy_ecs | Minimal deps, no renderer needed, fits CLI tool |
| 60s tick | Sweet spot between responsiveness and overhead |
| 240-tick cap | Prevents exponential idle exploitation; sqrt tail is fair |
| Two-writer rule | Simplest correctness guarantee for concurrent access |
| RON for game content | Typed deserialization without derive macros; human-readable |
