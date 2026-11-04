import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { Layout, PetView, SessionView, SpriteData } from '../types'
import {
  BAND_GAP,
  GLYPH,
  KEY_W,
  displayWidth,
  bandLayout,
  bandTier,
  halfBlockCells,
  heartbeatPath,
  heartbeatText,
  liveBaseOf,
  meter,
  padLeft,
  padRight,
  parseSessionView,
  parseSnapshot,
  sessionViewPath,
  rgbHex,
  staleMinutes,
  BAND_MIN_ROWS,
  MINI_MIN_ROWS,
  parseFleet,
  summaryParts,
  USAGE_MAX_BYTES,
  USAGE_STALE_MS,
  compactCount,
  countdown,
  parseUsage,
  usageSummaryPath,
} from './model'
import type { UsageView } from './model'
import { NO_QUOTA, parseQuotaNs, pickProviders, pickViews, quotaBand, quotaBandForms, quotaEntries, quotaLiveCandidates, quotaLiveDir, quotaPaneLines, QUOTA_NS_MAX_BYTES } from './quota-view'
import type { DirEntry, QuotaState } from './quota-view'
import { parseQuotaView } from './quota'
import { localeFrom, makeT } from './strings'
import type { Locale, T } from './strings'

const PANE = 'forge-stats'

const pet = atom(
  { plugin: 'forge-stats', key: 'pet' } as const,
  { status: 'missing', readAt: 0, writtenAt: null, path: null, pet: null, village: null } as PetView,
  { shape: 'pet-3' },
)
const session = atom({ plugin: 'forge-stats', key: 'session' } as const, null as SessionView | null, { shape: 'session-1' })

// autopilot's fleet summary (its live mod writes `autopilot.fleet`; PM G33a). Read only, never written; no autopilot or another shape is no summary. Not declared as a
// plugin dependency: forge-stats stands alone (B31), and a missing plugin must not keep it from loading.
async function readFleet($: EngineInterface) {
  try {
    const got = await ($.state.get as (ref: { plugin: string; key: string }) => Promise<{ value: unknown }>)({ plugin: 'autopilot', key: 'fleet' })
    return parseFleet(got.value)
  } catch (_e) {
    return null
  }
}

// Module variables: recomputed on every session.start (a reload runs it again).
let locale: Locale = 'en'
// Sprite animation: the frame on screen and the cells last sent, so an unchanged frame is never
// re-sent. Kept out of $.state on purpose: a state write would redraw the whole pane every frame.
let spriteFrame = 0
let lastCells: string | null = null
const SPRITE_KEY = 'pet-sprite'
// Band and heartbeat (statusline-band plan §2.5). `layout` comes from $.store at session.start.
let layout: Layout = 'band'
let sid = ''
// The live base codeforge wrote pet.json into: heartbeats go there, so the statusline finds them.
let liveBase: string | null = null
// The band instance the AbovePrompt hook last drew (its `e.requestId`); null when it passed.
let bandId: string | null = null
let bandLastCells: string | null = null
// The sprite the band drew last (full or mini): its blits must match the mounted Raster's size.
let bandSprite: SpriteData | null = null
const BAND_KEY = 'pet-band'
const t = (): T => makeT(locale)

async function resolveLocale($: EngineInterface): Promise<Locale> {
  const env = await $.env.get('CODEFORGE_LOCALE').catch(() => undefined)
  let language: string | null = null
  try {
    const home = await $.env.get('HOME')
    const text = home ? await $.fs.read(home + '/.claude/settings.json') : null
    const parsed = typeof text === 'string' ? (JSON.parse(text) as { language?: unknown }) : null
    if (parsed && typeof parsed.language === 'string') language = parsed.language
  } catch {
    language = null
  }
  const lang = (await $.env.get('LC_ALL').catch(() => undefined)) || (await $.env.get('LANG').catch(() => undefined))
  return localeFrom({ env, language, lang })
}

