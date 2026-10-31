import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { DEFAULT_COLOR, halfBlockCells, LOWER_HALF, parseSnapshot, parseSprite, SPACE, sparkline, staleMinutes, UPPER_HALF } from '../hooks/model'
import { FERRIS } from './ferris-sprite'

const PANE = {
  plugin: 'forge-stats',
  component: 'Pane',
  requestId: 'forge-stats',
  props: {
    title: 'Forge stats',
    isFocused: false,
    bodyColumns: 60,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 40 },
    view: {},
  },
} as const

// What `codeforge statusline` writes to <live_base>/codeforge/pet.json (src/pet/view.rs).
const SNAP = {
  schema: 'codeforge.pet-view/1',
  written_at: new Date().toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: 'explorer', message: null },
  village: {
    id: 'rust',
    display_name: 'The Forge-Ruins',
    language: 'Rust',
    tagline: '所有權是責任，錯誤是公民',
    rgb: [215, 95, 0],
    art_small: [' _~^~^~_'],
    art_full: ['   _~^~^~_', '\\) / o o \\ (/', "  '_  -  _'"],
  },
}
const NO_PET = { schema: 'codeforge.pet-view/1', written_at: new Date().toISOString(), pet: null, village: null }
const LIVE = '/run/user/1000/autopilot'

const spin = (ms: number) => {
  const until = performance.now() + ms
  while (performance.now() < until) {}
}

// The engine stands beneath: HOME, the autopilot live pointer, and the snapshot file.
const base = (on: Parameters<TestBody>[1], snapshot: unknown | null) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : undefined }))
  on('fs.read', (_, e) => {
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path === LIVE + '/codeforge/pet.json' && snapshot !== null) return { value: JSON.stringify(snapshot) }
    throw new Error('ENOENT ' + e.path)
  })
}

test('parseSnapshot reads pet and village, and names each failure', () => {
  const ok = parseSnapshot(JSON.stringify(SNAP), 'p', Date.now())
  expect(ok.status).toBe('ok')
  expect(ok.pet?.name).toBe('Ferris')
  expect(ok.pet?.level).toBe(3)
  expect(ok.village?.art_full[0]).toBe('   _~^~^~_')
  expect(parseSnapshot(JSON.stringify(NO_PET), 'p', 0).status).toBe('no-pet')
  expect(parseSnapshot(null, 'p', 0).status).toBe('missing')
  expect(parseSnapshot('{', 'p', 0).status).toBe('bad')
  expect(parseSnapshot(JSON.stringify({ ...SNAP, schema: 'other/9' }), 'p', 0).status).toBe('bad')
})

test('a snapshot older than ten minutes reports its age', () => {
  const now = Date.parse('2026-10-31T01:00:00Z')
  const view = parseSnapshot(JSON.stringify({ ...SNAP, written_at: '2026-10-31T00:30:00Z' }), 'p', now)
  expect(staleMinutes(view, now)).toBe(30)
  expect(staleMinutes(parseSnapshot(JSON.stringify({ ...SNAP, written_at: '2026-10-31T00:59:00Z' }), 'p', now), now)).toBe(null)
})

test('sparkline scales to the largest value', () => {
  expect(sparkline([0, 50, 100])).toBe('▁▅█')
  expect(sparkline([null, 10])).toBe('▁█')
})

