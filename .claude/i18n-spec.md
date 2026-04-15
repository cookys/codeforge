# CodeForge i18n Architecture Spec

> Phase 1 decision log. Captures all architecture decisions from the CEO survey session on i18n approaches.
> Phase 1 implements Layer 1 (UI strings). Layer 2 (game content) is Phase 2.

## Two-Layer Design

```
Layer 1 — UI Strings (compile-time)          Layer 2 — Game Content (runtime)
────────────────────────────────────         ────────────────────────────────────
Crate:   rust-i18n v3                        Format:  RON (Rusty Object Notation)
Format:  YAML (locales/*.yaml)               Load:    daemon reads on startup
Load:    embedded at compile time            Scope:   room descriptions, NPC
Macro:   t!("key")                                    dialogue, mob names, lore
Scope:   all UI chrome, labels,              Owned:   daemon (not CLI)
         village names, stat labels
```

**Why two layers?**
- UI strings are small, stable, and need zero startup overhead → compile-time embedding
- Game content is large, updated between releases, and only needed by the daemon → runtime loading
- Mixing them would either bloat the binary or add startup latency to the CLI

## Layer 1: rust-i18n v3

### File Structure
```
locales/
  en.yaml       ← base locale; ALL keys must exist here
  zh-TW.yaml    ← Traditional Chinese; only overrides needed
```

### Key Naming Convention
```
ui.{component}.{element}     UI chrome
  ui.memory_label            "Memory:"
  ui.status_active           "active"
  ui.status_inactive         "inactive"

stat.{name}                  Stat labels (include punctuation)
  stat.atk                   "ATK:"
  stat.def                   "DEF:"
  stat.sup                   "SUP:"
  stat.ver                   "VER:"
  stat.hp                    "HP"
  stat.xp                    "XP"
  stat.lv                    "Lv."

village.{id}.name            Village display name
village.{id}.tagline         Village tagline/motto
```

### Wire-up (src/main.rs)
```rust
rust_i18n::i18n!("locales", fallback = "en");  // at crate root

fn detect_locale() -> String {
    std::env::var("CODEFORGE_LOCALE")
        .unwrap_or_else(|_| {
            sys_locale::get_locale()
                .unwrap_or_else(|| "en".to_string())
        })
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    rust_i18n::set_locale(&detect_locale());   // before any output
    let cli = cli::Cli::parse();
    cli::run(cli)
}
```

### Locale Detection Priority
1. `CODEFORGE_LOCALE` env var (user override, highest priority)
2. `sys_locale::get_locale()` — reads LANG / LC_ALL / LC_MESSAGES from environment
3. Fallback: `"en"` (rust-i18n fallback = "en" covers missing keys too)

### Usage in Rust Code
```rust
use rust_i18n::t;

// t!() returns Cow<'static, str>
// Use &* to get &str for functions expecting &str
tc(&*t!("stat.hp"), STAT_LBL)

// Or bind to variable for multiple uses / vis() calls
let mem_label = t!("ui.memory_label").to_string();
let mem_vis = vis(&mem_label);

// Dynamic village keys
let vname = t!(&format!("village.{}.name", village.id));
```

### Layout Calculation Rule

When a translated string contributes to fixed-width layout math, use `vis()` dynamically rather than hardcoded char counts:

```rust
// WRONG (breaks when translated string has different width):
let r5_fill = panel_w.saturating_sub(23 + ver_vis);

// RIGHT (works for any locale):
let mem_label = t!("ui.memory_label").to_string();
let mem_status = t!("ui.status_active").to_string();
let r5_fixed = 3 + vis(&mem_label) + 1 + vis(&mem_status) + 2 + ver_vis + 4;
let r5_fill = panel_w.saturating_sub(r5_fixed);
```

## Layer 2: Game Content (Phase 2)

### File Structure
```
content/
  en/
    villages.ron    ← English game content
  zh-TW/
    villages.ron    ← Traditional Chinese game content
```

### Format: RON
RON (Rusty Object Notation) chosen over JSON/TOML/YAML because:
- Typed: maps directly to Rust structs via serde
- No proc-macro overhead (unlike toml/serde_yaml)
- Human-readable and diff-friendly
- Supports enums natively (e.g., `tier: Boss`)

### Daemon Loading Pattern (Phase 2)
```rust
// At daemon startup:
let locale = rust_i18n::locale();
let content_path = format!("content/{}/villages.ron", locale);
let content: VillageContent = ron::from_str(&std::fs::read_to_string(content_path)?)?;
// Falls back to "en" if locale-specific file missing
```

## Phase Boundary

| Work item | Phase |
|-----------|-------|
| Add rust-i18n + sys-locale crates | ✅ Phase 1 |
| locales/en.yaml + locales/zh-TW.yaml | ✅ Phase 1 |
| Wire t!() in statusline.rs + village.rs | ✅ Phase 1 |
| content/en/villages.ron scaffold | ✅ Phase 1 (template only) |
| Daemon loading Layer 2 content | Phase 2 |
| Full game content translation (zh-TW) | Phase 2 |
| Additional locales (ja, ko, etc.) | Future |

## Decision Log

| Decision | Rationale |
|----------|-----------|
| rust-i18n v3 over fluent-rs | Simpler API, compile-time embedding, no runtime parser |
| sys-locale over LANG env direct | Handles macOS/Linux differences; normalizes LC_ALL precedence |
| CODEFORGE_LOCALE override | Allows CI testing and user override without env hacks |
| RON over TOML for game content | Native Rust enum support; no ambiguity in typed deserialization |
| Separate YAML per locale (not one file) | Smaller diffs, easier translator workflow |
| Fallback = "en" at crate level | Guarantees no missing-key panics even in partial translations |
