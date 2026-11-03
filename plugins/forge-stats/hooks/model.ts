// Pure helpers for the forge-stats band and pane: no `$`, so tests and the renderer share them.
// Visual vocabulary follows autopilot's live pane design system
// (~/projects/autopilot/docs/plans/2026-10-13-live-pane-design-system.md): grid 0/2/4/12,
// semantic colour only, verified single-cell glyphs only, no emoji-presentation glyphs.

import type { Layout, PetData, PetView, SessionView, SpriteData, VillageData } from '../types'

// ---------------------------------------------------------------------------------------------
// display width (copied from autopilot mods/live/ui.ts: a plugin cannot import another plugin)

export function cellWidth(c: number): number {
  if (c === 0 || (c >= 0x0300 && c <= 0x036f) || (c >= 0x200b && c <= 0x200f) || (c >= 0xfe00 && c <= 0xfe0f) || (c >= 0x20d0 && c <= 0x20ff)) return 0
  if ((c >= 0x1100 && c <= 0x115f) || (c >= 0x2e80 && c <= 0x303e) || (c >= 0x3041 && c <= 0x33ff) || (c >= 0x3400 && c <= 0x4dbf)
    || (c >= 0x4e00 && c <= 0x9fff) || (c >= 0xa000 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) || (c >= 0xf900 && c <= 0xfaff)
    || (c >= 0xfe30 && c <= 0xfe4f) || (c >= 0xff00 && c <= 0xff60) || (c >= 0xffe0 && c <= 0xffe6)
    || (c >= 0x1f300 && c <= 0x1f64f) || (c >= 0x1f900 && c <= 0x1f9ff) || (c >= 0x20000 && c <= 0x3fffd)) return 2
  return 1
}
export function displayWidth(text: string): number {
  let w = 0
  for (const ch of text) w += cellWidth(ch.codePointAt(0) as number)
  return w
}
export const padRight = (t: string, w: number) => t + ' '.repeat(Math.max(0, w - displayWidth(t)))
export const padLeft = (t: string, w: number) => ' '.repeat(Math.max(0, w - displayWidth(t))) + t

// ---------------------------------------------------------------------------------------------
// tokens

export const KEY_W = 12
export const GLYPH = {
  open: '▾',
  rule: '─',
  bullet: '◦',
  latest: '●',
  empty: '—',
  meterOn: '▰',
  meterOff: '▱',
} as const

// ---------------------------------------------------------------------------------------------
// meter

export function meter(ratio: number, width: number): { on: string; off: string } {
  const r = Math.max(0, Math.min(1, Number.isFinite(ratio) ? ratio : 0))
  const on = Math.floor(r * width)
  return { on: GLYPH.meterOn.repeat(on), off: GLYPH.meterOff.repeat(width - on) }
}

// ---------------------------------------------------------------------------------------------
// pet: read codeforge's `codeforge.pet-view/1` snapshot (README → Live context files)

export const PET_SCHEMA = 'codeforge.pet-view/1'
/** Older than this, the snapshot is labelled with its age (statusline rewrites it every render). */
export const STALE_MS = 10 * 60_000

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
const num = (v: unknown, d = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : d)
const numOrNull = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : null)
const str = (v: unknown, d = '') => (typeof v === 'string' ? v : d)
const strOrNull = (v: unknown) => (typeof v === 'string' ? v : null)
const lines = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [])