// Where codeforge may have written the snapshot, in the order codeforge resolves its live base
// (`src/live.rs`: AUTOPILOT_LIVE_DIR, XDG_RUNTIME_DIR/autopilot, …, ~/.autopilot), with the
// pointer autopilot's own mods read slotted in after the override. The first readable file wins.
async function liveBases($: EngineInterface): Promise<string[]> {
  const home = (await $.env.get('HOME').catch(() => undefined)) || undefined
  const over = (await $.env.get('AUTOPILOT_LIVE_DIR').catch(() => undefined)) || undefined
  const xdg = (await $.env.get('XDG_RUNTIME_DIR').catch(() => undefined)) || undefined
  let pointed: string | undefined
  if (home) {
    try {
      const text = await $.fs.read(home + '/.autopilot/live-pointer.json')
      const ptr = typeof text === 'string' ? (JSON.parse(text) as { live_base?: unknown }) : null
      if (ptr && typeof ptr.live_base === 'string' && ptr.live_base !== '') pointed = ptr.live_base
    } catch {
      pointed = undefined
    }
  }
  const bases = [over, pointed, xdg ? xdg + '/autopilot' : undefined, home ? home + '/.autopilot' : undefined]
  return [...new Set(bases.filter((x): x is string => typeof x === 'string'))]
}

// Next idle frame → $.ui.blit, only when the cells change. A deny (pane closed, Raster not
// mounted, non-terminal surface) is fine: the next render draws the current frame anyway.
async function stepSprite($: EngineInterface) {
  const view = await read($, pet)
  const sprite = view.status === 'ok' ? (view.village?.sprite ?? null) : null
  if (sprite === null) return
  spriteFrame = (spriteFrame + 1) % sprite.frames.length
  const { cells } = halfBlockCells(sprite, spriteFrame)
  if (paneTab === 'pet' && cells !== lastCells) {
    lastCells = cells
    // literal routing at the call site (pixel-pet plan §2.5); must match PANE and SPRITE_KEY
    await $.ui.blit({ requestId: 'forge-stats', key: 'pet-sprite', cells })
  }
  // The band's Raster: its site is the band instance the render hook received (statusline-band
  // plan §2.5), its size the sprite the band drew (forge-tabs plan §2.5).
  const id = bandId
  const drawn = bandSprite
  if (layout !== 'band' || id === null || drawn === null) return
  const bandCells = halfBlockCells(drawn, spriteFrame % drawn.frames.length).cells
  if (bandCells === bandLastCells) return
  bandLastCells = bandCells
  const result = await $.ui.blit({ requestId: id, key: 'pet-band', cells: bandCells })
  // denied (the band is not mounted right now): resend the frame on the next tick
  if ('deny' in result) bandLastCells = null
}

// One line codeforge statusline reads: is forge-stats alive in this session, and in which layout?
async function writeHeartbeat($: EngineInterface) {
  if (liveBase === null || sid === '') return
  try {
    const now = await $.clock.now()
    await $.fs.write(heartbeatPath(liveBase, sid), heartbeatText(sid, now, layout))
  } catch {
    // best effort: no heartbeat means the statusline stays full
  }
}

async function refreshPet($: EngineInterface) {
  const bases = await liveBases($)
  const candidates = bases.map(b => b + '/codeforge/pet.json')
  let text: string | null = null
  let path: string | null = candidates[0] ?? null
  for (const c of candidates) {
    try {
      const v = await $.fs.read(c)
      if (typeof v === 'string') {
        text = v
        path = c
        break
      }
    } catch {
      // not there: next candidate
    }
  }
  const next = parseSnapshot(text, path, Date.now())
  await update($, pet, () => next)
  liveBase = liveBaseOf(text === null ? null : path) ?? bases[0] ?? null
  if (liveBase === null || sid === '') return
  let viewText: string | null = null
  try {
    const v = await $.fs.read(sessionViewPath(liveBase, sid))
    viewText = typeof v === 'string' ? v : null
  } catch {
    viewText = null
  }
  const view = parseSessionView(viewText, sid)
  await update($, session, () => view)
}

// ---------------------------------------------------------------------------------------------
// lines: a line is a list of segments; a segment's role is the only way colour enters the pane

type Role = 'structure' | 'idle' | 'dim' | 'bold' | 'mono'
type Seg = { text: string; role?: Role; color?: string }
type Line = Seg[]

// One colour per meaning in the band (forge-ux), the same roles as autopilot's palette: OK (green, `done`) for lights
// that are fine; the pet in its village colour. Speed is autopilot's to measure and draw (B32).
const OK = 'success'

const seg = (text: string, role?: Role, color?: string): Seg => ({ text, ...(role ? { role } : {}), ...(color ? { color } : {}) })

function rule(title: string, cols: number, note?: string): Line {
  const head = `${GLYPH.open} ${title}`
  const tail = note ? `  ${note} ` : ' '
  const fill = Math.max(2, cols - displayWidth(head) - displayWidth(tail))
  return [seg(GLYPH.open + ' ', 'structure'), seg(title, 'bold'), seg(tail, 'dim'), seg(GLYPH.rule.repeat(fill), 'structure')]
}

