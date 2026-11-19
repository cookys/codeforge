// Shared fixtures are pinned by VENDOR.lock; tux-cross reruns against the sibling's golden dir.
// tabrow-* belongs to autopilot (its renderer is not vendored).
import { expect, test } from 'claude-code/testing'
import { TUX_GOLDEN } from './tux-golden-data'
import { GLYPHS } from '../vendor/tux/glyphs'
import { pct, pctInt, dur } from '../vendor/tux/format'
import { GLYPH, countdown, displayWidth } from '../hooks/model'

// scripts/tux-golden-data.py reads TUX_GOLDEN_DIR outside the sandbox.
const golden = <K extends keyof typeof TUX_GOLDEN>(name: K) => TUX_GOLDEN[name]

test('tux golden: GLYPHS equals glyphs.json', () => {
  expect(GLYPHS).toEqual(golden('glyphs'))
})

test('tux golden: forge-stats width matches emoji, CJK, Ambiguous, ZWJ, VS16 and ANSI cases', () => {
  for (const c of golden('width-cases').cases) expect(displayWidth(c.text)).toBe(c.width)
})

test('tux golden: pctInt and pct floor the pinned ratios', () => {
  for (const c of golden('pct-cases').cases) {
    expect(pctInt(c.ratio)).toBe(c.pctInt)
    expect(pct(c.ratio)).toBe(c.pct)
  }
})

test('tux golden: dur uses the pinned units and zero-padded minutes', () => {
  for (const c of golden('dur-cases').cases) expect(dur(c.ms ?? Number.NaN)).toBe(c.dur)
})

test('latest uses the vendor fresh meaning and is distinct from running', () => {
  const glyphs = golden('glyphs')
  expect(GLYPH.latest).toBe(glyphs.structure.fresh)
  expect(GLYPH.latest === glyphs.state.running).toBe(false)
  const meanings = Object.entries(glyphs.structure).filter(([, glyph]) => glyph === GLYPH.latest).map(([name]) => name)
  expect(meanings).toEqual(['fresh'])
})

test('forge-stats countdown uses tux duration formatting', () => {
  for (const c of golden('dur-cases').cases) {
    if (typeof c.ms === 'number') expect(countdown(c.ms, 0)).toBe(c.dur)
  }
})
