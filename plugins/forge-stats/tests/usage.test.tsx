// The 用量 / usage tab (PM G36 phase 4): a bounded read of codeforge's summary.json, accounts and today's models, the summary's age.
import { expect, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { countdown, parseUsage, USAGE_MAX_BYTES } from '../hooks/model'

const LIVE = '/run/user/1000/autopilot'
const cmd = (args: string) => ({ command: 'forge-stats', args, origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } }) as const
const SUMMARY = LIVE + '/codeforge/usage/summary.json'
const NOW = Date.parse('2026-11-03T12:00:00Z')
const DAY = '2026-11-03'

const row = (o: Record<string, unknown>) => ({
  day: DAY, source: 'claude', account: 'acct-a', model: 'claude-opus-5-5', speed: 'standard',
  input: 100, output: 50, cache_write_5m: 0, cache_write_1h: 0, cache_read: 850, requests: 3, cost_usd: 1.5, ...o,
})
const summary = (o: Record<string, unknown> = {}) => ({
  schema: 'codeforge.usage-summary/1',
  generated_at_ms: Date.now() - 5 * 60_000,
  host: 'host-1a2b3c4d',
  prices_as_of: '2026-10-06',
  accounts: [{ alias: 'smith1', written_at_ms: Date.now(), five_hour: { pct: 0.3, resets_at: Math.floor(Date.now() / 1000) + 7800 }, seven_day: null }],
  days: [
    row({ day: new Date().toISOString().slice(0, 10) }),
    row({ day: new Date().toISOString().slice(0, 10), model: 'gpt-6.1-sol', source: 'codex', account: 'codex-1a2b3c4d', cost_usd: null, requests: 9 }),
  ],
  ...o,
})

test('parseUsage takes today, sums per model, priced first, and refuses each contract break', () => {
  const v = parseUsage(
    JSON.stringify({ ...summary(), days: [row({}), row({ speed: 'fast', cost_usd: 3 }), row({ model: 'gpt-6.1-sol', cost_usd: null }), row({ day: '2026-11-02', cost_usd: 99 })] }),
    NOW,
  )
  expect(v.status).toBe('ok')
  expect(v.day).toBe(DAY)
  expect(v.models.map(m => m.model)).toEqual(['claude-opus-5-5', 'gpt-6.1-sol'])
  expect(v.models[0]).toEqual({ model: 'claude-opus-5-5', tokens: 2000, requests: 6, cost: 4.5, unpriced: 0 })
  expect(v.models[1].cost).toBe(null)
  expect(v.accounts[0].fiveHour?.pct).toBe(0.3)
  expect(parseUsage(null, NOW).status).toBe('missing')
  const breaks: unknown[] = [
    { ...summary(), schema: 'other/1' },
    { ...summary(), accounts: [{ alias: 'me@example.com', written_at_ms: 1, five_hour: null, seven_day: null }] },
    { ...summary(), days: [row({ model: '/home/me/x' })] },
    { ...summary(), days: [row({ input: -1 })] },
    { ...summary(), days: [row({ cost_usd: 'free' })] },
    { ...summary(), days: [{ day: DAY }] },
    // as strict as codeforge's summary::validate (G36 r1 D)
    { ...summary(), extra: 1 },
    { ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: null, seven_day: null, email: 'x' }] },
    { ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: { pct: 0.1, resets_at: 1, note: 'x' }, seven_day: null }] },
    { ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: { pct: 1.6, resets_at: 1 }, seven_day: null }] },
    { ...summary(), days: [{ ...row({}), project: 'repo' }] },
    { ...summary(), host: 'my-laptop' },
    { ...summary(), prices_as_of: 'yesterday' },
    { ...summary(), days: [row({ day: '2026-02-30' })] },
    { ...summary(), days: [row({ source: 'gemini' })] },
    { ...summary(), days: [row({ speed: 'turbo' })] },
    { ...summary(), days: [row({ account: '/home/me' })] },
    // null (no login) only on a Claude row; a codex row always names its account
    { ...summary(), days: [row({ source: 'codex', account: null })] },
    { ...summary(), days: [row({ account: '' })] },
    // numbers past the contract's bound (G36 r2 a46d57ea): an integer to JS, not to the writer
    { ...summary(), days: [row({ input: 1e100 })] },
    { ...summary(), days: [row({ requests: 2 ** 53 })] },
    { ...summary(), generated_at_ms: 1e100 },
    { ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: { pct: 0.1, resets_at: 1e100 }, seven_day: null }] },
    { ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: { pct: 0.1, resets_at: -1 }, seven_day: null }] },
  ]
  for (const b of breaks) expect(parseUsage(JSON.stringify(b), NOW).status).toBe('bad')
  expect(parseUsage('{', NOW).status).toBe('bad')
  expect(parseUsage(' '.repeat(USAGE_MAX_BYTES + 1), NOW).status).toBe('bad')
})