const kv = (key: string, ...value: Seg[]): Line => [seg(padRight(key, KEY_W), 'dim'), ...value]

function petLines(view: PetView, cols: number, tr: T, now: number, withArt = true): Line[] {
  const { pet: p, village: v } = view
  if (view.status !== 'ok' || p === null || v === null) {
    const why = view.status === 'no-pet' ? tr('noPetAdopt') : view.status === 'bad' ? tr('badSnapshot') : tr('noSnapshot')
    return [[seg(`${GLYPH.empty} ${tr('noPet')}`, 'dim')], [seg('  ' + why, 'dim')]]
  }
  const out: Line[] = withArt ? v.art_full.map(a => [seg('  ' + a, 'mono')] as Line) : []
  if (withArt && v.art_full.length) out.push([seg(' ')])
  out.push(kv(tr('name'), seg(p.name, 'bold'), seg(`  Lv.${p.level}`)))
  out.push(kv(tr('village'), seg(v.display_name), seg(` · ${v.language}`, 'dim')))
  if (p.xp_to_next > 0) {
    const width = cols >= 100 ? 40 : Math.max(8, Math.min(20, cols - KEY_W - 18))
    const m = meter(p.xp / p.xp_to_next, width)
    const pct = Math.floor((p.xp / p.xp_to_next) * 100)
    out.push(kv(tr('xp'), seg(m.on), seg(m.off, 'idle'), seg(padLeft(`${pct}%`, 5)), seg(`  ${p.xp}/${p.xp_to_next}`, 'dim')))
  }
  const stats: [string, number][] = [['ATK', p.atk], ['HP', p.hp], ['DEF', p.def], ['SUP', p.sup], ['VER', p.ver]]
  const parts: Seg[] = []
  stats.forEach(([k, n], i) => {
    if (i > 0) parts.push(seg(' · ', 'structure'))
    parts.push(seg(k + ' ', 'dim'), seg(String(n)))
  })
  out.push(kv(tr('stats'), ...parts))
  if (p.strategy) out.push(kv(tr('strategy'), seg(p.strategy)))
  if (p.message) out.push(kv(tr('says'), seg(p.message)))
  const stale = staleMinutes(view, now)
  if (stale !== null) out.push([seg('  ' + tr('staleNote', { n: stale }), 'dim')])
  return out
}

// ---- the 用量 / usage tab (PM G36 phase 4): accounts, then today's models, read from codeforge's summary.json ----
type PaneTab = 'pet' | 'usage'
let paneTab: PaneTab = 'pet'

// Bounded: a regular file that is not a link and at most USAGE_MAX_BYTES, checked before the read and again on the text. Never spawns codeforge.
async function readUsage($: EngineInterface, now: number): Promise<UsageView> {
  const bases = liveBase !== null ? [liveBase] : await liveBases($)
  for (const b of bases) {
    const path = usageSummaryPath(b)
    const st = await $.fs.stat(path).catch(() => undefined)
    if (st === undefined) continue
    if (st.isLink || st.kind !== 'file' || st.size > USAGE_MAX_BYTES) return parseUsage('', now) // present but not readable as the contract says: bad
    try {
      const text = await $.fs.read(path)
      return parseUsage(typeof text === 'string' ? text : '', now)
    } catch {
      return parseUsage('', now)
    }
  }
  return parseUsage(null, now)
}

// ---- the quota mirror (G40): autopilot's `<live_dir>/quota/<provider>/<alias>.json` views, read here, never written, never in a render ----
let quotaState: QuotaState = NO_QUOTA
// the last time anything was drawn (band or pane): the timer reads only while someone is looking (guideline §8-2)
let drawnAt = 0
let uidCache: number | null | undefined
const QUOTA_VIEW_CAP = 256
const QUOTA_WATCH_MS = 60_000

async function quotaUid($: EngineInterface): Promise<number | null> {
  if (uidCache !== undefined) return uidCache
  try {
    const r = await $.process.run(['id', '-u'])
    const n = Number(r.stdout.trim())
    uidCache = r.exitCode === 0 && Number.isInteger(n) && n >= 0 ? n : null
  } catch {
    uidCache = null
  }
  return uidCache
}

const dirEntries = async ($: EngineInterface, path: string): Promise<DirEntry[] | null> => {
  try {
    return (await $.fs.list(path)) as DirEntry[]
  } catch {
    return null
  }
}

