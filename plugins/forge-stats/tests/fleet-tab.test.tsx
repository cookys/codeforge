// Fleet parser regression coverage; pane rendering is exercised by pane-v2 and UX shots.
import { expect, test } from 'claude-code/testing'
import { displayWidth } from '../hooks/model'
import { FLEET_MAX_BYTES, parseFleetQuota } from '../hooks/fleet-view'

const s = () => Math.floor(Date.now() / 1000)
const acct = (o: Record<string, unknown>) => ({ provider: 'claude', hosts: ['host-a'], observed_at_ms: Date.now(), age_s: 0, state: 'ok', weekly: null, five_hour: null, name: o.alias, ...o })
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
    acct({ alias: 'tight', hosts: ['host-a', 'host-b'], observed_at_ms: Date.now() - 45 * 60_000, age_s: 2700, weekly: wk({ pct: 0.91, resets_at: s() + 4 * 86400 + 3600 * 5 + 600 }), five_hour: { pct: 0.1, resets_at: s() + 600 } }),
    acct({ alias: 'codex-1a2b3c4d', provider: 'codex', state: 'no_source', observed_at_ms: null, age_s: null }),
    acct({ alias: 'old-acct', state: 'expired', observed_at_ms: Date.now() - 8 * 3600_000, age_s: 8 * 3600 }),
  ])

const NOTED = () =>
  fleet([
    acct({ alias: 'acct-11111111', name: 'cookys', email: 'me@example.com', weekly: wk({ pct: 0.2 }), note: '公司帳號，週五重置' }),
    acct({ alias: 'kimi', provider: 'kimi', name: 'kimi', state: 'no_source', hosts: [], observed_at_ms: null, age_s: null, source_note: 'web_only', note: '週配額用完' }),
    acct({ alias: 'glm', provider: 'glm', name: 'glm', state: 'no_source', hosts: [], observed_at_ms: null, age_s: null, source_note: 'needs_token' }),
    acct({ alias: 'codex', provider: 'codex', name: 'codex', state: 'no_source', hosts: [], observed_at_ms: null, age_s: null, source_note: 'no_window' }),
  ])

const RULED = () =>
  fleet([
    acct({ alias: 'acct-11111111', name: 'cookys', weekly: wk({ pct: 0.2 }) }),
    acct({ alias: 'kimi', provider: 'kimi', name: 'kimi', state: 'no_source', hosts: [], observed_at_ms: null, age_s: null, source_note: 'web_only', note: '本週已用完', note_resets_at: s() + 2 * 86400 + 3 * 3600 + 600, note_reset_rule: 'fri@17:00+08:00' }),
    acct({ alias: 'twgame', provider: 'claude', name: 'twgame', state: 'no_source', hosts: [], observed_at_ms: null, age_s: null, note_resets_at: s() + 3600 + 1800, note_reset_rule: 'thu@00:00+08:00' }),
  ])

const REAL = () =>
  fleet([
    acct({
      alias: 'acct-65d2bffc',
      name: 'stranity',
      org: 'Stranity',
      hosts: ['host-a', 'host-b'],
      old_hosts: ['host-b'],
      weekly: { pct: 0.04, resets_at: s() - 360, projected: null, warn: null, exhaust_at: null, reset: true },
      five_hour: { pct: 0.25, resets_at: s() + 13000, reset: false },
    }),
  ])

test('parseFleetQuota reads old_hosts and reset, and refuses a bad one', () => {
  const v = parseFleetQuota(JSON.stringify(REAL()))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts[0]?.oldHosts).toEqual(['host-b'])
  expect(v.accounts[0]?.weekly?.reset).toBe(true)
  expect(v.accounts[0]?.fiveHour?.reset).toBe(false)
  // absent from an older file: no old hosts, not reset
  const old = parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', weekly: wk({}) })])))
  if (old.status !== 'ok') throw new Error('not ok')
  expect(old.accounts[0]?.oldHosts).toEqual([])
  expect(old.accounts[0]?.weekly?.reset).toBe(false)
  for (const bad of [
    acct({ alias: 'a', old_hosts: 'host-b' }),
    acct({ alias: 'a', old_hosts: ['nope'] }),
    acct({ alias: 'a', old_hosts: ['gpu-a\u0007'], hosts: ['gpu-a\u0007'] }),
    acct({ alias: 'a', weekly: wk({ reset: 'yes' }) }),
    acct({ alias: 'a', five_hour: { pct: 0.1, resets_at: 5, reset: 1 } }),
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([bad]))).status).toBe('bad')
})

test('parseFleetQuota reads a window with age_s and stale (codeforge 2-hour mark); without them it still parses; a half or bad pair is refused', () => {
  const v = parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', weekly: wk({ age_s: 60, stale: false }), five_hour: { pct: 0.99, resets_at: 5, reset: true, age_s: 50100, stale: true } })])))
  expect(v.status).toBe('ok')
  for (const bad of [
    acct({ alias: 'a', weekly: wk({ age_s: 60 }) }),
    acct({ alias: 'a', weekly: wk({ stale: true }) }),
    acct({ alias: 'a', weekly: wk({ age_s: -1, stale: false }) }),
    acct({ alias: 'a', five_hour: { pct: 0.1, resets_at: 5, age_s: 60, stale: 'no' } }),
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([bad]))).status).toBe('bad')
})

test('parseFleetQuota never throws and refuses what is not the schema', () => {
  expect(parseFleetQuota(null).status).toBe('missing')
  for (const t of ['', '[]', 'null', '{"schema":1}', '"x"']) expect(parseFleetQuota(t).status).toBe('bad')
  expect(parseFleetQuota(JSON.stringify(FIXTURE())).status).toBe('ok')
})

