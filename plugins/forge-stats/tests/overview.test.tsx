import { expect, test } from 'claude-code/testing'
import { overviewPlan } from '../hooks/overview-view'
import { paneChrome } from '../hooks/pane-controls'
import { parseBrief } from '../hooks/brief-view'
import { displayWidth } from '../hooks/model'
import type { FleetAccount } from '../hooks/fleet-view'

import { NOW, pools, assignments, input } from './fixtures/pane-sample'
const text = (p: ReturnType<typeof overviewPlan>) => p.lines.map(l => l.segs.map(s => s.text).join(''))

test('R2-C1: compact tight pools take priority over summaries at every terminal width', () => {
  for (const cols of [78, 118, 158]) {
    const p = overviewPlan(input(cols, 24))
    const summary = p.lines.find(l => l.key === 'pool-summary')!
    const body = summary.segs.map(s => s.text).join('')
    expect(body).toContain('smith2 82% · codexseat 54% · xaiseat 39% · kimi 18%')
    expect(body).not.toContain('另 4 池吃緊')
    expect(displayWidth(body)).toBeLessThanOrEqual(cols)
    expect(p.lines.length + 2).toBe(14)
  }
})

test('R2-C1: a single pool row at wider widths keeps every tight name and the full lead note', () => {
  for (const cols of [118, 158]) {
    const p = overviewPlan(input(cols, 23))
    const line = p.lines.find(l => l.group === 'pools')!
    const body = line.segs.map(s => s.text).join('')
    expect(body).toContain('smith1')
    expect(body).toContain('99%')
    expect(body).toContain('週五 17:00 前只派審查')
    expect(body).toContain('smith2 82% · codexseat 54% · xaiseat 39% · kimi 18%')
    expect(body).not.toContain('另 4 池吃緊')
    expect(displayWidth(body)).toBeLessThanOrEqual(cols)
    expect(p.lines.length + 2).toBeLessThanOrEqual(13)
  }
})

