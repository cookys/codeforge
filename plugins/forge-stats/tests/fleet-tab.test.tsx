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
const acct = (o: Record<string, unknown>) => ({ provider: 'claude', hosts: ['gpu-a'], observed_at_ms: Date.now(), age_s: 0, state: 'ok', weekly: null, five_hour: null, name: o.alias, ...o })
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
    acct({ alias: 'waste', weekly: wk({ pct: 0.5, resets_at: s() + 30 * 3600 + 600, warn: 'waste' }), five_hour: { pct: 0.2, resets_at: s() + 7200 } }),
    acct({ alias: 'short', weekly: wk({ pct: 0.6, projected: 1.4, warn: 'short', exhaust_at: s() + 2 * 86400 + 5 * 3600 + 600 }), five_hour: { pct: 0.91, resets_at: s() + 3600 } }),
    acct({ alias: 'tight', hosts: ['gpu-a', 'pm'], observed_at_ms: Date.now() - 45 * 60_000, age_s: 2700, weekly: wk({ pct: 0.91, resets_at: s() + 4 * 86400 + 3600 * 5 + 600 }), five_hour: { pct: 0.1, resets_at: s() + 600 } }),
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
  expect(rows[1]).toMatch(/^will waste · waste · week 50% · resets ◷ 1d6h · target 85%$/)
  expect(rows[2]).toMatch(/^will fall short \(est\.\) · short · week 60% · est\. out in 2d5h · resets ◷ 2d23h$/)
  const body = rows.slice(3)
  expect(body.find(r => r.startsWith('waste'))).toMatch(/^waste +week 50% ◷ 1d6h {2}5h 20% {2}gpu-a$/)
  expect(body.find(r => r.startsWith('short'))).toMatch(/^short +week 60% ◷ 2d23h {2}5h 91% tight {2}gpu-a$/)
  expect(body.find(r => r.startsWith('tight'))).toMatch(/^tight +week 91% tight ◷ 4d5h {2}5h 10% {2}gpu-a,pm {2}45m ago$/)
  expect(body.find(r => r.startsWith('codeforge') || r.startsWith('codex'))).toMatch(/^codex +no source {2}gpu-a$/)
  expect(body.find(r => r.startsWith('old-acct'))).toMatch(/^old-acct +expired {2}gpu-a {2}8h0m ago$/)
  await ui.unmount()
})

test('zh-TW draws the tab with the CLI wording', async ($, on) => {
  world(on, { fleet: JSON.stringify(FIXTURE()), zh: true })
  const ui = await open($)
  expect(await strip(ui)).toEqual(['寵物', '用量', '全部帳號'])
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn())
  expect(rows.some(r => /^會浪費 · waste · 週 50% · 重置 ◷ 1d6h · 目標 85%$/.test(r))).toBe(true)
  expect(rows.some(r => /^會不夠 估 · short · 週 60% · 估 2d5h 後用完 · 重置 ◷ 2d23h$/.test(r))).toBe(true)
  expect(rows.some(r => /^codex +無來源/.test(r))).toBe(true)
  expect(rows.some(r => /^old-acct +過期/.test(r))).toBe(true)
  await ui.unmount()
})

test('the warnings come from the JSON: a stale file is shown as it is, not re-evaluated', async ($, on) => {
  // the weekly window of this account reset an hour ago: the JSON still says waste, and it is still drawn as written
  const f = fleet([acct({ alias: 'old', weekly: wk({ pct: 0.1, resets_at: s() - 3600, warn: 'waste' }) })], { generated_at_ms: Date.now() - 5 * 3600_000 })
  world(on, { fleet: JSON.stringify(f) })
  const ui = await open($)
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn())
  expect(rows.some(r => /^▾ All accounts +merged 300 min ago/.test(r))).toBe(true)
  expect(rows.some(r => /^will waste · old · week 10%/.test(r))).toBe(true)
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
    expect(rows.some(r => /waste/.test(r))).toBe(true)
    await ui.unmount()
  }
})

