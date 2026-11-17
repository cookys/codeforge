import { expect, test } from 'claude-code/testing'
import { overviewPlan } from '../hooks/overview-view'
import { parseBrief } from '../hooks/brief-view'
import { displayWidth } from '../hooks/model'
import type { FleetAccount } from '../hooks/fleet-view'

import { NOW, pools, assignments, input } from './fixtures/pane-sample'
const text = (p: ReturnType<typeof overviewPlan>) => p.lines.map(l => l.segs.map(s => s.text).join(''))

test('overview stays within 60%, has no blank rows, decisions first and one no-source row at every width', () => {
  for (const [cols, rows] of [[160, 40], [120, 30], [80, 24]]) {
    const p = overviewPlan(input(cols, rows))
    const ls = text(p)
    expect(ls.length + 2).toBeLessThanOrEqual(Math.floor(rows * 0.6))
    expect(ls.every(l => l.trim() !== '' && displayWidth(l) <= cols)).toBe(true)
    expect(ls.filter(l => l.includes('另 6 池無來源'))).toHaveLength(1)
    expect(ls.some(l => l.includes('待決 3'))).toBe(true)
    expect(ls.some(l => l.includes('pool-0'))).toBe(true)
    expect(p.groups.blocked.visible).toBeGreaterThanOrEqual(3)
    expect(ls.join('\n')).not.toMatch(/優先派工|停派新工|池 \d+\//)
  }
})

test('short overview is at most ten rows; missing and stale brief are explicit warning states', () => {
  const small = input()
  small.brief = parseBrief(JSON.stringify({ schema: 'fleet.brief/1', generated_at_ms: NOW, counts: { needs_human: 0, waiting_pm: 0, stuck: 0, offline: 0, high_context: 0, waiting: 0, info: 0 }, items: [] }))
  small.fleet.accounts = pools().slice(0, 3)
  small.assignments = assignments().slice(0, 1)
  expect(overviewPlan(small).lines.length + 2).toBeLessThanOrEqual(10)
  const missing = overviewPlan({ ...small, brief: parseBrief(null) })
  expect(text(missing).join('')).toContain('fleet 摘要檔還沒出現')
  const old = overviewPlan({ ...input(), now: NOW + 6 * 60000 })
  expect(old.lines.some(l => l.segs.some(s => s.text.includes('舊資料 6m') && s.color === 'warning'))).toBe(true)
})