/** Parse the snapshot text; `text === null` means the file was not there. */
export function parseSnapshot(text: string | null, path: string | null, now: number): PetView {
  const base: PetView = { status: 'missing', readAt: now, writtenAt: null, path, pet: null, village: null }
  if (text === null) return base
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return { ...base, status: 'bad' }
  }
  if (!isObj(raw) || raw.schema !== PET_SCHEMA) return { ...base, status: 'bad' }
  const written = Date.parse(str(raw.written_at))
  const writtenAt = Number.isFinite(written) ? written : null
  if (!isObj(raw.pet) || !isObj(raw.village)) return { ...base, status: 'no-pet', writtenAt }
  const p = raw.pet
  const v = raw.village
  const pet: PetData = {
    name: str(p.name, '?'),
    level: num(p.level),
    xp: num(p.xp),
    xp_to_next: num(p.xp_to_next),
    hp: num(p.hp),
    atk: num(p.atk),
    def: num(p.def),
    sup: num(p.sup),
    ver: num(p.ver),
    mood: numOrNull(p.mood),
    strategy: strOrNull(p.strategy),
    message: strOrNull(p.message),
  }
  const village: VillageData = {
    id: str(v.id),
    display_name: str(v.display_name),
    language: str(v.language),
    tagline: str(v.tagline),
    rgb: Array.isArray(v.rgb) ? v.rgb.map(x => num(x)) : [],
    art_small: lines(v.art_small),
    art_full: lines(v.art_full),
    sprite: parseSprite(v.sprite),
    sprite_mini: parseSprite(v.sprite_mini, MINI_WIDTH, MINI_HEIGHT),
  }
  return { status: 'ok', readAt: now, writtenAt, path, pet, village }
}

/** Minutes the snapshot is behind `now`, or null when fresh / unknown. */
export function staleMinutes(view: PetView, now: number): number | null {
  if (view.writtenAt === null) return null
  const age = now - view.writtenAt
  return age > STALE_MS ? Math.floor(age / 60_000) : null
}

// ---------------------------------------------------------------------------------------------
// sprite: validate codeforge's `village.sprite`, then turn a frame into Raster cells
// (contract: doc/plans/2026-10-31-pixel-pet.md §2.5 in the codeforge repo)

const CUBE = [0, 95, 135, 175, 215, 255]
export const SPRITE_FRAME_MS = 500
export const SPRITE_SIZE = 24
export const SPRITE_TRANSPARENT = '.'
export const DEFAULT_COLOR = 0x01000000
export const UPPER_HALF = 0x2580
export const LOWER_HALF = 0x2584
export const SPACE = 0x20

/** A valid sprite, or null (missing, null, or any contract break → the pane draws ASCII). */
export function parseSprite(raw: unknown, w: number = SPRITE_SIZE, h: number = SPRITE_SIZE): SpriteData | null {
  if (!isObj(raw)) return null
  const { width, height, transparent, palette, frames, frame_ms } = raw
  if (typeof width !== 'number' || typeof height !== 'number' || !Number.isInteger(width) || !Number.isInteger(height)) return null
  if (width !== w || height !== h) return null
  if (transparent !== SPRITE_TRANSPARENT) return null
  if (frame_ms !== SPRITE_FRAME_MS) return null
  if (!isObj(palette)) return null
  const pal: Record<string, number> = {}
  for (const [k, v] of Object.entries(palette)) {
    if (k.length !== 1 || k.charCodeAt(0) > 0x7f || k === transparent) return null
    if (!Array.isArray(v) || v.length !== 3 || !v.every(c => typeof c === 'number' && CUBE.includes(c))) return null
    pal[k] = ((v[0] as number) << 16) | ((v[1] as number) << 8) | (v[2] as number)
  }
  if (!Array.isArray(frames) || frames.length < 2) return null
  const out: string[][] = []
  for (const f of frames) {
    if (!Array.isArray(f) || f.length !== height) return null
    const rows: string[] = []
    for (const r of f) {
      if (typeof r !== 'string' || r.length !== width) return null
      for (const ch of r) if (ch !== transparent && !(ch in pal)) return null
      rows.push(r)
    }
    out.push(rows)
  }
  return { width, height, transparent, palette: pal, frames: out, frame_ms }
}