/** Where the mirror root is (contract §21.3), with the candidates that exist found by `$.fs.stat`. */
async function quotaRoot($: EngineInterface): Promise<string | null> {
  const env = {
    AUTOPILOT_LIVE_DIR: (await $.env.get('AUTOPILOT_LIVE_DIR').catch(() => undefined)) || undefined,
    XDG_RUNTIME_DIR: (await $.env.get('XDG_RUNTIME_DIR').catch(() => undefined)) || undefined,
    HOME: (await $.env.get('HOME').catch(() => undefined)) || undefined,
  }
  const uid = env.AUTOPILOT_LIVE_DIR ? null : await quotaUid($)
  const there = new Set<string>()
  for (const c of quotaLiveCandidates(env, uid)) {
    const st = await $.fs.stat(c.dir).catch(() => undefined)
    if (st !== undefined && st.kind === 'dir' && !st.isLink) there.add(c.dir)
  }
  return quotaLiveDir(env, uid, d => there.has(d))
}

// Bounded: at most 64 provider dirs and 256 views, a view a regular file that is not a link and at most the contract's view cap (checked on the listing and again on the text).
async function refreshQuota($: EngineInterface, force = false) {
  try {
    if (!force && Date.now() - drawnAt > QUOTA_WATCH_MS) return
    const root = await quotaRoot($)
    const dir = root === null ? undefined : await $.fs.stat(root + '/quota').catch(() => undefined)
    if (root === null || dir === undefined || dir.kind !== 'dir' || dir.isLink) {
      quotaState = NO_QUOTA
      return
    }
    const top = (await dirEntries($, root + '/quota')) ?? []
    let ns: string | null = null
    const nsEntry = top.find(e => e.name === 'namespace.json')
    if (nsEntry !== undefined && nsEntry.kind === 'file' && !nsEntry.isLink && nsEntry.size <= QUOTA_NS_MAX_BYTES) {
      const text = await $.fs.read(root + '/quota/namespace.json').catch(() => null)
      ns = parseQuotaNs(typeof text === 'string' ? text : null)
    }
    const { names, skipped: skippedDirs } = pickProviders(top)
    const raw: QuotaState['raw'] = []
    let bad = 0
    let skipped = skippedDirs
    let reads = 0
    for (const provider of names) {
      const listed = await dirEntries($, `${root}/quota/${provider}`)
      if (listed === null) continue
      const picked = pickViews(provider, listed)
      bad += picked.bad
      for (const v of picked.views) {
        if (reads >= QUOTA_VIEW_CAP) {
          skipped++
          continue
        }
        reads++
        let text: string | null = null
        try {
          const t = await $.fs.read(`${root}/quota/${provider}/${v.name}`)
          text = typeof t === 'string' ? t : null
        } catch {
          text = null
        }
        const parsed = text === null ? null : parseQuotaView(text, provider, v.alias)
        if (parsed === null || parsed.status === 'bad') {
          bad++
          continue
        }
        bad += parsed.rejected
        raw.push({ provider, alias: v.alias, view: parsed })
      }
    }
    quotaState = { present: true, ns, raw, bad, skipped }
  } catch {
    // a read that fails keeps what was last read
  }
}

const money = (c: number) => '$' + c.toFixed(2)
const lineWidth = (l: Line) => l.reduce((n, x) => n + displayWidth(x.text), 0)

