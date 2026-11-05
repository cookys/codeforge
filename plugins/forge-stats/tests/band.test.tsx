import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { bandLayout, halfBlockCells, rgbHex, parseFleet, parseSessionView, parseSprite, sanitizeSessionId, summaryParts } from '../hooks/model'
import { FERRIS_MINI } from './ferris-mini'
import { FERRIS } from './ferris-sprite'
import { VECTORS } from './session-id-vectors'

const LIVE = '/run/user/1000/autopilot'
const SID = 'a/b c' // sanitized: a_b_c
const HB = LIVE + '/codeforge/mod-alive/a_b_c.json'
const SESSION_FILE = LIVE + '/codeforge/session/a_b_c.json'

const SNAP = {
  schema: 'codeforge.pet-view/1',
  written_at: new Date().toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: null, message: null },
  village: {
    id: 'rust',
    display_name: 'The Forge-Ruins',
    language: 'Rust',
    tagline: '',
    rgb: [215, 95, 0],
    art_small: [' _~^~^~_'],
    art_full: ['   _~^~^~_'],
    sprite: FERRIS,
    sprite_mini: FERRIS_MINI,
  },
}
const SESSION = { schema: 'codeforge.session-view/1', session_id: SID, written_at: '2026-10-31T00:00:00Z', memory: { light: 'active', active: 12 }, brain: { light: 'ok' } }

// `/forge-stats <args>` as the person types it
const cmd = (args: string) => ({ command: 'forge-stats', args, origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } }) as const

const band = (bodyColumns: number, over: Record<string, unknown> = {}) =>
  ({
    plugin: 'forge-stats',
    component: 'AbovePrompt',
    requestId: 'band-1',
    surface: 'terminal',
    props: { hasSurvey: false, isWorking: false, maxRows: 20, bodyColumns, scroll: { offset: 0, bodyRows: 19 }, view: {}, ...over },
  }) as const

type World = { writes: { path: string; text: string }[]; blits: { requestId: string; key: string; cells: string }[]; store: Record<string, unknown>; opened: number; deny: boolean; session?: unknown }

// The engine beneath: HOME, the live pointer, pet.json, the session view, the store, writes and blits.
function world(on: Parameters<TestBody>[1], layout?: string): World {
  const w: World = { writes: [], blits: [], store: layout === undefined ? {} : { layout }, opened: 0, deny: false }
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => {
    w.opened += 1
    return { value: { isPlaced: true } }
  })
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('session.id', () => ({ value: SID }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : undefined }))
  on('fs.read', (_, e) => {
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path === LIVE + '/codeforge/pet.json') return { value: JSON.stringify(SNAP) }
    if (e.path === SESSION_FILE) return { value: JSON.stringify(w.session ?? SESSION) }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.write', (_, e) => {
    w.writes.push({ path: e.path, text: e.text })
    return { value: undefined }
  })
  on('store.get', (_, e) => ({ value: w.store[e.key] }))
  on('store.set', (_, e) => {
    w.store[e.key] = e.value
    return { value: undefined }
  })
  on('ui.invalidate', () => ({ value: undefined }))
  // what draws beneath when the band passes: nothing
  on('ui.render', { component: 'AbovePrompt' }, ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box key="beneath" />
  })
  on('ui.blit', (_, e) => {
    if ('cells' in e) w.blits.push({ requestId: e.requestId, key: e.key, cells: e.cells })
    return { value: w.deny ? { deny: 'not mounted' } : {} }
  })
  return w
}

const lastBeat = (w: World) => {
  const beats = w.writes.filter(x => x.path === HB)
  return beats.length ? (JSON.parse(beats[beats.length - 1]!.text) as Record<string, unknown>) : null
}

// ---------------------------------------------------------------------------------------------

test('sanitizeSessionId matches codeforge on the shared vectors', () => {
  expect(VECTORS.length > 0).toBe(true)
  for (const v of VECTORS) expect(sanitizeSessionId(v.input)).toBe(v.expected)
})