test('parseFleetQuota reads source_note and note; an old file without them still parses; bad ones are refused', () => {
  const v = parseFleetQuota(JSON.stringify(NOTED()))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts.map(a => [a.sourceNote, a.note])).toEqual([
    [null, '公司帳號，週五重置'],
    ['web_only', '週配額用完'],
    ['needs_token', null],
    ['no_window', null],
  ])
  const old = parseFleetQuota(JSON.stringify(FIXTURE()))
  if (old.status !== 'ok') throw new Error('not ok')
  expect(old.accounts.every(a => a.sourceNote === null && a.note === null)).toBe(true)
  const ns = { provider: 'kimi', state: 'no_source', observed_at_ms: null, age_s: null }
  for (const bad of [
    acct({ alias: 'kimi', ...ns, source_note: 'maybe' }),
    acct({ alias: 'kimi', ...ns, source_note: null }),
    acct({ alias: 'a', source_note: 'web_only' }), // only a no_source row carries a reason
    acct({ alias: 'a', note: '' }),
    acct({ alias: 'a', note: ' padded' }),
    acct({ alias: 'a', note: 'mail me@example.com' }),
    acct({ alias: 'a', note: 'bell\u0007' }),
    acct({ alias: 'a', note: 'tab\there' }),
    acct({ alias: 'a', note: 'x'.repeat(41) }),
    acct({ alias: 'a', note: '寬'.repeat(21) }),
    acct({ alias: 'a', note: 5 }),
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([bad]))).status).toBe('bad')
  expect(parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', note: 'x'.repeat(40) })]))).status).toBe('ok')
})

test('parseFleetQuota reads note_resets_at and note_reset_rule; old files parse; half or bad pairs are refused', () => {
  const v = parseFleetQuota(JSON.stringify(RULED()))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts.map(a => [a.noteResetRule, a.noteResetsAt === null ? null : 'at'])).toEqual([
    [null, null],
    ['fri@17:00+08:00', 'at'],
    ['thu@00:00+08:00', 'at'],
  ])
  const old = parseFleetQuota(JSON.stringify(NOTED()))
  if (old.status !== 'ok') throw new Error('not ok')
  expect(old.accounts.every(a => a.noteResetRule === null && a.noteResetsAt === null)).toBe(true)
  const ok = { note_resets_at: 1793955600, note_reset_rule: 'fri@17:00+08:00' }
  expect(parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', ...ok })]))).status).toBe('ok')
  // monthly and one-shot rules, as codeforge writes them
  for (const rule of ['17@00:00+08:00', '1@00:00-05:00', '31@23:59+00:00', '2026-11-21T00:00:00+08:00', '2026-11-20T16:00:00Z'])
    expect(parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', ...ok, note_reset_rule: rule })]))).status).toBe('ok')
  for (const bad of [
    { ...ok, note_reset_rule: '0@00:00+08:00' },
    { ...ok, note_reset_rule: '01@00:00+08:00' },
    { ...ok, note_reset_rule: '32@00:00+08:00' },
    { ...ok, note_reset_rule: '2026-11-21T00:00+08:00' },
    { ...ok, note_reset_rule: '2026-11-21t00:00:00+08:00' },
    { ...ok, note_reset_rule: '2026-11-21T00:00:00.5Z' },
    { ...ok, note_reset_rule: '2026-13-21T00:00:00Z' },
    { ...ok, note_reset_rule: '2026-12-31T23:59:60Z' }, // no leap second, as in codeforge
    { note_resets_at: 1793955600 },
    { note_reset_rule: 'fri@17:00+08:00' },
    { ...ok, note_reset_rule: 'friday@17:00+08:00' },
    { ...ok, note_reset_rule: 'fri@17:00' },
    { ...ok, note_reset_rule: 'Fri@17:00+08:00' },
    { ...ok, note_reset_rule: 'fri@24:00+08:00' },
    { ...ok, note_resets_at: -1 },
    { ...ok, note_resets_at: 1.5 },
    { ...ok, note_resets_at: '1793955600' },
    { ...ok, note_resets_at: null },
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', ...bad })]))).status).toBe('bad')
})

test('parseFleetQuota reads a valid plan, and treats an invalid or absent one as unknown (not bad)', () => {
  const accounts = [
    acct({ alias: 'a', plan: 'max5x' }),
    acct({ alias: 'b', plan: 'api' }),
    acct({ alias: 'c' }),
    acct({ alias: 'd', plan: 'gold' }),
    acct({ alias: 'e', plan: 'Pro' }),
    acct({ alias: 'f', plan: 5 }),
    acct({ alias: 'g', plan: null }),
  ]
  const v = parseFleetQuota(JSON.stringify(fleet(accounts)))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts.map(a => [a.alias, a.plan])).toEqual([['a', 'max5x'], ['b', 'api'], ['c', null], ['d', null], ['e', null], ['f', null], ['g', null]])
})

test('parseFleetQuota reads five_hour_state no_source only with a null five_hour', () => {
  const v = parseFleetQuota(JSON.stringify(fleet([acct({ alias: 'a', weekly: wk({}), five_hour_state: 'no_source' }), acct({ alias: 'b', weekly: wk({}) })])))
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts.map(a => a.fiveNoSource)).toEqual([true, false])
  for (const bad of [
    acct({ alias: 'a', five_hour_state: 'gone' }),
    acct({ alias: 'a', five_hour_state: 'no_source', five_hour: { pct: 0.99, resets_at: 5, reset: true } }),
  ])
    expect(parseFleetQuota(JSON.stringify(fleet([bad]))).status).toBe('bad')
})
