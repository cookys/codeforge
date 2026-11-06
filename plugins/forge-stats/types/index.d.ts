/** `pet` of codeforge's `codeforge.pet-view/1` snapshot. */
export type PetData = {
  name: string
  level: number
  xp: number
  xp_to_next: number
  hp: number
  atk: number
  def: number
  sup: number
  ver: number
  mood: number | null
  strategy: string | null
  message: string | null
}

/** `village.sprite` of the snapshot (codeforge `src/pet/sprite.rs`), already validated. */
export type SpriteData = {
  width: number
  /** Even: one terminal cell holds two vertical pixels. */
  height: number
  transparent: string
  /** palette char → 0x00RRGGBB */
  palette: Record<string, number>
  /** frames[i][row] is a string of `width` palette chars. */
  frames: string[][]
  frame_ms: number
}

/** `village` of the snapshot. */
export type VillageData = {
  id: string
  display_name: string
  language: string
  tagline: string
  rgb: number[]
  art_small: string[]
  art_full: string[]
  /** null: no sprite, or one that breaks the contract → draw the ASCII art. */
  sprite: SpriteData | null
  /** The 16x12 sprite for a 6–11 row band; null when absent or off-contract. */
  sprite_mini: SpriteData | null
}

/** What the pane last read from `<live_base>/codeforge/pet.json`. */
export type PetView = {
  /** ok: a pet; no-pet: codeforge has none yet; missing: no file; bad: unreadable or other schema. */
  status: 'ok' | 'no-pet' | 'missing' | 'bad'
  /** Wall-clock ms of the read. */
  readAt: number
  /** The snapshot's own `written_at`, ms; null when absent. */
  writtenAt: number | null
  path: string | null
  pet: PetData | null
  village: VillageData | null
}

/** Where forge-stats draws: the band above the prompt, or the pane. Kept in `$.store` (`layout`). */
export type Layout = 'band' | 'pane'

/** This session's lights, from codeforge's `codeforge/session/<sid>.json` (`codeforge.session-view/1`). */
export type SessionView = {
  memory: 'active' | 'empty' | 'hidden'
  /** Active L1 entries in this project's memory. */
  active: number
  brain: 'ok' | 'degraded' | 'offline' | 'pending' | 'hidden'
}

/** A tone of the integrated panel: two theme text-colour keys and three Text styles; never a background or inverse. */
export type PanelTone = 'subtle' | 'inactive' | 'dim' | 'bold' | 'mono' | 'success'

/** One line as the pane lays it out: parts drawn in order, none ever cut. */
export type PanelLine = { text: string; tone?: PanelTone }[]

/**
 * `codeforge.panel/1`, the 用量 / 寵物 tabs of autopilot's integrated panel (design v2, operator d-fb45042e).
 * `tiers` maps a width ("40" … "120") to the lines laid out for it; the reader takes the widest that fits.
 */
export type PanelState = {
  schema: 'codeforge.panel/1'
  updated_at_ms: number
  usage: { tiers: Record<string, PanelLine[]> }
  /** `sprite` is pet.json's `village.sprite` as forge-stats validated it (SpriteData: palette 0xRRGGBB; static in v1); `tiers` the stats lines without the art. */
  pet: { title: string; sprite: SpriteData | null; tiers: Record<string, PanelLine[]> }
  /** The O4 band's right-hand summary columns (doc/specs/panel-band-contract.md §2); optional: a reader without it draws the 圓桌 column alone. */
  side?: PanelSide
}

/** Lines per column width ("30" … "50"): the reader takes the widest key that fits its inner width. Ratios and counts only, never money. */
export type PanelSide = {
  usage: { tiers: Record<string, PanelLine[]> }
  usage_full: { tiers: Record<string, PanelLine[]> }
  memory: { tiers: Record<string, PanelLine[]> }
}

declare module 'claude-code' {
  interface PluginState {
    'forge-stats': {
      pet: Shaped<PetView>
      session: Shaped<SessionView | null>
      /** Read by autopilot's panel; plain (no shape): its `schema` field versions it. */
      panel: PanelState | null
    }
  }
}