test('bandLayout adds columns in priority order as the width allows, 1 to 3: no speed or request table (B32)', () => {
  const ids = (cols: number, sprite = 24) => bandLayout(cols, sprite).map(c => c.id).join(' ')
  // each threshold and one cell below it (24-wide Ferris: 24, 56, 86)
  expect(ids(23)).toBe('')
  expect(ids(24)).toBe('ferris')
  expect(ids(55)).toBe('ferris')
  expect(ids(56)).toBe('ferris pet')
  expect(ids(85)).toBe('ferris pet')
  expect(ids(86)).toBe('ferris pet memory')
  expect(ids(300)).toBe('ferris pet memory')
  // the 16-wide mini Ferris moves every threshold down by 8
  expect(ids(78, 16)).toBe('ferris pet memory')
  expect(ids(77, 16)).toBe('ferris pet')
  // the columns keep their widths: nothing takes the spare width
  expect(bandLayout(200, 24).map(c => c.width)).toEqual([24, 30, 28])
})

test('parseSessionView reads this session only', () => {
  expect(parseSessionView(JSON.stringify(SESSION), SID)).toEqual({ memory: 'active', active: 12, brain: 'ok' })
  expect(parseSessionView(JSON.stringify(SESSION), 'other')).toBe(null)
  expect(parseSessionView(JSON.stringify({ ...SESSION, schema: 'x/9' }), SID)).toBe(null)
  expect(parseSessionView('{', SID)).toBe(null)
  expect(parseSessionView(null, SID)).toBe(null)
})

test('the heartbeat is written at session start, before any tick, and every 2 s', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const first = lastBeat(w)
  expect(first).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_000_000, layout: 'band', summary_row: true })
  expect(w.writes.filter(x => x.path === HB)[0]!.text.length < 512).toBe(true)
  await clock.advance(2000)
  expect(lastBeat(w)?.written_at_ms).toBe(1_002_000)
})

test('the band layout is the default and does not open the pane', async ($, on) => {
  const w = world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  expect(w.opened).toBe(0)
})

test('the stored pane layout opens the pane at session start', async ($, on) => {
  const w = world(on, 'pane')
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  expect(w.opened).toBe(1)
})

test('the band draws 1 to 3 columns by width: Ferris, pet, memory; never speed or requests (B32)', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const cells0 = halfBlockCells(parseSprite(FERRIS)!, 0).cells
  // one marker Text per column; speed and the request table must never show
  const MARK = { pet: /^Ferris$/, memory: /^memory/, speed: /Speed|tok\/s|ϟ/, recent: /^Recent requests$/ } as const
  const shows = async (cols: number) => {
    const m = await $.ui.mount(band(cols))
    const r = await m.find({ type: 'Raster' })
    expect(r?.props.columns).toBe(24)
    expect(r?.props.rows).toBe(12)
    expect(r?.props.cells).toBe(cells0)
    const seen: string[] = []
    for (const [k, re] of Object.entries(MARK)) if ((await m.find({ type: 'Text', text: re })) !== undefined) seen.push(k)
    await m.unmount()
    return seen.join(' ')
  }
  expect(await shows(200)).toBe('pet memory')
  expect(await shows(90)).toBe('pet memory')
  expect(await shows(60)).toBe('pet')
  expect(await shows(30)).toBe('')
})

test('the pet column carries all five stats and the memory column the session lights', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const m = await $.ui.mount(band(170))
  for (const k of ['ATK ', 'DEF ', 'HP ', 'SUP ', 'VER ']) expect((await m.find({ type: 'Text', text: k })) !== undefined).toBe(true)
  expect((await m.find({ type: 'Text', text: /^12$/ })) !== undefined).toBe(true)
  await m.unmount()
})

test('the band passes on a survey, desktop, or the pane layout', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const target of [band(100, { hasSurvey: true }), { ...band(100), surface: 'desktop' }] as const) {
    const ui = await $.ui.mount(target)
    expect(await ui.find({ type: 'Raster' })).toBe(undefined)
    await ui.unmount()
  }
  await $.command.run(cmd('layout pane'))
  const ui = await $.ui.mount(band(100))
  expect(await ui.find({ type: 'Raster' })).toBe(undefined)
  await ui.unmount()
})

test('band blits go to the band instance', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount(band(100))
  await clock.advance(2000)
  const bandBlits = w.blits.filter(b => b.key === 'pet-band')
  expect(bandBlits.length > 0).toBe(true)
  expect(bandBlits.every(b => b.requestId === 'band-1')).toBe(true)
  await ui.unmount()
})