test('parseUsage takes a Claude day row with no account (no login, unattributed) into today\'s models', () => {
  const v = parseUsage(JSON.stringify(summary({ days: [row({ day: DAY, account: null, input: 5 })] })), NOW)
  expect(v.status).toBe('ok')
  expect(v.models.length).toBe(1)
})

test('parseUsage accepts the optional dropped_accounts record (closed reasons, uint counts) and refuses a malformed one', () => {
  expect(parseUsage(JSON.stringify(summary({ dropped_accounts: { resets_at_out_of_range: 1, pct_out_of_range: 2 } })), NOW).status).toBe('ok')
  const bad: unknown[] = [
    { acct_a: 1 },
    { resets_at_out_of_range: -1 },
    { resets_at_out_of_range: 'x' },
    { resets_at_out_of_range: 2 ** 53 },
    [],
    null,
  ]
  for (const d of bad) expect(parseUsage(JSON.stringify(summary({ dropped_accounts: d })), NOW).status).toBe('bad')
})

test('parseUsage accepts the optional codex_sessions array (sorted rollout ids, null cache_write) and refuses a malformed one', () => {
  const e = (id: string, o: object = {}) => ({ rollout_id: id, account: 'codex-4cc1b5c8', input: 1, output: 2, cache_read: 3, cache_write: null, reasoning: 1, ...o })
  const a = '0199aaaa-bbbb-7ccc-8ddd-eeeeffff0001'
  const b = '0199aaaa-bbbb-7ccc-8ddd-eeeeffff0002'
  expect(parseUsage(JSON.stringify(summary({ codex_sessions: [e(a), e(b)] })), NOW).status).toBe('ok')
  const bad: unknown[] = [[], [e(b), e(a)], [e(a), e(a)], [e('x')], [e(a, { cache_write: 0 })], [e(a, { input: -1 })], [e(a, { path: '/x' })], {}]
  for (const d of bad) expect(parseUsage(JSON.stringify(summary({ codex_sessions: d })), NOW).status).toBe('bad')
})

test('countdown reads minutes, hours and days, and a dash once past', () => {
  expect(countdown(NOW + 45 * 60_000, NOW)).toBe('45m')
  expect(countdown(NOW + 130 * 60_000, NOW)).toBe('2h10m')
  expect(countdown(NOW + (3 * 24 + 4) * 3_600_000, NOW)).toBe('3d4h')
  expect(countdown(NOW - 1, NOW)).toBe('—')
})

const PANE = {
  plugin: 'forge-stats',
  component: 'Pane',
  requestId: 'forge-stats',
  props: { title: 'Forge stats', isFocused: false, bodyColumns: 70, placement: 'dock', scroll: { offset: 0, bodyRows: 40 }, view: {} },
} as const

// the engine beneath: HOME, the live pointer, no pet, and the summary as given (a string, or a stat override)
const base = (on: Parameters<TestBody>[1], text: string | null, stat: Record<string, unknown> = {}) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : undefined }))
  on('fs.stat', (_, e) => {
    if (e.path === SUMMARY && text !== null) return { value: { kind: 'file', size: text.length, mtimeMs: 0, isLink: false, ...stat } }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.read', (_, e) => {
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path === SUMMARY && text !== null) return { value: text }
    throw new Error('ENOENT ' + e.path)
  })
}

const openUsage = async ($: Parameters<TestBody>[0]) => {
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  await ui.press({ key: 'tab-usage' })
  return ui
}

