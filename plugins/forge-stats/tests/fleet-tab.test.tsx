// The 全部帳號 / all-accounts pane tab: shown only while `<base>/codeforge/usage/fleet-quota.json` exists, strict parse, warnings first, no recomputation.
import { expect, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { displayWidth } from '../hooks/model'
import { FLEET_MAX_BYTES, parseFleetQuota } from '../hooks/fleet-view'

const LIVE = '/run/user/1000/autopilot'
const FLEET = LIVE + '/codeforge/usage/fleet-quota.json'

type Node = { type: string; props?: Record<string, unknown>; children?: (Node | string)[] }
const textOf = (n: Node | string): string => (typeof n === 'string' ? n : (n.children ?? []).map(textOf).join(''))
const kids = (tree: unknown) => ((tree as Node).children ?? []) as Node[]
const rowsOf = (tree: unknown) => kids(tree).map(textOf)

const target = (bodyColumns: number) =>
  ({ plugin: 'forge-stats', component: 'Pane', requestId: 'forge-stats', surface: 'terminal', props: { title: 'Forge stats', isFocused: false, bodyColumns, placement: 'inline', scroll: { offset: 0, bodyRows: 60 }, view: {} } }) as const

const s = () => Math.floor(Date.now() / 1000)
const acct = (o: Record<string, unknown>) => ({ provider: 'claude', hosts: ['gpu-a'], observed_at_ms: Date.now(), age_s: 0, state: 'ok', weekly: null, five_hour: null, ...o })
const wk = (o: Record<string, unknown>) => ({ pct: 0.5, resets_at: s() + 3 * 86400, projected: 0.9, warn: null, exhaust_at: null, ...o })
const fleet = (accounts: unknown[], o: Record<string, unknown> = {}) => ({
  schema: 'codeforge.fleet-quota/1',
  generated_at_ms: Date.now() - 3 * 60_000,
  use_target: 0.85,
  skipped: {},
  accounts,
  ...o,
})
const FIXTURE = () =>
  fleet([
    acct({ alias: 'acct-waste', weekly: wk({ pct: 0.5, resets_at: s() + 30 * 3600 + 600, warn: 'waste' }), five_hour: { pct: 0.2, resets_at: s() + 7200 } }),
    acct({ alias: 'acct-short', weekly: wk({ pct: 0.6, projected: 1.4, warn: 'short', exhaust_at: s() + 2 * 86400 + 5 * 3600 + 600 }), five_hour: { pct: 0.91, resets_at: s() + 3600 } }),
    acct({ alias: 'acct-tight', hosts: ['gpu-a', 'pm'], observed_at_ms: Date.now() - 45 * 60_000, age_s: 2700, weekly: wk({ pct: 0.91, resets_at: s() + 4 * 86400 + 3600 * 5 + 600 }), five_hour: { pct: 0.1, resets_at: s() + 600 } }),
    acct({ alias: 'codex-1a2b3c4d', provider: 'codex', state: 'no_source', observed_at_ms: null, age_s: null }),
    acct({ alias: 'old-acct', state: 'expired', observed_at_ms: Date.now() - 8 * 3600_000, age_s: 8 * 3600 }),
  ])

const world = (on: Parameters<TestBody>[1], w: { fleet?: string | null; stat?: Record<string, unknown>; zh?: boolean }) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : e.name === 'CODEFORGE_LOCALE' && w.zh ? 'zh-TW' : undefined }))
  on('fs.stat', (_, e) => {
    if (e.path === FLEET && typeof w.fleet === 'string') return { value: { kind: 'file', size: w.fleet.length, mtimeMs: 0, isLink: false, ...w.stat } }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.read', (_, e) => {
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path === FLEET && typeof w.fleet === 'string') return { value: w.fleet }
    throw new Error('ENOENT ' + e.path)
  })
}
const open = async ($: Parameters<TestBody>[0], cols = 82) => {
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(target(cols))
  return ui
}
const strip = async (ui: Awaited<ReturnType<typeof open>>) => (await ui.findAll({ type: 'Button' })).map(b => textOf(b as Node))

test('without the file the strip is Pet | Usage and no frame mentions the fleet tab', async ($, on) => {
  world(on, { fleet: null })
  const ui = await open($)
  expect(await strip(ui)).toEqual(['Pet', 'Usage'])
  const frame = async () => JSON.stringify(await ui.drawn()).replace(/"handle":\d+/g, '"handle":0')
  const pet = await frame()
  await ui.press({ key: 'tab-usage' })
  expect(await strip(ui)).toEqual(['Pet', 'Usage'])
  expect(await frame()).not.toMatch(/All accounts|tab-fleet/)
  await ui.press({ key: 'tab-pet' })
  expect(await frame()).toBe(pet)
  expect(pet).not.toMatch(/All accounts|tab-fleet/)
  await ui.unmount()
})