/** Base64 of raw bytes: the engine's `toBase64` when present, else `btoa` (same output). */
function base64(bytes: Uint8Array): string {
  const native = (bytes as Uint8Array & { toBase64?: () => string }).toBase64
  if (typeof native === 'function') return native.call(bytes)
  let bin = ''
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

/** One frame as Raster cells: half blocks, little-endian u32 [codePoint, fg, bg] per cell. */
export function halfBlockCells(sprite: SpriteData, frameIndex: number): { columns: number; rows: number; cells: string } {
  const frame = sprite.frames[((frameIndex % sprite.frames.length) + sprite.frames.length) % sprite.frames.length] as string[]
  const columns = sprite.width
  const rows = sprite.height / 2
  const buf = new DataView(new ArrayBuffer(columns * rows * 12))
  const colorOf = (ch: string): number | null => (ch === sprite.transparent ? null : (sprite.palette[ch] ?? null))
  let o = 0
  for (let y = 0; y < rows; y++) {
    const top = frame[y * 2] as string
    const bottom = frame[y * 2 + 1] as string
    for (let x = 0; x < columns; x++) {
      const t = colorOf(top[x] as string)
      const b = colorOf(bottom[x] as string)
      let cp = SPACE
      let fg = DEFAULT_COLOR
      let bg = DEFAULT_COLOR
      if (t !== null && b !== null) [cp, fg, bg] = [UPPER_HALF, t, b]
      else if (t !== null) [cp, fg] = [UPPER_HALF, t]
      else if (b !== null) [cp, fg] = [LOWER_HALF, b]
      buf.setUint32(o, cp, true)
      buf.setUint32(o + 4, fg, true)
      buf.setUint32(o + 8, bg, true)
      o += 12
    }
  }
  return { columns, rows, cells: base64(new Uint8Array(buf.buffer)) }
}

// ---------------------------------------------------------------------------------------------
// band, heartbeat and session view (doc/plans/2026-10-31-statusline-band.md §2.5)

/** codeforge's `crate::live::sanitize_session_id`: `[A-Za-z0-9_-]` kept, every other Unicode scalar → `_`, at most 64, empty → `unknown`. */
export function sanitizeSessionId(raw: string): string {
  let out = ''
  let n = 0
  for (const ch of raw) {
    if (n === 64) break
    out += /^[A-Za-z0-9_-]$/.test(ch) ? ch : '_'
    n += 1
  }
  return out === '' ? 'unknown' : out
}

export const HEARTBEAT_SCHEMA = 'codeforge.mod-alive/1'

export const SESSION_SCHEMA = 'codeforge.session-view/1'

/** The live base a snapshot path sits in (`<base>/codeforge/pet.json` → `<base>`); null when it is not one. */
export function liveBaseOf(petPath: string | null): string | null {
  const tail = '/codeforge/pet.json'
  return petPath !== null && petPath.endsWith(tail) ? petPath.slice(0, -tail.length) : null
}

export const heartbeatPath = (base: string, sid: string) => `${base}/codeforge/mod-alive/${sanitizeSessionId(sid)}.json`
export const sessionViewPath = (base: string, sid: string) => `${base}/codeforge/session/${sanitizeSessionId(sid)}.json`

/** The heartbeat line, as `codeforge statusline` reads it. */
// `summary_row: true` tells autopilot's band this forge-stats draws the one-line summary on the band's second row (PM G33; TUI guideline §1.1 rule 3). An older
// forge-stats answered a one-row ask with a speed label, so autopilot hands that row only to a heartbeat that says so.
export function heartbeatText(sid: string, now: number, layout: Layout): string {
  return JSON.stringify({ schema: HEARTBEAT_SCHEMA, session_id: sid, written_at_ms: now, layout, summary_row: true })
}

export const BAND_MIN_ROWS = 12
export const BAND_MIN_COLUMNS = 26
export const BAND_GAP = 2

export const BAND_PET_W = 30
export const BAND_MEMORY_W = 28

export type BandColumnId = 'ferris' | 'pet' | 'memory'
export type BandColumn = { id: BandColumnId; width: number }

/**
 * The band's columns, left to right, in priority order: Ferris, pet, memory and brain (forge-columns plan §2.5; speed
 * and the request table moved to autopilot, B32). As many as fit in `bodyColumns`, a gap before each but the first;
 * the first that does not fit ends the row.
 */
export function bandLayout(bodyColumns: number, spriteWidth: number): BandColumn[] {
  const want: BandColumn[] = [
    { id: 'ferris', width: spriteWidth },
    { id: 'pet', width: BAND_PET_W },
    { id: 'memory', width: BAND_MEMORY_W },
  ]
  const out: BandColumn[] = []
  let used = 0
  for (const c of want) {
    const need = (out.length > 0 ? BAND_GAP : 0) + c.width
    if (used + need > bodyColumns) break
    out.push({ ...c })
    used += need
  }
  return out
}

const LIGHTS_MEMORY = ['active', 'empty', 'hidden'] as const
const LIGHTS_BRAIN = ['ok', 'degraded', 'offline', 'pending', 'hidden'] as const

/** Parse `codeforge/session/<sid>.json`; null when missing, unreadable, another schema or another session. */
export function parseSessionView(text: string | null, sid: string): SessionView | null {
  if (text === null) return null
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return null
  }
  if (!isObj(raw) || raw.schema !== SESSION_SCHEMA || raw.session_id !== sid) return null
  const memory = isObj(raw.memory) ? raw.memory : {}
  const brain = isObj(raw.brain) ? raw.brain : {}
  const ml = str(memory.light)
  const bl = str(brain.light)
  return {
    memory: (LIGHTS_MEMORY as readonly string[]).includes(ml) ? (ml as SessionView['memory']) : 'hidden',
    active: Math.max(0, Math.floor(num(memory.active))),
    brain: (LIGHTS_BRAIN as readonly string[]).includes(bl) ? (bl as SessionView['brain']) : 'hidden',
  }
}