// `quota` is the mirror when its directory exists (design §7 rule 7): then its blocks replace the G36 accounts rows and the models section stays. Without it, today's pane exactly.
function usageLines(view: UsageView, cols: number, tr: T, now: number, quota: { state: QuotaState; nowMs: number } | null = null): Line[] {
  const useQuota = quota !== null && quota.state.present
  const missing = (): Line => [seg('  ' + (view.status === 'bad' ? tr('usageBad') : tr('usageMissing')), 'dim')]
  if (view.status !== 'ok' && !useQuota) return [missing()]
  const out: Line[] = []
  if (useQuota) {
    out.push([seg(tr('usageAccounts'), 'dim')], ...quotaPaneLines(quota.state, quotaEntries(quota.state, Math.floor(quota.nowMs / 1000)), cols, quota.nowMs, tr))
    if (view.status !== 'ok') return [...out, missing()]
  }
  const age = view.generatedAt === null ? 0 : now - view.generatedAt
  if (age > USAGE_STALE_MS) out.push([seg('  ' + tr('usageAge', { n: Math.floor(age / 60_000) }) + ' · ' + tr('usageRefresh'), 'dim')])
  if (!useQuota) out.push([seg(tr('usageAccounts'), 'dim')])
  if (!useQuota && view.accounts.length === 0) out.push([seg('  ' + tr('usageNoAccounts'), 'dim')])
  for (const a of useQuota ? [] : view.accounts) {
    const win = (label: string, w: UsageView['accounts'][number]['fiveHour'], withReset: boolean): Seg[] =>
      w === null
        ? [seg(`  ${label} `, 'dim'), seg(GLYPH.empty, 'dim')]
        : [seg(`  ${label} `, 'dim'), seg(`${Math.floor(w.pct * 100)}%`), ...(withReset ? [seg(' ' + countdown(w.resetsAtMs, now), 'dim')] : [])]
    const name = seg('  ' + padRight(a.alias, KEY_W))
    // the widest form that fits: both windows with countdowns, then without, then 5h alone; a field is dropped whole, never cut
    const forms: Line[] = [
      [name, ...win('5h', a.fiveHour, true), ...win('7d', a.sevenDay, true)],
      [name, ...win('5h', a.fiveHour, false), ...win('7d', a.sevenDay, false)],
      [name, ...win('5h', a.fiveHour, false)],
      [name],
    ]
    out.push(forms.find(f => lineWidth(f) <= cols) ?? [name])
  }
  out.push([seg(tr('usageToday', { day: view.day }), 'dim')])
  if (view.models.length === 0) out.push([seg('  ' + tr('usageNone'), 'dim')])
  // model | tokens | requests | cost; the name takes what the numbers leave. Narrow (< 50) drops the requests column so a model id stays readable.
  const withReq = cols >= 50
  const nameW = Math.max(8, cols - 2 - 8 - (withReq ? 7 : 0) - 10)
  let total = 0
  let priced = false
  let unpriced = 0
  for (const m of view.models) {
    const name = displayWidth(m.model) > nameW ? [...m.model].slice(0, nameW - 1).join('') + '…' : m.model
    const cost = m.cost === null ? GLYPH.empty : (m.unpriced > 0 ? '≥' : '') + money(m.cost)
    const req = withReq ? [seg(padLeft(`${m.requests}${tr('usageReq')}`, 7), 'dim')] : []
    out.push([seg('  ' + padRight(name, nameW)), seg(padLeft(compactCount(m.tokens), 8)), ...req, seg(padLeft(cost, 10))])
    if (m.cost !== null) {
      total += m.cost
      priced = true
    }
    unpriced += m.unpriced
  }
  if (view.models.length > 0) {
    const sum = priced ? (unpriced > 0 ? '≥' : '') + money(total) : GLYPH.empty
    const head = '  ' + tr('usageTotal', { cost: sum })
    const note = unpriced > 0 ? tr('usageUnpriced', { n: unpriced }) : null
    // the unpriced note on the same line when it fits, else on its own: neither is ever cut
    if (note !== null && displayWidth(head + ' · ' + note) <= cols) out.push([seg(head, 'dim'), seg(' · ' + note, 'dim')])
    else {
      out.push([seg(head, 'dim')])
      if (note !== null) out.push([seg('  ' + note, 'dim')])
    }
  }
  return out
}

// band columns: at most 12 rows each, beside the 12-row Ferris

function bandPetLines(view: PetView, width: number, tr: T, short = false): Line[] {
  const { pet: p, village: v } = view
  if (view.status !== 'ok' || p === null || v === null) return [[seg(`${GLYPH.empty} ${tr('noPet')}`, 'dim')]]
  const accent = rgbHex(v.rgb) ?? undefined
  const out: Line[] = [[seg(p.name, 'bold', accent), seg(`  Lv.${p.level}`, undefined, accent)], [seg(v.display_name, 'dim')]]
  if (p.xp_to_next > 0) {
    const m = meter(p.xp / p.xp_to_next, Math.max(6, Math.min(20, width - 12)))
    out.push([seg(tr('xp') + ' ', 'dim'), seg(m.on, undefined, accent), seg(m.off, 'idle'), seg(padLeft(`${Math.floor((p.xp / p.xp_to_next) * 100)}%`, 5))])
  }
  const stat = (k: string, n: number) => [seg(k + ' ', 'dim'), seg(String(n))]
  const sep = seg(' · ', 'structure')
  out.push([...stat('ATK', p.atk), sep, ...stat('DEF', p.def), sep, ...stat('HP', p.hp)])
  out.push([...stat('SUP', p.sup), sep, ...stat('VER', p.ver)])
  if (p.message && !short) out.push([seg(' ')], [seg(p.message, 'dim')])
  return out
}

