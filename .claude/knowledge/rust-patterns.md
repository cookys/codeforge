# Rust Patterns — CodeForge

<!-- last-verified: 2026-04-16 -->

## CJK 字串截斷必須用 .chars().take(N)

**Date**: 2026-04-14 | **Context**: `src/cli/learn.rs` preview truncation, `src/cli/search.rs` FTS snippet
**Problem**: `&s[..80]` panics with "byte index N is not a char boundary" when string contains CJK characters (each takes 3 bytes in UTF-8). E.g. `"更"` spans bytes 78–81, so `&s[..80]` lands inside the character.
**Solution**: `let preview: String = s.chars().take(80).collect();`
**Rule**: NEVER index into a Rust `&str` with a byte offset when input may contain multibyte characters (CJK, emoji, etc.). Always use `.chars().take(N).collect()`.
**Files fixed**: `src/cli/learn.rs:38`, `src/cli/search.rs:24`, `src/memory/l0.rs:95`, `src/dream/compile.rs:74`, `src/brain/episode.rs:33`

## UUID id[..8] — 雖然 ASCII 安全，仍應用 .chars().take(8)

**Date**: 2026-04-16 | **Context**: Phase 1 code review
**Problem**: UUID v4 字串是 ASCII，所以 `&id[..8]` 不會 panic。但跟 CJK fix 不一致，且未來若 ID 格式改為包含 Unicode 字元（如 nanoid with emoji），就會炸。
**Solution**: 統一用 `id.chars().take(8).collect::<String>()` 以防禦性一致性。

## f32/u32 overflow 在 exponential XP scaling

**Date**: 2026-04-16 | **Context**: `src/pet/state.rs` xp_to_next 計算
**Problem**: `(xp_to_next as f32 * 1.5) as u32` — f32 在 ~16M 失去精度，u32 在 ~4.3B overflow，`as u32` 在 release build 會 wraps（非 panic），若 wraps 為 0 則 `while xp >= xp_to_next` 變無限迴圈。
**Solution**: 用 f64 + 上限 cap：`((xp_to_next as f64 * 1.5) as u64).min(10_000_000) as u32`
