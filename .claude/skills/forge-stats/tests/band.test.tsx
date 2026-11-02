import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { bandLayout, halfBlockCells, rgbHex, parseSessionView, parseSprite, sanitizeSessionId } from '../hooks/model'
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

type World = { writes: { path: string; text: string }[]; blits: { requestId: string; key: string; cells: string }[]; store: Record<string, unknown>; opened: number; deny: boolean }

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
    if (e.path === SESSION_FILE) return { value: JSON.stringify(SESSION) }
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
  expect(first).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_000_000, layout: 'band' })
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
  expect(lastBeat(w)).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_002_000, layout: 'pane' })
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

test('one row passes on: the tab label is autopilot\'s since the speed moved there (B32)', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const cols of [30, 16, 8]) {
    const ui = await $.ui.mount(band(cols, { maxRows: 1 }))
    expect(await ui.find({ type: 'Raster' })).toBe(undefined)
    expect(await ui.find({ type: 'Text' })).toBe(undefined)
    // what draws beneath stands: forge-stats returned next(e)
    expect((await ui.findAll({ type: 'Box' })).map(b => b.props.key)).toEqual(['beneath'])
    await ui.unmount()
  }
})

test('two to five rows, or a thin column, is a one-row summary Text', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  // the pet alone: no speed (B32)
  for (const target of [band(100, { maxRows: 3 }), band(20, { maxRows: 12 })]) {
    const ui = await $.ui.mount(target)
    expect(await onlyText(ui)).toBe('Ferris Lv.3')
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
