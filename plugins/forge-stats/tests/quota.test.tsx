// G40 quota model: the pure reader of autopilot's `fleet.quota/1` mirror view (parse + §5.2 merge + token warn + tightest).
import { expect, test } from 'claude-code/testing'

import { mergeQuota, parseQuotaView, QUOTA_CONTRACT, tightest, tokenWarn } from '../hooks/quota'
import type { QuotaView } from '../hooks/quota'

const N = 1793700000
const DAY = 86400

type O = Record<string, unknown>
const win = (kind: string, utilization: number | null, observed_at: number, o: O = {}): O => ({
  kind, utilization, used: null, limit: null, unit: null, resets_at: N + 7500, source: 'passive', observed_at, ...o,
})
const rec = (o: O = {}, provider = 'claude', alias = 'work'): O => ({
  schema: 'fleet.quota/1', provider, alias, visibility: 'L3', windows: [], refusal: null, token_state: 'unknown', token_state_at: null,
  host_role: 'agent', token_issued_at: null, token_expires_at: null, ...o,
})
const row = (record: O, o: O = {}): O => ({ writer_courier: 'local', host_role: 'agent', observed_at_hub: null, skewed: false, record, ...o })
const view = (rows: O[], o: O = {}) => JSON.stringify({ schema: 'fleet.quota-view/1', namespace: 'cookys', hub_offset: 0, rows, ...o })
function ok(text: string, provider = 'claude', alias = 'work'): QuotaView {
  const v = parseQuotaView(text, provider, alias)
  if (v.status !== 'ok') throw new Error('bad view: ' + v.reason)
  return v
}
// the path's provider/alias come from the first row's record (default claude:work)
const merged = (rows: O[], o: O = {}, ns: string | null = 'cookys', now = N) => {
  const r = ((rows[0]?.record ?? {}) as O)
  return mergeQuota(ok(view(rows, o), (r.provider as string) ?? 'claude', (r.alias as string) ?? 'work'), ns, now)
}

// ---- fixtures D1-D8 (autopilot digest) ----
const D1 = () =>
  rec({
    windows: [win('rolling_5h', 0.62, 1793699940, { resets_at: 1793707500 }), win('weekly', 0.31, 1793699940, { resets_at: 1793973600 })],
    token_state: 'ok', token_state_at: 1793600000, token_issued_at: 1780740000, token_expires_at: 1812276000,
  })

test('D1 claude:work 5h + 7d is L3, aged from the local clock, token ok', () => {
  const m = merged([row(D1())])
  expect(m.windows.map(w => [w.kind, w.utilization, w.ageS, w.writerCourier, w.source, w.estimated])).toEqual([
    ['rolling_5h', 0.62, 60, 'local', 'passive', false],
    ['weekly', 0.31, 60, 'local', 'passive', false],
  ])
  expect(m.windows[0].observedAt).toBe(1793699940)
  expect(m.level).toBe('L3')
  expect(m.tokenState).toBe('ok')
  expect(m.tokenExpiresAt).toBe(1812276000)
  expect(m.hosts).toEqual(['agent'])
  expect(m.multiHost).toBe(false)
  expect(m.placeholder).toBe(false)
  expect(m.hadObservation).toBe(true)
  expect(m.refusal).toBe(null)
  expect(tightest(m)?.kind).toBe('rolling_5h')
})

test('D2 codex credits remaining-only: text only, L2, never the tightest window', () => {
  const credits = win('credits', null, 1793699000, { remaining: 12.5, unit: 'credits', resets_at: null, source: 'endpoint' })
  const m = merged([row(rec({ visibility: 'L2', windows: [credits] }, 'codex', 'main'))])
  expect(m.windows).toHaveLength(1)
  expect(m.windows[0]).toMatchObject({ kind: 'credits', utilization: null, remaining: 12.5, unit: 'credits', resetsAt: null, ageS: 1000 })
  expect(m.level).toBe('L2')
  expect(tightest(m)).toBe(null)
  // with a weekly beside it the weekly is the tightest
  const m2 = merged([row(rec({ visibility: 'L2', windows: [credits, win('weekly', 0.2, 1793699000, { resets_at: null })] }, 'codex', 'main'))])
  expect(tightest(m2)?.kind).toBe('weekly')
})

