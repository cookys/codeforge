# src/ — CodeForge source (detail split from the root CLAUDE.md)

Loaded by Claude Code when a file under `src/` is touched. Rules whose violation causes
damage are also kept as one-liners in the root `CLAUDE.md`. Command handlers: `src/cli/CLAUDE.md`;
Mnemos ship/cite: `src/mnemos/CLAUDE.md`.

## Architecture

### Source Layout (`src/`)

```
src/
  main.rs           ← clap CLI dispatch (locale: CODEFORGE_LOCALE → system locale → "en")
  llm.rs            ← LLM backend chain: claude -p headless → ANTHROPIC_API_KEY (Haiku) → rule-based
  cli/              ← command handlers (one file per command)
    init.rs         ← codeforge init
    learn.rs        ← codeforge learn
    dream.rs        ← codeforge dream (--quiet / --only <op>)
    search.rs       ← codeforge memory search
    ingest.rs       ← codeforge ingest
    pet.rs          ← codeforge pet
    statusline.rs   ← codeforge statusline (ANSI 5-line panel + right-column pet art)
    install.rs      ← codeforge install (--hooks / --all / --project-hooks) + uninstall
    ship.rs         ← codeforge ship (Mnemos L2 ledger)
    mnemos_cli.rs   ← codeforge mnemos-cli (cite / cite-detect / context)
  memory/
    l0.rs           ← raw signals (JSONL file-based)
    l1.rs           ← compiled knowledge (SQLite FTS5)
    fts.rs          ← FTS5 search
  dream/
    compile.rs      ← L0 → L1 via the LLM backend chain (src/llm.rs)
    absorb.rs       ← aggregate recent signals
    ingest_digests.rs ← pull per-repo session digests into L0
  brain/
    episode.rs      ← session episode records
  pet/
    state.rs        ← PetState: level, HP, XP, stats
    xp.rs           ← XP award logic
    badges.rs       ← badge system (skeleton, not yet wired)
    live_state.rs   ← live-overlay read path (pet_snapshot + unseen event_inbox)
    village.rs      ← 5 hardcoded villages (rust/python/typescript/go/javascript)
    view.rs         ← pet view snapshot `<live_base>/codeforge/pet.json` (codeforge.pet-view/1) for UI readers
    sprite.rs       ← pixel sprites for pet.json `village.sprite` (Ferris 24×24, 2 frames) + contract validator
    ability.rs      ← ability unlock catalog (display-only; combat effects not yet wired)
  daemon/           ← Phase 2 MUD engine: tick loop, ECS, event_inbox, combat, mob_scanner + dup_detect (Doppelganger), loot, strategy, mood, lifecycle
  combat (in daemon/) ← auto-combat resolution
  mnemos/           ← ship transport/digest/config/state/cite/evidence (Mnemos source role)
  clan/             ← CodePower clan content provider skeleton (not yet wired to village.rs)
  craft/            ← loot crafting + active items (Phase 3e)
  tui/              ← alt-screen TUI + Local Map (Phase 2c)
  world/            ← World Map + Zone unlock (Phase 3a)
  commentary/       ← AI commentary (Haiku, ≤1/hour, Phase 3c)
  snapshot/         ← codeforge snapshot ASCII monthly card (Phase 3f)
  db/
    mod.rs          ← Context, Connection, PRAGMA setup
    migrations.rs   ← schema migrations (inline SQL)
  import/           ← claude.ai export parser
  power/            ← CharacterStats (5 fields: ATK/DEF/SUP/VER + HP/Activity)
```

### Data Layout (`.codeforge/`)

```
.codeforge/
  store/
    concepts/       ← L1 knowledge .md files (by topic)
    connections/    ← L1 link files
    qa/             ← Q&A pairs
  signals/
    YYYY-MM-DD.jsonl ← L0 raw signals
  projections/
    AGENTS.md       ← auto-projected context for sub-agents
  codeforge.db     ← SQLite: pet state, FTS index, episodes
```

### Key Design Decisions

- **L0**: JSONL files (human-readable, git-friendly, no lock needed for append)
- **L1**: SQLite FTS5 (full-text search, structured)
- **Daemon-free** (Phase 1): statusline is a one-shot command, not a daemon
- **Phase 2**: daemon owns the authoritative `pet_snapshot` (CLI never writes it). The CLI still appends XP events (`xp_events` + `event_inbox`) and computes a level-up cascade in-memory on read (live-overlay) — so "daemon-owned" means `pet_snapshot` specifically, not all game rows
- **CJK strings**: ALWAYS use `.chars().take(N).collect::<String>()`, NEVER `&s[..N]`

### CodePower as Phase 2 Test Zone

The CodePower repo is the primary test environment for Phase 2 features:
- The MUD daemon will first be piloted in CodePower sessions
- Phase 2 TUI will render in the CodePower terminal context
- This allows testing with real workload before generalizing

### LLM Backend (Shared, key-optional)

**dream/ship** resolve their LLM via a 3-layer **fallback chain**, not a single key:
`claude -p` headless (Claude Code CLI, no key, Opus default, highest quality) → `ANTHROPIC_API_KEY` (direct Haiku API, per-token billing) → rule-based passthrough (no LLM). See `src/llm.rs` + `src/dream/compile.rs` + `src/cli/ship.rs`. **AI commentary is NOT on this chain** — it's 2-layer only (Haiku API if `ANTHROPIC_API_KEY` set, else rule-based; never `claude -p` — `src/commentary/` doesn't import `crate::llm`). See `doc/concepts.md` for the user-facing explanation.

`ANTHROPIC_API_KEY` is therefore **optional** — only the second fallback. When set, it's read from the `~/.claude/` environment or shell profile (shared with CodePower), not a per-project `.env` (though `dotenvy` will also pick up a local `.env`). `.env.example` lists it under Optional.

### Dev Rule: Keep projects independent

Do NOT import CodePower's codebase into CodeForge tests or vice versa. CodeForge must be self-contained. The interaction is at the **runtime level** (hooks + CLI invocation), not at the code level.
