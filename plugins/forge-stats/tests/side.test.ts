// O4 band contract (doc/specs/panel-band-contract.md §2): the `side` of codeforge.panel/1, pure. Widths, no money, no ●, the mirror as the only source when present, fallbacks.
import { expect, test } from 'claude-code/testing'

import { displayWidth } from '../hooks/model'
import type { UsageView } from '../hooks/model'
import { SIDE_FULL_W, SIDE_MEMORY_W, SIDE_USAGE_W, sideAccounts, sideState } from '../hooks/side'
import type { QEntry } from '../hooks/quota-view'
import { makeT } from '../hooks/strings'
import type { PanelLine, PanelSide, SessionView } from '../types'

const NOW = 1793700000_000
const zh = makeT('zh-TW')
const text = (l: PanelLine) => l.map(p => p.text).join('')
const width = (l: PanelLine) => displayWidth(text(l))

const view = (o: Partial<UsageView> = {}): UsageView => ({
  status: 'ok',
  generatedAt: NOW,
  day: '2026-11-06',
  accounts: [
    { alias: 'work', writtenAt: NOW, fiveHour: { pct: 0.32, resetsAtMs: NOW + 2 * 3600_000 + 47 * 60_000 }, sevenDay: { pct: 0.56, resetsAtMs: NOW + 4 * 86400_000 } },
    { alias: 'home', writtenAt: NOW, fiveHour: { pct: 0.81, resetsAtMs: NOW + 3600_000 }, sevenDay: null },
  ],
  models: [
    { model: 'claude-opus-5-5', tokens: 7000, requests: 10, cost: 12.5, unpriced: 0 },
    { model: 'claude-sonnet-5-5', tokens: 2000, requests: 5, cost: 1.2, unpriced: 0 },
    { model: 'claude-haiku-5-5', tokens: 900, requests: 5, cost: 0.1, unpriced: 0 },
    { model: 'gpt-6-astra-with-a-very-long-model-name', tokens: 100, requests: 1, cost: null, unpriced: 1 },
  ],
  ...o,
})
const lights: SessionView = { memory: 'active', active: 12, brain: 'ok' }

const allLines = (side: PanelSide) => [side.usage, side.usage_full, side.memory].flatMap(b => Object.entries(b.tiers))

test('every line fits its key; the keys are the contract widths', () => {
  const side = sideState(view(), null, lights, NOW, zh) as PanelSide
  expect(Object.keys(side.usage.tiers)).toEqual(SIDE_USAGE_W.map(String))
  expect(Object.keys(side.usage_full.tiers)).toEqual(SIDE_FULL_W.map(String))
  expect(Object.keys(side.memory.tiers)).toEqual(SIDE_MEMORY_W.map(String))
  for (const [key, lines] of allLines(side)) for (const l of lines) expect(width(l) <= Number(key)).toBe(true)
  expect(side.usage.tiers['30'].length <= 3).toBe(true)
  expect(side.usage_full.tiers['40'].length <= 7).toBe(true)
  expect(side.memory.tiers['40'].length <= 2).toBe(true)
})

test('usage shows the most constrained account, then the others; the reset only where it fits', () => {
  const side = sideState(view(), null, lights, NOW, zh) as PanelSide
  const u30 = side.usage.tiers['30'].map(text)
  // home (81 %) is tighter than work (56 %): it leads, work is counted on the third line
  expect(u30[0].startsWith('5h ')).toBe(true)
  expect(u30[0].includes('81%')).toBe(true)
  expect(u30[1]).toBe('另 1 個帳號 · 最高 56%')
  const u50 = side.usage.tiers['50'].map(text)
  expect(u50[0].includes('後重置')).toBe(true)
})

test('usage_full: the windows, a blank line, the top three models by share; a long name is cut with …', () => {
  const full = (sideState(view(), null, lights, NOW, zh) as PanelSide).usage_full.tiers['40'].map(text)
  const blank = full.indexOf('')
  expect(blank > 0).toBe(true)
  const models = full.slice(blank + 1)
  expect(models.length).toBe(3)
  expect(models[0].startsWith('claude-opus-5-5')).toBe(true)
  expect(models[0].endsWith('70%')).toBe(true)
  expect(full.some(l => l.startsWith('gpt-6'))).toBe(false)
  const long = (sideState(view({ models: [{ model: 'gpt-6-astra-with-a-very-long-model-name', tokens: 5, requests: 1, cost: null, unpriced: 1 }] }), null, lights, NOW, zh) as PanelSide)
    .usage_full.tiers['40'].map(text)
  expect(long[long.length - 1].includes('…')).toBe(true)
  // wide characters are counted in display cells: cut and padded to the same width, never past the key
  for (const model of ['模型', '模型模型模型模型模型模型模型模型模型模型模型模型模型模型']) {
    const wide = (sideState(view({ models: [{ model, tokens: 5, requests: 1, cost: null, unpriced: 0 }] }), null, lights, NOW, zh) as PanelSide).usage_full.tiers['40']
    for (const l of wide) expect(width(l) <= 40).toBe(true)
  }
})

test('no money and no ● anywhere; a stray $ refuses the whole side', () => {
  const side = sideState(view(), null, lights, NOW, zh) as PanelSide
  const all = JSON.stringify(side)
  expect(all.includes('$')).toBe(false)
  expect(all.includes('●')).toBe(false)
  // the summary contract never lets a `$` into a model id; this one is forced past it to prove the guard
  const bad = view({ models: [{ model: 'm$x', tokens: 5, requests: 1, cost: null, unpriced: 0 }] })
  expect(sideState(bad, null, lights, NOW, zh)).toBe(null)
})

test('the mirror is the only source when present: its 5h / weekly windows, the summary accounts ignored', () => {
  const mirror = [
    {
      provider: 'anthropic',
      alias: 'm1',
      key: 'anthropic:m1',
      m: { windows: [{ kind: 'rolling_5h', utilization: 0.12, resetsAtLocal: NOW / 1000 + 600 }, { kind: 'weekly', utilization: 0.4, resetsAtLocal: null }, { kind: 'rpm', utilization: 0.9, resetsAtLocal: null }] },
    },
  ] as unknown as QEntry[]
  const accts = sideAccounts(view(), mirror)
  expect(accts.map(a => a.alias)).toEqual(['m1'])
  expect(accts[0].wins.map(w => w.label)).toEqual(['5h', '7d'])
  // an empty mirror shows no usage rather than falling back to the summary
  expect(sideAccounts(view(), [])).toEqual([])
})

test('fallbacks: no usage data, no models today, no session lights, hidden lights', () => {
  const side = sideState(view({ status: 'missing', accounts: [], models: [] }), null, null, NOW, zh) as PanelSide
  expect(side.usage.tiers['30'].map(text)).toEqual(['沒有用量資料'])
  expect(side.usage_full.tiers['40'].map(text)).toEqual(['沒有用量資料', '', '今天還沒有用量紀錄'])
  expect(side.memory.tiers['30'].map(text)).toEqual(['記憶 —'])
  const lit = (sideState(view(), null, lights, NOW, zh) as PanelSide).memory.tiers['40'].map(text)
  expect(lit).toEqual(['記憶 L1 12 條', '大腦 已連線'])
  const hidden = (sideState(view(), null, { memory: 'hidden', active: 0, brain: 'hidden' }, NOW, zh) as PanelSide).memory.tiers['40']
  expect(hidden).toEqual([])
})