test('D2 the contract spells `remaining` in the record (parse accepts the full used/limit/remaining shape and checks the sum)', () => {
  const good = win('credits', 0.5, N - 10, { used: 5, limit: 10, remaining: 5, unit: 'credits' })
  expect(ok(view([row(rec({ windows: [good] }))])).rows).toHaveLength(1)
  const wrong = win('credits', 0.5, N - 10, { used: 5, limit: 10, remaining: 4, unit: 'credits' })
  expect(ok(view([row(rec({ windows: [wrong] }))])).rejected).toBe(1)
})

test('D3 placeholder: L0, placeholder, never observed, not retired by age', () => {
  const p = rec({ visibility: 'L0', source: 'registry', refusal: undefined, token_state_at: undefined }, 'claude', 'spare')
  const m = merged([row(p)], {}, 'cookys', N + 10 * DAY)
  expect(m.level).toBe('L0')
  expect(m.placeholder).toBe(true)
  expect(m.hadObservation).toBe(false)
  expect(m.windows).toEqual([])
  expect(m.hosts).toEqual([])
})

test('D4 a 7 h old window is dropped (observed, but nothing live); a variant with a passed resets_at is dropped too', () => {
  const stale = rec({ windows: [win('rolling_5h', 0.4, N - 25200, { resets_at: N + 3000 })] }, 'minimax', 'main')
  const m = merged([row(stale)])
  expect(m.windows).toEqual([])
  expect(m.level).toBe('L0')
  expect(m.hadObservation).toBe(true)
  expect(m.placeholder).toBe(false)
  const passed = rec({ windows: [win('rolling_5h', 0.4, N - 60, { resets_at: N - 1 })] })
  expect(merged([row(passed)]).windows).toEqual([])
  // 30 min .. 6 h old is valid and carries its age
  const aged = rec({ windows: [win('rolling_5h', 0.4, N - 8000)] })
  expect(merged([row(aged)]).windows[0].ageS).toBe(8000)
})

test('D5 an estimated window alone is used and marked, L2 with no reset', () => {
  const e = rec({ visibility: 'L2', windows: [win('daily', 0.41, 1793699700, { used: 410, limit: 1000, unit: 'requests', resets_at: null, source: 'estimated' })] }, 'glm', 'main')
  const m = merged([row(e)])
  expect(m.windows[0]).toMatchObject({ kind: 'daily', utilization: 0.41, used: 410, limit: 1000, unit: 'requests', estimated: true, source: 'estimated', ageS: 300 })
  expect(m.level).toBe('L2')
})

test('D6 refusal-only: L1, end = retry_at, age from the refusal stamp; without retry_at +30 min; expiry clears it', () => {
  const r = (retry_at: number | null) =>
    rec({ visibility: 'L1', refusal: { observed_at: 1793699880, retry_at, reason: 'rate_limit' } }, 'kimi', 'main')
  const m = merged([row(r(1793704800))])
  expect(m.windows).toEqual([])
  expect(m.refusal).toEqual({ endS: 1793704800, reason: 'rate_limit', ageS: 120 })
  expect(m.level).toBe('L1')
  expect(m.hadObservation).toBe(true)
  expect(merged([row(r(null))]).refusal?.endS).toBe(1793699880 + QUOTA_CONTRACT.refusal_default_cooldown_s)
  expect(merged([row(r(1793704800))], {}, 'cookys', 1793704800).refusal).toBe(null)
  expect(merged([row(r(1793704800))], {}, 'cookys', 1793704799).refusal).not.toBe(null)
  // a retry_at far away still expires MAX_AGE_S after the refusal
  const far = rec({ visibility: 'L1', refusal: { observed_at: N - 100, retry_at: N + 10 * DAY, reason: 'quota' } })
  expect(merged([row(far)]).refusal?.endS).toBe(N + 10 * DAY)
  expect(merged([row(far)], {}, 'cookys', N - 100 + QUOTA_CONTRACT.max_age_s).refusal).toBe(null)
  expect(merged([row(far)], {}, 'cookys', N - 100 + QUOTA_CONTRACT.max_age_s - 1).refusal).not.toBe(null)
})

test('D7 token near expiry warns urgent at 6 days; D7b invalid beats fresh low windows (windows stay, state is invalid)', () => {
  expect(tokenWarn(1794218400, N)).toEqual({ level: 'urgent', days: 6 })
  const inv = { ...D1(), token_state: 'invalid', token_state_at: 1793690000 }
  const m = merged([row(inv)])
  expect(m.tokenState).toBe('invalid')
  expect(m.windows[0].utilization).toBe(0.62)
  expect(m.level).toBe('L3')
})

