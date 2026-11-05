// The chat-style pane (TUI guideline §13): the tab strip and the key hint are the last two rows of the tree, the content grows up from them and scrolls in the mod's own window.
import { expect, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { clipCells, newPaneScroll, scrolled, windowOf } from '../hooks/pane-view'
import { FERRIS } from './ferris-sprite'

const LIVE = '/run/user/1000/autopilot'
const SUMMARY = LIVE + '/codeforge/usage/summary.json'
const PET = LIVE + '/codeforge/pet.json'

type Node = { type: string; props?: Record<string, unknown>; children?: (Node | string)[] }
const textOf = (n: Node | string): string => (typeof n === 'string' ? n : (n.children ?? []).map(textOf).join(''))
const kids = (tree: unknown) => ((tree as Node).children ?? []) as Node[]
const rowsOf = (tree: unknown) => kids(tree).map(textOf)
const tabsAt = (tree: unknown) => kids(tree).findIndex(k => k.props?.key === 'tabs')
// the content row right above the tab strip
const newest = (tree: unknown) => textOf(kids(tree)[tabsAt(tree) - 1])

const props = (bodyRows: number, placement: 'dock' | 'inline' = 'dock') => ({ title: 'Forge stats', isFocused: false, bodyColumns: 70, placement, scroll: { offset: 0, bodyRows }, view: {} }) as const
const target = (bodyRows: number, placement: 'dock' | 'inline' = 'dock') => ({ plugin: 'forge-stats', component: 'Pane', requestId: 'forge-stats', surface: 'terminal', props: props(bodyRows, placement) }) as const

const day = new Date().toISOString().slice(0, 10)
const summary = (models: number, cost = 1.5) => ({
  schema: 'codeforge.usage-summary/1',
  generated_at_ms: Date.now() - 5 * 60_000,
  host: 'host-1a2b3c4d',
  prices_as_of: '2026-10-06',
  accounts: [{ alias: 'acct-a', written_at_ms: Date.now(), five_hour: { pct: 0.3, resets_at: Math.floor(Date.now() / 1000) + 7800 }, seven_day: null }],
  days: Array.from({ length: models }, (_, i) => ({
    day, source: 'claude', account: 'acct-a', model: 'claude-opus-5-' + i, speed: 'standard',
    input: 100, output: 50, cache_write_5m: 0, cache_write_1h: 0, cache_read: 850, requests: 3, cost_usd: cost,
  })),
})

// the engine beneath: HOME, the live pointer, a pet snapshot (or none) and the usage summary as `world.usage` stands
const world = (on: Parameters<TestBody>[1], w: { usage?: unknown; pet?: unknown; log?: string[] } = {}) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', (_, e) => {
    w.log?.push('open focus=' + String(e.focus))
    return { value: { isPlaced: true } }
  })
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : undefined }))
  on('fs.stat', (_, e) => {
    if (e.path.endsWith('/.autopilot/quota')) w.log?.push('stat quota')
    if (e.path === SUMMARY && w.usage !== undefined) return { value: { kind: 'file', size: JSON.stringify(w.usage).length, mtimeMs: 0, isLink: false } }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.read', (_, e) => {
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path === SUMMARY && w.usage !== undefined) return { value: JSON.stringify(w.usage) }
    if (e.path === PET && w.pet !== undefined) return { value: JSON.stringify(w.pet) }
    throw new Error('ENOENT ' + e.path)
  })
}
const start = ($: Parameters<TestBody>[0]) => $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })

// a ui.scroll as the engine raises it for the person's key: `by` rows, over a tree of `contentRows` in a window of `bodyRows`
type Mount = Awaited<ReturnType<Parameters<TestBody>[0]['ui']['mount']>>
const key = async ($: Parameters<TestBody>[0], ui: Mount, bodyRows: number, by: number) => {
  const contentRows = kids(await ui.drawn()).length
  await $.ui.scroll({ component: 'Pane', requestId: 'forge-stats', offset: 0, by, bodyRows, contentRows, origin: { kind: 'person' } })
}
const END = 1000
const HOME = -1000

const PET_SNAP = {
  schema: 'codeforge.pet-view/1',
  written_at: new Date().toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: 'explorer', message: null },
  village: { id: 'rust', display_name: 'The Forge-Ruins', language: 'Rust', tagline: 'x', rgb: [215, 95, 0], art_small: [' _~^~^~_'], art_full: ['   _~^~^~_'], sprite: FERRIS },
}