test('/forge-stats layout switches, persists, and the beat follows', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const out = await $.command.run(cmd('layout pane'))
  expect(out.text?.includes('pane')).toBe(true)
  expect(w.store.layout).toBe('pane')
  expect(w.opened).toBe(1)
  await clock.advance(2000)
  expect(lastBeat(w)).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_002_000, layout: 'pane', summary_row: true })
  const bad = await $.command.run(cmd('layout sideways'))
  expect(bad.text?.includes('band|pane')).toBe(true)
  expect(w.store.layout).toBe('pane')
})

// ---------------------------------------------------------------------------------------------
// tiers by maxRows: the autopilot tab band (doc/plans/2026-10-31-forge-tabs.md §2.5)

// a drawing with exactly one Text and nothing else is what autopilot can wrap in its Button
async function onlyText(ui: { find: (q: { type: string; text?: RegExp }) => Promise<{ text: string } | undefined> }) {
  expect(await ui.find({ type: 'Box' })).toBe(undefined)
  expect(await ui.find({ type: 'Raster' })).toBe(undefined)
  const t = await ui.find({ type: 'Text' })
  expect(t !== undefined).toBe(true)
  return t!.text
}

test('rgbHex turns the village colour into #rrggbb and refuses anything else', () => {
  expect(rgbHex([215, 95, 0])).toBe('#d75f00')
  expect(rgbHex([0, 0, 0])).toBe('#000000')
  expect(rgbHex([256, 0, 0])).toBe(null)
  expect(rgbHex([1.5, 0, 0])).toBe(null)
  expect(rgbHex([1, 2])).toBe(null)
})

test('the band colours by meaning: the pet in its village colour, lights by state', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const m = await $.ui.mount(band(170))
  expect((await m.find({ type: 'Text', text: /^Ferris$/ }))?.props.color).toBe('#d75f00')
  // memory active and brain ok: both lights green
  const lights = await m.findAll({ type: 'Text', text: /^● $/ })
  expect(lights.map(t => t.props.color)).toEqual(['success', 'success'])
  await m.unmount()
})

// the summary row's text (TUI guideline §2.2): the Texts of the Box keyed `summary`; undefined when forge-stats passed the row on
async function summaryText(ui: { findAll: (q: { type: string }) => Promise<{ props: { key?: string } }[]>; find: (q: { type: string }) => Promise<unknown> }) {
  expect(await ui.find({ type: 'Raster' })).toBe(undefined)
  const boxes = (await ui.findAll({ type: 'Box' })).map(b => b.props.key)
  if (boxes.includes('beneath')) return undefined
  expect(boxes).toEqual(['summary'])
  const texts = (await ui.findAll({ type: 'Text' })) as unknown as { text: string }[]
  return texts.map(t => t.text).join('')
}

test('one row is the summary row (autopilot hands it down under its tab row, summary_row in the heartbeat): the pet when it fits, else passed on (PM G33)', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const [cols, want] of [[30, 'Ferris Lv.3'], [13, 'Ferris Lv.3'], [12, undefined], [8, undefined]] as const) {
    const ui = await $.ui.mount(band(cols, { maxRows: 1 }))
    expect(await summaryText(ui)).toBe(want) // 'Ferris Lv.3' is 11 cells; the row keeps a 2-cell margin
    await ui.unmount()
  }
})

test('two to five rows, or a thin column, is the same one-row summary', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  // the pet alone: no speed (B32), no fleet summary without autopilot
  for (const target of [band(100, { maxRows: 3 }), band(20, { maxRows: 12 })]) {
    const ui = await $.ui.mount(target)
    expect(await summaryText(ui)).toBe('Ferris Lv.3')
    await ui.unmount()
  }
})