test('overview stays within 60%, has no blank rows, decisions first and source summary when space permits', () => {
  for (const [cols, rows] of [[160, 40], [120, 30], [80, 24]]) {
    const p = overviewPlan(input(cols, rows))
    const ls = text(p)
    expect(ls.length + 2).toBeLessThanOrEqual(Math.floor(rows * 0.6))
    expect(ls.every(l => l.trim() !== '' && displayWidth(l) <= cols)).toBe(true)
    expect(ls.filter(l => l.includes('無來源 ×6'))).toHaveLength(cols >= 120 ? 1 : 0)
    expect(ls.some(l => l.includes('待決 3'))).toBe(true)
    expect(ls.some(l => l.includes('smith1'))).toBe(true)
    expect(p.groups.blocked.visible).toBe(8)
    expect(ls.join('\n')).not.toMatch(/優先派工|停派新工|池 \d+\//)
  }
})

test('every nonempty assignment group keeps a real scrollable row even with no need_you state', () => {
  const inp = input(80, 24)
  inp.assignments = assignments().map(a => ({ ...a, state: 'waiting' }))
  const p = overviewPlan(inp)
  expect(p.groups.assignments.visible).toBeGreaterThanOrEqual(1)
  const reached = new Set<string>()
  for (let top = 0; top < inp.assignments.length; top++) {
    const view = overviewPlan({ ...inp, offsets: { ...inp.offsets, assignments: top } })
    for (const a of inp.assignments) if (text(view).some(l => l.includes(a.who))) reached.add(a.who)
  }
  expect(reached.size).toBe(inp.assignments.length)
})

test('R2-K1: assignment overflow count and action have their own tones', () => {
  for (const buttons of [true, false]) for (const cols of [78, 118, 158]) {
    const p = overviewPlan({ ...input(cols, 24), buttons })
    const line = p.lines.find(l => l.key === 'assignment-PM')!
    expect(line.segs.find(s => s.text === '▲ 等操作員 · 2d0h')?.color).toBe('warning')
    expect(line.segs.find(s => s.text === ' · +2 件')?.color).toBe('inactive')
    expect(line.segs.find(s => s.text === '（a）')?.color).toBe(buttons ? 'merged' : 'subtle')
    expect(line.segs.some(s => s.color === 'warning' && s.text.includes('+2 件'))).toBe(false)
    expect(displayWidth(line.segs.map(s => s.text).join(''))).toBeLessThanOrEqual(cols)
  }
})

test('80-column chrome retains every live action after scrolling and the autopilot explanation', () => {
  const p = overviewPlan({ ...input(80, 18), offsets: { blocked: 1, pools: 0, assignments: 0 } })
  const chrome = paneChrome({ cols: 78, tab: 'overview', statusTail: p.statusTail, groups: p.groups, decisions: 3, info: false, autopilotHint: true, buttons: true })
  expect(displayWidth(chrome.hint.map(s => s.text).join(''))).toBeLessThanOrEqual(78)
  for (const key of ['j', 'k', 'u', 'a', 'i']) expect(chrome.hint.some(s => s.hotkey === key)).toBe(true)
  expect(chrome.hint.map(s => s.text).join('')).toContain('待決卡在 autopilot pane：輸入 /autopilot')
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
  expect([...old.lines.flatMap(l => l.segs), ...old.statusTail].some(s => s.text.includes('摘要過舊 6m') && s.color === 'warning')).toBe(true)
})


test('row allocation gives blocked all spare rows before pools and assignments, including one-row summaries', () => {
  for (const [cols, rows] of [[158, 40], [118, 30], [78, 24]]) {
    const inp = input(cols, rows)
    const p = overviewPlan(inp)
    expect(p.lines[0].key).toBe('decisions')
    expect(p.lines.filter(l => l.group === 'blocked' && !l.more)).toHaveLength(8)
    expect(p.lines.some(l => l.key === 'status')).toBe(false)
    expect(p.statusTail.length).toBeGreaterThan(0)
    expect(text(p).some(l => l.includes('↓'))).toBe(false)
    if (rows === 24) {
      expect(p.lines.length + 2).toBe(14)
      expect(p.lines.filter(l => l.group === 'pools')).toHaveLength(2)
      expect(p.lines.filter(l => l.group === 'assignments')).toHaveLength(1)
      expect(text(p).some(l => l.includes('+2 件'))).toBe(true)
    }
  }
  // With only one pool row, its lead and the entire remainder summary share that row.
  const p = overviewPlan(input(158, 23))
  const pool = p.lines.find(l => l.group === 'pools')!
  const line = pool.segs.map(s => s.text).join('')
  expect(line).toContain('smith1')
  expect(line).toContain('smith2 82% · codexseat 54% · xaiseat 39% · kimi 18%')
  expect(line).toContain('用不完 ×1')
  expect(line).toContain('無來源 ×6')
})


test('R2-M1: complete notes outrank bars and reset descriptions without an eighteen-cell cap', () => {
  for (const cols of [78, 118, 158]) {
    const inp = input(cols)
    const p = overviewPlan(inp)
    const line = p.lines.find(l => l.key === 'pool-smith1')!
    expect(line.segs.find(s => s.text.startsWith('週五 17'))!.text).toBe('週五 17:00 前只派審查')
    const widerNote = '週五 17:00 前只派審查，保留人工核對'
    inp.fleet.accounts[0].note = widerNote
    const wider = overviewPlan(inp).lines.find(l => l.key === 'pool-smith1')!
    expect(wider.segs.find(s => s.text.startsWith('週五 17'))!.text).toBe(widerNote)
    expect(wider.segs.map(s => s.text).join('')).toContain('99%')
    expect(displayWidth(wider.segs.map(s => s.text).join(''))).toBeLessThanOrEqual(cols)
    if (cols === 78) expect(wider.segs.some(s => /[▰▱]/.test(s.text))).toBe(false)
    const longNote = '核對'.repeat(cols === 78 ? 10 : cols === 118 ? 16 : 27)
    inp.fleet.accounts[0].note = longNote
    const long = overviewPlan(inp).lines.find(l => l.key === 'pool-smith1')!
    expect(long.segs.find(s => s.text.startsWith('核對'))?.text).toBe(longNote)
    expect(displayWidth(long.segs.map(s => s.text).join(''))).toBeLessThanOrEqual(cols)
  }
})

test('R2-M2: missing tokens are a subset of no-source pools at every width', () => {
  for (const cols of [78, 118, 158]) {
    const inp = input(cols, 24)
    // Leave room for summaries at S3 dimensions, without changing its six no-source pools.
    for (const a of inp.fleet.accounts.slice(1, 5)) a.weekly!.warn = null
    const p = overviewPlan(inp)
    expect(text(p).filter(l => l.includes('無來源 ×6（缺權杖 2）'))).toHaveLength(1)
    expect(text(p).join('\n')).not.toContain('缺權杖 ×2')
    expect(text(p).every(l => displayWidth(l) <= cols)).toBe(true)
  }
})

test('pool notes retain at least eight cells and explicit ellipses when longer than the row', () => {
  for (const cols of [78, 118, 158]) {
    const inp = input(cols)
    inp.fleet.accounts[0].note = '週五 17:00 前只派審查，超長備註'.repeat(10)
    const p = overviewPlan(inp)
    const line = p.lines.find(l => l.key === 'pool-smith1')!
    const note = line.segs.find(s => s.text.startsWith('週五 17'))!
    expect(note).not.toBe(undefined)
    expect(displayWidth(note.text)).toBeGreaterThanOrEqual(8)
    expect(note.text.endsWith('…')).toBe(true)
    expect(displayWidth(line.segs.map(s => s.text).join(''))).toBeLessThanOrEqual(cols)
  }
})


test('pool bars and short warnings use warning, waste and reading text use inactive, never error', () => {
  const inp = input()
  const threshold = { ...pools()[2], alias: 'near-limit', name: 'near-limit', weekly: { ...pools()[2].weekly!, pct: 0.95, warn: null } }
  inp.fleet.accounts.push(threshold)
  const p = overviewPlan(inp)
  for (const l of p.lines.filter(l => l.group === 'pools')) expect(l.segs.some(s => s.color === 'error')).toBe(false)
  for (const alias of ['smith1', 'smith2', 'near-limit']) {
    const line = p.lines.find(l => l.key === 'pool-' + alias)!
    expect(line.segs.some(s => s.text.includes('▰') && s.color === 'warning')).toBe(true)
    expect(line.segs.find(s => s.text.includes(alias))!.color).toBe('inactive')
    if (alias !== 'near-limit') expect(line.segs.some(s => s.text.includes('不夠') && s.color === 'warning')).toBe(true)
  }
  const waste = p.lines.find(l => l.key === 'pool-team-a')!
  expect(waste.segs.find(s => s.text.includes('用不完'))!.color).toBe('inactive')
})


test('missing and damaged brief use a summary diagnosis, never zero decisions or zero blocked', () => {
  for (const brief of [parseBrief(null), parseBrief('{')]) {
    const p = overviewPlan({ ...input(78, 24), brief })
    const line = p.lines.find(l => l.key === 'brief-unavailable')!
    expect(line.segs[0].text.trim()).toBe('摘要')
    expect(line.segs.slice(2).map(s => s.text).join('')).toBe('fleet 摘要檔還沒出現（待決、卡住無法顯示）')
    expect(text(p).join('')).not.toMatch(/待決 0|卡住 0/)
  }
})


test('only bound elements use merged; labels and current tab read inactive with a bold current tab', () => {
  for (const buttons of [true, false]) {
    const p = overviewPlan({ ...input(78, 20), buttons })
    const chrome = paneChrome({ cols: 78, tab: 'overview', statusTail: p.statusTail, groups: p.groups, decisions: 3, info: false, autopilotHint: false, buttons })
    expect(chrome.tabs[0].color).toBe('inactive')
    expect(chrome.hint[0].color).toBe('inactive')
    const active = chrome.tabs.find(s => s.text === '▸總覽')!
    expect(active.color).toBe('inactive')
    expect(active.bold).toBe(true)
    expect(chrome.tabs.find(s => s.text === '寵物')!.color).toBe(buttons ? 'merged' : 'inactive')
    for (const seg of [...chrome.tabs, ...chrome.hint].filter(s => s.color === 'merged')) expect(seg.action).not.toBe(undefined)
    for (const l of p.lines.filter(l => l.more)) {
      if (!buttons) expect(l.segs.some(s => s.color === 'merged')).toBe(false)
    }
    const marker = p.lines.find(l => l.group === 'blocked' && l.more)!
    expect(marker.segs.map(s => s.text).join('')).toMatch(/↓ 還有 \d+（j）/)
  }
})

test('pool and assignment subjects are bold, and empty meter slots are subtle', () => {
  const p = overviewPlan(input())
  expect(p.lines.find(l => l.key === 'pool-smith1')!.segs.find(s => s.text.includes('smith1'))!.bold).toBe(true)
  expect(p.lines.find(l => l.key === 'assignment-PM')!.segs.find(s => s.text.includes('PM'))!.bold).toBe(true)
  const slots = p.lines.filter(l => l.group === 'pools').flatMap(l => l.segs).filter(s => s.text.includes('▱'))
  expect(slots.length).toBeGreaterThan(0)
  for (const s of slots) expect(s.color).toBe('subtle')
})


test('R2-K2: stale brief warns at the end of the decisions row without adding rows', () => {
  for (const cols of [78, 118, 158]) {
    const inp = input(cols, 24)
    const fresh = overviewPlan(inp)
    const old = overviewPlan({ ...inp, now: NOW + 6 * 60000 })
    const line = old.lines.find(l => l.key === 'decisions')!
    expect(line.segs[line.segs.length - 1]).toEqual({ text: '（摘要過舊 6m）', color: 'warning' })
    expect(line.segs.map(s => s.text).join('')).toContain('待決 3 決策線（host-b）')
    expect(line.segs.map(s => s.text).join('')).toContain(' · 1h6m')
    expect(old.lines.length).toBe(fresh.lines.length)
    expect(old.lines.length + 2).toBe(14)
    expect(text(old).every(l => displayWidth(l) <= cols)).toBe(true)
    expect(text(fresh).join('\n')).not.toContain('摘要過舊')
    const empty = { ...inp, brief: { ...inp.brief, counts: { ...inp.brief.counts, needs_human: 0 }, items: [] } }
    const emptyOld = overviewPlan({ ...empty, now: NOW + 6 * 60000 })
    const emptyLine = emptyOld.lines.find(l => l.key === 'decisions')!
    expect(emptyLine.segs.some(s => s.text === '（摘要過舊 6m）' && s.color === 'warning')).toBe(true)
    expect(emptyLine.segs.some(s => s.text.includes('待決 0') && s.color === 'inactive')).toBe(true)
  }
})

test('both stale sources keep their names and overdue wording in the tab status at every width', () => {
  for (const cols of [78, 118, 158]) {
    const p = overviewPlan({ ...input(cols), now: NOW + 6 * 60000 })
    const chrome = paneChrome({ cols, tab: 'overview', statusTail: p.statusTail, groups: p.groups, decisions: 3, info: false, autopilotHint: false, buttons: true })
    for (const label of ['摘要過舊 6m', '各池過舊 6m']) expect(chrome.tabs.some(s => s.text.includes(label) && s.color === 'warning')).toBe(true)
  }
})


test('notes share the pool column budget so meter starts and lengths stay aligned', () => {
  for (const cols of [78, 118, 158]) {
    const p = overviewPlan(input(cols))
    const shapes = p.lines.filter(l => l.key.startsWith('pool-') && l.key !== 'pool-summary').map(l => {
      let prefix = ''
      for (const s of l.segs) {
        if (s.text.includes('▰') || s.text.includes('▱') || /^\s*\d+%/.test(s.text)) break
        prefix += s.text
      }
      return { start: displayWidth(prefix), slots: l.segs.reduce((n, s) => n + (s.text.match(/[▰▱]/g) ?? []).length, 0) }
    })
    expect(shapes.length).toBeGreaterThan(0)
    expect(shapes.every(s => s.start === shapes[0].start && s.slots === shapes[0].slots)).toBe(true)
  }
})


test('blocked overflow uses every allocated row for an item and keeps the hidden count minimal', () => {
  const p = overviewPlan(input(78, 20))
  expect(p.groups.blocked.visible).toBe(7)
  expect(p.groups.blocked.below).toBe(1)
  expect(p.lines.filter(l => l.group === 'blocked')).toHaveLength(7)
  expect(p.lines.find(l => l.group === 'blocked' && l.more)!.segs.map(s => s.text).join('')).toContain('↓ 還有 1（j）')
})


test('a single 80-column pool row keeps its lead, an eight-cell note and remainder counts', () => {
  const p = overviewPlan(input(78, 23))
  const l = p.lines.find(l => l.group === 'pools')!
  const body = l.segs.map(s => s.text).join('')
  for (const text of ['smith1', '99%', '另 4 池吃緊', '無來源 ×6（缺權杖 2）']) expect(body).toContain(text)
  expect(body).not.toContain('用不完')
  expect(displayWidth(l.segs.find(s => s.text.startsWith('週五 17'))!.text)).toBeGreaterThanOrEqual(8)
  expect(displayWidth(body)).toBeLessThanOrEqual(78)
})


test('overview rows omit trailing layout padding in exported text', () => {
  for (const cols of [78, 118, 158]) expect(text(overviewPlan(input(cols))).every(l => l === l.trimEnd())).toBe(true)
})