test('short content: a dock pads blank rows on top and sits right above the tab strip; inline has no blank rows', async ($, on) => {
  world(on)
  await start($)
  const dock = await $.ui.mount(target(12))
  const d = kids(await dock.drawn())
  expect(d.length).toBe(12)
  expect(tabsAt(await dock.drawn())).toBe(10)
  const pad = rowsOf(await dock.drawn()).findIndex(r => r.trim() !== '')
  expect(pad).toBe(7)
  expect(textOf(d[9])).toMatch(/no snapshot yet/)
  expect(textOf(d[11])).toMatch(/PgUp\/PgDn/)
  await dock.unmount()
  const inline = await $.ui.mount(target(12, 'inline'))
  const i = rowsOf(await inline.drawn())
  expect(i.length).toBe(5)
  expect(i[0]).toMatch(/Pet/)
  expect(i.some(r => r.trim() === '')).toBe(false)
  expect(tabsAt(await inline.drawn())).toBe(3)
})

test('overflow: the tab strip stays on its screen row through scrolling, the window is bodyRows, the indicator counts the hidden rows', async ($, on) => {
  world(on, { usage: summary(10) })
  await start($)
  const ui = await $.ui.mount(target(8))
  await ui.press({ key: 'tab-usage' })
  const at = tabsAt(await ui.drawn())
  expect(at).toBe(6)
  const bottom = kids(await ui.drawn())
  // indicator + 5 content rows + the two operation rows fill bodyRows (8); the spare row under the hint is the ninth
  expect(bottom.length).toBe(9)
  expect(textOf(bottom[0])).toMatch(/^↑ \d+ more$/)
  const hidden = Number(/\d+/.exec(textOf(bottom[0]))?.[0])
  expect(hidden).toBeGreaterThan(0)
  expect(newest(await ui.drawn())).toMatch(/total/)
  for (const by of [-1, -3, 1, HOME, END, -8]) {
    await key($, ui, 8, by)
    const t = await ui.drawn()
    expect(tabsAt(t), 'by ' + by).toBe(at)
    expect(kids(t).length).toBeLessThanOrEqual(9)
  }
  await key($, ui, 8, HOME)
  expect(textOf(kids(await ui.drawn())[0])).toMatch(/^↓ \d+$/)
  await ui.press({ key: 'tab-pet' })
  expect(tabsAt(await ui.drawn())).toBeLessThanOrEqual(6)
})

test('End returns to the newest row, Home to the oldest, a page is the content area, an arrow is one row', async ($, on) => {
  world(on, { usage: summary(10) })
  await start($)
  const ui = await $.ui.mount(target(8))
  await ui.press({ key: 'tab-usage' })
  const rows = async () => rowsOf(await ui.drawn())
  expect(newest(await ui.drawn())).toMatch(/total/)
  await key($, ui, 8, HOME)
  expect((await rows())[1]).toMatch(/^▾ Usage/)
  await key($, ui, 8, 1)
  expect((await rows())[1]).not.toMatch(/^▾ Usage/)
  await key($, ui, 8, -1)
  expect((await rows())[1]).toMatch(/^▾ Usage/)
  const first = (await rows())[1]
  await key($, ui, 8, 8) // PgDn: bodyRows
  expect((await rows())[1]).not.toBe(first)
  await key($, ui, 8, END)
  expect(newest(await ui.drawn())).toMatch(/total/)
  expect((await rows())[0]).toMatch(/^↑ \d+ more$/)
})

test('resize while stuck to the newest row stays at the newest; a scrolled window keeps its top row', async ($, on) => {
  world(on, { usage: summary(10) })
  await start($)
  const ui = await $.ui.mount(target(8))
  await ui.press({ key: 'tab-usage' })
  await ui.redraw({ ...props(10) })
  let r = rowsOf(await ui.drawn())
  expect(r[tabsAt(await ui.drawn()) - 1]).toMatch(/total/)
  await ui.redraw({ ...props(6) })
  r = rowsOf(await ui.drawn())
  expect(r[tabsAt(await ui.drawn()) - 1]).toMatch(/total/)
  await ui.redraw({ ...props(8) })
  await key($, ui, 8, HOME)
  await key($, ui, 8, 2)
  const top = (t: unknown) => String(kids(t)[1]?.props?.key)
  const before = top(await ui.drawn())
  await ui.redraw({ ...props(10) })
  expect(top(await ui.drawn())).toBe(before)
})

test('a snapshot refresh while scrolled keeps the same top row; at the bottom it stays at the bottom', async ($, on) => {
  const w = { usage: summary(10, 1.5) as unknown }
  world(on, w)
  await start($)
  const ui = await $.ui.mount(target(8))
  await ui.press({ key: 'tab-usage' })
  await key($, ui, 8, HOME)
  await key($, ui, 8, 3)
  const top = (t: unknown) => String(kids(t)[1]?.props?.key)
  const before = top(await ui.drawn())
  expect(before).toBe('l3')
  w.usage = summary(10, 2.5)
  await ui.redraw()
  expect(top(await ui.drawn())).toBe(before)
  expect(textOf(kids(await ui.drawn())[2])).toBeTruthy()
  await key($, ui, 8, END)
  w.usage = summary(10, 3.5)
  await ui.redraw()
  expect(newest(await ui.drawn())).toMatch(/total ≥?\$35\.00/)
})

