# Consult: pixel-pet sprite contract for a terminal Raster (half-block) renderer

Context: codeforge (Rust CLI) writes a JSON snapshot `pet.json` (schema `codeforge.pet-view/1`) that a
Claude Code "mod" (TypeScript, no Node/DOM) reads and draws in a side pane. The pane can draw a `Raster`:
`columns × rows` terminal cells, each cell = little-endian u32 triplet `[codePoint, fg 0x00RRGGBB, bg]`
(`0x01000000` = terminal default colour), base64. Only single-width BMP chars allowed. `$.ui.blit` swaps
the cells of a mounted Raster up to ~60 fps; `$.clock.every(ms)` drives timers. Owner terminals: Windows
Terminal (tmux reports no RGB → colours likely clamped to 256) and WezTerm (RGB). Pane width ≈ 40–60 cols.
Plan: half-block rendering — one cell = two vertical pixels via `▀` (fg = upper pixel, bg = lower pixel).

Questions (answer concretely; recommend one option each, with the trade-off):
1. JSON sprite encoding: palette + rows of palette-index characters per frame (e.g. "..aab.."), vs RLE,
   vs base64 indexed bytes. Transparency representation (map to terminal default bg). What keeps it
   human-diffable in a Rust source file AND cheap for the statusline (written every few seconds)?
2. Size: 16×16 vs 24×24 vs 32×24 pixels for a pane ≈ 40–60 columns that also shows text stats. Aspect:
   terminal cells are ~1:2, half-block makes pixels ~square — confirm.
3. Idle animation for a tiny creature (Ferris the crab): how many frames, which motions (blink, bob,
   claw wave), timing (ms per frame), and how to avoid flicker/CPU waste (only blit on change, pause
   when pane not visible?).
4. Edge cases of half-block: a cell whose top is transparent and bottom opaque (use `▄` instead?), both
   transparent (space with default bg), colour clamping to 256 colours (pick a palette that survives it).
5. Authoring pixel art with an LLM reliably: what process/format minimises broken sprites (grid with
   fixed-width rows, symmetry, validation tests: row lengths, palette coverage)?
6. Anything in this contract design you would do differently (schema versioning, where sprite lives:
   inside pet.json vs separate file).