test('the pane has a tab strip and the usage tab shows accounts and today\'s models', async ($, on) => {
  base(on, JSON.stringify(summary()))
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect((await ui.findAll({ type: 'Button' })).length >= 2).toBe(true)
  expect(await ui.find({ type: 'Text', text: /smith1/ })).toBe(undefined)
  await ui.press({ key: 'tab-usage' })
  expect((await ui.find({ type: 'Text', text: /^Usage$/ }))?.props.bold).toBe(true)
  expect((await ui.find({ type: 'Text', text: /smith1/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /^30%$/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /^ 2h\d+m$/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /claude-opus-5-5/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /\$1\.50/ })) !== undefined).toBe(true)
  // the codex model has no price: a dash, and the total says it is partial
  expect((await ui.find({ type: 'Text', text: /total ≥\$1\.50/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /1 rows unpriced/ })) !== undefined).toBe(true)
  // fresh: no refresh hint
  expect(await ui.find({ type: 'Text', text: /to refresh/ })).toBe(undefined)
  // and back to the pet
  await ui.press({ key: 'tab-pet' })
  expect(await ui.find({ type: 'Text', text: /smith1/ })).toBe(undefined)
  await ui.unmount()
})

test('parseUsage takes an optional account email by the same rule as the writer', () => {
  const acc = (email: unknown) => ({ ...summary(), accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: null, seven_day: null, email }] })
  expect(parseUsage(JSON.stringify(acc('me@example.com')), NOW).status).toBe('ok')
  for (const bad of ['nobody', 'a b@example.com', 'a@@b', '@b', 'a@', '', 'x'.repeat(250) + '@example.com', 5, null]) expect(parseUsage(JSON.stringify(acc(bad)), NOW).status).toBe('bad')
})

test('parseUsage draws the account by name; an acct- hash with no name, email or org is not drawn; a bad name refuses the summary', () => {
  const one = (a: Record<string, unknown>) => ({ ...summary(), accounts: [{ alias: 'acct-1a2b3c4d', written_at_ms: 1, five_hour: null, seven_day: null, ...a }] })
  const names = (a: Record<string, unknown>) => {
    const v = parseUsage(JSON.stringify(one(a)), NOW)
    if (v.status !== 'ok') throw new Error('not ok')
    return v.accounts.map(x => x.name)
  }
  expect(names({ name: 'stranity', email: 'me@example.com', org: 'Stranity' })).toEqual(['stranity'])
  expect(names({ name: '公司帳號' })).toEqual(['公司帳號'])
  // a writer from before `name`
  expect(names({ email: 'me@example.com', org: 'Stranity' })).toEqual(['me@example.com'])
  expect(names({ org: 'Stranity' })).toEqual(['Stranity'])
  expect(names({})).toEqual([])
  expect(names({ alias: 'work' })).toEqual(['work'])
  // a provider key shows its provider, never the hash
  expect(names({ alias: 'codex-4cc1b5c8' })).toEqual(['codex'])
  expect(names({ alias: 'ci-20261105' })).toEqual(['ci-20261105'])
  // a file from before the key rule
  expect(names({ name: 'codex-4cc1b5c8' })).toEqual(['codex'])
  for (const bad of ['acct-1a2b3c4d', 'a@b', '', ' x', 'a\tb', 'x'.repeat(25), '工'.repeat(13), '工作\uFEFF', 5, null]) expect(parseUsage(JSON.stringify(one({ name: bad })), NOW).status).toBe('bad')
})

test('a summary older than an hour names `codeforge usage` as the refresh', async ($, on) => {
  base(on, JSON.stringify(summary({ generated_at_ms: Date.now() - 90 * 60_000 })))
  const ui = await openUsage($)
  const hint = await ui.find({ type: 'Text', text: /summary from 9\d min ago · run `codeforge usage` to refresh/ })
  expect(hint?.props.dimColor).toBe(true)
  await ui.unmount()
})

test('no summary file says how to make one', async ($, on) => {
  base(on, null)
  const ui = await openUsage($)
  expect((await ui.find({ type: 'Text', text: /no usage summary yet: run `codeforge usage`/ })) !== undefined).toBe(true)
  await ui.unmount()
})

test('a summary path that is a link is refused before reading', async ($, on) => {
  base(on, JSON.stringify(summary()), { isLink: true })
  const ui = await openUsage($)
  expect((await ui.find({ type: 'Text', text: /unreadable/ })) !== undefined).toBe(true)
  expect(await ui.find({ type: 'Text', text: /smith1/ })).toBe(undefined)
  await ui.unmount()
})

test('a summary larger than the bound is refused before reading', async ($, on) => {
  base(on, JSON.stringify(summary()), { size: USAGE_MAX_BYTES + 1 })
  const ui = await openUsage($)
  expect((await ui.find({ type: 'Text', text: /unreadable/ })) !== undefined).toBe(true)
  expect(await ui.find({ type: 'Text', text: /smith1/ })).toBe(undefined)
  await ui.unmount()
})

