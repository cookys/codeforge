import { expect, test } from 'claude-code/testing'
import { BRIEF_MAX_BYTES, parseBrief, selectBrief } from '../hooks/brief-view'

const item = (id: string, kind = 'stuck', who = id, task = id) => ({ id, kind, level: 'red', subject: { display: who }, title: '核對資料', text: '等回報', since_ms: 1000, next_step_zh: '查看任務原文', ids: { task_id: task } })
const brief = (items: unknown[], extra = {}) => JSON.stringify({ schema: 'fleet.brief/1', generated_at_ms: 2000, counts: { needs_human: 3, waiting_pm: 0, stuck: 6, offline: 2, high_context: 0, waiting: 9, info: 1 }, items, ...extra })

test('brief rejects bad schema, malformed known items and UTF-8 bytes over 512 KiB; unknown kinds are skipped', () => {
  expect(parseBrief(null).status).toBe('missing')
  for (const raw of ['{', brief([], { schema: 'fleet.brief/2' }), brief([{}]), brief([item('a')], { generated_at_ms: -1 }), brief(Array.from({ length: 501 }, () => item('a'))), brief([item('a')]) + ' '.repeat(BRIEF_MAX_BYTES), brief([], { extra: '中'.repeat(BRIEF_MAX_BYTES / 2) })]) expect(parseBrief(raw).status).toBe('bad')
  const v = parseBrief(brief([{ kind: 'future_kind' }, item('host-a')]))
  expect(v.status).toBe('ok')
  if (v.status === 'ok') {
    expect(v.items).toHaveLength(1)
    expect(v.counts.needs_human).toBe(3)
  }
})

test('brief dedup: decisions > assignments > blocked; distinct lanes sharing a task stay distinct', () => {
  const v = parseBrief(brief([item('card', 'needs_human', 'lane-a', 'task-a'), item('dup', 'stuck', 'lane-a'), item('cross', 'stuck', 'lane-b', 'task-a'), item('a', 'stuck', 'lane-c', 'shared'), item('b', 'stuck', 'lane-d', 'shared'), item('copy', 'stuck', 'lane-c', 'shared'), item('assigned', 'stuck', 'lane-e'), item('bg', 'waiting')]))
  const selected = selectBrief(v, [{ who: 'lane-e', title: '核對交辦', state: 'need_you', sinceMs: 0 }])
  expect(selected.blocked.map(i => i.subject.display)).toEqual(['lane-c', 'lane-d'])
  expect(selected.assignments.map(a => a.who)).toEqual(['lane-e'])
  expect(selected.decisions).toHaveLength(1)
  expect(selected.background).toBe(10)
})

test('blocked sort is level, offline/stuck/waiting_pm/high_context, then oldest; decisions show the oldest', () => {
  const a = item('amber', 'offline'); a.level = 'amber'
  const v = parseBrief(brief([a, item('context', 'high_context'), item('pm', 'waiting_pm'), item('stuck'), item('offline', 'offline'), { ...item('new', 'needs_human'), since_ms: 1500 }, item('old', 'needs_human')]))
  const s = selectBrief(v, [])
  expect(s.blocked.map(i => i.id)).toEqual(['offline', 'stuck', 'pm', 'context', 'amber'])
  expect(s.decisions.map(i => i.id)).toEqual(['old', 'new'])
})
