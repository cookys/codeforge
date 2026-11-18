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

const band = (bodyColumns: number, over: Record<string, unknown> = {}) =>
  ({
    plugin: 'forge-stats',
    component: 'AbovePrompt',
    requestId: 'band-1',
    surface: 'terminal',
    props: { hasSurvey: false, isWorking: false, maxRows: 20, bodyColumns, scroll: { offset: 0, bodyRows: 19 }, view: {}, ...over },
  }) as const

type World = { writes: { path: string; text: string }[]; blits: { requestId: string; key: string; cells: string }[]; store: Record<string, unknown>; opened: number; deny: boolean; invalidates: number; session?: unknown }

// The engine beneath: HOME, the live pointer, pet.json, the session view, the store, writes and blits.
function world(on: Parameters<TestBody>[1], layout?: string): World {
  const w: World = { writes: [], blits: [], store: layout === undefined ? {} : { layout }, opened: 0, deny: false, invalidates: 0 }
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
  on('ui.invalidate', () => {
    w.invalidates += 1
    return { value: undefined }
  })
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
  expect(first).toEqual({ schema: 'codeforge.mod-alive/1', session_id: SID, written_at_ms: 1_000_000, layout: 'band', summary_row: true, band_seen: false })
  expect(w.writes.filter(x => x.path === HB)[0]!.text.length < 512).toBe(true)
  await clock.advance(2000)
  expect(lastBeat(w)?.written_at_ms).toBe(1_002_000)
})

// B34: the statusline compacts only for a band that drew lately. A plugin above forge-stats on the AbovePrompt chain that draws its own band and never
// calls `next(e)` (autopilot's O4 shell) means forge-stats' band hook is never called: the band is not mounted for it.

test('band_seen: true while the band is on screen and drawing; false within 32 s once its hook stops being called (B34)', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  expect(lastBeat(w)?.band_seen).toBe(false)
  const ui = await $.ui.mount(band(120))
  for (let i = 0; i < 20; i++) {
    await clock.advance(2000)
    expect(lastBeat(w)).toMatchObject({ layout: 'band', band_seen: true })
  }
  // a shell opened over it: the hook is not called any more
  await ui.unmount()
  await clock.advance(32_000)
  expect(lastBeat(w)).toMatchObject({ layout: 'band', summary_row: true, band_seen: false })
})

test('a band hook never called (a plugin above draws its own band: autopilot\'s O4 shell) never says band_seen; layout stays band (B34)', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (let i = 0; i < 20; i++) {
    await clock.advance(2000)
    expect(lastBeat(w)).toMatchObject({ layout: 'band', summary_row: true, band_seen: false })
  }
  // the heartbeat never forces a render: nothing reads band_seen any more, and an invalidate every 2 s redraws the whole UI for nothing
  expect(w.invalidates).toBe(0)
})

test('the band layout is the default and does not open the pane', async ($, on) => {
  const w = world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  expect(w.opened).toBe(0)
})

test('the band passes on a survey or desktop', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const target of [band(100, { hasSurvey: true }), { ...band(100), surface: 'desktop' }] as const) {
    const ui = await $.ui.mount(target)
    expect(await ui.find({ type: 'Raster' })).toBe(undefined)
    await ui.unmount()
  }
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