test('an invalid token is not retired by age; a newer ok state change clears it', () => {
  const inv = rec({ visibility: 'L0', token_state: 'invalid', token_state_at: N - 10 * DAY })
  expect(merged([row(inv)]).tokenState).toBe('invalid')
  const okPeer = rec({ visibility: 'L0', token_state: 'ok', token_state_at: N - 100 })
  const m = merged([row(inv), row(okPeer, { writer_courier: 'cookys/desk/peer', host_role: 'desk', observed_at_hub: N - 100 })])
  expect(m.tokenState).toBe('ok')
})

const peerRow = (record: O, oah: number, o: O = {}) => row(record, { writer_courier: 'cookys/desk/peer', host_role: 'desk', observed_at_hub: oah, ...o })

test('D8 two hosts: local wins by age (hub_offset 3), both hosts listed, multiHost', () => {
  const peer = rec({
    host_role: 'desk',
    windows: [win('rolling_5h', 0.58, 1793699000, { resets_at: 1793707500 }), win('weekly', 0.3, 1793699000, { resets_at: 1793973600, source: 'probe' })],
    token_state: 'ok', token_state_at: 1793600000,
  })
  const m = merged([row(D1()), peerRow(peer, 1793699100)], { hub_offset: 3 })
  expect(m.windows.map(w => [w.kind, w.utilization, w.writerCourier, w.ageS])).toEqual([['rolling_5h', 0.62, 'local', 60], ['weekly', 0.31, 'local', 60]])
  expect(m.hosts).toEqual(['agent', 'desk'])
  expect(m.multiHost).toBe(true)
  // the peer's age is (N + 3) - 1793699100 = 903 s when it is the only row
  const only = merged([peerRow(peer, 1793699100)], { hub_offset: 3 })
  expect(only.windows[0]).toMatchObject({ writerCourier: 'cookys/desk/peer', hostRole: 'desk', ageS: 903, utilization: 0.58, observedAt: 1793699000 })
  expect(only.windows[1].source).toBe('probe')
  // a fresher peer wins
  const fresh = merged([row(D1()), peerRow(peer, N - 5)])
  expect(fresh.windows[0]).toMatchObject({ writerCourier: 'cookys/desk/peer', utilization: 0.58, ageS: 5 })
})

// ---- ties ----
test('exact age tie: local wins; two peers tie: higher utilization', () => {
  const a = rec({ windows: [win('weekly', 0.2, N - 100)] })
  const b = rec({ host_role: 'desk', windows: [win('weekly', 0.9, 777)] })
  const m = merged([row(a), peerRow(b, N - 100)])
  expect(m.windows[0]).toMatchObject({ writerCourier: 'local', utilization: 0.2 })
  const c = rec({ host_role: 'lap', windows: [win('weekly', 0.5, 5)] })
  const m2 = merged([peerRow(b, N - 100), peerRow(c, N - 100, { writer_courier: 'cookys/lap/peer', host_role: 'lap' })])
  expect(m2.windows[0]).toMatchObject({ writerCourier: 'cookys/desk/peer', utilization: 0.9 })
})

// ---- N80: one clock domain ----
const hubStamp = (off: number, age: number) => N + off - age
const n80Windows = (off: number) => [
  row(rec({ windows: [win('rolling_5h', 0.2, N - 120, { resets_at: null })] })),
  // the peer writer's own clock is arbitrary (123456789): only observed_at_hub counts
  peerRow(rec({ host_role: 'desk', windows: [win('rolling_5h', 0.95, 123456789, { resets_at: null })] }), hubStamp(off, 30)),
]
const n80Refusals = (off: number) => [
  row(rec({ visibility: 'L1', refusal: { observed_at: N - 600, retry_at: N + 60, reason: 'rate_limit' } })),
  peerRow(rec({ host_role: 'desk', visibility: 'L1', refusal: { observed_at: 5000, retry_at: 5000 + 1230, reason: 'quota' } }), hubStamp(off, 30)),
]
/** The planted wrong comparator: raw `t` of each row's refusal / 5h window, compared as if one clock. */
function naiveRaw(rows: O[], kind: 'window' | 'refusal'): number {
  const ts = rows.map(r => {
    const record = r.record as O
    const stamp = kind === 'window' ? (record.windows as O[])[0].observed_at as number : ((record.refusal as O).observed_at as number)
    if (r.observed_at_hub === null) return stamp
    const stamps = [stamp]
    return (r.observed_at_hub as number) - (Math.max(...stamps) - stamp)
  })
  return ts.indexOf(Math.max(...ts))
}

