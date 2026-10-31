import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { bandColumns, halfBlockCells, parseSessionView, parseSprite, petShown, sanitizeSessionId } from '../hooks/model'
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

test('bandColumns hides memory first, then speed', () => {
  expect(bandColumns(100, 24)).toEqual({ speed: true, memory: 38 })
  expect(bandColumns(90, 24)).toEqual({ speed: true, memory: 28 })
  expect(bandColumns(89, 24)).toEqual({ speed: true, memory: 0 })
  expect(bandColumns(60, 24)).toEqual({ speed: true, memory: 0 })
  expect(bandColumns(59, 24)).toEqual({ speed: false, memory: 0 })
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

test('the band draws speed | Ferris | memory, dropping columns as it narrows', async ($, on) => {
  world(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const cells0 = halfBlockCells(parseSprite(FERRIS)!, 0).cells

  const wide = await $.ui.mount(band(100))
  const r = await wide.find({ type: 'Raster' })
  expect(r?.props.columns).toBe(24)
  expect(r?.props.rows).toBe(12)
  expect(r?.props.cells).toBe(cells0)
  expect((await wide.find({ type: 'Text', text: /tok\/s/ })) !== undefined).toBe(true)
  expect((await wide.find({ type: 'Text', text: /^Ferris$/ })) !== undefined).toBe(true)
  expect((await wide.find({ type: 'Text', text: /12/ })) !== undefined).toBe(true)
  await wide.unmount()

  const mid = await $.ui.mount(band(70))
  expect((await mid.find({ type: 'Raster' })) !== undefined).toBe(true)
  expect((await mid.find({ type: 'Text', text: /tok\/s/ })) !== undefined).toBe(true)
  expect(await mid.find({ type: 'Text', text: /^Ferris$/ })).toBe(undefined)
  await mid.unmount()

  const narrow = await $.ui.mount(band(40))
  expect((await narrow.find({ type: 'Raster' })) !== undefined).toBe(true)
  expect(await narrow.find({ type: 'Text', text: /tok\/s/ })).toBe(undefined)
  await narrow.unmount()
})

test('the band passes on a survey, a short slot, a thin column, desktop, or the pane layout', async ($, on) => {
  world(on)
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  for (const target of [band(100, { hasSurvey: true }), band(100, { maxRows: 11 }), band(25), { ...band(100), surface: 'desktop' }] as const) {
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
