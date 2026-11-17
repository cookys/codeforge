// Sandbox tests emit JSON receipts; export-ux-shots.py writes them when UX_SHOTS_DIR is set.
import { expect, test } from 'claude-code/testing'
import { parseBrief } from '../hooks/brief-view'
import { parseFleetQuota } from '../hooks/fleet-view'
import { displayWidth, parseUsage } from '../hooks/model'
import { overviewPlan } from '../hooks/overview-view'
import type { PaneSeg } from '../hooks/overview-view'
import { paneChrome } from '../hooks/pane-controls'
import { BRIEF_SAMPLE } from './fixtures/brief-sample'
import { NOW, assignments } from './fixtures/pane-sample'
import { command, paneTarget, quotaFixture, runtimeWorld, start, textOf, usageFixture } from './fixtures/pane-runtime'
import type { RenderNode } from './fixtures/pane-runtime'

type ShotLine = PaneSeg[]
const words = (l: ShotLine) => l.map(s => s.text).join('')
const ROLES = ['inactive', 'subtle', 'merged', 'warning', 'error', 'success', 'pixel']

// Resolve both terminal themes through semantic names; RGB belongs only to sprite content.
const resolveTone = (tone: string | undefined, theme: 'dark' | 'light') => {
  expect(['dark', 'light']).toContain(theme)
  expect(ROLES).toContain(tone ?? 'inactive')
  return tone ?? 'inactive'
}
function receipt(name: string, cols: number, rows: number, lines: ShotLine[], overview: boolean, theme: 'dark' | 'light') {
  expect(lines.length).toBeLessThanOrEqual(Math.min(30, Math.floor(rows * 0.6)))
  for (const l of lines) {
    expect(displayWidth(words(l))).toBeLessThanOrEqual(cols)
    expect(words(l).replace(/│/g, '').trim()).not.toBe('')
  }
  if (overview) for (const subject of new Set(BRIEF_SAMPLE.items.map(i => i.subject.display))) {
    // Count rows, so a source's next_step_zh may name the same subject on its own row.
    expect(lines.filter(l => words(l).includes(subject)).length).toBeLessThanOrEqual(1)
  }
  const plain = lines.map(words)
  expect(plain[plain.length - 2]).toContain('分頁')
  expect(plain[plain.length - 1]).toContain('Esc 回輸入框')
  const semantic = lines.map(l => l.map(s => `[${resolveTone(s.color, theme)}]${s.text}[/]`).join(''))
  console.log('UX_SHOT ' + JSON.stringify({ name: `${name}-${cols}x${rows}-${theme}.txt`, plain, semantic }))
}

function rasterRows(n: RenderNode): ShotLine[] {
  const p = n.props as { rows: number; columns: number; cells: string }
  const data = Uint8Array.from(atob(p.cells), c => c.charCodeAt(0))
  const view = new DataView(data.buffer)
  return Array.from({ length: p.rows }, (_, y) => [{ text: Array.from({ length: p.columns }, (_, x) => String.fromCodePoint(view.getUint32((y * p.columns + x) * 12, true))).join(''), color: 'pixel' as never }])
}
function drawnRows(n: RenderNode | string): ShotLine[] {
  if (typeof n === 'string') return [[{ text: n }]]
  if (n.type === 'Raster') return rasterRows(n)
  if (n.type === 'Text') return [[{ text: textOf(n), color: n.props?.color ?? 'inactive' }]]
  const children = (n.children ?? []).map(drawnRows)
  if (n.props?.flexDirection === 'column') return children.flat()
  const height = Math.max(0, ...children.map(c => c.length))
  const widths = children.map(c => Math.max(0, ...c.map(l => displayWidth(words(l)))))
  return Array.from({ length: height }, (_, i) => children.flatMap((c, j) => {
    const line = c[i] ?? []
    const pad = widths[j] - displayWidth(words(line)) + (j < children.length - 1 ? Number(n.props?.gap ?? 0) : 0)
    return [...line, ...(pad ? [{ text: ' '.repeat(pad), color: 'inactive' as const }] : [])]
  }))
}

for (const theme of ['dark', 'light'] as const) {
  for (const [name, cols, rows] of [['S1', 160, 40], ['S2', 120, 30], ['S3', 80, 24], ['S4', 160, 40], ['S5-missing', 160, 40], ['S5-stale', 160, 40]] as const) {
    test(`UX ${name} ${cols}x${rows} ${theme}`, () => {
      const raw = JSON.parse(JSON.stringify(BRIEF_SAMPLE))
      const quota = quotaFixture()
      let assigned = assignments()
      if (name === 'S4') {
        raw.items = []
        raw.counts = { needs_human: 0, waiting_pm: 0, stuck: 0, offline: 0, high_context: 0, waiting: 0, info: 0 }
        quota.accounts = quota.accounts.slice(0, 3)
        assigned = assigned.slice(0, 1)
      }
      if (name === 'S5-stale') raw.generated_at_ms = NOW - 6 * 60000
      const plan = overviewPlan({ cols: cols - 2, rows, maxRows: 30, now: NOW, brief: parseBrief(name === 'S5-missing' ? null : JSON.stringify(raw)), fleet: parseFleetQuota(JSON.stringify(quota)), assignments: assigned, offsets: { blocked: 0, pools: 0, assignments: 0 }, usage: parseUsage(JSON.stringify(usageFixture()), NOW) })
      const chrome = paneChrome({ cols: cols - 2, tab: 'overview', statusTail: plan.statusTail, groups: plan.groups, decisions: plan.decisions, info: false, autopilotHint: false, buttons: true })
      const lines = [...plan.lines.map(l => l.segs), chrome.tabs, chrome.hint]
      if (name === 'S4') expect(lines.length).toBeLessThanOrEqual(10)
      if (name === 'S5-missing') expect(lines.some(l => words(l).includes('fleet 摘要檔還沒出現'))).toBe(true)
      if (name === 'S5-stale') expect(lines.some(l => l.some(s => s.color === 'warning' && s.text.includes('舊資料 6m')))).toBe(true)
      if (['S1', 'S2', 'S3'].includes(name)) {
        expect(lines.some(l => words(l).includes('待決 3') && words(l).includes('bridge'))).toBe(true)
        expect(lines.some(l => words(l).includes('pool-0'))).toBe(true)
        expect(lines.filter(l => words(l).includes('另 6 池無來源'))).toHaveLength(1)
        expect(plan.groups.blocked.visible).toBeGreaterThanOrEqual(3)
        expect(lines.some(l => words(l).includes('交辦線-0') && l.some(s => s.color === 'warning'))).toBe(true)
      }
      receipt(name, cols, rows, lines, true, theme)
    })
  }
  test(`UX S6 160x40 ${theme}: actual mounted pet tree, six cost rows and no overview facts`, async ($, on) => {
    runtimeWorld(on)
    await start($)
    await command($, 'pet')
    const ui = await $.ui.mount(paneTarget())
    const tree = await ui.drawn() as RenderNode
    const lines = drawnRows(tree)
    const all = lines.map(words).join('\n')
    expect(all).not.toMatch(/待決|卡住|pool-0|⋯10/)
    expect(lines.filter(l => /model-/.test(words(l)))).toHaveLength(6)
    expect(all).toContain('▸寵物')
    receipt('S6', 160, 40, lines, false, theme)
    await ui.unmount()
  })
}

test('UX exporter fixture receipt: exact supplied brief data', () => {
  console.log('UX_BRIEF ' + JSON.stringify(BRIEF_SAMPLE))
})
