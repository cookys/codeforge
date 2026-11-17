// Pane v2 replaces the former padding and Usage-tab scroll contract.
import { expect, test } from 'claude-code/testing'
import { clipCells, newPaneScroll, scrolled, windowOf } from '../hooks/pane-view'
import { displayWidth } from '../hooks/model'
import { BRIEF, SUMMARY, command, paneTarget, runtimeWorld, start, topRows } from './fixtures/pane-runtime'

const runCases = [12, 18, 30]
for (const bodyRows of runCases) test(`content height ${bodyRows}: dock and inline have no padding and keep the controls last`, async ($, on) => {
  runtimeWorld(on)
  await start($)
  for (const placement of ['dock', 'inline'] as const) {
    const ui = await $.ui.mount(paneTarget(160, 40, bodyRows, placement))
    const rows = topRows(await ui.drawn())
    expect(rows.length).toBeLessThanOrEqual(Math.min(24, bodyRows))
    expect(rows.every(r => r.replace(/│/g, '').trim() !== '' && displayWidth(r) <= 158)).toBe(true)
    expect(rows[rows.length - 2]).toContain('分頁')
    expect(rows[rows.length - 1]).toContain('Esc 回輸入框')
    await ui.unmount()
  }
})

test('scrolling blocked and pools keeps both control rows on their rows; Home/End clamp, resize stays bounded', async ($, on) => {
  const w = runtimeWorld(on)
  const raw = JSON.parse(w.files[BRIEF])
  const source = raw.items.find((i: { kind: string }) => i.kind === 'stuck')
  for (let i = 0; i < 4; i++) raw.items.push({ ...source, id: 'overflow-' + i, subject: { display: `測試線-${i}（host-z）` } })
  raw.counts.stuck += 4
  w.files[BRIEF] = JSON.stringify(raw)
  await start($)
  const ui = await $.ui.mount(paneTarget(80, 24, 12))
  const before = topRows(await ui.drawn())
  const at = before.length - 2
  for (const key of ['key-j', 'key-u']) {
    await ui.press({ key })
    const rows = topRows(await ui.drawn())
    expect(rows.length - 2).toBe(at)
    expect(rows[at]).toContain('分頁')
    expect(rows[at + 1]).toContain('Esc 回輸入框')
  }
  await ui.press({ key: 'key-d' })
  for (const key of ['key-j', 'key-k', 'key-u', 'key-i']) expect(await ui.find({ type: 'Button', key })).not.toBe(undefined)
  expect(topRows(await ui.drawn()).slice(-1)[0]).toContain('待決卡在 autopilot pane：輸入 /autopilot')
  await ui.press({ key: 'key-i' })
  await ui.press({ key: 'key-i' })
  await ui.press({ key: 'key-u' })
  for (const by of [-1000, 1000, -1]) {
    await $.ui.scroll({ component: 'Pane', requestId: 'forge-stats', offset: 0, by, bodyRows: 12, contentRows: 100, origin: { kind: 'person' } })
    expect(topRows(await ui.drawn()).length - 2).toBe(at)
  }
  await ui.redraw(paneTarget(120, 30, 12).props)
  const rows = topRows(await ui.drawn())
  expect(rows.length).toBeLessThanOrEqual(12)
  expect(rows.every(r => displayWidth(r) <= 118 && r.trim() !== '')).toBe(true)
})

test('legacy usage opens overview; pet costs have at most six rows; focus is requested before reads', async ($, on) => {
  const w = runtimeWorld(on)
  await start($)
  w.log.length = 0
  await command($, 'usage')
  expect(w.log[0]).toBe('open:forge-stats:true')
  const ui = await $.ui.mount(paneTarget())
  expect(topRows(await ui.drawn()).join('')).toContain('▸總覽')
  await ui.press({ key: 'tab-pet' })
  expect(topRows(await ui.drawn()).join('')).toContain('▸寵物')
  const models = (await ui.findAll({ type: 'Text' })).filter(n => /model-/.test(n.text))
  expect(models).toHaveLength(6)
  expect(models[0].text).toContain('前 6/8')
  expect(await ui.find({ type: 'Raster' })).not.toBe(undefined)
  expect(topRows(await ui.drawn()).join('')).not.toMatch(/待決|smith1|⋯10/)
})

test('brief boundary rejects links and oversize before reading; malformed and missing data keep quota visible', async ($, on) => {
  const w = runtimeWorld(on, undefined, { isLink: true })
  await start($)
  const ui = await $.ui.mount(paneTarget())
  expect(w.log.includes('read:' + BRIEF)).toBe(false)
  expect(topRows(await ui.drawn()).join('')).toContain('fleet 摘要檔還沒出現（待決、卡住無法顯示）')
  expect(topRows(await ui.drawn()).join('')).toContain('smith1')
})

test('pane window starts at the highest priority row, clamps scroll, and never pads dock or inline', () => {
  const st = newPaneScroll()
  expect(windowOf(3, 12, true, st)).toMatchObject({ area: 3, pad: 0, top: 0, overflow: false })
  expect(windowOf(3, 12, false, st)).toMatchObject({ pad: 0 })
  expect(windowOf(30, 12, true, st)).toMatchObject({ area: 9, top: 0, below: 21, indicator: true })
  expect(scrolled(st, 30, { by: 1, bodyRows: 12, contentRows: 30 })).toEqual({ bottom: false, top: 1 })
  expect(scrolled(st, 30, { by: 30, bodyRows: 12, contentRows: 30 })).toEqual({ bottom: true, top: 21 })
  expect(scrolled({ bottom: true, top: 21 }, 30, { by: -30, bodyRows: 12, contentRows: 30 })).toEqual({ bottom: false, top: 0 })
  expect(clipCells('你好嗎', 5)).toBe('你好')
})
