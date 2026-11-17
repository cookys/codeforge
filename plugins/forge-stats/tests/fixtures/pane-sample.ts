import { parseBrief } from '../../hooks/brief-view'
import type { FleetAccount } from '../../hooks/fleet-view'
export const NOW = 1794200000000
export const blocked = Array.from({ length: 8 }, (_, i) => ({ id: 'blocked-' + i, kind: i < 2 ? 'offline' : 'stuck', level: 'red', subject: { display: '線-' + i + '（host-a）' }, title: '核對欄位與時區'.repeat(12), text: '任務逾時沒回報進度', since_ms: NOW - 3600000 - i * 1000, next_step_zh: '查看這條線的任務', ids: { task_id: 'task-' + i } }))
export const briefFixture = () => parseBrief(JSON.stringify({ schema: 'fleet.brief/1', generated_at_ms: NOW - 18000, counts: { needs_human: 3, waiting_pm: 0, stuck: 6, offline: 2, high_context: 0, waiting: 9, info: 1 }, items: [...blocked, { ...blocked[0], id: 'decision', kind: 'needs_human', subject: { display: '決策線（host-b）' }, title: '正式站要不要先切到新版結帳流程？', ids: { card_id: 'card-a' } }] }))
// realistic fake pools (names are fictional; numbers shaped like a real week)
type Row = [string, number | null, 'short' | 'waste' | null, number, number | null, number | null, number | null, 'needs_token' | 'web_only' | null, FleetAccount['plan']]
const POOLS: Row[] = [
  // name, weekly pct, warn, weekly reset in h, exhaust in h, 5h pct, 5h reset in min, source note, plan
  ['smith1', 0.99, 'short', 52, 2, null, null, null, 'max20x'],
  ['smith2', 0.82, 'short', 83, 26, 0, 290, null, 'max20x'],
  ['codexseat', 0.54, 'short', 104, 50, 0.23, 135, null, 'pro'],
  ['xaiseat', 0.39, 'short', 122, 74, 0.32, 100, null, null],
  ['kimi', 0.18, 'short', 138, 98, null, null, null, null],
  ['team-a', 0.096, 'waste', 18, null, null, null, null, 'team'],
  ['tok-echo', null, null, 0, null, null, null, 'needs_token', 'max5x'],
  ['tok-foxtrot', null, null, 0, null, null, null, 'needs_token', 'max5x'],
  ['web-alpha', null, null, 0, null, null, null, 'web_only', null],
  ['web-beta', null, null, 0, null, null, null, 'web_only', null],
  ['web-gamma', null, null, 0, null, null, null, 'web_only', null],
  ['web-delta', null, null, 0, null, null, null, 'web_only', null],
]
const S = NOW / 1000
export const pools = (): FleetAccount[] => POOLS.map(([name, pct, warn, resetH, exhaustH, fivePct, fiveMin, sourceNote, plan], i) => ({ alias: name, name, org: null, email: null, hosts: ['host-a'], oldHosts: [], observedAtMs: NOW, state: pct === null ? 'no_source' : 'ok', weekly: pct === null ? null : { pct, resetsAt: S + resetH * 3600, reset: false, warn, exhaustAt: exhaustH === null ? null : S + exhaustH * 3600 }, fiveHour: fivePct === null ? null : { pct: fivePct, resetsAt: S + (fiveMin ?? 0) * 60, reset: false }, fiveNoSource: false, sourceNote, note: i === 0 ? '週五 17:00 前只派審查' : null, noteResetRule: null, noteResetsAt: null, plan }))
const ASSIGN: [string, string, 'need_you' | 'waiting' | 'running', number][] = [
  ['PM', 'twgame 新帳號註冊', 'need_you', 48],
  ['gpu-lead', 'host-b 換 live 版', 'waiting', 5],
  ['codeforge', 'forge-stats 介面重做', 'running', 2],
]
export const assignments = () => ASSIGN.map(([who, title, state, h]) => ({ who, title, state, sinceMs: NOW - h * 3600000 }))
export const input = (cols = 160, rows = 40) => ({ cols, rows, maxRows: 30, now: NOW, brief: briefFixture(), fleet: { status: 'ok' as const, generatedAt: NOW - 18000, useTarget: 0.85, skipped: 0, accounts: pools() }, assignments: assignments(), offsets: { blocked: 0, pools: 0, assignments: 0 } })