test('with the file the strip gains the tab, and it draws warnings first, then one line per account', async ($, on) => {
  world(on, { fleet: JSON.stringify(FIXTURE()) })
  const ui = await open($, 92)
  expect(await strip(ui)).toEqual(['Pet', 'Usage', 'All accounts'])
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn()).filter(r => r.trim() !== '')
  expect(rows[0]).toMatch(/^▾ All accounts +merged 3 min ago ─+$/)
  expect(rows[1]).toMatch(/^will waste · acct-waste · week 50% · resets ◷ 1d6h · target 85%$/)
  expect(rows[2]).toMatch(/^will fall short \(est\.\) · acct-short · week 60% · est\. out in 2d5h · resets ◷ 2d23h$/)
  const body = rows.slice(3)
  expect(body.find(r => r.startsWith('acct-waste'))).toMatch(/^acct-waste +week 50% ◷ 1d6h {2}5h 20% {2}gpu-a$/)
  expect(body.find(r => r.startsWith('acct-short'))).toMatch(/^acct-short +week 60% ◷ 2d23h {2}5h 91% tight {2}gpu-a$/)
  expect(body.find(r => r.startsWith('acct-tight'))).toMatch(/^acct-tight +week 91% tight ◷ 4d5h {2}5h 10% {2}gpu-a,pm {2}45m ago$/)
  expect(body.find(r => r.startsWith('codeforge') || r.startsWith('codex'))).toMatch(/^codex-1a2b3c4d +no source {2}gpu-a$/)
  expect(body.find(r => r.startsWith('old-acct'))).toMatch(/^old-acct +expired {2}gpu-a {2}8h0m ago$/)
  await ui.unmount()
})

test('zh-TW draws the tab with the CLI wording', async ($, on) => {
  world(on, { fleet: JSON.stringify(FIXTURE()), zh: true })
  const ui = await open($)
  expect(await strip(ui)).toEqual(['寵物', '用量', '全部帳號'])
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn())
  expect(rows.some(r => /^會浪費 · acct-waste · 週 50% · 重置 ◷ 1d6h · 目標 85%$/.test(r))).toBe(true)
  expect(rows.some(r => /^會不夠 估 · acct-short · 週 60% · 估 2d5h 後用完 · 重置 ◷ 2d23h$/.test(r))).toBe(true)
  expect(rows.some(r => /^codex-1a2b3c4d +無來源/.test(r))).toBe(true)
  expect(rows.some(r => /^old-acct +過期/.test(r))).toBe(true)
  await ui.unmount()
})

test('the warnings come from the JSON: a stale file is shown as it is, not re-evaluated', async ($, on) => {
  // the weekly window of this account reset an hour ago: the JSON still says waste, and it is still drawn as written
  const f = fleet([acct({ alias: 'acct-old', weekly: wk({ pct: 0.1, resets_at: s() - 3600, warn: 'waste' }) })], { generated_at_ms: Date.now() - 5 * 3600_000 })
  world(on, { fleet: JSON.stringify(f) })
  const ui = await open($)
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn())
  expect(rows.some(r => /^▾ All accounts +merged 300 min ago/.test(r))).toBe(true)
  expect(rows.some(r => /^will waste · acct-old · week 10%/.test(r))).toBe(true)
  await ui.unmount()
})

test('a narrow pane (40 cols) keeps every row inside the width, CJK too', async ($, on) => {
  world(on, { fleet: JSON.stringify(FIXTURE()), zh: true })
  for (const bodyColumns of [42, 52, 72]) {
    const ui = await open($, bodyColumns)
    await ui.press({ key: 'tab-fleet' })
    const cols = bodyColumns - 2
    const rows = rowsOf(await ui.drawn())
    expect(rows.length > 8).toBe(true)
    for (const r of rows) expect(displayWidth(r) <= cols).toBe(true)
    expect(rows.some(r => /acct-waste/.test(r))).toBe(true)
    await ui.unmount()
  }
})

const BAD_CASES: [string, string, Record<string, unknown>][] = [
  ['invalid JSON', '{', {}],
  ['another schema', JSON.stringify({ ...fleet([]), schema: 'other/1' }), {}],
  ['an unknown key', JSON.stringify({ ...fleet([]), extra: 1 }), {}],
  ['an unknown state', JSON.stringify(fleet([acct({ alias: 'a', state: 'weird' })])), {}],
  ['a bad window', JSON.stringify(fleet([acct({ alias: 'a', weekly: { pct: 'x', resets_at: 1 } })])), {}],
  ['a link', JSON.stringify(FIXTURE()), { isLink: true }],
  ['an oversized file', JSON.stringify(FIXTURE()), { size: FLEET_MAX_BYTES + 1 }],
]
for (const [name, text, stat] of BAD_CASES) {
  test(`${name}: the tab draws the one dim unreadable line`, async ($, on) => {
    world(on, { fleet: text, stat, zh: true })
    const ui = await open($)
    await ui.press({ key: 'tab-fleet' })
    const rows = rowsOf(await ui.drawn()).filter(r => r.trim() !== '')
    expect(rows[1]).toBe('  全部帳號資料無法讀取')
    expect(rows.length).toBe(4) // rule, the line, tab strip, hint
    const line = await ui.find({ type: 'Text', text: /全部帳號資料無法讀取/ })
    expect(line?.props.dimColor).toBe(true)
    await ui.unmount()
  })
}

test('parseFleetQuota never throws and refuses what is not the schema', () => {
  expect(parseFleetQuota(null).status).toBe('missing')
  for (const t of ['', '[]', 'null', '{"schema":1}', '"x"']) expect(parseFleetQuota(t).status).toBe('bad')
  expect(parseFleetQuota(JSON.stringify(FIXTURE())).status).toBe('ok')
})