test('`/forge-stats usage` opens the pane on the usage tab, `/forge-stats pet` back', async ($, on) => {
  base(on, JSON.stringify(summary()))
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await $.command.run(cmd('usage'))
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect((await ui.find({ type: 'Text', text: /smith1/ })) !== undefined).toBe(true)
  await $.command.run(cmd('pet'))
  await ui.unmount()
  const again = await $.ui.mount({ ...PANE, surface: 'terminal' })
  expect(await again.find({ type: 'Text', text: /smith1/ })).toBe(undefined)
  await again.unmount()
})

test('a narrow pane drops the requests column and keeps the model id and the total whole', async ($, on) => {
  base(on, JSON.stringify(summary()))
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await $.command.run(cmd('usage'))
  const ui = await $.ui.mount({ ...PANE, surface: 'terminal', props: { ...PANE.props, bodyColumns: 37 } })
  expect((await ui.find({ type: 'Text', text: /claude-opus-5-5/ })) !== undefined).toBe(true)
  expect(await ui.find({ type: 'Text', text: /\d+req/ })).toBe(undefined)
  expect((await ui.find({ type: 'Text', text: /^  total ≥\$1\.50 · API-equivalent estimate$/ })) !== undefined).toBe(true)
  expect((await ui.find({ type: 'Text', text: /^  1 rows unpriced$/ })) !== undefined).toBe(true)
  await ui.unmount()
})

test('the account row drops countdowns, then 7d, as the pane narrows, and never cuts a field', async ($, on) => {
  const both = summary({ accounts: [{ alias: 'acct-1a2b3c4d', name: 'stranity·amy1', written_at_ms: Date.now(), five_hour: { pct: 0.3, resets_at: Math.floor(Date.now() / 1000) + 7800 }, seven_day: { pct: 0.45, resets_at: Math.floor(Date.now() / 1000) + 3 * 86400 } }] })
  base(on, JSON.stringify(both))
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await $.command.run(cmd('usage'))
  const at = async (bodyColumns: number) => {
    const ui = await $.ui.mount({ ...PANE, surface: 'terminal', props: { ...PANE.props, bodyColumns } })
    const got = {
      reset: (await ui.find({ type: 'Text', text: /^ \d+h\d+m$|^ \d+d\d+h$/ })) !== undefined,
      sevenDay: (await ui.find({ type: 'Text', text: /^  7d $/ })) !== undefined,
      fiveHour: (await ui.find({ type: 'Text', text: /^30%$/ })) !== undefined,
    }
    await ui.unmount()
    return got
  }
  expect(await at(70)).toEqual({ reset: true, sevenDay: true, fiveHour: true })
  expect(await at(37)).toEqual({ reset: false, sevenDay: true, fiveHour: true })
  expect(await at(30)).toEqual({ reset: false, sevenDay: false, fiveHour: true })
})


test('parseUsage takes an optional window seen_at_ms: a uint, at most 10 min after generated_at_ms', () => {
  const base = summary()
  const gen = base.generated_at_ms
  const acc = (seen: unknown) => ({ ...base, accounts: [{ alias: 'smith1', written_at_ms: 1, five_hour: { pct: 0.1, resets_at: 5, seen_at_ms: seen }, seven_day: null }] })
  expect(parseUsage(JSON.stringify(acc(gen)), NOW).status).toBe('ok')
  expect(parseUsage(JSON.stringify(acc(gen + 600_000)), NOW).status).toBe('ok')
  for (const bad of [gen + 600_001, -1, 1.5, 'x', null]) expect([bad, parseUsage(JSON.stringify(acc(bad)), NOW).status]).toEqual([bad, 'bad'])
})

test('parseUsage accepts a codex account entry with rollout windows; a note never belongs in a summary', () => {
  const t = Math.floor(Date.now() / 1000)
  const codex = { alias: 'codex-1a2b3c4d', written_at_ms: Date.now() - 60_000, five_hour: { pct: 0.42, resets_at: t + 3600, seen_at_ms: Date.now() - 60_000 }, seven_day: { pct: 0.175, resets_at: t + 86400, seen_at_ms: Date.now() - 60_000 } }
  const v = parseUsage(JSON.stringify(summary({ accounts: [codex] })), NOW)
  if (v.status !== 'ok') throw new Error('not ok')
  expect(v.accounts[0]?.sevenDay?.pct).toBe(0.175)
  expect(parseUsage(JSON.stringify(summary({ accounts: [{ ...codex, note: 'x' }] })), NOW).status).toBe('bad')
})
