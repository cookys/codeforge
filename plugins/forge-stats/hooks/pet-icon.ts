import { GLYPHS } from '../vendor/tux/glyphs'
// The pet tab's icon (PM 01M7MD5Z, plan item 6): one pet emoji on autopilot-live's tab row, exactly 2 cells, with a plain 2-cell fallback for a terminal
// that cannot draw it. forge-stats picks the candidate and decides emoji vs fallback; autopilot only draws `text` (contract: codeforge.panel/1 `pet.icon`).
import type { PetIcon } from '../types'

export type PetIconCandidate = { id: string; emoji: string; fallback: string }

// Each emoji is one code point with Emoji_Presentation=Yes (drawn as emoji with no U+FE0F) and East Asian Width W, so every terminal that draws it gives it 2 cells.
// Each fallback is two printable ASCII characters: 2 cells everywhere, never East Asian Ambiguous (a CJK locale cannot widen it).
export const PET_ICON_CANDIDATES: readonly PetIconCandidate[] = Object.values(GLYPHS.pet.icons).map(({ id, emoji, fallback }) => ({ id, emoji, fallback }))
export const DEFAULT_PET_ICON = 'cat'

// What the env says about the terminal, read once at session start ($.env.get with literal names; see register.tsx).
export type IconEnv = {
  term: string | undefined
  lcAll: string | undefined
  lcCtype: string | undefined
  lang: string | undefined
  /** CODEFORGE_PET_ICON: a candidate id, or `plain` to force the fallback. */
  pick: string | undefined
}

// Terminals known not to draw emoji: the Linux VT console, a dumb terminal, the old serial / vt100 family.
const NO_EMOJI_TERM = /^(linux|dumb|vt\d+|ansi|cons\d+)$/

// The locale that the C library uses for LC_CTYPE: LC_ALL, else LC_CTYPE, else LANG (POSIX precedence). Unset everywhere means the POSIX locale (no UTF-8)
// only when something says so; with all three unset (common under ssh + tmux) the terminal is assumed UTF-8, the way Claude Code itself draws its UI.
function utf8Locale(env: IconEnv): boolean {
  const loc = [env.lcAll, env.lcCtype, env.lang].find(v => v !== undefined && v !== '')
  if (loc === undefined) return true
  return /utf-?8/i.test(loc)
}

export function emojiOk(env: IconEnv): boolean {
  if (env.pick === 'plain') return false
  if (env.term !== undefined && NO_EMOJI_TERM.test(env.term)) return false
  return utf8Locale(env)
}

export function petIcon(env: IconEnv): PetIcon {
  const c = PET_ICON_CANDIDATES.find(x => x.id === env.pick) ?? (PET_ICON_CANDIDATES.find(x => x.id === DEFAULT_PET_ICON) as PetIconCandidate)
  const emoji = emojiOk(env)
  return { id: c.id, emoji: c.emoji, fallback: c.fallback, text: emoji ? c.emoji : c.fallback }
}