// the summary row's text (TUI guideline §2.2): the Texts of the Box keyed `summary`; undefined when forge-stats passed the row on
async function summaryText(ui: { findAll: (q: { type: string }) => Promise<{ props: { key?: string } }[]>; find: (q: { type: string }) => Promise<unknown> }) {
  expect(await ui.find({ type: 'Raster' })).toBe(undefined)
  const boxes = (await ui.findAll({ type: 'Box' })).map(b => b.props.key)
  // The engine composes plugin parts in an unkeyed Box, including the empty test part beneath.
  if (boxes[0] === undefined) boxes.shift()
  const texts = (await ui.findAll({ type: 'Text' })) as unknown as { text: string }[]
  if (!boxes.includes('summary')) {
    expect(boxes).toEqual(['beneath'])
    expect(texts).toEqual([])
    return undefined
  }
  expect(boxes.filter(k => k !== 'beneath')).toEqual(['summary'])
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

// ---- 01M7MBV76H: the band never takes rows from autopilot-live ----
// Since Claude Code 2.1.326 the engine composes every plugin's AbovePrompt part in one column, so forge-stats is no longer asked only from inside autopilot's forge
// tab: its part stacks under autopilot-live's band. A 13-row Ferris there pushed the decision cards and the send row out of view (operator, 11-17).

test('without autopilot the band is one summary row at every height and width: never a Raster, never a band blit', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  for (const rows of [1, 3, 6, 7, 12, 13, 20, 40]) {
    for (const cols of [40, 80, 120, 160]) {
      const ui = await $.ui.mount(band(cols, { maxRows: rows }))
      expect(await summaryText(ui)).toBe('Ferris Lv.3')
      await clock.advance(1000)
      await ui.unmount()
    }
  }
  expect(w.blits.filter(b => b.key === 'pet-band')).toEqual([])
})

// autopilot-live as the operator's terminal composes it: the band row, a decision card and the send row at the bottom, then whatever the plugins below add
const DECIDE_ROWS = ['ⓘ autopilot · 待決 1', '待決 d-1234abcd：要不要上線？ [A] 上線 [B] 等等', '送出 ⏎']
const AUTOPILOT_LIVE = {
  name: 'autopilot',
  register: ((on: any) => {
    on('session.start', async ($: any, e: any, next: any) => {
      await $.state.set({ plugin: 'autopilot', key: 'fleet' }, { v: 1, fetchedMs: 999000, error: null, capped: 0, totals: { needYou: 1, failed: 0, stuck: 0, running: 0, waiting: 0 }, lanes: [], topStuck: null })
      return next(e)
    })
    on('ui.render', { component: 'AbovePrompt' }, async ($: any, e: any, next: any) => {
      const { Box, Text } = $.ui.resolve(e)
      const below = await next(e)
      return (
        <Box key="autopilot-live" flexDirection="column">
          <Text key="ap-0">{'ⓘ autopilot · 待決 1'}</Text>
          <Text key="ap-1">{'待決 d-1234abcd：要不要上線？ [A] 上線 [B] 等等'}</Text>
          <Text key="ap-send">{'送出 ⏎'}</Text>
          {below}
        </Box>
      )
    })
  }) as never,
}

// golden: with autopilot-live loaded, its decision card and send row are the last rows drawn above the prompt, so they stay in view without scrolling
test('golden: with autopilot-live loaded the decision card and the send row stay at the bottom; forge-stats adds no row and no Raster (160x40, 80x24)', { plugins: [AUTOPILOT_LIVE] }, async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  // the rows the engine leaves above the prompt in a 160x40 and an 80x24 terminal
  for (const [cols, rows] of [[160, 20], [80, 12]] as const) {
    const ui = await $.ui.mount(band(cols, { maxRows: rows, scroll: { offset: 0, bodyRows: rows - 1 } }))
    expect(await ui.find({ type: 'Raster' })).toBe(undefined)
    const texts = ((await ui.findAll({ type: 'Text' })) as unknown as { text: string }[]).map(t => t.text)
    expect(texts).toEqual(DECIDE_ROWS)
    expect(texts[texts.length - 1]).toBe('送出 ⏎')
    await clock.advance(1000)
    await ui.unmount()
  }
  expect(w.blits.filter(b => b.key === 'pet-band')).toEqual([])
})

test('any autopilot (another summary version too) makes the band pass at every height', { plugins: [AUTOPILOT_V2] }, async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const rows of [1, 6, 13, 20]) {
    const ui = await $.ui.mount(band(120, { maxRows: rows }))
    expect(await summaryText(ui)).toBe(undefined)
    await ui.unmount()
  }
})