// ---- the fleet summary (PM G33): autopilot's `autopilot.fleet` atom, read only ----
const FLEET = { v: 1, fetchedMs: 999_000, error: null, capped: 0, totals: { needYou: 1, failed: 1, stuck: 2, running: 4, waiting: 11 }, lanes: [], topStuck: null }
// inline plugins named autopilot that publish a summary at session start. A register runs in its own environment (no test variable, no code from strings), so each
// value is written out in its source.
const AUTOPILOT_V1 = {
  name: 'autopilot',
  register: ((on: any) => {
    on('session.start', async ($: any, e: any, next: any) => {
      await $.state.set({ plugin: 'autopilot', key: 'fleet' }, { v: 1, fetchedMs: 999000, error: null, capped: 0, totals: { needYou: 1, failed: 1, stuck: 2, running: 4, waiting: 11 }, lanes: [], topStuck: null })
      return next(e)
    })
  }) as never,
}
const AUTOPILOT_V2 = {
  name: 'autopilot',
  register: ((on: any) => {
    on('session.start', async ($: any, e: any, next: any) => {
      await $.state.set({ plugin: 'autopilot', key: 'fleet' }, { v: 2, fetchedMs: 999000, error: null, capped: 0, totals: { needYou: 1, failed: 1, stuck: 2, running: 4, waiting: 11 }, lanes: [], topStuck: null })
      return next(e)
    })
  }) as never,
}