for (const off of [600, -600]) {
  test(`N80 windows: the newer peer window (age 30) beats the older local one (age 120) under hub_offset ${off}`, () => {
    const m = merged(n80Windows(off), { hub_offset: off })
    expect(m.windows[0]).toMatchObject({ writerCourier: 'cookys/desk/peer', utilization: 0.95, ageS: 30 })
  })
  test(`N80 refusals: the peer refusal (age 30, 1200 s left) beats the local one (age 600, 60 s left) under hub_offset ${off}`, () => {
    const m = merged(n80Refusals(off), { hub_offset: off })
    expect(m.refusal).toEqual({ endS: N + 1200, reason: 'quota', ageS: 30 })
  })
  test(`N80 resets_at is judged against now + hub_offset for both rows (offset ${off})`, () => {
    const mk = (resets: number) => [
      row(rec({ windows: [win('weekly', 0.2, N - 10, { resets_at: resets })] })),
      peerRow(rec({ host_role: 'desk', windows: [win('rolling_5h', 0.3, 1, { resets_at: resets })] }), hubStamp(off, 10)),
    ]
    expect(merged(mk(N + off), { hub_offset: off }).windows).toEqual([])
    expect(merged(mk(N + off + 1), { hub_offset: off }).windows.map(w => w.kind)).toEqual(['rolling_5h', 'weekly'])
  })
}

test('N80 the planted raw-t comparator picks the WRONG row under hub_offset -600 (so these tests can fail)', () => {
  // +600: raw t happens to agree with the right answer; -600: it picks the older local row
  expect(naiveRaw(n80Windows(600), 'window')).toBe(1)
  expect(naiveRaw(n80Windows(-600), 'window')).toBe(0)
  expect(naiveRaw(n80Refusals(600), 'refusal')).toBe(1)
  expect(naiveRaw(n80Refusals(-600), 'refusal')).toBe(0)
  const right = merged(n80Windows(-600), { hub_offset: -600 }).windows[0].writerCourier
  expect(right).toBe('cookys/desk/peer')
  expect(naiveRaw(n80Windows(-600), 'window') === 1).not.toBe(right === 'cookys/desk/peer')
})

// ---- drop rules ----
test('age 21600 s is kept, 21601 s dropped; observed 300 s in the future is kept, 301 s dropped', () => {
  const at = (x: number) => merged([row(rec({ windows: [win('weekly', 0.3, x, { resets_at: null })] }))]).windows.length
  expect(at(N - QUOTA_CONTRACT.max_age_s)).toBe(1)
  expect(at(N - QUOTA_CONTRACT.max_age_s - 1)).toBe(0)
  expect(at(N + QUOTA_CONTRACT.future_skew_s)).toBe(1)
  expect(at(N + QUOTA_CONTRACT.future_skew_s + 1)).toBe(0)
})

test('a window with resets_at == now + hub_offset is dropped; one second later it lives; null never expires by the clock', () => {
  const at = (r: number | null) => merged([row(rec({ windows: [win('weekly', 0.3, N - 5, { resets_at: r })] }))]).windows.length
  expect(at(N)).toBe(0)
  expect(at(N + 1)).toBe(1)
  expect(at(null)).toBe(1)
})

// ---- estimated ----
test('an estimate never lowers or replaces a measured window; it can only raise', () => {
  const meas = (u: number) => win('daily', u, N - 100, { resets_at: null })
  const est = (u: number) => win('daily', u, N - 5, { resets_at: null, source: 'estimated', used: 1, limit: 2, unit: 'requests' })
  const lower = merged([row(rec({ windows: [meas(0.8)] })), peerRow(rec({ host_role: 'desk', windows: [est(0.3)] }), N - 5)])
  expect(lower.windows[0]).toMatchObject({ utilization: 0.8, estimated: false, source: 'passive' })
  const higher = merged([row(rec({ windows: [meas(0.3)] })), peerRow(rec({ host_role: 'desk', windows: [est(0.8)] }), N - 5)])
  expect(higher.windows[0]).toMatchObject({ utilization: 0.8, estimated: true, source: 'estimated' })
  // the estimate is newer in both; the measured one is still the baseline, never swapped for a lower estimate
  const alone = merged([row(rec({ windows: [est(0.3)] }))])
  expect(alone.windows[0]).toMatchObject({ utilization: 0.3, estimated: true })
})

