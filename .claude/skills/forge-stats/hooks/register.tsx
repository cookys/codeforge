import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { PetView, Step } from '../types'
import {
  GLYPH,
  KEY_W,
  displayWidth,
  fmtCount,
  fmtMs,
  fmtTps,
  halfBlockCells,
  meter,
  padLeft,
  padRight,
  parseSnapshot,
  shortModel,
  sparkline,
  staleMinutes,
  summarize,
} from './model'
import { localeFrom, makeT } from './strings'
import type { Locale, T } from './strings'

const PANE = 'forge-stats'
const KEEP = 200
// Below this decode window the rate is noise (one-chunk replies).
const MIN_DECODE_MS = 150

const steps = atom({ plugin: 'forge-stats', key: 'steps' } as const, [] as Step[], { shape: 'steps-1' })
const pet = atom(
  { plugin: 'forge-stats', key: 'pet' } as const,
  { status: 'missing', readAt: 0, writtenAt: null, path: null, pet: null, village: null } as PetView,
  { shape: 'pet-3' },
)

// Module variables: recomputed on every session.start (a reload runs it again).
let locale: Locale = 'en'
// Sprite animation: the frame on screen and the cells last sent, so an unchanged frame is never
// re-sent. Kept out of $.state on purpose: a state write would redraw the whole pane every frame.
let spriteFrame = 0
let lastCells: string | null = null
const SPRITE_KEY = 'pet-sprite'
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
async function snapshotCandidates($: EngineInterface): Promise<string[]> {
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
  return [...new Set(bases.filter((x): x is string => typeof x === 'string'))].map(b => b + '/codeforge/pet.json')
}

// Next idle frame → $.ui.blit, only when the cells change. A deny (pane closed, Raster not
// mounted, non-terminal surface) is fine: the next render draws the current frame anyway.
async function stepSprite($: EngineInterface) {
  const view = await read($, pet)
  const sprite = view.status === 'ok' ? (view.village?.sprite ?? null) : null
  if (sprite === null) return
  spriteFrame = (spriteFrame + 1) % sprite.frames.length
  const { cells } = halfBlockCells(sprite, spriteFrame)
  if (cells === lastCells) return
  lastCells = cells
  // literal routing at the call site (pixel-pet plan §2.5); must match PANE and SPRITE_KEY
  await $.ui.blit({ requestId: 'forge-stats', key: 'pet-sprite', cells })
}

async function refreshPet($: EngineInterface) {
  const candidates = await snapshotCandidates($)
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
}

// ---------------------------------------------------------------------------------------------
// lines: a line is a list of segments; a segment's role is the only way colour enters the pane

type Role = 'structure' | 'idle' | 'dim' | 'bold' | 'mono'
type Seg = { text: string; role?: Role }
type Line = Seg[]

const seg = (text: string, role?: Role): Seg => (role ? { text, role } : { text })

function rule(title: string, cols: number, note?: string): Line {
  const head = `${GLYPH.open} ${title}`
  const tail = note ? `  ${note} ` : ' '
  const fill = Math.max(2, cols - displayWidth(head) - displayWidth(tail))
  return [seg(GLYPH.open + ' ', 'structure'), seg(title, 'bold'), seg(tail, 'dim'), seg(GLYPH.rule.repeat(fill), 'structure')]
}

const kv = (key: string, ...value: Seg[]): Line => [seg(padRight(key, KEY_W), 'dim'), ...value]

function speedLines(list: readonly Step[], cols: number, tr: T): Line[] {
  const main = summarize(list.filter(s => !s.isSubagent))
  const all = summarize(list)
  if (list.length === 0) return [[seg(`${GLYPH.empty} ${tr('noRequests')}`, 'dim')]]
  const unit = tr('reqUnit')
  const n = (k: number) => (unit ? `${k} ${unit}` : String(k))
  const out: Line[] = [
    kv(tr('main'), seg(fmtTps(main.avgTps), 'bold'), seg(' tok/s', 'dim'), seg(`  ·  ${tr('firstToken')} `, 'dim'), seg(fmtMs(main.medianTtft)), seg(` ${tr('median')}`, 'dim')),
    kv(tr('requests'), seg(n(main.requests)), seg(`  ·  ${tr('out')} `, 'dim'), seg(fmtCount(main.outTokens)), seg(' tokens', 'dim')),
  ]
  if (all.requests !== main.requests) {
    out.push(kv(tr('withSub'), seg(n(all.requests)), seg(`  ·  ${tr('avg')} `, 'dim'), seg(fmtTps(all.avgTps)), seg(' tok/s', 'dim')))
  }
  const room = Math.max(8, Math.min(40, cols - KEY_W - 16))
  const recent = list.filter(s => !s.isSubagent).slice(-room)
  if (recent.length >= 2) {
    out.push(
      kv(tr('lastN', { n: recent.length }), seg(sparkline(recent.map(s => s.tps)), 'mono'), seg(`  ${tr('best')} `, 'dim'), seg(fmtTps(main.bestTps)), seg(' tok/s', 'dim')),
    )
  }
  return out
}