test('parseFleet takes autopilot.fleet v 1 only; summaryParts gives the widest form that fits, dropping from the end and keeping the most severe count', () => {
  expect(parseFleet(FLEET)?.totals).toEqual(FLEET.totals)
  for (const bad of [null, 3, {}, { ...FLEET, v: 2 }, { ...FLEET, totals: { ...FLEET.totals, stuck: -1 } }, { ...FLEET, totals: { ...FLEET.totals, waiting: '1' } }]) expect(parseFleet(bad)).toBe(null)
  // review g1 (e4ebee78): odd fetch fields are no summary, so a broken atom can never draw ✓
  const zeroed = { ...FLEET, totals: { needYou: 0, failed: 0, stuck: 0, running: 0, waiting: 0 }, fetchedMs: 1 }
  const { error: _drop, ...noError } = zeroed
  const { fetchedMs: _drop2, ...noTime } = zeroed
  for (const bad of [noError, noTime, { ...zeroed, error: 42 }, { ...zeroed, error: false }, { ...zeroed, fetchedMs: NaN }, { ...zeroed, fetchedMs: Infinity }, { ...zeroed, fetchedMs: '1' }]) {
    expect(parseFleet(bad)).toBe(null)
    expect(summaryParts(parseFleet(bad), null, 40, 'fleet')).toBe(null) // nothing drawn: the row is passed on
  }
  expect(summaryParts(parseFleet(zeroed), null, 40, 'fleet')?.map(p => p.text).join('')).toBe('fleet ✓') // the good one still does
  // the zh-TW word: 艦隊 is 4 cells
  expect(summaryParts(parseFleet(FLEET), null, 18, '艦隊')?.map(p => p.text).join('')).toBe('艦隊 ▲ 1  ✗ 1  ⊘ 2')
  const f = parseFleet(FLEET)
  const text = (cols: number, pet: string | null = 'Ferris Lv.3') => summaryParts(f, pet, cols, 'fleet')?.map(p => p.text).join('')
  expect(text(80)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2  ● 4  ⋯ 11 · Ferris Lv.3')
  expect(text(43)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2  ● 4  ⋯ 11') // the pet goes first
  expect(text(30)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2  ● 4  ⋯ 11') // 30 cells: fits exactly
  expect(text(29)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2  ● 4') // then ⋯
  expect(text(24)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2  ● 4') // 24 cells: fits exactly
  expect(text(23)).toBe('fleet ▲ 1  ✗ 1  ⊘ 2') // then ●
  expect(text(18)).toBe('fleet ▲ 1  ✗ 1') // then ⊘
  expect(text(13)).toBe('fleet ▲ 1')
  expect(text(8)).toBe('▲ 1') // the most severe count alone, no word
  expect(text(2)).toBe(undefined) // nothing fits: the row is passed on
  // the most severe non-zero count leads when ▲ is 0
  const noNeed = parseFleet({ ...FLEET, totals: { needYou: 0, failed: 0, stuck: 2, running: 1, waiting: 0 } })
  expect(summaryParts(noNeed, null, 3, 'fleet')?.map(p => p.text).join('')).toBe('⊘ 2')
  // spacing invariant: a count glyph is always followed by a space, never three spaces in a row, at every width
  const all = parseFleet({ ...FLEET, totals: { needYou: 1, failed: 1, stuck: 2, running: 4, waiting: 11 } })
  for (let cols = 0; cols <= 80; cols++) {
    const t = summaryParts(all, 'Ferris Lv.3', cols, 'fleet')?.map(p => p.text).join('') ?? ''
    expect(/[▲✗⊘●⋯][^ ]/.test(t)).toBe(false)
    expect(/ {3}/.test(t)).toBe(false)
    // a label belongs to its chip (§3.5 rule 2): the word is followed by exactly one space, never the two-space chip gap
    if (t.startsWith('fleet')) expect(/^fleet [▲✗⊘●⋯] /.test(t)).toBe(true)
  }
  // colours are the shared table's theme keys; the word and separator dim
  const parts = summaryParts(f, 'Ferris Lv.3', 80, 'fleet')!
  expect(parts.filter(p => p.color).map(p => p.text + ':' + p.color)).toEqual(['▲:warning', '✗:error', '⊘:error', '●:suggestion', '⋯:inactive'])
  expect(parts.filter(p => p.dim).map(p => p.text)).toEqual(['fleet', ' · '])
  // no count: ✓ only after a fetch that succeeded; ? on an error or before any fetch
  const zero = { needYou: 0, failed: 0, stuck: 0, running: 0, waiting: 0 }
  expect(summaryParts(parseFleet({ ...FLEET, totals: zero }), null, 40, 'fleet')?.map(p => p.text).join('')).toBe('fleet ✓')
  expect(summaryParts(parseFleet({ ...FLEET, totals: zero, error: 'hub unreachable' }), null, 40, 'fleet')?.map(p => p.text).join('')).toBe('fleet ?')
  expect(summaryParts(parseFleet({ ...FLEET, totals: zero, fetchedMs: null }), null, 40, 'fleet')?.map(p => p.text).join('')).toBe('fleet ?')
  // no summary and no pet: nothing to draw
  expect(summaryParts(null, null, 40, 'fleet')).toBe(null)
})

test('with autopilot\'s summary the one-row band draws the fleet counts before the pet (PM G33)', { plugins: [AUTOPILOT_V1] }, async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const [cols, want] of [[60, 'fleet ▲ 1  ✗ 1  ⊘ 2  ● 4  ⋯ 11 · Ferris Lv.3'], [28, 'fleet ▲ 1  ✗ 1  ⊘ 2  ● 4'], [6, '▲ 1']] as const) {
    const ui = await $.ui.mount(band(cols, { maxRows: 1 }))
    expect(await summaryText(ui)).toBe(want)
    await ui.unmount()
  }
})

test('another summary version is no summary: the pet alone', { plugins: [AUTOPILOT_V2] }, async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(band(60, { maxRows: 1 }))
  expect(await summaryText(ui)).toBe('Ferris Lv.3')
  await ui.unmount()
})

test('mini and full tiers put the fleet row on top only with a row to spare above the tier minimum (mini >= 7, full >= 13)', { plugins: [AUTOPILOT_V1] }, async ($, on) => {
  world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  for (const [rows, top] of [[6, false], [7, true], [12, false], [13, true]] as const) {
    const ui = await $.ui.mount(band(100, { maxRows: rows }))
    expect(await ui.find({ type: 'Raster' }) !== undefined).toBe(true)
    const keys = (await ui.findAll({ type: 'Box' })).map(b => b.props.key)
    expect(keys.includes('fleet')).toBe(top)
    await ui.unmount()
  }
})

test('six to eleven rows draw the 16x6 mini Ferris, and its blits are mini-sized', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const mini = parseSprite(FERRIS_MINI, 16, 12)!
  const ui = await $.ui.mount(band(100, { maxRows: 8 }))
  const r = await ui.find({ type: 'Raster' })
  expect(r?.props.columns).toBe(16)
  expect(r?.props.rows).toBe(6)
  expect(r?.props.cells).toBe(halfBlockCells(mini, 0).cells)
  await clock.advance(2000)
  const bandBlits = w.blits.filter(b => b.key === 'pet-band')
  expect(bandBlits.length > 0).toBe(true)
  expect(bandBlits.every(b => b.cells.length === halfBlockCells(mini, 0).cells.length)).toBe(true)
  await ui.unmount()
})

test('a label alone never blits the band', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount(band(16, { maxRows: 1 }))
  await clock.advance(2000)
  expect(w.blits.filter(b => b.key === 'pet-band').length).toBe(0)
  await ui.unmount()
})