// ---- namespace guard ----
test('namespace mismatch (or a null side) ignores peer rows and keeps the local row', () => {
  const rows = [row(D1()), peerRow(rec({ host_role: 'desk', windows: [win('rolling_5h', 0.99, 1, {})] }), N - 1)]
  for (const ns of ['other', null]) {
    const m = merged(rows, {}, ns)
    expect(m.windows[0]).toMatchObject({ writerCourier: 'local', utilization: 0.62 })
    expect(m.hosts).toEqual(['agent'])
    expect(m.multiHost).toBe(false)
  }
  expect(merged(rows, { namespace: null }, 'cookys').hosts).toEqual(['agent'])
  expect(merged(rows).hosts).toEqual(['agent', 'desk'])
  const peerOnly = merged([rows[1]], {}, 'other')
  expect(peerOnly.windows).toEqual([])
  expect(peerOnly.hadObservation).toBe(false)
})

// ---- tokenWarn / tightest ----
test('tokenWarn: 30 d notice, 7 d urgent, past expired, null null', () => {
  expect(tokenWarn(null, N)).toBe(null)
  expect(tokenWarn(N + 30 * DAY + 1, N)).toBe(null)
  expect(tokenWarn(N + 30 * DAY, N)).toEqual({ level: 'notice', days: 30 })
  expect(tokenWarn(N + 7 * DAY + 1, N)?.level).toBe('notice')
  expect(tokenWarn(N + 7 * DAY, N)).toEqual({ level: 'urgent', days: 7 })
  expect(tokenWarn(N + 1, N)).toEqual({ level: 'urgent', days: 1 })
  expect(tokenWarn(N, N)?.level).toBe('expired')
  expect(tokenWarn(N - 3 * DAY, N)).toEqual({ level: 'expired', days: -3 })
})

test('tightest takes the largest utilization over 5h/weekly/daily/monthly/credits, never rpm/tpm or null credits', () => {
  const m = merged([row(rec({ windows: [win('rpm', 0.99, N - 5, { resets_at: null }), win('weekly', 0.4, N - 5), win('daily', 0.7, N - 5, { resets_at: null }), win('credits', null, N - 5, { remaining: 1, unit: 'credits' })] }))])
  expect(tightest(m)?.kind).toBe('daily')
  expect(tightest(merged([row(rec({ windows: [win('tpm', 0.9, N - 5)] }))]))).toBe(null)
  expect(tightest(merged([]))).toBe(null)
})