function bandMemoryLines(lights: SessionView | null, tr: T): Line[] {
  const out: Line[] = []
  const brainWord: Record<SessionView['brain'], string> = {
    ok: tr('brainOk'),
    degraded: tr('brainDegraded'),
    offline: tr('brainOffline'),
    pending: tr('brainPending'),
    hidden: GLYPH.empty,
  }
  // a light before each value: the colour says the state, the word says it again for whoever cannot tell the colour
  const brainColor: Record<SessionView['brain'], string | undefined> = { ok: OK, degraded: 'warning', offline: 'error', pending: undefined, hidden: undefined }
  if (lights === null || lights.memory === 'hidden') {
    out.push([seg(padRight(tr('memory'), 6), 'dim'), seg(GLYPH.empty, 'dim')])
  } else {
    const on = lights.memory === 'active'
    out.push([seg(padRight(tr('memory'), 6), 'dim'), seg(GLYPH.latest + ' ', on ? undefined : 'idle', on ? OK : undefined), seg(String(lights.active)), seg(` ${tr('entries')}`, 'dim')])
  }
  if (lights === null || lights.brain === 'hidden') {
    out.push([seg(padRight(tr('brain'), 6), 'dim'), seg(GLYPH.empty, 'dim')])
  } else {
    const c = brainColor[lights.brain]
    out.push([seg(padRight(tr('brain'), 6), 'dim'), seg(GLYPH.latest + ' ', c ? undefined : 'idle', c), seg(brainWord[lights.brain], lights.brain === 'ok' ? undefined : 'dim')])
  }
  return out
}