// ---------------------------------------------------------------------------------------------
// tiers by maxRows for the autopilot tab band (doc/plans/2026-10-31-forge-tabs.md §2.5)

export const MINI_WIDTH = 16
export const MINI_HEIGHT = 12
export const MINI_MIN_ROWS = 6

export type BandTier = 'label' | 'summary' | 'mini' | 'full'

/** One row → nothing (the tab label is autopilot's, B32); 2–5 rows or a thin column → a one-row summary; 6–11 → mini Ferris; 12+ → full. */
export function bandTier(maxRows: number, bodyColumns: number, hasMini: boolean): BandTier {
  if (maxRows <= 1) return 'label'
  if (maxRows < MINI_MIN_ROWS || bodyColumns < BAND_MIN_COLUMNS) return 'summary'
  if (maxRows < BAND_MIN_ROWS) return hasMini ? 'mini' : 'summary'
  return 'full'
}

/** `village.rgb` as a `#rrggbb` text colour; null when it is not three 0–255 integers. */
export function rgbHex(rgb: readonly number[]): string | null {
  if (rgb.length !== 3 || !rgb.every(c => Number.isInteger(c) && c >= 0 && c <= 255)) return null
  return '#' + rgb.map(c => c.toString(16).padStart(2, '0')).join('')
}