function recentLines(list: readonly Step[], rows: number, tr: T): Line[] {
  if (list.length === 0) return []
  const W = { tps: 6, ttft: 8, out: 7 }
  const header: Line = [
    seg('  ' + padLeft(tr('colTps'), W.tps) + padLeft(tr('colTtft'), W.ttft + 1) + padLeft(tr('colOut'), W.out + 1) + '  ' + tr('colModel'), 'dim'),
  ]
  const shown = list.slice(-rows).reverse()
  const body = shown.map((s, i): Line => {
    const glyph = i === 0 ? GLYPH.latest : GLYPH.bullet
    const cells = padLeft(fmtTps(s.tps), W.tps) + ' ' + padLeft(fmtMs(s.ttftMs), W.ttft) + ' ' + padLeft(fmtCount(s.outTokens), W.out) + '  ' + shortModel(s.model)
    return s.isSubagent
      ? [seg(glyph + ' ', 'structure'), seg(cells, 'dim'), seg(` · ${tr('sub')}`, 'dim')]
      : [seg(glyph + ' ', 'structure'), seg(cells, 'mono')]
  })
  return [header, ...body]
}

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

// ---------------------------------------------------------------------------------------------

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    locale = await resolveLocale($)
    // a new session starts its animation from frame 0 with nothing sent yet
    spriteFrame = 0
    lastCells = null
    await $.command.register({ name: 'forge-stats', description: t()('cmdDesc') })
    void $.ui.open({ id: PANE, title: t()('title') })
    void refreshPet($)
    // statusline rewrites the snapshot every render; a slow poll keeps the pane current
    // between turns (a reload cancels the timer).
    $.clock.every(15_000, () => void refreshPet($))
    // idle animation: 2 fps, interval written as a literal (pixel-pet plan §2.5)
    $.clock.every(500, () => void stepSprite($))
    return next(e)
  })

  on('command.run', { command: 'forge-stats' }, async $ => {
    await $.ui.open({ id: PANE, title: t()('title') })
    void refreshPet($)
    return { text: t()('opened') }
  })

  // Time every model request from its stream: send → first content chunk
  // (thinking counts: `output_tokens` includes thinking), then → last chunk.
  on('turn.step', async function* ($, e, next) {
    const sent = performance.now()
    let first: number | null = null
    let last: number | null = null
    const stream = next(e)
    for await (const chunk of stream) {
      if (chunk.kind !== 'engine') {
        const now = performance.now()
        if (first === null) first = now
        last = now
      }
      yield chunk
    }
    const result = await stream.result
    const usage = result.usage
    if (usage) {
      const outTokens = usage.output_tokens ?? 0
      const decodeMs = first !== null && last !== null ? last - first : null
      const step: Step = {
        at: Date.now(),
        model: usage.model ?? e.model,
        isSubagent: e.agentId !== undefined,
        ttftMs: first !== null ? first - sent : null,
        decodeMs,
        outTokens,
        tps: decodeMs !== null && decodeMs >= MIN_DECODE_MS && outTokens > 0 ? outTokens / (decodeMs / 1000) : null,
      }
      await update($, steps, list => [...list, step].slice(-KEEP))
      if (!step.isSubagent && step.tps !== null) $.ui.status(`${fmtTps(step.tps)} tok/s`)
    }
    return result
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
    const list = await read($, steps)
    const p = await read($, pet)
    // bodyColumns may run ~2 cells wider than what draws; keep a margin.
    const cols = Math.max(30, (e.props?.bodyColumns ?? 44) - 2)
    const bodyRows = e.props?.scroll?.bodyRows ?? e.viewport?.rows ?? 30

    const speed = speedLines(list, cols, tr)
    const sprite = p.status === 'ok' && Raster !== undefined ? (p.village?.sprite ?? null) : null
    // sprite beside the stats when both fit (sprite + 2 gap + 28 text), else stacked
    const side = sprite !== null && cols >= sprite.width + 2 + 28
    const petPart = sprite === null ? petLines(p, cols, tr, Date.now()) : petLines(p, side ? cols - sprite.width - 2 : cols, tr, Date.now(), false)
    const spriteRows = sprite === null ? 0 : sprite.height / 2
    const petHeight = sprite === null ? petPart.length : side ? Math.max(spriteRows, petPart.length) : spriteRows + 1 + petPart.length
    const fixed = 3 + speed.length + petHeight + 2
    const room = Math.max(3, Math.min(12, bodyRows - fixed - 1))
    const recent = recentLines(list, room, tr)

    const lines: Line[] = [rule(tr('speed'), cols), ...speed]
    if (recent.length) lines.push([seg(' ')], rule(tr('recent'), cols), ...recent)
    lines.push([seg(' ')], rule(tr('pet'), cols))
    if (sprite === null) lines.push(...petPart)

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
    let petBlock = null
    if (sprite !== null && Raster !== undefined) {
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
        {lines.map((l, i) => row(l, 'l' + i))}
        {petBlock}
      </Box>
    )
  })
}