test('label then expanded in one render: the expanded call decides the band blits', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  // the engine runs both calls under the one band instance; the harness mounts one at a time
  const label = await $.ui.mount(band(16, { maxRows: 1 }))
  await label.unmount()
  const full = await $.ui.mount(band(100, { maxRows: 12 }))
  await clock.advance(2000)
  const bandBlits = w.blits.filter(b => b.key === 'pet-band')
  expect(bandBlits.length > 0).toBe(true)
  expect(bandBlits.every(b => b.requestId === 'band-1' && b.cells.length === halfBlockCells(parseSprite(FERRIS)!, 0).cells.length)).toBe(true)
  await full.unmount()
})

test('after a model step the band and the summary show no speed (B32)', async ($, on) => {
  world(on)
  on('turn.step', async function* () {
    yield { kind: 'text', index: 0, text: 'ab' } as const
    return {
      turnId: 't1',
      index: 0,
      answer: 'ab',
      toolUses: [],
      stopReason: 'end_turn',
      usage: { input_tokens: 10, output_tokens: 50, cache_read_input_tokens: 0, cache_creation_input_tokens: 0, model: 'claude-opus-5-5' },
    } as const
  })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const stream = $.turn.step({ turnId: 't1', index: 0, model: 'opus', messageCount: 1 })
  const chunks: string[] = []
  for await (const c of stream) if (c.kind === 'text') chunks.push(c.text)
  expect(chunks).toEqual(['ab'])
  await stream.result
  for (const target of [band(200), band(100, { maxRows: 3 })]) {
    const ui = await $.ui.mount(target)
    expect(await ui.find({ type: 'Text', text: /tok\/s|ϟ/ })).toBe(undefined)
    await ui.unmount()
  }
})

// ---- the prompt hint tail (PM, after the prompt-slot probe): the lights after the engine's own hint, never a rewrite of it ----
const HINT = { plugin: 'forge-stats', component: 'PromptHint', requestId: 'hint', surface: 'terminal', props: { isDraft: false, isWorking: false, hint: '? for shortcuts' } } as const
// beneath: the engine draws the hint; record what reached it
const hintBeneath = (on: Parameters<TestBody>[1], seen: { hint?: string; tail?: string }[]) =>
  on('ui.render', { component: 'PromptHint' }, ($, e) => {
    seen.push({ hint: e.props.hint, tail: e.props.tail })
    const { Text } = $.ui.resolve(e)
    return <Text key="hint">{e.props.hint}</Text>
  })

test('the prompt hint keeps the engine\'s text and gets the memory and brain lights as its tail', async ($, on) => {
  world(on)
  const seen: { hint?: string; tail?: string }[] = []
  hintBeneath(on, seen)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(HINT)
  await ui.unmount()
  expect(seen.at(-1)).toEqual({ hint: '? for shortcuts', tail: 'memory 12 entries · brain connected' })
})

test('hidden lights are left out of the tail; with both hidden the hint passes unchanged', async ($, on) => {
  const w = world(on)
  w.session = { ...SESSION, memory: { light: 'hidden', active: 0 }, brain: { light: 'hidden' } }
  const seen: { hint?: string; tail?: string }[] = []
  hintBeneath(on, seen)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(HINT)
  await ui.unmount()
  expect(seen.at(-1)).toEqual({ hint: '? for shortcuts', tail: undefined })
})

test('a tail a plugin above already set stays, forge-stats\' lights follow it', async ($, on) => {
  world(on)
  const seen: { hint?: string; tail?: string }[] = []
  hintBeneath(on, seen)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...HINT, props: { ...HINT.props, tail: 'other plugin' } })
  await ui.unmount()
  expect(seen.at(-1)).toEqual({ hint: '? for shortcuts', tail: 'other plugin · memory 12 entries · brain connected' })
})