// ---- the fleet summary (PM G33; TUI guideline §2.2) ----
// autopilot's live mod publishes `autopilot.fleet` (its fleet.ts FleetSummary, `v: 1`) in `$.state`; this mod only reads it. Anything else (no autopilot, another `v`,
// a wrong shape) is no summary: the fleet part is not drawn.
export type FleetCounts = { needYou: number; failed: number; stuck: number; running: number; waiting: number }
export type FleetSummary = { v: 1; fetchedMs: number | null; error: string | null; totals: FleetCounts }
const COUNT_KEYS = ['needYou', 'failed', 'stuck', 'running', 'waiting'] as const
const isCount = (n: unknown): n is number => typeof n === 'number' && Number.isInteger(n) && n >= 0
export function parseFleet(value: unknown): FleetSummary | null {
  if (typeof value !== 'object' || value === null) return null
  const v = value as Record<string, unknown>
  if (v.v !== 1 || typeof v.totals !== 'object' || v.totals === null) return null
  const t = v.totals as Record<string, unknown>
  if (!COUNT_KEYS.every(k => isCount(t[k]))) return null
  const fetchedMs = typeof v.fetchedMs === 'number' ? v.fetchedMs : null
  const error = typeof v.error === 'string' && v.error !== '' ? v.error : null
  return { v: 1, fetchedMs, error, totals: { needYou: t.needYou as number, failed: t.failed as number, stuck: t.stuck as number, running: t.running as number, waiting: t.waiting as number } }
}

// One part of the summary line: a glyph in its state colour, a number or word in the default colour, a dim separator. Colours are theme keys (guideline §3.3-4).
export type SummaryPart = { text: string; color?: 'warning' | 'error' | 'suggestion' | 'inactive' | 'success'; dim?: boolean }
// the state glyphs of the shared table (guideline §3.1), most severe first: ▲ need_you, ✗ failed, ⊘ stalled, ● running, ⧗ waiting
const COUNTS: readonly { key: keyof FleetCounts; glyph: string; color: SummaryPart['color'] }[] = [
  { key: 'needYou', glyph: '▲', color: 'warning' },
  { key: 'failed', glyph: '✗', color: 'error' },
  { key: 'stuck', glyph: '⊘', color: 'error' },
  { key: 'running', glyph: '●', color: 'suggestion' },
  { key: 'waiting', glyph: '⧗', color: 'inactive' },
]
const partsWidth = (parts: readonly SummaryPart[]) => parts.reduce((n, p) => n + displayWidth(p.text), 0)

// The line, widest form first, the first that fits `cols` wins (guideline §2.2):
//   艦隊 ▲1 ✗1 ⊘2 ●4 ⧗11 · Ferris Lv.3
// dropping from the end: the pet, then ⧗, ●, ⊘, ✗ (each only while a more severe count stays), then the word, leaving the most severe non-zero count alone.
// No count: 「艦隊 ✓」 only after a fetch that succeeded, 「艦隊 ?」 when the last fetch failed or none succeeded yet. No summary (`fleet` null): the pet alone.
// null when nothing fits or there is nothing to draw: the caller passes the row on (`next(e)`), never draws a blank row.
export function summaryParts(fleet: FleetSummary | null, petTag: string | null, cols: number, word: string): SummaryPart[] | null {
  const sep: SummaryPart = { text: ' · ', dim: true }
  const pet: SummaryPart[] = petTag === null ? [] : [{ text: petTag }]
  const forms: SummaryPart[][] = []
  if (fleet === null) {
    if (pet.length > 0) forms.push(pet)
  } else {
    const label: SummaryPart = { text: word, dim: true }
    const counts = COUNTS.filter(c => fleet.totals[c.key] > 0).map(c => [{ text: ' ' }, { text: c.glyph, color: c.color }, { text: String(fleet.totals[c.key]) }] as SummaryPart[])
    if (counts.length === 0) {
      const mark: SummaryPart = fleet.error === null && fleet.fetchedMs !== null ? { text: '✓', color: 'success' } : { text: '?', color: 'inactive' }
      const line = [label, { text: ' ' }, mark]
      if (pet.length > 0) forms.push([...line, sep, ...pet])
      forms.push(line)
    } else {
      const withWord = (n: number) => [label, ...counts.slice(0, n).flat()]
      if (pet.length > 0) forms.push([...withWord(counts.length), sep, ...pet])
      for (let n = counts.length; n >= 1; n--) forms.push(withWord(n))
      forms.push(counts[0].slice(1)) // the most severe count alone, no word
    }
  }
  for (const f of forms) if (partsWidth(f) <= cols) return f
  return null
}
