import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { FERRIS_MINI } from './ferris-mini'
import { FERRIS } from './ferris-sprite'

// Shared-TUI plan P1b (§4.2): forge-stats has one layout, the band. The pane layout drew a tab row at the top of the
// window, above the chat; the tab row now lives only at the bottom (autopilot-live). A stored `layout: pane` must
// neither open the pane nor survive the session start, and `/forge-stats layout pane` must be refused.
// Restoring the pane layout turns these red.

const LIVE = '/run/user/1000/autopilot'
const SID = 'p1b'
const HB = LIVE + '/codeforge/mod-alive/p1b.json'
const SNAP = {
  schema: 'codeforge.pet-view/1',
  written_at: new Date().toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: null, message: null },
  village: { id: 'rust', display_name: 'The Forge-Ruins', language: 'Rust', tagline: '', rgb: [215, 95, 0], art_small: [' _~^~^~_'], art_full: ['   _~^~^~_'], sprite: FERRIS, sprite_mini: FERRIS_MINI },
}
const cmd = (args: string) => ({ command: 'forge-stats', args, origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } }) as const
const band = { plugin: 'forge-stats', component: 'AbovePrompt', requestId: 'band-1', surface: 'terminal', props: { hasSurvey: false, isWorking: false, maxRows: 20, bodyColumns: 100, scroll: { offset: 0, bodyRows: 19 }, view: {} } } as const

type World = { store: Record<string, unknown>; opened: number; beats: string[] }

function world(on: Parameters<TestBody>[1], layout?: string): World {
  const w: World = { store: layout === undefined ? {} : { layout }, opened: 0, beats: [] }
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
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.write', (_, e) => {
    if (e.path === HB) w.beats.push(e.text)
    return { value: undefined }
  })
  on('store.get', (_, e) => ({ value: w.store[e.key] }))
  on('store.set', (_, e) => {
    w.store[e.key] = e.value
    return { value: undefined }
  })
  on('ui.invalidate', () => ({ value: undefined }))
  on('ui.render', { component: 'AbovePrompt' }, ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box />
  })
  on('ui.blit', () => ({ value: {} }))
  return w
}

const start = { cwd: '/', surface: 'terminal', isInteractive: true } as const
const lastLayout = (w: World) => (w.beats.length ? (JSON.parse(w.beats[w.beats.length - 1]!) as { layout: string }).layout : null)

test('a stored pane layout is migrated to band at session start and the pane is not opened', async ($, on) => {
  const w = world(on, 'pane')
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start(start)
  await clock.settle()
  expect(w.opened).toBe(0)
  expect(w.store.layout).toBe('band')
  expect(lastLayout(w)).toBe('band')
  // the band draws its row (under the pane layout it passed to the plugin below, which draws only an empty Box)
  const ui = await $.ui.mount(band)
  expect((await ui.find({ type: 'Text' })) !== undefined).toBe(true)
  await ui.unmount()
})

test('a session with no stored layout is the band, opens nothing and stores nothing', async ($, on) => {
  const w = world(on)
  await $.session.start(start)
  expect(w.opened).toBe(0)
  expect(w.store.layout).toBe(undefined)
})

test('/forge-stats layout pane is refused: no pane, the stored layout and the beat stay band', async ($, on) => {
  const w = world(on)
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start(start)
  await clock.settle()
  const out = await $.command.run(cmd('layout pane'))
  expect(out.text?.includes('layout band')).toBe(true)
  expect(w.opened).toBe(0)
  expect(w.store.layout).toBe(undefined)
  await clock.advance(2000)
  expect(lastLayout(w)).toBe('band')
  const ok = await $.command.run(cmd('layout band'))
  expect(ok.text?.includes('band')).toBe(true)
  expect(w.store.layout).toBe('band')
})