test('the name labels the row, then the email; an acct- hash never shows; a writer without name falls back', async ($, on) => {
  const f = fleet([
    acct({ alias: 'acct-11111111', name: 'cookys', email: 'me@example.com', org: 'personal', weekly: wk({ pct: 0.2 }) }),
    acct({ alias: 'acct-22222222', name: 'acme', email: null, org: 'Acme 研發', weekly: wk({ pct: 0.3 }) }),
    acct({ alias: 'acct-33333333', name: undefined, email: null, org: null, weekly: wk({ pct: 0.4 }) }),
    acct({ alias: 'acct-44444444', name: undefined, email: 'old@example.com', org: 'Acme', weekly: wk({ pct: 0.5 }) }),
    acct({ alias: 'acct-55555555', name: undefined, email: null, org: 'Beta', weekly: wk({ pct: 0.6 }) }),
    acct({ alias: 'codex', name: undefined, provider: 'codex', weekly: wk({ pct: 0.7 }) }),
  ])
  world(on, { fleet: JSON.stringify(f), zh: true })
  const ui = await open($)
  await ui.press({ key: 'tab-fleet' })
  const rows = rowsOf(await ui.drawn())
  expect(rows.some(r => /^cookys · me@example\.com +週 20%/.test(r))).toBe(true)
  expect(rows.some(r => /^acme +週 30%/.test(r))).toBe(true)
  expect(rows.some(r => r.includes('週 40%'))).toBe(false)
  expect(rows.some(r => /^old@example\.com +週 50%/.test(r))).toBe(true)
  expect(rows.some(r => /^Beta +週 60%/.test(r))).toBe(true)
  expect(rows.some(r => /^codex +週 70%/.test(r))).toBe(true)
  expect(rows.some(r => r.includes('acct-'))).toBe(false)
  await ui.unmount()
})

test('the name and email label every row inside the pane', async ($, on) => {
  const long = 'x'.repeat(240) + '@example.com'
  const f = fleet([
    acct({ alias: 'acct-11111111', name: 'cookys', email: 'me@example.com', org: 'personal', weekly: wk({ pct: 0.2 }) }),
    acct({ alias: 'acct-22222222', name: 'acme', email: 'dev@example.com', org: 'Acme', weekly: wk({ pct: 0.3, warn: 'waste' }) }),
    acct({ alias: 'acct-44444444', name: 'acme·x', email: long, org: 'Acme', weekly: wk({ pct: 0.57 }) }),
  ])
  world(on, { fleet: JSON.stringify(f), zh: true })
  for (const bodyColumns of [42, 52, 72, 102]) {
    const ui = await open($, bodyColumns)
    await ui.press({ key: 'tab-fleet' })
    const rows = rowsOf(await ui.drawn())
    for (const r of rows) expect(displayWidth(r) <= bodyColumns - 2).toBe(true)
    if (bodyColumns >= 52) {
      expect(rows.some(r => /^cookys · me@example\.com +週 20%/.test(r))).toBe(true)
      expect(rows.some(r => /^acme · dev@example\.com +週 30%/.test(r))).toBe(true)
      expect(rows.some(r => /^會浪費 · acme · dev@example\.com/.test(r))).toBe(true)
    }
    // the 254-column email never pushes the weekly figure off its line
    expect(rows.some(r => /^acme·x.*週 57%/.test(r))).toBe(true)
    expect(rows.some(r => r.includes('acct-'))).toBe(false)
    await ui.unmount()
  }
})

// the real failure: one account on two hosts; the newer capture is a weekly that has reset, the older one still says 97 %
const REAL = () =>
  fleet([
    acct({
      alias: 'acct-65d2bffc',
      name: 'stranity',
      org: 'Stranity',
      hosts: ['4124gs', 'c4140'],
      old_hosts: ['c4140'],
      weekly: { pct: 0.04, resets_at: s() - 360, projected: null, warn: null, exhaust_at: null, reset: true },
      five_hour: { pct: 0.25, resets_at: s() + 13000, reset: false },
    }),
  ])

