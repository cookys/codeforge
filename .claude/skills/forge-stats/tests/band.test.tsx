import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { bandLayout, displayWidth, halfBlockCells, rgbHex, tabLabel, parseSessionView, parseSprite, petShown, sanitizeSessionId } from '../hooks/model'
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

test('bandLayout adds columns in priority order as the width allows, 1 to 5', () => {
  const ids = (cols: number, sprite = 24) => bandLayout(cols, sprite).map(c => c.id).join(' ')
  // each threshold and one cell below it (24-wide Ferris: 24, 56, 92, 140, 170)
  expect(ids(23)).toBe('')
  expect(ids(24)).toBe('ferris')
  expect(ids(55)).toBe('ferris')
  expect(ids(56)).toBe('ferris pet')
  expect(ids(91)).toBe('ferris pet')
  expect(ids(92)).toBe('ferris pet speed')
  expect(ids(139)).toBe('ferris pet speed')
  expect(ids(140)).toBe('ferris pet speed recent')
  expect(ids(169)).toBe('ferris pet speed recent')
  expect(ids(170)).toBe('ferris pet speed recent memory')
  // the 16-wide mini Ferris moves every threshold down by 8
  expect(ids(162, 16)).toBe('ferris pet speed recent memory')
  expect(ids(161, 16)).toBe('ferris pet speed recent')
})

test('bandLayout gives the spare width to the request table only, and always fills to the edge with it', () => {
  const total = (cols: ReturnType<typeof bandLayout>) => cols.reduce((n, c) => n + c.width, 0) + 2 * (cols.length - 1)
  const five = bandLayout(200, 24)
  expect(five.map(c => c.width)).toEqual([24, 30, 34, 76, 28])
  expect(total(five)).toBe(200)
  const four = bandLayout(150, 24)
  expect(four.find(c => c.id === 'recent')?.width).toBe(56)
  expect(total(four)).toBe(150)
  // without the table the columns keep their widths
  expect(bandLayout(120, 24).map(c => c.width)).toEqual([24, 30, 34])
})

test('petShown needs the band layout and an ok blit under 2 s old', () => {
  expect(petShown('band', { at: 1000, ok: true }, 2999)).toBe(true)
  expect(petShown('band', { at: 1000, ok: true }, 3000)).toBe(false)
  expect(petShown('band', { at: 1000, ok: false }, 1500)).toBe(false)
  expect(petShown('band', null, 1500)).toBe(false)
  expect(petShown('pane', { at: 1000, ok: true }, 1500)).toBe(false)
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
  expect(first).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_000_000, layout: 'band', pet_shown: false })
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

test('the band draws 1 to 5 columns by width: Ferris, pet, speed, recent requests, memory', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const cells0 = halfBlockCells(parseSprite(FERRIS)!, 0).cells
  // one marker Text per column
  const MARK = { pet: /^Ferris$/, speed: /^Speed$/, recent: /^Recent requests$/, memory: /^memory/ } as const
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
  expect(await shows(170)).toBe('pet speed recent memory')
  expect(await shows(145)).toBe('pet speed recent')
  expect(await shows(100)).toBe('pet speed')
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

test('band blits go to the band instance and drive pet_shown', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount(band(100))
  await clock.advance(2000)
  const bandBlits = w.blits.filter(b => b.key === 'pet-band')
  expect(bandBlits.length > 0).toBe(true)
  expect(bandBlits.every(b => b.requestId === 'band-1')).toBe(true)
  expect(lastBeat(w)?.pet_shown).toBe(true)

  // the band stops being mounted: blits are denied, and the next beat says so
  w.deny = true
  await clock.advance(2000)
  expect(lastBeat(w)?.pet_shown).toBe(false)
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
  expect(lastBeat(w)).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_002_000, layout: 'pane', pet_shown: false })
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

test('tabLabel takes the longest form that fits: pet, then sparkline, then the bare speed', () => {
  const spark = '▁▂▃▅▇'
  const full = tabLabel(30, 93, spark, 'Ferris Lv.1')
  expect(full).toBe('ϟ 93 tok/s ▁▂▃▅▇ · Ferris Lv.1')
  expect(displayWidth(full)).toBe(30)
  expect(tabLabel(29, 93, spark, 'Ferris Lv.1')).toBe('ϟ 93 tok/s ▁▂▃▅▇')
  expect(tabLabel(16, 93, spark, 'Ferris Lv.1')).toBe('ϟ 93 tok/s ▁▂▃▅▇')
  expect(tabLabel(15, 93, spark, 'Ferris Lv.1')).toBe('ϟ 93 tok/s')
  expect(tabLabel(10, 93, spark, 'Ferris Lv.1')).toBe('ϟ 93 tok/s')
  expect(tabLabel(9, 93, spark, 'Ferris Lv.1')).toBe('ϟ93')
  // no sparkline yet: the pet follows the speed directly
  expect(tabLabel(30, 93, '', 'Ferris Lv.1')).toBe('ϟ 93 tok/s · Ferris Lv.1')
  // a wide pet name counts two cells a character
  expect(tabLabel(30, 93, spark, '鐵匠 Lv.1')).toBe('ϟ 93 tok/s ▁▂▃▅▇ · 鐵匠 Lv.1')
  expect(tabLabel(26, 93, spark, '鐵匠 Lv.1')).toBe('ϟ 93 tok/s ▁▂▃▅▇')
})