test('a streamed request is timed and shown with its tok/s', async ($, on) => {
  base(on, SNAP)
  // Stands for the API: first chunk, ~250 ms of decoding, last chunk.
  on('turn.step', async function* () {
    yield { kind: 'text', index: 0, text: 'a' } as const
    spin(250)
    yield { kind: 'text', index: 0, text: 'b' } as const
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
  for await (const _ of stream) {
  }
  await stream.result

  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    const row = await ui.find({ type: 'Text', text: /opus-5-5/ })
    const tps = Number(row?.text.trim().split(/\s+/)[0])
    // 50 tokens over >= 250 ms: at most 200 tok/s, and not wildly lower.
    expect(tps).toBeGreaterThan(50)
    expect(tps).toBeLessThanOrEqual(200)
    const name = await ui.find({ type: 'Text', text: /^Ferris$/ })
    expect(name?.props.bold).toBe(true)
    expect((await ui.find({ type: 'Text', text: /Lv\.3/ })) !== undefined).toBe(true)
    // XP meter: 25% of the bar on, the rest drawn in the idle colour
    const off = await ui.find({ type: 'Text', text: /^▱+$/ })
    expect(off?.props.color).toBe('inactive')
    await ui.unmount()
  }
})

test('a snapshot with no pet says so, dim', async ($, on) => {
  base(on, NO_PET)

  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect((await ui.find({ type: 'Text', text: /no requests timed yet|還沒有計時/ })) !== undefined).toBe(true)
  const err = await ui.find({ type: 'Text', text: /codeforge adopt/ })
  expect(err?.props.dimColor).toBe(true)
  await ui.unmount()
})

test('no snapshot file at all is shown as no snapshot yet', async ($, on) => {
  base(on, null)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect((await ui.find({ type: 'Text', text: /statusline/ }))?.props.dimColor).toBe(true)
  await ui.unmount()
})

// ---------------------------------------------------------------------------------------------
// pixel sprite (codeforge doc/plans/2026-10-31-pixel-pet.md §2.5)

const SPRITE_SNAP = { ...SNAP, village: { ...SNAP.village, sprite: FERRIS } }

// Raster cells → [codePoint, fg, bg] per cell
const decode = (cells: string): [number, number, number][] => {
  const bin = atob(cells)
  const view = new DataView(new Uint8Array([...bin].map(c => c.charCodeAt(0))).buffer)
  const out: [number, number, number][] = []
  for (let o = 0; o < view.byteLength; o += 12) out.push([view.getUint32(o, true), view.getUint32(o + 4, true), view.getUint32(o + 8, true)])
  return out
}

test('parseSprite takes the Ferris sprite and refuses each contract break', () => {
  const ok = parseSprite(FERRIS)
  expect(ok?.width).toBe(24)
  expect(ok?.frames.length).toBe(2)
  expect(ok?.palette['o']).toBe(0xd75f00)
  const bad = (patch: Record<string, unknown>) => parseSprite({ ...FERRIS, ...patch })
  expect(bad({ height: 23 })).toBe(null)
  expect(bad({ height: 22, frames: FERRIS.frames.map(f => f.slice(0, 22)) })).toBe(null)
  expect(bad({ width: 23, frames: FERRIS.frames.map(f => f.map(r => r.slice(0, 23))) })).toBe(null)
  expect(bad({ transparent: ' ', frames: FERRIS.frames.map(f => f.map(r => r.replaceAll('.', ' '))) })).toBe(null)
  expect(bad({ palette: { ...FERRIS.palette, é: [0, 0, 0] } })).toBe(null)
  expect(bad({ frame_ms: 100 })).toBe(null)
  expect(bad({ frames: [FERRIS.frames[0]] })).toBe(null)
  expect(bad({ palette: { ...FERRIS.palette, o: [200, 95, 0] } })).toBe(null)
  expect(bad({ frames: [FERRIS.frames[0], (FERRIS.frames[1] ?? []).map(r => r.replace('o', 'z'))] })).toBe(null)
  expect(parseSprite(null)).toBe(null)
  expect(parseSprite(undefined)).toBe(null)
})

test('a frame becomes half-block cells with the right colours', () => {
  const sprite = parseSprite(FERRIS)!
  const { columns, rows, cells } = halfBlockCells(sprite, 0)
  expect(columns).toBe(24)
  expect(rows).toBe(12)
  const all = decode(cells)
  expect(all.length).toBe(24 * 12)
  const glyphs = new Set(all.map(c => c[0]))
  for (const g of glyphs) expect([UPPER_HALF, LOWER_HALF, SPACE].includes(g)).toBe(true)
  expect(glyphs.has(UPPER_HALF)).toBe(true)
  // Check every cell against the pixel pair it stands for.
  const f = sprite.frames[0]!
  const col = (ch: string) => (ch === '.' ? null : sprite.palette[ch]!)
  all.forEach(([cp, fg, bg], i) => {
    const x = i % 24
    const y = Math.floor(i / 24)
    const t = col(f[y * 2]![x]!)
    const b = col(f[y * 2 + 1]![x]!)
    if (t !== null && b !== null) expect([cp, fg, bg]).toEqual([UPPER_HALF, t, b])
    else if (t !== null) expect([cp, fg, bg]).toEqual([UPPER_HALF, t, DEFAULT_COLOR])
    else if (b !== null) expect([cp, fg, bg]).toEqual([LOWER_HALF, b, DEFAULT_COLOR])
    else expect([cp, fg, bg]).toEqual([SPACE, DEFAULT_COLOR, DEFAULT_COLOR])
  })
  // a known pixel pair: cell row 2 col 1 (pixel rows 4/5) is shell over shell
  const shell = all[2 * 24 + 1]!
  expect(shell).toEqual([UPPER_HALF, 0xd75f00, 0xd75f00])
})

test('the pane draws the sprite as a Raster in the terminal, ASCII elsewhere', async ($, on) => {
  base(on, SPRITE_SNAP)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  const r = await ui.find({ type: 'Raster' })
  expect(r?.props.columns).toBe(24)
  expect(r?.props.rows).toBe(12)
  expect(r?.props.cells).toBe(halfBlockCells(parseSprite(FERRIS)!, 0).cells)
  expect(await ui.find({ type: 'Text', text: /_~\^~\^~_/ })).toBe(undefined)
  expect((await ui.find({ type: 'Text', text: /^Ferris$/ }))?.props.bold).toBe(true)
  await ui.unmount()
  const desk = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await desk.find({ type: 'Raster' })).toBe(undefined)
  expect((await desk.find({ type: 'Text', text: /_~\^~\^~_/ })) !== undefined).toBe(true)
  await desk.unmount()
})