for (const [zh, reset, old, none] of [
  [true, /週 已重置/, /4124gs,c4140 舊/, '目前沒有會浪費或會不夠的帳號'],
  [false, /week reset/, /4124gs,c4140 old/, 'no account will waste or fall short'],
] as const) {
  test(`an old host and a reset window (${zh ? 'zh-TW' : 'en'}): 舊 / old and 已重置 / reset, never the old pct, no warning`, async ($, on) => {
    world(on, { fleet: JSON.stringify(REAL()), zh })
    for (const bodyColumns of [42, 62, 82]) {
      const ui = await open($, bodyColumns)
      await ui.press({ key: 'tab-fleet' })
      const rows = rowsOf(await ui.drawn())
      for (const r of rows) expect(displayWidth(r) <= bodyColumns - 2).toBe(true)
      expect(rows.some(r => r.includes('4%') || r.includes('97%'))).toBe(false)
      if (bodyColumns >= 82) {
        const line = rows.find(r => r.startsWith('stranity'))
        expect(line).toMatch(reset)
        expect(line).toMatch(old)
        expect(rows.some(r => r.includes(none))).toBe(true)
      }
      await ui.unmount()
    }
  })
}

test('parseFleetQuota reads old_hosts and reset, and refuses a bad one', () => {
  const v = parseFleetQuota(JSON.stringify(REAL()))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts[0]?.oldHosts).toEqual(['c4140'])
  expect(v.accounts[0]?.weekly?.reset).toBe(true)
  expect(v.accounts[0]?.fiveHour?.reset).toBe(false)
  // absent from an older file: no old hosts, not reset
  const old = parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', weekly: wk({}) })])))
  if (old.status !== 'ok') throw new Error('not ok')
  expect(old.accounts[0]?.oldHosts).toEqual([])
  expect(old.accounts[0]?.weekly?.reset).toBe(false)
  for (const bad of [
    acct({ alias: 'a', old_hosts: 'c4140' }),
    acct({ alias: 'a', old_hosts: ['nope'] }),
    acct({ alias: 'a', old_hosts: ['gpu-a\u0007'], hosts: ['gpu-a\u0007'] }),
    acct({ alias: 'a', weekly: wk({ reset: 'yes' }) }),
    acct({ alias: 'a', five_hour: { pct: 0.1, resets_at: 5, reset: 1 } }),
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([bad]))).status).toBe('bad')
})

const BAD_CASES: [string, string, Record<string, unknown>][] = [
  ['invalid JSON', '{', {}],
  ['another schema', JSON.stringify({ ...fleet([]), schema: 'other/1' }), {}],
  ['an unknown key', JSON.stringify({ ...fleet([]), extra: 1 }), {}],
  ['an unknown state', JSON.stringify(fleet([acct({ alias: 'a', state: 'weird' })])), {}],
  ['a bad window', JSON.stringify(fleet([acct({ alias: 'a', weekly: { pct: 'x', resets_at: 1 } })])), {}],
  ['an org with a control char', JSON.stringify(fleet([acct({ alias: 'a', org: 'x\u0007y' })])), {}],
  ['an email without an @', JSON.stringify(fleet([acct({ alias: 'a', email: 'nobody' })])), {}],
  ['an email with a space', JSON.stringify(fleet([acct({ alias: 'a', email: 'a b@example.com' })])), {}],
  ['an email over 254', JSON.stringify(fleet([acct({ alias: 'a', email: 'x'.repeat(250) + '@example.com' })])), {}],
  ['an email that is not a string', JSON.stringify(fleet([acct({ alias: 'a', email: 5 })])), {}],
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