test('the pet sprite is atomic: it draws whole when the content area holds it, else the text-only pet lines', async ($, on) => {
  world(on, { pet: PET_SNAP })
  await start($)
  const tall = await $.ui.mount(target(60))
  expect(await tall.find({ type: 'Raster' })).not.toBe(undefined)
  // the block is one node of several rows: the blank pad above the rule plus the rule and the block's rows fill the 58-row content area
  const t = await tall.drawn()
  const blanks = rowsOf(t).findIndex(r => r.trim() !== '')
  const raster = await tall.find({ type: 'Raster' })
  expect(tabsAt(t)).toBe(blanks + 2)
  expect(blanks + 1 + Number(raster?.props?.rows)).toBeLessThanOrEqual(58)
  expect(blanks).toBeGreaterThan(0)
  await tall.unmount()
  const short = await $.ui.mount(target(8))
  expect(await short.find({ type: 'Raster' })).toBe(undefined)
  expect((await short.find({ type: 'Text', text: /^Ferris$/ })) !== undefined).toBe(true)
})

test('pane-view arithmetic: window, clamps and the cell clip', () => {
  const st = newPaneScroll()
  expect(windowOf(3, 12, true, st)).toMatchObject({ area: 3, pad: 7, overflow: false })
  expect(windowOf(3, 12, false, st)).toMatchObject({ pad: 0 })
  expect(windowOf(30, 12, true, st)).toMatchObject({ area: 9, top: 21, above: 21, below: 0, overflow: true })
  expect(scrolled(st, 30, { by: -1, bodyRows: 12, contentRows: 13 })).toEqual({ bottom: false, top: 20 })
  expect(scrolled(st, 30, { by: -12, bodyRows: 12, contentRows: 13 })).toEqual({ bottom: false, top: 12 })
  expect(scrolled(st, 30, { by: -13, bodyRows: 12, contentRows: 13 })).toEqual({ bottom: false, top: 0 })
  expect(scrolled({ bottom: false, top: 0 }, 30, { by: 13, bodyRows: 12, contentRows: 13 })).toEqual({ bottom: true, top: 21 })
  expect(scrolled(st, 5, { by: -1, bodyRows: 12, contentRows: 5 })).toBe(st)
  expect(clipCells('你好嗎', 5)).toBe('你好')
})

test('the sprite that does not fit with its rule row drops the rule and keeps the Raster; if it still does not fit, the stats alone without art', async ($, on) => {
  world(on, { pet: { ...PET_SNAP, pet: { ...PET_SNAP.pet, strategy: null } } })
  await start($)
  const tall = await $.ui.mount(target(60))
  const rows = Number((await tall.find({ type: 'Raster' }))?.props?.rows)
  await tall.unmount()
  // the control rows + the sprite block exactly (11 for a 9-row sprite): no room left for the rule row
  const fit = await $.ui.mount(target(rows + 2))
  const t = await fit.drawn()
  expect(Number((await fit.find({ type: 'Raster' }))?.props?.rows)).toBe(rows)
  expect(rowsOf(t).some(r => /▾ Pet/.test(r))).toBe(false)
  expect(kids(t).length).toBeLessThanOrEqual(rows + 2)
  expect(rowsOf(t).some(r => /^(↑ \d+ more|↓ \d+)/.test(r))).toBe(false)
  await fit.unmount()
  // bodyRows 6: not even the block fits: stats only, no Raster, no ASCII art, no overflow row
  const small = await $.ui.mount(target(6))
  const s = await small.drawn()
  expect(await small.find({ type: 'Raster' })).toBe(undefined)
  expect(await small.find({ type: 'Text', text: /_~\^~\^~_/ })).toBe(undefined)
  expect(rowsOf(s).some(r => /^(↑ \d+ more|↓ \d+)/.test(r))).toBe(false)
  expect(kids(s).length).toBeLessThanOrEqual(6)
  expect((await small.find({ type: 'Text', text: /^Ferris$/ })) !== undefined).toBe(true)
})

// the arrows and page keys reach only a pane that holds the keyboard (§13 rule 7); the wheel needs no focus.
// `focus` is granted only over an empty composer, so the command asks before its first wait.
test('/forge-stats opens the pane asking for the keyboard, before it reads anything', async ($, on) => {
  const log: string[] = []
  world(on, { usage: summary(3), log })
  await start($)
  const run = (args: string) => $.command.run({ command: 'forge-stats', args, origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } } as never)
  for (const args of ['', 'pet']) {
    log.length = 0
    await run(args)
    expect(log[0]).toBe('open focus=true')
  }
  log.length = 0
  await run('usage')
  // the open comes first: the usage tab's quota re-read waits on it
  expect(log[0]).toBe('open focus=true')
  expect(log.includes('stat quota')).toBe(true)
  const ui = await $.ui.mount(target(12))
  expect((await ui.find({ type: 'Text', text: /acct-a/ })) !== undefined).toBe(true)
  await ui.unmount()
})
