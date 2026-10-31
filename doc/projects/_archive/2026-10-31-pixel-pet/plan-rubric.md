# Rubric — 2026-10-31-pixel-pet.md

> Source plan: doc/plans/2026-10-31-pixel-pet.md

R1: pet.json schema name stays exactly `codeforge.pet-view/1`; the sprite is a new OPTIONAL field `village.sprite`, JSON `null` when the village has no sprite. No other existing field changes name, type or meaning.
R2: Sprite shape: `width` 24, `height` 24 (height MUST be even), `transparent` is the single ASCII char `.`, `palette` maps single ASCII chars to `[r,g,b]`, every r/g/b ∈ {0,95,135,175,215,255} (xterm-256 cube), `frames` is an array of ≥ 2 frames, each frame exactly `height` strings of exactly `width` chars, each char either `transparent` or a palette key; `frame_ms` 500.
R3: Half-block mapping (one Raster cell = pixel rows 2y and 2y+1): both opaque → U+2580 `▀`, fg = top colour, bg = bottom colour; top opaque + bottom transparent → U+2580, fg = top, bg = 0x01000000; top transparent + bottom opaque → U+2584 `▄`, fg = bottom, bg = 0x01000000; both transparent → U+0020, fg = bg = 0x01000000.
R4: Raster `cells` = standard padded base64 of `columns*rows` little-endian u32 triplets `[codePoint, fg 0x00RRGGBB, bg]`, row-major; `columns` = width, `rows` = height/2.
R5: `codeforge statusline` must never fail, block, or change its printed text because of the sprite; `pet::view::publish` stays best-effort (one stderr line on failure).
R6: Mod code: every `$` call is written at the call site with literal arguments (e.g. `$.env.get('HOME')`); the mod never spawns processes (`$.process.*` not used); animation ≤ 4 frames per second; skip `$.ui.blit` when the next cells equal the current ones.
R7: Formatting only through `./scripts/fmt.sh`; never bare `cargo fmt`.
