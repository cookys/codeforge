// Pure helpers for the forge-stats pane: no `$`, so tests and the renderer share them.
// Visual vocabulary follows autopilot's live pane design system
// (~/projects/autopilot/docs/plans/2026-10-13-live-pane-design-system.md): grid 0/2/4/12,
// semantic colour only, verified single-cell glyphs only, no emoji-presentation glyphs.

import type { Layout, PetData, PetView, SessionView, SpriteData, Step, VillageData } from '../types'

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
  /** speed (U+03DF, one cell, never emoji presentation) */
  speed: 'ϟ',
  spark: ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'],
} as const

// ---------------------------------------------------------------------------------------------
// formats

/** Sub-second durations (live has none): `840ms`, `1.2s`, `14s`, `2m05s`. */
export function fmtMs(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return GLYPH.empty
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`
  if (ms < 60_000) return `${Math.floor(ms / 1000)}s`
  const m = Math.floor(ms / 60_000)
  return `${m}m${String(Math.floor((ms % 60_000) / 1000)).padStart(2, '0')}s`
}
export const fmtTps = (tps: number | null) => (tps === null || !Number.isFinite(tps) ? GLYPH.empty : String(Math.round(tps)))
export const fmtCount = (n: number) => n.toLocaleString('en-US')
export const shortModel = (m: string) => m.replace(/^claude-/, '').replace(/-\d{8}$/, '').replace(/\[1m\]$/, '')

// ---------------------------------------------------------------------------------------------
// throughput

export type Summary = { requests: number; outTokens: number; avgTps: number | null; medianTtft: number | null; bestTps: number | null }

export function summarize(list: readonly Step[]): Summary {
  const timed = list.filter(s => s.tps !== null && s.decodeMs !== null)
  const tokens = timed.reduce((n, s) => n + s.outTokens, 0)
  const decode = timed.reduce((n, s) => n + (s.decodeMs ?? 0), 0)
  const ttfts = list.map(s => s.ttftMs).filter((x): x is number => x !== null).sort((a, b) => a - b)
  const rates = timed.map(s => s.tps as number)
  return {
    requests: list.length,
    outTokens: list.reduce((n, s) => n + s.outTokens, 0),
    // token-weighted: total tokens over total decode time
    avgTps: decode > 0 ? tokens / (decode / 1000) : null,
    medianTtft: ttfts[Math.floor(ttfts.length / 2)] ?? null,
    bestTps: rates.length ? Math.max(...rates) : null,
  }
}

/** One spark cell per value, scaled to the largest; a missing value draws as the lowest step, dim. */
export function sparkline(values: readonly (number | null)[]): string {
  const max = Math.max(0, ...values.map(v => v ?? 0))
  return values
    .map(v => {
      if (v === null || max <= 0) return GLYPH.spark[0]
      const i = Math.min(GLYPH.spark.length - 1, Math.floor((v / max) * (GLYPH.spark.length - 1) + 0.5))
      return GLYPH.spark[i]
    })
    .join('')
}

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
export const HEARTBEAT_MS = 2000
/** A band blit older than this no longer counts as the pet being shown. */
export const SHOWN_MS = 2000

export const SESSION_SCHEMA = 'codeforge.session-view/1'

/** The live base a snapshot path sits in (`<base>/codeforge/pet.json` → `<base>`); null when it is not one. */
export function liveBaseOf(petPath: string | null): string | null {
  const tail = '/codeforge/pet.json'
  return petPath !== null && petPath.endsWith(tail) ? petPath.slice(0, -tail.length) : null
}

export const heartbeatPath = (base: string, sid: string) => `${base}/codeforge/mod-alive/${sanitizeSessionId(sid)}.json`
export const sessionViewPath = (base: string, sid: string) => `${base}/codeforge/session/${sanitizeSessionId(sid)}.json`

export type BandBlit = { at: number; ok: boolean }

/** True only in the band layout with a successful band blit in the last 2 s. */
export const petShown = (layout: Layout, last: BandBlit | null, now: number): boolean =>
  layout === 'band' && last !== null && last.ok && now - last.at >= 0 && now - last.at < SHOWN_MS

/** The heartbeat line, as `codeforge statusline` reads it. */
export function heartbeatText(sid: string, now: number, layout: Layout, shown: boolean): string {
  return JSON.stringify({ schema: HEARTBEAT_SCHEMA, session_id: sid, written_at_ms: now, layout, pet_shown: layout === 'band' && shown })
}

export const BAND_MIN_ROWS = 12
export const BAND_MIN_COLUMNS = 26
export const BAND_SPEED_W = 34
export const BAND_GAP = 2

export const BAND_PET_W = 30
// a sub-agent row is 33 cells + the model name; 46 holds a 13-character name
export const BAND_RECENT_W = 46
export const BAND_MEMORY_W = 28

export type BandColumnId = 'ferris' | 'pet' | 'speed' | 'recent' | 'memory'
export type BandColumn = { id: BandColumnId; width: number }

/**
 * The band's columns, left to right, in priority order: Ferris, pet, speed, recent requests, memory and brain
 * (forge-columns plan §2.5). As many as fit in `bodyColumns`, a gap before each but the first; the first that does not
 * fit ends the row. The spare width goes to the request table when it is shown.
 */
export function bandLayout(bodyColumns: number, spriteWidth: number): BandColumn[] {
  const want: BandColumn[] = [
    { id: 'ferris', width: spriteWidth },
    { id: 'pet', width: BAND_PET_W },
    { id: 'speed', width: BAND_SPEED_W },
    { id: 'recent', width: BAND_RECENT_W },
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
  const recent = out.find(c => c.id === 'recent')
  if (recent !== undefined) recent.width += bodyColumns - used
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

/** One row → a tab label; 2–5 rows or a thin column → a one-row summary; 6–11 → mini Ferris; 12+ → full. */
export function bandTier(maxRows: number, bodyColumns: number, hasMini: boolean): BandTier {
  if (maxRows <= 1) return 'label'
  if (maxRows < MINI_MIN_ROWS || bodyColumns < BAND_MIN_COLUMNS) return 'summary'
  if (maxRows < BAND_MIN_ROWS) return hasMini ? 'mini' : 'summary'
  return 'full'
}

/**
 * The forge tab's label: the longest form that fits `cols` cells — `ϟ 93 tok/s ▁▂▃▅▇ · Ferris Lv.1`, then without the
 * pet, then without the sparkline, then `ϟ93`. `spark` is '' with under two timed requests, `pet` null with no pet.
 */
export function tabLabel(cols: number, tps: number | null, spark = '', pet: string | null = null): string {
  const n = fmtTps(tps)
  const head = `${GLYPH.speed} ${n} tok/s`
  const tries = [
    spark && pet ? `${head} ${spark} · ${pet}` : null,
    !spark && pet ? `${head} · ${pet}` : null,
    spark ? `${head} ${spark}` : null,
    head,
  ]
  for (const t of tries) if (t !== null && displayWidth(t) <= cols) return t
  return `${GLYPH.speed}${n}`
}

/** `village.rgb` as a `#rrggbb` text colour; null when it is not three 0–255 integers. */
export function rgbHex(rgb: readonly number[]): string | null {
  if (rgb.length !== 3 || !rgb.every(c => Number.isInteger(c) && c >= 0 && c <= 255)) return null
  return '#' + rgb.map(c => c.toString(16).padStart(2, '0')).join('')
}
