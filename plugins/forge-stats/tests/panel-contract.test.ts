// Writer half of the shared panel golden. The sibling golden.test.js exercises the reader,
// including malformed input that the writer never produces. JSON crosses $.state unchanged.
import { expect, test } from 'claude-code/testing'
import { PANEL_SCHEMA, PANEL_TIERS, panelLine, panelState, panelTiers } from '../hooks/panel'
import { SIDE_USAGE_W, SIDE_MEMORY_W, sideMemoryLines, sideState } from '../hooks/side'
import { petIcon } from '../hooks/pet-icon'
import { makeT } from '../hooks/strings'
import { PANEL_TONES } from '../vendor/tux/contracts/panel'
import { TUX_GOLDEN } from './tux-golden-data'
import type { PanelSide } from '../types'

const doc = TUX_GOLDEN['panel-cases']
const example = (name: string): any => {
  const found = doc.cases.find(c => c.name === name)
  if (!found) throw new Error('missing panel golden: ' + name)
  return found
}
const wire = <T>(value: T): T => JSON.parse(JSON.stringify(value))
const emptyPet = { title: 'Pet', sprite: null, tiers: {} }
const env = { term: 'xterm-256color', lcAll: undefined, lcCtype: undefined, lang: 'en_US.UTF-8', pick: undefined }
const noUsage = { status: 'missing' as const, generatedAt: null, day: '2026-11-20', accounts: [], models: [] }

test('panel writer schema, tones and tiers equal the shared golden contract', () => {
  expect(PANEL_SCHEMA).toBe(doc.schema)
  expect(PANEL_TIERS).toEqual(doc.tierKeys.panel)
  expect(SIDE_USAGE_W).toEqual(doc.tierKeys.side)
  expect(SIDE_MEMORY_W).toEqual(doc.tierKeys.side)
  expect(PANEL_TONES).toEqual(doc.tones)
  expect(Object.keys(panelTiers(() => [[{ text: 'ok' }]]))).toEqual(doc.tierKeys.panel.map(String))
  const styled = panelLine(['structure', 'idle', 'dim', 'bold', 'mono'].map(role => ({ text: role, role: role as never })))
  const success = sideMemoryLines({ memory: 'hidden', active: 0, brain: 'ok' }, makeT('en'))
  const tones = [...styled, ...success.flat()].flatMap(p => p.tone ? [p.tone] : [])
  expect([...new Set(tones)].sort()).toEqual([...doc.tones].sort())
})

test('normal panel writer fields equal the golden usage and side fields', () => {
  const c = example('ok')
  const got = wire(panelState(c.input.updated_at_ms, { 40: [panelLine([{ text: 'ok' }])] }, emptyPet, c.input.side as PanelSide))
  expect(got.schema).toBe(c.input.schema)
  expect(got.updated_at_ms).toBe(c.input.updated_at_ms)
  expect(got.usage.tiers).toEqual(c.expect.usage)
  expect(got.side?.usage.tiers).toEqual(c.expect.side.usage)
})

test('pet writer output equals the golden pet-icon payload, including the entire tuple', () => {
  const c = example('pet-icon')
  const got = wire(panelState(1000, {}, {
    title: 'cat', sprite: null, tiers: { 40: [panelLine([{ text: 'hp' }])] }, icon: petIcon(env),
  }))
  expect(got.pet).toEqual(c.input.pet)
  expect(got.pet).toEqual(c.expect.pet)
})

test('an unknown writer role cannot introduce the golden unknown-tone warning', () => {
  const c = example('unknown-tone')
  const got = wire(panelLine([{ text: c.input.usage.tiers['40'][0][0].text, role: 'warning' as never }]))
  expect(got).toEqual(c.expect.usage['40'][0])
})

test('writer fills the pet fields absent in the missing-fields reader case', () => {
  const c = example('missing-fields')
  const got = wire(panelState(1000, {}, emptyPet))
  expect(got.schema).toBe(c.input.schema)
  expect(got.pet).toEqual(emptyPet)
  expect(Object.keys(got.pet).sort()).toEqual(['sprite', 'tiers', 'title'])
  expect('pet' in c.input).toBe(false)
})

test('writer rejects the golden side poison and emits the golden clean usage line', () => {
  const c = example('usage-side-poison')
  const poison = { ...noUsage, status: 'ok' as const, models: [{ model: '$2', tokens: 1, requests: 1, cost: null, unpriced: 1 }], accounts: [{ alias: '$2', name: '$2', writtenAt: 1000, fiveHour: { pct: 0.2, resetsAtMs: 10000 }, sevenDay: null }] }
  const side = sideState(poison, null, null, 1000, makeT('en'))
  expect(side).toBe(c.expect.side)
  const got = wire(panelState(1000, { 40: [panelLine([{ text: 'keep' }])] }, emptyPet, side))
  expect(got.usage.tiers).toEqual(c.expect.usage)
  expect('side' in got).toBe(false)
})