test('a snapshot without a sprite key stays ok and draws ASCII', async ($, on) => {
  expect(parseSnapshot(JSON.stringify(SNAP), 'p', Date.now()).status).toBe('ok')
  expect(parseSnapshot(JSON.stringify(SNAP), 'p', Date.now()).village?.sprite).toBe(null)
  const broken = { ...SNAP, village: { ...SNAP.village, sprite: { ...FERRIS, height: 23 } } }
  expect(parseSnapshot(JSON.stringify(broken), 'p', Date.now()).status).toBe('ok')
  base(on, SNAP)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect(await ui.find({ type: 'Raster' })).toBe(undefined)
  expect((await ui.find({ type: 'Text', text: /_~\^~\^~_/ })) !== undefined).toBe(true)
  await ui.unmount()
})

test('the idle animation blits the next frame every 500 ms, and only on change', async ($, on) => {
  base(on, SPRITE_SNAP)
  const blits: string[] = []
  on('ui.blit', (_, e) => {
    if ('cells' in e) blits.push(e.cells)
    return { value: {} }
  })
  const clock = mock.clock(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  const sprite = parseSprite(FERRIS)!
  const first = halfBlockCells(sprite, 0).cells
  await clock.advance(500)
  expect(blits.length).toBe(1)
  expect(blits[0]).toBe(halfBlockCells(sprite, 1).cells)
  expect(blits[0] !== first).toBe(true)
  await clock.advance(500)
  expect(blits.length).toBe(2)
  expect(blits[1]).toBe(first)
  await ui.unmount()
})

test('a new session starts the animation over from frame 0', async ($, on) => {
  base(on, SPRITE_SNAP)
  const blits: string[] = []
  on('ui.blit', (_, e) => {
    if ('cells' in e) blits.push(e.cells)
    return { value: {} }
  })
  const clock = mock.clock(on)
  const sprite = parseSprite(FERRIS)!
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  await clock.advance(500) // first session leaves the animation on frame 1
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect((await ui.find({ type: 'Raster' }))?.props.cells).toBe(halfBlockCells(sprite, 0).cells)
  await ui.unmount()
})
