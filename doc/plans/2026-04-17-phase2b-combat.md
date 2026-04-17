# Plan — Phase 2b: MOB + Auto-Combat + Loot

> See `doc/projects/2026-04-17-phase2b-combat/README.md` for KR map + scope boundaries.

## Summary

Daemon 獲得掃描 codebase 生成 MOB 的能力，每 tick 執行自動戰鬥。MOB 死亡落 loot。Pet 透過擊殺累積 XP + item。

## Design

### Data model

```sql
CREATE TABLE mobs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    zone_id     TEXT NOT NULL,            -- pet village id (rust/python/...)
    kind        TEXT NOT NULL,            -- boss | elite | zombie | ghost | doppelganger | void
    name        TEXT NOT NULL,            -- e.g. "src/auth/middleware.rs" or "TODO cluster @ handlers/"
    hp          INTEGER NOT NULL,
    hp_max      INTEGER NOT NULL,
    atk         INTEGER NOT NULL,
    def         INTEGER NOT NULL,
    difficulty  INTEGER NOT NULL DEFAULT 1,
    spawned_at  INTEGER NOT NULL,         -- unix ts
    defeated_at INTEGER                   -- NULL = alive
);
CREATE INDEX idx_mobs_alive ON mobs(zone_id) WHERE defeated_at IS NULL;

CREATE TABLE loot_inventory (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,            -- xp | skill_point | tome | item | crystal | fragment | gem
    name        TEXT NOT NULL,            -- "Rare Item" / "TODO Cleaner" / "Pattern Fragment" / ...
    quantity    INTEGER NOT NULL DEFAULT 1,
    first_acquired_at INTEGER NOT NULL,
    last_acquired_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_loot_dedupe ON loot_inventory(kind, name);
```

### Module layout

```
src/daemon/
  combat.rs         -- NEW: attack/damage/defeat per-tick
  loot.rs           -- NEW: per-kind loot table + inventory upsert
  mob_scanner.rs    -- NEW: source-file scan → MobSpec vec
  mob.rs            -- NEW: MobSpec struct + kind enum
```

### Tick body integration (tick.rs)

```rust
pub fn run_one(conn, world) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let drained = inbox::drain_once(&tx)?;
    for ev in &drained { events::dispatch(world, ev); }
    systems::regen_hp(world);

    // Phase 2b additions
    mob_scanner::rate_limited_scan(&tx, world, tick_count)?;  // every 10 ticks
    combat::run_tick(&tx, world)?;                             // attack alive MOBs
    // (loot rolls happen inside combat::run_tick on defeat)

    systems::check_levelup(world);
    world.serialize_to_db(&tx)?;
    // advance last_tick_at (unchanged)

    tx.commit()?;
    Ok(())
}
```

### RNG

Deterministic Xorshift64*, seeded from `(tick_at, salt)`. No external crate.

```rust
pub fn rng64(seed: u64) -> u64 {
    let mut x = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
    x ^= x >> 21; x ^= x << 35; x ^= x >> 4;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
```

### Scanner rate-limit

Scanner called every 10 ticks (i.e. 10 min at 60s tick). Store "last_scan_tick" in `settings` table or a module-local static (tied to tick_count read from last_tick_at).

### Combat semantics (spec §2)

```
for mob in alive_mobs_in_zone:
    seed = hash(tick_at, mob.id)
    roll = rng(seed)
    hit_chance = (pet.atk + pet.ver) as f64 / (mob.def + mob.difficulty).max(1) as f64
    if (roll / u64::MAX) < hit_chance.min(0.95):
        damage = pet.atk * (0.8 + 0.4 * (roll >> 32 as f32 / u32::MAX as f32))
        mob.hp = mob.hp.saturating_sub(damage)
        if mob.hp == 0:
            mark defeated_at; spawn loot
    # Mob counter-attack (spec: "pet 受傷 → HP 下降")
    counter = mob.atk.saturating_sub(pet.def / 2)
    pet.hp = pet.hp.saturating_sub(counter)
```

### Loot table (spec §2 Loot 系統)

| MOB | Primary | Secondary |
|-----|---------|-----------|
| boss | Rare Item + 100 XP | L1 Connection Tome |
| elite | 40 XP + 1 skill point | Refactor Scroll |
| zombie | 10 XP | TODO Cleaner |
| ghost | Dead Code Crystal | — |
| doppelganger | Pattern Fragment | Abstract Gem |
| void | 20 XP | — |

Per defeat: always drop primary; secondary = RNG roll (30% default).

## Risks

- Scanner path unset → daemon 默默不掃；需 warn-once log
- Large repo scan > 10ms → 違反 tick budget；P2 加 cap（max 1000 files）
- Pet HP 戰鬥吃光 → HP=0 stuck 狀態；regen 系統會救（每 tick +5 HP）

## Phase sequence

P1 → P2 → P3 → P4 → P5 → P6 → QG → review → merge → archive
