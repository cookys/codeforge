/** One model request, timed from the stream. */
export type Step = {
  /** Wall-clock ms when the request finished. */
  at: number
  model: string
  /** True when the request ran inside a subagent. */
  isSubagent: boolean
  /** Request sent → first content chunk; null when nothing streamed. */
  ttftMs: number | null
  /** First content chunk → last chunk. */
  decodeMs: number | null
  outTokens: number
  /** outTokens / decode time; null when too short to measure. */
  tps: number | null
}

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

declare module 'claude-code' {
  interface PluginState {
    'forge-stats': {
      steps: Shaped<Step[]>
      pet: Shaped<PetView>
    }
  }
}