// ---- rejects ----
const goodWin = () => win('rolling_5h', 0.5, N - 60)
const withRec = (patch: O, base = rec({ windows: [goodWin()] })) => row({ ...base, ...patch })
const withWin = (patch: O) => withRec({ windows: [{ ...goodWin(), ...patch }] })
const REJECTS: [string, O][] = [
  ['drift pct in a window', withWin({ pct: 0.5 })],
  ['drift resetsAt in a window', withWin({ resetsAt: 5 })],
  ['drift five_hour in the record', withRec({ five_hour: {} })],
  ['drift seven_day in the record', withRec({ seven_day: {} })],
  ['an *_ms key in the record', withRec({ written_at_ms: 1 })],
  ['an *_ms key in a window', withWin({ observed_at_ms: 1 })],
  ['utilization above 1', withWin({ utilization: 1.01 })],
  ['utilization below 0', withWin({ utilization: -0.01 })],
  ['null utilization on a non-credits window', withWin({ utilization: null })],
  ['null utilization on credits without remaining', withWin({ kind: 'credits', utilization: null })],
  ['a millisecond-sized observed_at', withWin({ observed_at: 1793699940000 })],
  ['a millisecond-sized resets_at', withWin({ resets_at: 1793707500000 })],
  ['a fractional observed_at', withWin({ observed_at: N - 0.5 })],
  ['provider mismatch', withRec({ provider: 'codex' })],
  ['alias mismatch', withRec({ alias: 'other' })],
  ['a duplicate kind', withRec({ windows: [goodWin(), goodWin()] })],
  ['an unknown kind', withWin({ kind: 'hourly' })],
  ['window source 429', withWin({ source: '429' })],
  ['window source statusline', withWin({ source: 'statusline' })],
  ['used without limit', withWin({ used: 5 })],
  ['limit without used', withWin({ limit: 5, unit: 'requests' })],
  ['used and limit without unit', withWin({ used: 1, limit: 5 })],
  ['a unit outside the enum', withWin({ used: 1, limit: 5, unit: 'bytes' })],
  ['remaining on a non-credits window', withWin({ remaining: 1, unit: 'credits' })],
  ['more than 7 windows', withRec({ windows: Array.from({ length: 8 }, () => goodWin()) })],
  ['a record carrying writer_courier', withRec({ writer_courier: 'local' })],
  ['a record carrying namespace', withRec({ namespace: 'cookys' })],
  ['a record carrying sent_at', withRec({ sent_at: N })],
  ['a bad visibility', withRec({ visibility: 'L9' })],
  ['a bad schema', withRec({ schema: 'fleet.quota/2' })],
  ['a bad refusal reason', withRec({ visibility: 'L1', windows: [], refusal: { observed_at: N, retry_at: null, reason: 'busy' } })],
  ['a refusal with an extra key', withRec({ visibility: 'L1', windows: [], refusal: { observed_at: N, retry_at: null, reason: 'quota', kind: 'weekly' } })],
  ['a refusal with L0', withRec({ visibility: 'L0', refusal: { observed_at: N, retry_at: null, reason: 'quota' } })],
  ['a bad token_state', withRec({ token_state: 'revoked' })],
  ['token_state_at set while unknown', withRec({ token_state_at: N - 1 })],
  ['token_state ok without token_state_at', withRec({ token_state: 'ok' })],
  ['record source on a record with windows', withRec({ source: 'registry' })],
  ['an unknown record source', withRec({ visibility: 'L0', windows: [], source: 'manual' })],
  ['L3 with no windows', withRec({ visibility: 'L3', windows: [], source: 'registry' })],
  ['empty windows with no placeholder source, refusal or state', withRec({ visibility: 'L0', windows: [] })],
  ['a refusal-only record that also carries a source', withRec({ visibility: 'L1', windows: [], source: 'registry', refusal: { observed_at: N, retry_at: null, reason: 'quota' } })],
  ['an unknown extra record key', withRec({ extra: 1 })],
  ['a row with a bad observed_at_hub', row(rec({ windows: [goodWin()] }), { observed_at_hub: 1793699940000 })],
  ['a row with a non-boolean skewed', row(rec({ windows: [goodWin()] }), { skewed: 'no' })],
]
for (const [name, bad] of REJECTS) {
  test(`reject: ${name}`, () => {
    const v = ok(view([bad]))
    expect(v.rows).toEqual([])
    expect(v.rejected).toBe(1)
  })
}

test('a bad row is dropped and counted; the good row beside it survives; a repeated writer_courier is a bad row', () => {
  const v = ok(view([row(D1()), withWin({ utilization: 2 }), row(D1())]))
  expect(v.rows).toHaveLength(1)
  expect(v.rejected).toBe(2)
})

test('absent optional fields read as null; absent token_state reads as unknown', () => {
  const bare = { schema: 'fleet.quota/1', provider: 'claude', alias: 'work', visibility: 'L2', host_role: 'agent', windows: [{ kind: 'weekly', utilization: 0.2, source: 'passive', observed_at: N - 5 }] }
  const v = ok(view([row(bare)]))
  const r = v.rows[0].record
  expect(r.windows[0]).toMatchObject({ used: null, limit: null, unit: null, remaining: null, resetsAt: null })
  expect([r.refusal, r.tokenState, r.tokenStateAt, r.tokenIssuedAt, r.tokenExpiresAt]).toEqual([null, 'unknown', null, null, null])
})

test('a bad view wrapper is bad: not JSON, wrong schema, extra key, rows not an array, oversize, bad path', () => {
  const bad = (t: string, p = 'claude', a = 'work') => parseQuotaView(t, p, a).status
  expect(bad('{')).toBe('bad')
  expect(bad(view([], { schema: 'fleet.quota/1' }))).toBe('bad')
  expect(bad(view([], { extra: 1 }))).toBe('bad')
  expect(bad(view([], { rows: {} }))).toBe('bad')
  expect(bad(view([], { hub_offset: 'x' }))).toBe('bad')
  expect(bad(view([], { namespace: 5 }))).toBe('bad')
  expect(bad(JSON.stringify({ schema: 'fleet.quota-view/1', namespace: 'c', hub_offset: 0, rows: [], pad: 'x'.repeat(QUOTA_CONTRACT.view_max_bytes) }))).toBe('bad')
  expect(bad(view([]), 'Claude')).toBe('bad')
  expect(bad(view([]), 'claude', '../x')).toBe('bad')
  expect(bad(view([]))).toBe('ok')
  expect(ok(view([], { namespace: null })).namespace).toBe(null)
  expect(ok(view([], { hub_offset: -600 })).hubOffset).toBe(-600)
})