test('the label splits at its first " · ": speed and sparkline before it, never a separator inside (autopilot colours the two parts)', () => {
  const SPEED_PART = /^ϟ ?(\d+|—)( tok\/s( [▁▂▃▄▅▆▇█]+)?)?$/
  for (const tps of [null, 0, 93, 12345.6, Infinity])
    for (const spark of ['', '▁▂▃▅▇'])
      for (const pet of [null, 'Ferris Lv.1', 'a · b Lv.2'])
        for (const cols of [4, 9, 15, 16, 29, 30, 60]) {
          const label = tabLabel(cols, tps, spark, pet)
          const at = label.indexOf(' · ')
          expect(SPEED_PART.test(at < 0 ? label : label.slice(0, at))).toBe(true)
        }
})

test('rgbHex turns the village colour into #rrggbb and refuses anything else', () => {
  expect(rgbHex([215, 95, 0])).toBe('#d75f00')
  expect(rgbHex([0, 0, 0])).toBe('#000000')
  expect(rgbHex([256, 0, 0])).toBe(null)
  expect(rgbHex([1.5, 0, 0])).toBe(null)
  expect(rgbHex([1, 2])).toBe(null)
})

test('the band colours by meaning: speed in success, the pet in its village colour, lights by state', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const m = await $.ui.mount(band(170))
  expect((await m.find({ type: 'Text', text: /^ϟ $/ }))?.props.color).toBe('success')
  expect((await m.find({ type: 'Text', text: /^Ferris$/ }))?.props.color).toBe('#d75f00')
  // memory active and brain ok: both lights green
  const lights = await m.findAll({ type: 'Text', text: /^● $/ })
  expect(lights.map(t => t.props.color)).toEqual(['success', 'success'])
  await m.unmount()
})

test('one row is a tab label: a single Text that shortens with the width', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  // no request timed yet: the speed is a dash and there is no sparkline
  const cases: [number, RegExp][] = [
    [30, /^ϟ — tok\/s · Ferris Lv\.3$/],
    [16, /^ϟ — tok\/s$/],
    [8, /^ϟ—$/],
  ]
  for (const [cols, want] of cases) {
    const ui = await $.ui.mount(band(cols, { maxRows: 1 }))
    expect(want.test(await onlyText(ui))).toBe(true)
    await ui.unmount()
  }
})

test('two to five rows, or a thin column, is a one-row summary Text', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const target of [band(100, { maxRows: 3 }), band(20, { maxRows: 12 })]) {
    const ui = await $.ui.mount(target)
    const text = await onlyText(ui)
    expect(text.startsWith('ϟ ')).toBe(true)
    await ui.unmount()
  }
  const wide = await $.ui.mount(band(100, { maxRows: 3 }))
  expect((await onlyText(wide)).includes('Ferris Lv.3')).toBe(true)
  await wide.unmount()
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
  expect(lastBeat(w)?.pet_shown).toBe(true)
  await ui.unmount()
})

test('a label alone never blits the band, so the pet is not shown', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount(band(16, { maxRows: 1 }))
  await clock.advance(2000)
  expect(w.blits.filter(b => b.key === 'pet-band').length).toBe(0)
  expect(lastBeat(w)?.pet_shown).toBe(false)
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
  expect(lastBeat(w)?.pet_shown).toBe(true)
  await full.unmount()
})

test('the label speaks of the last request long enough to time, not a lone tool call after it', async ($, on) => {
  world(on)
  const spin = (ms: number) => {
    const until = performance.now() + ms
    while (performance.now() < until) {}
  }
  let n = 0
  on('turn.step', async function* () {
    n += 1
    const timed = n === 1 // the first step decodes for 250 ms; the second is a lone tool call, too short to time
    yield { kind: 'text', index: 0, text: 'a' } as const
    if (timed) spin(250)
    yield { kind: 'text', index: 0, text: 'b' } as const
    return {
      turnId: 't' + n,
      index: 0,
      answer: 'ab',
      toolUses: [],
      stopReason: 'end_turn',
      usage: { input_tokens: 10, output_tokens: timed ? 50 : 3, cache_read_input_tokens: 0, cache_creation_input_tokens: 0, model: 'claude-opus-5-5' },
    } as const
  })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const i of [1, 2]) {
    const stream = $.turn.step({ turnId: 't' + i, index: 0, model: 'opus', messageCount: 1 })
    for await (const _ of stream) {
    }
    await stream.result
  }
  const ui = await $.ui.mount(band(30, { maxRows: 1 }))
  const texts = await ui.findAll({ type: 'Text' })
  expect(/^ϟ \d+ tok\/s/.test(texts.map(t => t.text).join(''))).toBe(true)
  await ui.unmount()
})