// ---------------------------------------------------------------------------------------------

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    locale = await resolveLocale($)
    // a new session starts its animation from frame 0 with nothing sent yet
    spriteFrame = 0
    lastCells = null
    bandId = null
    bandLastCells = null
    sid = await $.session.id().catch(() => '')
    const stored = await $.store.get('layout').catch(() => undefined)
    layout = stored === 'pane' ? 'pane' : 'band'
    await $.command.register({ name: 'forge-stats', description: t()('cmdDesc') })
    // the band layout draws above the prompt instead of opening the pane (plan KR5)
    if (layout === 'pane') void $.ui.open({ id: PANE, title: t()('title') })
    await refreshPet($)
    await refreshQuota($, true)
    // first heartbeat before any timer (plan §2.5), then every 2 s
    await writeHeartbeat($)
    // statusline rewrites the snapshot every render; a slow poll keeps the pane current
    // between turns (a reload cancels the timer).
    $.clock.every(15_000, () => void refreshPet($))
    // the quota mirror: every 15 s, and only while the band or pane was drawn in the last minute (never read in a render)
    $.clock.every(15_000, () => void refreshQuota($))
    // idle animation: 2 fps, interval written as a literal (pixel-pet plan §2.5)
    $.clock.every(500, () => void stepSprite($))
    $.clock.every(2000, () => void writeHeartbeat($))
    return next(e)
  })

  on('command.run', { command: 'forge-stats' }, async ($, e) => {
    const [verb, arg] = e.args.trim().split(/\s+/)
    if (verb === 'layout') {
      if (arg !== 'band' && arg !== 'pane') return { text: t()('layoutUsage', { layout }) }
      layout = arg
      await $.store.set('layout', arg)
      bandLastCells = null
      if (arg === 'pane') await $.ui.open({ id: PANE, title: t()('title') })
      $.ui.invalidate('ui.render')
      await writeHeartbeat($)
      return { text: t()('layoutSet', { layout: arg }) }
    }
    // `/forge-stats usage` / `/forge-stats pet` open the pane on that tab (a path that needs no pointer)
    if (verb === 'usage' || verb === 'pet') {
      paneTab = verb
      if (verb === 'pet') lastCells = null
      if (verb === 'usage') await refreshQuota($, true)
      void $.ui.invalidate('ui.render')
    }
    await $.ui.open({ id: PANE, title: t()('title') })
    void refreshPet($)
    return { text: t()('opened') }
  })

  on('turn.complete', async ($, e, next) => {
    void refreshPet($)
    return next(e)
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const els = $.ui.resolve(e)
    const { Box, Text } = els
    // Raster paints only in the terminal (other surfaces have no cell grid): draw ASCII there
    const Raster = e.surface === 'terminal' && 'Raster' in els ? els.Raster : undefined
    const tr = t()
    const p = await read($, pet)
    // bodyColumns may run ~2 cells wider than what draws; keep a margin.
    const cols = Math.max(30, (e.props?.bodyColumns ?? 44) - 2)

    const sprite = p.status === 'ok' && Raster !== undefined ? (p.village?.sprite ?? null) : null
    // sprite beside the stats when both fit (sprite + 2 gap + 28 text), else stacked
    const side = sprite !== null && cols >= sprite.width + 2 + 28
    const petPart = sprite === null ? petLines(p, cols, tr, Date.now()) : petLines(p, side ? cols - sprite.width - 2 : cols, tr, Date.now(), false)

    const now = Date.now()
    const clockMs = Date.now()
    drawnAt = clockMs
    const usage = paneTab === 'usage' ? await readUsage($, now) : null
    const lines: Line[] = []
    if (usage !== null) {
      const age = usage.generatedAt === null ? null : Math.floor((now - usage.generatedAt) / 60_000)
      lines.push(rule(tr('usage'), cols, age === null ? undefined : tr('usageAge', { n: age })), ...usageLines(usage, cols, tr, now, { state: quotaState, nowMs: clockMs }))
    } else {
      lines.push(rule(tr('pet'), cols))
      if (sprite === null) lines.push(...petPart)
    }

    const draw = (s: Seg, i: number) => {
      const k = "s" + i
      switch (s.role) {
        case 'structure':
          return <Text key={k} color="subtle" wrap="truncate">{s.text}</Text>
        case 'idle':
          return <Text key={k} color="inactive" wrap="truncate">{s.text}</Text>
        case 'dim':
          return <Text key={k} dimColor wrap="truncate">{s.text}</Text>
        case 'bold':
          return <Text key={k} bold wrap="truncate">{s.text}</Text>
        case 'mono':
          return <Text key={k} mono wrap="truncate">{s.text}</Text>
        default:
          return <Text key={k} wrap="truncate">{s.text}</Text>
      }
    }

    const row = (l: Line, k: string) => (
      <Box key={k} flexDirection="row">
        {l.map(draw)}
      </Box>
    )
    // the tab strip: Buttons where the surface has them (a press switches the tab), plain Text otherwise; the active tab bold, the other dim
    const Button = 'Button' in els ? els.Button : undefined
    const tabs: [PaneTab, string][] = [['pet', tr('tabPet')], ['usage', tr('tabUsage')]]
    const select = (id: PaneTab) => {
      if (paneTab === id) return
      paneTab = id
      if (id === 'pet') lastCells = null
      if (id === 'usage') void refreshQuota($, true)
      void $.ui.invalidate('ui.render')
    }
    const strip = (
      <Box key="tabs" flexDirection="row" gap={2}>
        {tabs.map(([id, label]) => {
          const text = id === paneTab ? <Text bold>{label}</Text> : <Text dimColor>{label}</Text>
          return Button !== undefined ? (
            <Button key={'tab-' + id} onPress={() => select(id)}>
              {text}
            </Button>
          ) : (
            <Box key={'tab-' + id}>{text}</Box>
          )
        })}
      </Box>
    )
    let petBlock = null
    if (usage === null && sprite !== null && Raster !== undefined) {
      const frame = halfBlockCells(sprite, spriteFrame)
      lastCells = frame.cells
      const art = <Raster key={SPRITE_KEY} columns={frame.columns} rows={frame.rows} cells={frame.cells} />
      const stats = <Box flexDirection="column">{petPart.map((l, i) => row(l, 'p' + i))}</Box>
      petBlock = side ? (
        <Box flexDirection="row" gap={2}>
          {art}
          {stats}
        </Box>
      ) : (
        <Box flexDirection="column" gap={1}>
          {art}
          {stats}
        </Box>
      )
    }

    return (
      <Box flexDirection="column">
        {strip}
        {lines.map((l, i) => row(l, 'l' + i))}
        {petBlock}
      </Box>
    )
  })
  // The band above the prompt: Ferris | pet | memory and brain (statusline-band plan §2.5; speed is autopilot's, B32).
  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    const props = e.props
    if (e.surface !== 'terminal' || layout !== 'band' || props.hasSurvey || props.maxRows < 1) {
      bandId = null
      return next(e)
    }
    const els = $.ui.resolve(e)
    const { Box, Text } = els
    const p = await read($, pet)
    const full = p.status === 'ok' ? (p.village?.sprite ?? null) : null
    const mini = p.status === 'ok' ? (p.village?.sprite_mini ?? null) : null
    const Raster = 'Raster' in els ? els.Raster : undefined
    let tier = bandTier(props.maxRows, props.bodyColumns, mini !== null)
    if (tier === 'full' && (full === null || Raster === undefined)) tier = 'summary'
    if (tier === 'mini' && (mini === null || Raster === undefined)) tier = 'summary'
    const sprite = tier === 'full' ? full : tier === 'mini' ? mini : null
    const tr = t()
    const fleet = await readFleet($)
    const clockMs = Date.now()
    drawnAt = clockMs
    // the quota chips (G40 §6.1): only once the mirror directory exists (design §7 rule 7), none when it holds nothing to draw
    const quotaForms = quotaState.present ? quotaBandForms(quotaBand(quotaEntries(quotaState, Math.floor(clockMs / 1000)), clockMs, tr)) : null
    // the one-line summary (TUI guideline §2.2): fleet counts, then the pet; the widest form that fits, never wider than the row (bodyColumns may run ~2 wider)
    const summaryRow = (k: string, withPet: boolean) => {
      const petTag = withPet && p.status === 'ok' && p.pet !== null ? `${p.pet.name} Lv.${p.pet.level}` : null
      const parts = summaryParts(fleet, petTag, Math.max(0, props.bodyColumns - 2), tr('fleet'), quotaForms)
      if (parts === null) return null
      return (
        <Box key={k} flexDirection="row">
          {parts.map((s, i) => (
            <Text key={'f' + i} {...(s.color ? { color: s.color } : {})} {...(s.dim ? { dimColor: true } : {})}>
              {s.text}
            </Text>
          ))}
        </Box>
      )
    }
    if (sprite === null || Raster === undefined) {
      bandId = null
      bandSprite = null
      // 1–5 rows (or no sprite): one summary row. With autopilot this is the row it hands down under its tab row (summary_row in the heartbeat); with nothing to
      // draw the row is passed on, never left blank.
      return summaryRow('summary', true) ?? next(e)
    }
    const lights = await read($, session)
    const layoutCols = bandLayout(props.bodyColumns, sprite.width)
    const frame = halfBlockCells(sprite, spriteFrame % sprite.frames.length)
    bandId = e.requestId
    bandSprite = sprite
    bandLastCells = frame.cells

    const draw = (s: Seg, i: number) => {
      const k = 's' + i
      const color = s.color ?? (s.role === 'structure' ? 'subtle' : s.role === 'idle' ? 'inactive' : undefined)
      return (
        <Text key={k} wrap="truncate" {...(color ? { color } : {})} {...(s.role === 'dim' ? { dimColor: true } : {})} {...(s.role === 'bold' ? { bold: true } : {})} {...(s.role === 'mono' ? { mono: true } : {})}>
          {s.text}
        </Text>
      )
    }
    const column = (ls: Line[], width: number, k: string) => (
      <Box key={k} flexDirection="column" width={width}>
        {ls.slice(0, frame.rows).map((l, i) => (
          <Box key={'r' + i} flexDirection="row">
            {l.map(draw)}
          </Box>
        ))}
      </Box>
    )

    // the fleet outranks the pet (guideline §2.1): a summary row on top when there is a summary and a row to spare above the tier's minimum (mini >= 7, full >= 13)
    const top = (fleet !== null || (quotaForms !== null && quotaForms.length > 0)) && props.maxRows > (tier === 'mini' ? MINI_MIN_ROWS : BAND_MIN_ROWS) ? summaryRow('fleet', false) : null
    const columns = (
      <Box flexDirection="row" gap={BAND_GAP}>
        {layoutCols.map(c => {
          switch (c.id) {
            case 'ferris':
              return <Raster key={BAND_KEY} columns={frame.columns} rows={frame.rows} cells={frame.cells} />
            case 'pet':
              return column(bandPetLines(p, c.width, tr, tier === 'mini'), c.width, 'pet')
            case 'memory':
              return column(bandMemoryLines(lights, tr), c.width, 'memory')
          }
        })}
      </Box>
    )
    return top === null ? columns : (
      <Box flexDirection="column">
        {top}
        {columns}
      </Box>
    )
  })
}
