// G40 display: the mirror resolver, the pane blocks and the band chips (pure), then the reader and the two surfaces against a fake mirror.
import { expect, mock, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { parseQuotaView } from '../hooks/quota'
import { displayWidth, parseSprite, summaryParts } from '../hooks/model'
import { FERRIS } from './ferris-sprite'
import { makeT } from '../hooks/strings'
import {
  QUOTA_MAX_PROVIDERS, NO_QUOTA, parseQuotaNs, pickProviders, pickViews, quotaBand, quotaBandForms, quotaEntries, quotaLiveCandidates, quotaLiveDir, quotaPaneLines,
} from '../hooks/quota-view'
import type { DirEntry, QuotaState } from '../hooks/quota-view'

const N = 1793700000
const NOW_MS = N * 1000
const DAY = 86400
const zh = makeT('zh-TW')
const en = makeT('en')

type O = Record<string, unknown>
// the fixtures of the plan's §6.2 picture, as a function of `N` (epoch seconds): the pure tests pin N, the surface tests use the real clock
function mkFX(N: number): Record<string, O[]> {
const win = (kind: string, utilization: number | null, observed_at: number, o: O = {}): O => ({
  kind, utilization, used: null, limit: null, unit: null, resets_at: N + 7500, source: 'passive', observed_at, ...o,
})
const rec = (provider: string, alias: string, o: O = {}): O => ({
  schema: 'fleet.quota/1', provider, alias, visibility: 'L3', windows: [], refusal: null, token_state: 'unknown', token_state_at: null,
  host_role: 'agent', token_issued_at: null, token_expires_at: null, ...o,
})
const row = (record: O, o: O = {}): O => ({ writer_courier: 'local', host_role: 'agent', observed_at_hub: null, skewed: false, record, ...o })
return {
  'claude:work': [
    row(rec('claude', 'work', {
      windows: [win('rolling_5h', 0.62, N - 60, { resets_at: N + 7530 }), win('weekly', 0.31, N - 60, { resets_at: N + 3 * DAY + 4 * 3600 + 30 })],
      token_state: 'ok', token_state_at: N - 100000, token_expires_at: N + 214 * DAY - 30,
    })),
  ],
  'claude:home': [
    row(rec('claude', 'home', {
      windows: [win('rolling_5h', 0.12, N - 60, { resets_at: N + 4 * 3600 + 40 * 60 + 30 }), win('weekly', 0.91, N - 60, { resets_at: N + DAY + 2 * 3600 + 30 })],
      token_expires_at: N + 6 * DAY - 100,
    })),
  ],
  'codex:main': [
    row(rec('codex', 'main', {
      visibility: 'L2',
      windows: [
        win('weekly', 0.17, N - 7800, { resets_at: N + 3 * DAY + 30 }),
        win('credits', null, N - 7800, { remaining: 12.5, unit: 'credits', resets_at: null, source: 'endpoint' }),
      ],
    })),
  ],
  'glm:main': [
    row(rec('glm', 'main', { visibility: 'L2', windows: [win('daily', 0.41, N - 120, { used: 410, limit: 1000, unit: 'requests', resets_at: null, source: 'estimated' })] })),
  ],
  'qoder:main': [
    row(rec('qoder', 'main', { visibility: 'L2', windows: [win('credits', 0.38, N - 120, { used: 1900, limit: 5000, remaining: 3100, unit: 'credits', resets_at: null })] })),
  ],
  'kimi:main': [row(rec('kimi', 'main', { visibility: 'L1', refusal: { observed_at: N - 60, retry_at: N + 4800 - 60 + 30, reason: 'rate_limit' } }))],
  'agy:main': [row(rec('agy', 'main', { visibility: 'L0', source: 'registry' }))],
  'minimax:main': [row(rec('minimax', 'main', { windows: [win('rolling_5h', 0.4, N - 25200, { resets_at: N + 3000 })] }))],
  'claude:old': [row(rec('claude', 'old', { visibility: 'L0', token_state: 'invalid', token_state_at: N - 100 }))],
  'claude:gone': [row(rec('claude', 'gone', { windows: [win('rolling_5h', 0.2, N - 60, { resets_at: N + 7530 })], token_expires_at: N - 100 }))],
  'claude:multi': [
    row(rec('claude', 'multi', { windows: [win('rolling_5h', 0.5, N - 60, { resets_at: N + 7530 })] })),
    row(rec('claude', 'multi', { host_role: 'desk', windows: [win('rolling_5h', 0.4, N - 90, { resets_at: N + 7530 })] }), { writer_courier: 'peer1', host_role: 'desk', observed_at_hub: N - 60 }),
  ],
}
}
const view = (rows: O[]) => JSON.stringify({ schema: 'fleet.quota-view/1', namespace: 'cookys', hub_offset: 0, rows })
const FX = mkFX(N)
const win0 = (N: number, kind: string, utilization: number | null, observed_at: number, o: O = {}): O => ({ kind, utilization, used: null, limit: null, unit: null, resets_at: N + 7500, source: 'passive', observed_at, ...o })
const row0 = (N: number, windows: O[]): O => ({
  writer_courier: 'local', host_role: 'agent', observed_at_hub: null, skewed: false,
  record: { schema: 'fleet.quota/1', provider: 'codex', alias: 'main', visibility: 'L3', windows, refusal: null, token_state: 'unknown', token_state_at: null, host_role: 'agent', token_issued_at: null, token_expires_at: null },
})
function stateOf(keys: string[], extra: Partial<QuotaState> = {}, fx: Record<string, O[]> = FX): QuotaState {
  const raw = keys.map(k => {
    const [provider, alias] = k.split(':') as [string, string]
    const v = parseQuotaView(view(fx[k] as O[]), provider, alias)
    if (v.status !== 'ok') throw new Error('bad fixture ' + k + ': ' + v.reason)
    return { provider, alias, view: v }
  })
  return { present: true, ns: 'cookys', raw, bad: 0, skipped: 0, ...extra }
}
const ALL = Object.keys(FX)
const paneText = (st: QuotaState, cols: number, tr = zh) => quotaPaneLines(st, quotaEntries(st, N), cols, NOW_MS, tr).map(l => l.map(s => s.text).join(''))
const bandText = (st: QuotaState, tr = zh) => quotaBandForms(quotaBand(quotaEntries(st, N), NOW_MS, tr)).map(f => f.map(p => p.text).join(''))

// ---- the resolver (contract §21.3; N37 vectors, one per step) ----
test('quotaLiveDir follows the contract order: override, xdg, xdg-inferred, shm, fallback; never /tmp', () => {
  const never = () => false
  const all = () => true
  const only = (...dirs: string[]) => (d: string) => dirs.includes(d)
  const HOME = '/home/t'
  // override: honoured as asked, never replaced, even when it does not exist
  expect(quotaLiveDir({ AUTOPILOT_LIVE_DIR: '/x/live/', XDG_RUNTIME_DIR: '/run/user/1000', HOME }, 1000, never)).toBe('/x/live')
  // xdg
  expect(quotaLiveDir({ XDG_RUNTIME_DIR: '/run/user/1000', HOME }, 1000, only('/run/user/1000/autopilot'))).toBe('/run/user/1000/autopilot')
  // xdg set but its autopilot dir missing: the next candidate, shm
  expect(quotaLiveDir({ XDG_RUNTIME_DIR: '/run/user/1000', HOME }, 1000, only('/dev/shm/autopilot-1000'))).toBe('/dev/shm/autopilot-1000')
  // xdg-inferred: XDG unset, /run/user/<uid>/autopilot exists
  expect(quotaLiveDir({ HOME }, 1000, only('/run/user/1000/autopilot', '/dev/shm/autopilot-1000'))).toBe('/run/user/1000/autopilot')
  // xdg-inferred absent: shm
  expect(quotaLiveDir({ HOME }, 1000, only('/dev/shm/autopilot-1000'))).toBe('/dev/shm/autopilot-1000')
  // the N37 failing vector: xdg-inferred absent and shm absent, /tmp/autopilot-<uid> present: the fallback, never the /tmp candidate
  expect(quotaLiveDir({ HOME }, 1000, only('/tmp/autopilot-1000'))).toBe('/home/t/.autopilot')
  expect(quotaLiveCandidates({ XDG_RUNTIME_DIR: '/run/user/1000', HOME }, 1000).some(c => c.dir.startsWith('/tmp'))).toBe(false)
  // fallback
  expect(quotaLiveDir({ HOME }, 1000, never)).toBe('/home/t/.autopilot')
  expect(quotaLiveDir({ HOME: '/home/t/' }, 1000, never)).toBe('/home/t/.autopilot')
  // an unknown uid drops the uid-named candidates; XDG still counts
  expect(quotaLiveDir({ HOME }, null, all)).toBe('/home/t/.autopilot')
  expect(quotaLiveDir({ XDG_RUNTIME_DIR: '/run/user/7', HOME }, null, all)).toBe('/run/user/7/autopilot')
  // nothing to go on
  expect(quotaLiveDir({}, null, all)).toBe(null)
  expect(quotaLiveDir({ AUTOPILOT_LIVE_DIR: '', HOME }, 1000, never)).toBe('/home/t/.autopilot')
})

// ---- the listing rules ----
const ent = (name: string, o: Partial<DirEntry> = {}): DirEntry => ({ name, kind: 'file', size: 300, isLink: false, ...o })
test('pickViews skips lock and non-json files silently and counts links, non-regular files, oversize and off-pattern names; pickProviders caps at 64', () => {
  const got = pickViews('claude', [
    ent('work.json'), ent('home.json'), ent('namespace.json'), ent('work.json.lock'), ent('.tmp.json'), ent('notes.txt'),
    ent('linked.json', { isLink: true, kind: 'other' }), ent('dir.json', { kind: 'dir' }), ent('big.json', { size: 32769 }), ent('Bad_Alias.json'), ent('edge.json', { size: 32768 }),
  ])
  // `namespace.json` inside a provider directory is the alias `namespace` (only `quota/namespace.json` is metadata)
  expect(got.views.map(v => v.alias)).toEqual(['edge', 'home', 'namespace', 'work'])
  expect(got.bad).toBe(4)
  const many: DirEntry[] = Array.from({ length: 70 }, (_, i) => ent('p' + String(i).padStart(2, '0'), { kind: 'dir', size: 0 }))
  const picked = pickProviders([...many, ent('namespace.json'), ent('Bad', { kind: 'dir' }), ent('lnk', { kind: 'other', isLink: true })])
  expect(picked.names).toHaveLength(QUOTA_MAX_PROVIDERS)
  expect(picked.skipped).toBe(6)
  expect(parseQuotaNs(JSON.stringify({ schema: 'fleet.quota-ns/1', namespace: 'cookys' }))).toBe('cookys')
  for (const bad of [null, '{', '[]', JSON.stringify({ schema: 'fleet.quota-ns/2', namespace: 'x' }), JSON.stringify({ schema: 'fleet.quota-ns/1', namespace: 3 })]) expect(parseQuotaNs(bad)).toBe(null)
})

// ---- the pane blocks ----
const block = (lines: string[], head: string) => {
  const from = lines.findIndex(l => l.startsWith(head + ' '))
  expect(from >= 0).toBe(true)
  const to = lines.findIndex((l, j) => j > from && l.trim() === '')
  return lines.slice(from, to < 0 ? undefined : to)
}

test('pane at 100: every state, in the §6.2 order (▲, tight, 429, the rest alphabetical, L0 and dropped last)', () => {
  const lines = paneText(stateOf(ALL), 100)
  const heads = lines.filter(l => /^[a-z]/.test(l) && l.includes(' L')).map(l => l.split(' ')[0])
  expect(heads).toEqual(['claude:gone', 'claude:home', 'claude:old', 'kimi:main', 'claude:multi', 'claude:work', 'codex:main', 'glm:main', 'qoder:main', 'agy:main', 'minimax:main'])
  // L3, token far off: the days go in the header, windows with a meter, a percent and a countdown
  expect(block(lines, 'claude:work')).toEqual([
    'claude:work  L3 · token 到期 214 天',
    '  5h    ▰▰▰▰▰▰▱▱▱▱  62%  ◷ 2h5m 後重置',
    '  週    ▰▰▰▱▱▱▱▱▱▱  31%  ◷ 3d4h 後重置',
  ])
  // tight: the word, no colour; token urgent: ▲ before the text
  expect(block(lines, 'claude:home')).toEqual([
    'claude:home  L3 · ▲ token 到期 6 天',
    '  5h    ▰▱▱▱▱▱▱▱▱▱  12%  ◷ 4h40m 後重置',
    '  週    ▰▰▰▰▰▰▰▰▰▱  91% 緊  ◷ 1d2h 後重置',
  ])
  expect(block(lines, 'claude:gone')[0]).toBe('claude:gone  L3 · ▲ token 已過期')
  // invalid: the header alone, no meter and no percent
  expect(block(lines, 'claude:old')).toEqual(['claude:old  L0 · ▲ token 已失效'])
  // credits remaining only: no meter, no %; an aged weekly beside it; both carry the age (> 30 min)
  expect(block(lines, 'codex:main')).toEqual([
    'codex:main  L2',
    '  週    ▰▱▱▱▱▱▱▱▱▱  17%  ◷ 3d0h 後重置 · 2h10m 前',
    '  額度  剩 12.50 credits · 2h10m 前',
  ])
  expect(block(lines, 'glm:main')[1]).toBe('  日    ▰▰▰▰▱▱▱▱▱▱  估 41%  重置時間未知')
  expect(block(lines, 'qoder:main')[1]).toBe('  額度  ▰▰▰▱▱▱▱▱▱▱  38%  剩 3,100 credits  重置時間未知')
  // L1: the refusal line, no meter
  expect(block(lines, 'kimi:main')).toEqual(['kimi:main  L1', '  429 冷卻  ◷ 1h19m 後重試'])
  expect(block(lines, 'agy:main')).toEqual(['agy:main  L0', '  從未觀察'])
  expect(block(lines, 'minimax:main')).toEqual(['minimax:main  L0', '  過期或已重置，等新觀察'])
  // the host part only for a key that arrived from two hosts
  expect(block(lines, 'claude:multi')[0]).toBe('claude:multi  L3 · host agent、desk')
  expect(lines.filter(l => l.includes('host ')).length).toBe(1)
  // the legend names what is used
  expect(lines[lines.length - 1]).toBe('估 = 本機計數，不是供應商回報；緊 = 已用 ≥ 90%')
  // no age under 30 min: claude:work has a 60 s old observation
  expect(block(lines, 'claude:work').some(l => l.includes('前'))).toBe(false)
  // never a bare 0% or a meter for the L1 / L0 states
  expect(lines.some(l => /^(  從未觀察|  過期).*▰/.test(l))).toBe(false)
})

test('pane in English, and the empty / unreadable footers', () => {
  const lines = paneText(stateOf(['claude:home', 'kimi:main']), 100, en)
  expect(lines).toContain('claude:home  L3 · ▲ token expires in 6 d')
  expect(lines).toContain('  429 cooling  ◷ 1h19m to retry')
  expect(lines).toContain('  week  ▰▰▰▰▰▰▰▰▰▱  91% tight  ◷ 1d2h to reset')
  expect(paneText({ ...NO_QUOTA, present: true }, 100)).toEqual(['  尚未收到配額資料（由 autopilot 的 reporter／wrapper 寫入）'])
  expect(paneText(stateOf(['claude:work'], { bad: 3, skipped: 5 }), 100).slice(-2)).toEqual(['  3 筆無法讀取', '  另有 5 筆未讀取'])
  // the legend line only for what is shown
  expect(paneText(stateOf(['claude:work']), 100).some(l => l.includes('='))).toBe(false)
})

test('pane narrows by dropping the age, then 後重置, then the meter; below 60 a block is one line', () => {
  // a long line: a tight credits gauge with a count, a countdown and an age
  const fx = mkFX(N)
  fx['codex:main'] = [row0(N, [win0(N, 'credits', 0.91, N - 7800, { used: 91000000000, limit: 100000000000, remaining: 9000000000, unit: 'credits', resets_at: N + 3 * DAY + 30, source: 'estimated' })])]
  const st = stateOf(['codex:main'], {}, fx)
  const credits = (cols: number) => paneText(st, cols).find(l => l.startsWith('  額度')) as string
  const full = credits(100)
  expect(full).toBe('  額度  ▰▰▰▰▰▰▰▰▰▱  估 91% 緊  剩 9,000,000,000 credits  ◷ 3d0h 後重置 · 2h10m 前')
  expect(displayWidth(full) > 60).toBe(true)
  const noAge = credits(displayWidth(full) - 1)
  expect(noAge).toBe('  額度  ▰▰▰▰▰▰▰▰▰▱  估 91% 緊  剩 9,000,000,000 credits  ◷ 3d0h 後重置')
  const noWord = credits(displayWidth(noAge) - 1)
  expect(noWord).toBe('  額度  ▰▰▰▰▰▰▰▰▰▱  估 91% 緊  剩 9,000,000,000 credits  ◷ 3d0h')
  expect(displayWidth(noWord) >= 60).toBe(true)
  // last, the meter goes; the numbers stay
  expect(credits(displayWidth(noWord) - 1)).toBe('  額度  估 91% 緊  剩 9,000,000,000 credits  ◷ 3d0h')
  // 60 keeps blocks, 59 is one line per alias
  expect(paneText(stateOf(ALL), 60)[0]).toBe('claude:gone  L3 · ▲ token 已過期')
  const compact = paneText(stateOf(ALL), 59)
  expect(compact.filter(l => /^[a-z]/.test(l))).toHaveLength(11)
  expect(compact).toContain('claude:gone 5h 20% ◷ 2h5m ▲ token 已過期')
  expect(compact).toContain('claude:home 週 91% 緊 ◷ 1d2h ▲ token 到期 6 天')
  expect(compact).toContain('kimi:main 429 冷卻 ◷ 1h19m')
  expect(compact).toContain('agy:main 無資料')
  expect(compact).toContain('claude:old ▲ token 已失效')
  expect(compact).toContain('codex:main 週 17% ◷ 3d0h · 2h10m 前')
  // narrower: whole fields drop, the name stays
  expect(paneText(stateOf(['codex:main']), 30)[0]).toBe('codex:main 週 17% ◷ 3d0h')
  expect(paneText(stateOf(['claude:home']), 40)[0]).toBe('claude:home 週 91% 緊 ▲ token 到期 6 天')
  expect(paneText(stateOf(['claude:work']), 40)[0]).toBe('claude:work 5h 62% ◷ 2h5m')
  expect(paneText(stateOf(['claude:home']), 30)[0]).toBe('claude:home 週 91% 緊')
  expect(paneText(stateOf(['claude:home']), 5)[0]).toBe('claude:home')
})

test('a peer row counts only past the namespace guard; with no guard the local row is all there is', () => {
  const guarded = paneText(stateOf(['claude:multi']), 100)
  expect(guarded[0]).toBe('claude:multi  L3 · host agent、desk')
  const unguarded = paneText(stateOf(['claude:multi'], { ns: null }), 100)
  expect(unguarded[0]).toBe('claude:multi  L3')
})

// ---- the band chips ----
test('band group: order ▲, tight, 429, the rest by utilization, then 無資料 N; the drop order of §6.1', () => {
  const forms = bandText(stateOf(ALL))
  expect(forms[0]).toBe(
    '▲ claude:gone token 已過期  ▲ claude:home token 到期 6 天  ▲ claude:old token 已失效  claude:home 週 91% 緊 ◷ 1d2h  kimi:main 429 冷卻 ◷ 1h19m  ' +
      'claude:work 5h 62% ◷ 2h5m  claude:multi 5h 50% ◷ 2h5m  glm:main 日 估 41%  qoder:main 額度 剩 3,100  claude:gone 5h 20% ◷ 2h5m  codex:main 週 17% ◷ 3d0h · 2h10m 前  無資料 2',
  )
  // 1: no 無資料   2: no ages   3: no countdowns   then whole chips from the tail   last: the alias alone
  expect(forms[1]).toBe(forms[0]!.replace('  無資料 2', ''))
  expect(forms[2]).toBe(forms[1]!.replace(' · 2h10m 前', ''))
  expect(forms[3]).not.toContain('◷')
  expect(forms[3]).toContain('claude:home 週 91% 緊  kimi:main 429 冷卻  claude:work 5h 62%')
  // 4: the prefixes go where the alias is unique (`main` is kimi's, glm's, qoder's and codex's: those keep theirs); only then whole chips
  expect(forms[4]).toBe(forms[3]!.replaceAll('claude:', ''))
  expect(forms[4]).toContain('▲ gone token 已過期')
  expect(forms[4]).toContain('kimi:main 429 冷卻')
  expect(forms[5]).toBe(forms[4]!.replace('  codex:main 週 17%', ''))
  // the ▲ chips go last; the one left is the most severe, and the alias stays (`gone` is also a chip of another provider? no: one key, so the prefix goes)
  expect(forms[forms.length - 1]).toBe('▲ gone token 已過期')
  // no ▲: the tightest chip alone, then `alias 91%`
  const calm = bandText(stateOf(['claude:work', 'glm:main']))
  expect(calm[calm.length - 2]).toBe('work 5h 62%')
  expect(calm[calm.length - 1]).toBe('work 62%')
  // a shared alias keeps its prefix: `main` is kimi's and glm's
  const same = bandText(stateOf(['claude:work', 'glm:main', 'kimi:main']))
  expect(same[same.length - 1]).toBe('kimi:main 429 冷卻')
  // nothing to draw: no group at all
  expect(bandText({ ...stateOf(['codex:main']), raw: [] })).toEqual([])
})

test('band chips: remaining-only credits, rpm and an invalid token draw no usage chip; 無資料 counts placeholders and dropped aliases only', () => {
  expect(bandText(stateOf(['claude:old']))).toEqual(['▲ claude:old token 已失效', '▲ old token 已失效'])
  const noData = bandText(stateOf(['agy:main', 'minimax:main']))
  expect(noData).toEqual(['無資料 2'])
  // codex has a weekly: its chip; the credits remaining-only line is the pane's
  expect(bandText(stateOf(['codex:main']))[0]).toBe('codex:main 週 17% ◷ 3d0h · 2h10m 前')
})

test('band chip colours: ▲ warning (the only yellow), 429 and 無資料 inactive, tight and 估 none', () => {
  const colours = (keys: string[]) => quotaBandForms(quotaBand(quotaEntries(stateOf(keys), N), NOW_MS, zh))[0]!.filter(p => p.color).map(p => p.text + ':' + p.color)
  expect(colours(['claude:home', 'kimi:main', 'agy:main', 'glm:main'])).toEqual(['▲:warning', '429 冷卻:inactive', '無資料:inactive', '1:inactive'])
})

test('band spacing invariants hold at every width 0..160, with and without fleet and pet (§3.5)', () => {
  const FLEET = { v: 1, fetchedMs: 1, error: null, totals: { needYou: 1, failed: 0, stuck: 2, running: 4, waiting: 11 } } as const
  for (const keys of [ALL, ['claude:work'], ['claude:home', 'kimi:main', 'glm:main'], ['agy:main']]) {
    for (const lang of [zh, en]) {
      const forms = quotaBandForms(quotaBand(quotaEntries(stateOf(keys), N), NOW_MS, lang))
      for (const fleet of [null, FLEET]) {
        for (const pet of [null, 'Ferris Lv.3']) {
          for (let cols = 0; cols <= 160; cols++) {
            const parts = summaryParts(fleet as never, pet, cols, 'fleet', forms)
            const t = parts?.map(p => p.text).join('') ?? ''
            if (parts !== null) expect(t.length > 0 && [...t].length <= cols * 2).toBe(true)
            // a glyph is always followed by a space; never three spaces in a row; no meter glyph on the band
            expect(/[▲✗⊘●⋯◷▰▱][^ ]/.test(t)).toBe(false)
            expect(/ {3}/.test(t)).toBe(false)
            expect(/[▰▱]/.test(t)).toBe(false)
            // a label belongs to its chip: provider:alias, one space, the value
            for (const k of keys) {
              const at = t.indexOf(k + ' ')
              if (at >= 0) expect(/^[^ ]+ [^ ]/.test(t.slice(at))).toBe(true)
              expect(t.includes(k + '  ')).toBe(false)
            }
          }
        }
      }
    }
  }
})

test('the quota group sits after the fleet part and before the pet; the pet goes first, the fleet counts only after the whole group', () => {
  const FLEET = { v: 1, fetchedMs: 1, error: null, totals: { needYou: 1, failed: 0, stuck: 0, running: 4, waiting: 0 } } as const
  const forms = quotaBandForms(quotaBand(quotaEntries(stateOf(['claude:work', 'claude:home']), N), NOW_MS, zh))
  const at = (cols: number) => summaryParts(FLEET as never, 'Ferris Lv.3', cols, '艦隊', forms)?.map(p => p.text).join('')
  const all = '艦隊 ▲ 1  ● 4 · ▲ claude:home token 到期 6 天  claude:home 週 91% 緊 ◷ 1d2h  claude:work 5h 62% ◷ 2h5m · Ferris Lv.3'
  expect(at(200)).toBe(all)
  expect(at(displayWidth(all))).toBe(all)
  // the pet is the first to go
  const noPet = all.replace(' · Ferris Lv.3', '')
  expect(at(displayWidth(all) - 1)).toBe(noPet)
  // narrow enough to lose the whole group: the fleet forms, then the pet alone
  const noGroup = summaryParts(FLEET as never, 'Ferris Lv.3', 12, '艦隊', forms)?.map(p => p.text).join('')
  expect(noGroup).toBe('艦隊 ▲ 1')
  // without autopilot the group leads
  expect(summaryParts(null, 'Ferris Lv.3', 200, 'fleet', forms)?.map(p => p.text).join('')?.startsWith('▲ claude:home token')).toBe(true)
  // no quota at all: today's output exactly
  expect(summaryParts(FLEET as never, 'Ferris Lv.3', 200, '艦隊')?.map(p => p.text).join('')).toBe('艦隊 ▲ 1  ● 4 · Ferris Lv.3')
  expect(summaryParts(FLEET as never, 'Ferris Lv.3', 200, '艦隊', [])?.map(p => p.text).join('')).toBe('艦隊 ▲ 1  ● 4 · Ferris Lv.3')
})

// ---- the reset time counts down on the local clock ----
function offsetState(off: number, resets: number): QuotaState {
  const r = [{ ...(row0(N, [win0(N, 'rolling_5h', 0.5, N - 60, { resets_at: resets })]) as O) }]
  const v = parseQuotaView(JSON.stringify({ schema: 'fleet.quota-view/1', namespace: 'cookys', hub_offset: off, rows: r }), 'codex', 'main')
  if (v.status !== 'ok') throw new Error('bad fixture')
  return { present: true, ns: 'cookys', raw: [{ provider: 'codex', alias: 'main', view: v }], bad: 0, skipped: 0 }
}
test('a hub clock ahead or behind shifts the reset countdown to the local clock, in the pane and in the band', () => {
  const ahead = offsetState(600, N + 660)
  expect(paneText(ahead, 100).find(l => l.includes('5h'))).toContain('◷ 1m 後重置')
  expect(bandText(ahead)[0]).toBe('codex:main 5h 50% ◷ 1m')
  const behind = offsetState(-600, N + 60)
  expect(paneText(behind, 100).find(l => l.includes('5h'))).toContain('◷ 11m 後重置')
  expect(bandText(behind)[0]).toBe('codex:main 5h 50% ◷ 11m')
  expect(paneText(behind, 50)[0]).toContain('◷ 11m')
})

// ---- credits on the band (design §4) ----
test('band credits: amounts show what is left, a bare utilization shows what is used, remaining-only stays on the pane', () => {
  const only = (windows: O[]) => {
    const v = parseQuotaView(view([row0(N, windows)]), 'codex', 'main')
    if (v.status !== 'ok') throw new Error('bad fixture')
    return { present: true, ns: 'cookys', raw: [{ provider: 'codex', alias: 'main', view: v }], bad: 0, skipped: 0 } as QuotaState
  }
  const amounts = win0(N, 'credits', 0.38, N - 120, { used: 1900, limit: 5000, unit: 'credits', resets_at: null })
  expect(bandText(only([amounts]))[0]).toBe('codex:main 額度 剩 3,100')
  expect(bandText(only([{ ...amounts, remaining: 3100 }]))[0]).toBe('codex:main 額度 剩 3,100')
  expect(bandText(only([win0(N, 'credits', 0.38, N - 120, { resets_at: null })]))[0]).toBe('codex:main 額度 已用 38%')
  expect(bandText(only([win0(N, 'credits', null, N - 120, { remaining: 12.5, unit: 'credits', resets_at: null })]))).toEqual([])
  expect(bandText(only([amounts]), en)[0]).toBe('codex:main credit 3,100 left')
  expect(bandText(only([win0(N, 'credits', 0.38, N - 120, { resets_at: null })]), en)[0]).toBe('codex:main credit 38% used')
})

// ---- narrow band: prefixes go before chips (§6.1) ----
test('band: provider prefixes drop before any chip does, but only where the alias is unique; a collision keeps its prefix', () => {
  const uniq = bandText(stateOf(['claude:work', 'claude:home', 'kimi:main']))
  const BARE = '▲ home token 到期 6 天  home 週 91% 緊  main 429 冷卻  work 5h 62%'
  const bareAt = uniq.indexOf(BARE)
  expect(bareAt > 0).toBe(true)
  // no chip is dropped before the bare form
  expect(uniq.slice(0, bareAt).every(t => t.includes('work') && t.includes('main') && t.includes('home'))).toBe(true)
  const forms = quotaBandForms(quotaBand(quotaEntries(stateOf(['claude:work', 'claude:home', 'kimi:main']), N), NOW_MS, zh))
  const cols = displayWidth(uniq[bareAt] as string)
  const at = (c: number) => summaryParts(null, null, c, 'fleet', forms)?.map(p => p.text).join('')
  expect(at(cols)).toBe(BARE)
  const prev = displayWidth(uniq[bareAt - 1] as string)
  expect(prev > cols).toBe(true)
  // the collision: `main` is kimi's and glm's, so both keep `provider:`; `work` is unique and goes bare
  const clash = bandText(stateOf(['claude:work', 'glm:main', 'kimi:main']))
  const bareClash = clash.find(t => t.startsWith('kimi:main 429 冷卻  work 5h 62%') && !t.includes('claude:'))
  expect(bareClash).toBe('kimi:main 429 冷卻  work 5h 62%  glm:main 日 估 41%')
})


// ---- the reader and the two surfaces, against a fake mirror ----
const LIVE = '/run/user/1000/autopilot'
const SESSION_NOW = Math.floor(Date.now() / 1000)
const PETSNAP = {
  schema: 'codeforge.pet-view/1', written_at: new Date().toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: null, message: null },
  village: { id: 'rust', display_name: 'The Forge-Ruins', language: 'Rust', tagline: '', rgb: [215, 95, 0], art_small: [], art_full: [] },
}
const USAGE = JSON.stringify({
  schema: 'codeforge.usage-summary/1', generated_at_ms: Date.now() - 60_000, host: 'host-1a2b3c4d', prices_as_of: '2026-10-06',
  accounts: [{ alias: 'acct-a', written_at_ms: Date.now(), five_hour: { pct: 0.3, resets_at: SESSION_NOW + 7800 }, seven_day: null }],
  days: [{ day: new Date().toISOString().slice(0, 10), source: 'claude', account: 'acct-a', model: 'claude-opus-5-5', speed: 'standard', input: 100, output: 50, cache_write_5m: 0, cache_write_1h: 0, cache_read: 850, requests: 3, cost_usd: 1.5 }],
})

type Fake = { files: Record<string, string>; over: Record<string, Partial<DirEntry>>; reads: string[]; lists: string[]; onRead?: (path: string) => void; onList?: (path: string) => void; afterList?: (path: string) => void }
// a virtual file system: `files` by absolute path; a directory exists when a file lies under it or `dirs` names it
function mirror(on: Parameters<TestBody>[1], files: Record<string, string>, dirs: string[] = [], zhLocale = true): Fake {
  const f: Fake = { files: { [LIVE + '/codeforge/pet.json']: JSON.stringify(PETSNAP), [LIVE + '/codeforge/usage/summary.json']: USAGE, ...files }, over: {}, reads: [], lists: [] }
  const isDir = (p: string) => dirs.includes(p) || Object.keys(f.files).some(k => k.startsWith(p + '/'))
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('session.id', () => ({ value: 'sid' }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : e.name === 'XDG_RUNTIME_DIR' ? '/run/user/1000' : e.name === 'CODEFORGE_LOCALE' && zhLocale ? 'zh-TW' : undefined }))
  on('process.run', () => ({ value: { exitCode: 0, stdout: '1000\n', stderr: '' } }))
  on('store.get', () => ({ value: undefined }))
  on('store.set', () => ({ value: undefined }))
  on('fs.write', () => ({ value: undefined }))
  on('ui.invalidate', () => ({ value: undefined }))
  on('fs.stat', (_, e) => {
    const o = f.over[e.path]
    if (e.path in f.files) return { value: { kind: 'file', size: (f.files[e.path] as string).length, mtimeMs: 0, isLink: false, ...o } }
    if (isDir(e.path)) return { value: { kind: 'dir', size: 0, mtimeMs: 0, isLink: false, ...o } }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.list', (_, e) => {
    const dir = e.path as string
    f.lists.push(dir)
    if (!isDir(dir)) throw new Error('ENOENT ' + dir)
    const names = new Set<string>()
    for (const k of Object.keys(f.files)) if (k.startsWith(dir + '/')) names.add(k.slice(dir.length + 1).split('/')[0] as string)
    f.onList?.(dir)
    const value = [...names].map(name => {
      const p = dir + '/' + name
      const file = p in f.files
      return { name, kind: file ? 'file' : 'dir', size: file ? (f.files[p] as string).length : 0, mtimeMs: 0, isLink: false, ...f.over[p] }
    })
    f.afterList?.(dir)
    return { value }
  })
  on('fs.read', (_, e) => {
    f.reads.push(e.path)
    f.onRead?.(e.path)
    if (e.path === '/home/t/.autopilot/live-pointer.json') return { value: JSON.stringify({ live_base: LIVE }) }
    if (e.path in f.files) return { value: f.files[e.path] }
    throw new Error('ENOENT ' + e.path)
  })
  on('ui.render', { component: 'AbovePrompt' }, ($, e) => {
    const { Box } = $.ui.resolve(e)
    return <Box key="beneath" />
  })
  return f
}
const viewFiles = (keys: string[], fx = mkFX(SESSION_NOW)) => Object.fromEntries(keys.map(k => [`${LIVE}/quota/${k.replace(':', '/')}.json`, view(fx[k] as O[])]))
const NS = { [LIVE + '/quota/namespace.json']: JSON.stringify({ schema: 'fleet.quota-ns/1', namespace: 'cookys' }) }
const PANE = {
  plugin: 'forge-stats', component: 'Pane', requestId: 'forge-stats',
  props: { title: 'Forge stats', isFocused: false, bodyColumns: 102, placement: 'dock', scroll: { offset: 0, bodyRows: 80 }, view: {} },
} as const
const band = (bodyColumns: number, over: Record<string, unknown> = {}) =>
  ({ plugin: 'forge-stats', component: 'AbovePrompt', requestId: 'band-1', surface: 'terminal', props: { hasSurvey: false, isWorking: false, maxRows: 1, bodyColumns, scroll: { offset: 0, bodyRows: 19 }, view: {}, ...over } }) as const
const flat = async (ui: { findAll: (q: { type: string }) => Promise<unknown[]> }) => ((await ui.findAll({ type: 'Text' })) as { text: string }[]).map(t => t.text).join('')
const openUsage = async ($: Parameters<TestBody>[0], cols = 102) => {
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await $.command.run({ command: 'forge-stats', args: 'usage', origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } } as never)
  return $.ui.mount({ ...PANE, surface: 'terminal', props: { ...PANE.props, bodyColumns: cols } })
}

test('the usage tab draws the mirror blocks and keeps the models; the G36 accounts rows are gone once the mirror exists', async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work', 'claude:home', 'kimi:main', 'agy:main']), ...NS })
  const ui = await openUsage($)
  const text = await flat(ui)
  expect(text).toContain('claude:work')
  expect(text).toContain('L3')
  expect(text).toContain('▲ token 到期 6 天')
  expect(text).toContain('429 冷卻')
  expect(text).toContain('從未觀察')
  expect(text).not.toContain('acct-a')
  expect(text).toContain('claude-opus-5-5')
  // never a read outside the mirror's own paths; only regular `.json` views and the namespace file
  expect([...new Set(f.reads.filter(p => p.includes('/quota/')))].sort()).toEqual(
    [...Object.keys(viewFiles(['claude:work', 'claude:home', 'kimi:main', 'agy:main'])), LIVE + '/quota/namespace.json'].sort(),
  )
  await ui.unmount()
})

test('rule 7: no quota directory keeps today\'s pane (the accounts rows); an empty one says nothing has arrived', async ($, on) => {
  mirror(on, {}, [], false)
  const old = await openUsage($)
  const text = await flat(old)
  expect(text).toContain('acct-a')
  expect(text).not.toContain('配額')
  expect(text).not.toContain('quota')
  await old.unmount()
})

test('rule 7: the mirror directory exists but holds nothing', async ($, on) => {
  mirror(on, {}, [LIVE + '/quota'])
  const ui = await openUsage($)
  const text = await flat(ui)
  expect(text).toContain('尚未收到配額資料（由 autopilot 的 reporter／wrapper 寫入）')
  expect(text).not.toContain('acct-a')
  expect(text).toContain('claude-opus-5-5')
  await ui.unmount()
})

test('the reader skips links, non-regular files, oversize files and lock files without reading them, counts the bad, never reads a path twice in one refresh (session start and `/forge-stats usage` each refresh once)', async ($, on) => {
  const files = {
    ...viewFiles(['claude:work']),
    [LIVE + '/quota/claude/home.json']: view(mkFX(SESSION_NOW)['claude:home'] as O[]),
    [LIVE + '/quota/claude/linked.json']: 'x',
    [LIVE + '/quota/claude/huge.json']: 'x',
    [LIVE + '/quota/claude/work.json.lock']: '1234',
    [LIVE + '/quota/claude/broken.json']: '{ not json',
    [LIVE + '/quota/claude/other.json']: JSON.stringify({ schema: 'fleet.quota/1' }),
    ...NS,
  }
  const f = mirror(on, files)
  f.over[LIVE + '/quota/claude/linked.json'] = { isLink: true, kind: 'other' }
  f.over[LIVE + '/quota/claude/huge.json'] = { size: 40000 }
  const ui = await openUsage($)
  const text = await flat(ui)
  const q = f.reads.filter(p => p.includes('/quota/'))
  expect(q.includes(LIVE + '/quota/claude/linked.json')).toBe(false)
  expect(q.includes(LIVE + '/quota/claude/huge.json')).toBe(false)
  expect(q.includes(LIVE + '/quota/claude/work.json.lock')).toBe(false)
  // linked + huge + broken + other = 4 unreadable
  expect(text).toContain('4 筆無法讀取')
  expect(text).toContain('claude:work')
  expect(text).toContain('claude:home')
  await ui.unmount()
})

test('the reader caps views at 256 and counts the rest without reading them', async ($, on) => {
  const base = mkFX(SESSION_NOW)['claude:work'] as O[]
  const files: Record<string, string> = { ...NS }
  for (let i = 0; i < 300; i++) {
    const alias = 'a' + String(i).padStart(3, '0')
    const r = JSON.parse(JSON.stringify(base)) as O[]
    ;((r[0] as O).record as O).alias = alias
    files[`${LIVE}/quota/claude/${alias}.json`] = view(r)
  }
  const f = mirror(on, files)
  const ui = await openUsage($, 42)
  const text = await flat(ui)
  const viewReads = f.reads.filter(p => /\/quota\/claude\/a\d+\.json$/.test(p))
  expect(new Set(viewReads).size).toBe(256)
  expect(viewReads).toHaveLength(512) // two refreshes (session start, the usage command), 256 each
  expect(text).toContain('另有 44 筆未讀取')
  await ui.unmount()
})

test('the band: chips after the fleet counts and before the pet, nothing without the mirror, today\'s row with an empty one', async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work', 'claude:home', 'kimi:main', 'agy:main']), ...NS })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const at = async (cols: number) => {
    const ui = await $.ui.mount(band(cols))
    const boxes = (await ui.findAll({ type: 'Box' })).map(b => b.props.key)
    const t = boxes.includes('beneath') ? undefined : await flat(ui)
    await ui.unmount()
    return t
  }
  const chips = '▲ claude:home token 到期 6 天  claude:home 週 91% 緊 ◷ 1d2h  kimi:main 429 冷卻 ◷ 1h19m  claude:work 5h 62% ◷ 2h5m'
  const full = chips + '  無資料 1 · Ferris Lv.3'
  // the row keeps a 2-cell margin
  expect(await at(displayWidth(full) + 2)).toBe(full)
  // the pet first, then 無資料, then the countdowns
  expect(await at(displayWidth(full) + 1)).toBe(chips + '  無資料 1')
  expect(await at(displayWidth(chips + '  無資料 1') + 1)).toBe(chips)
  expect(await at(displayWidth(chips) + 1)).toBe(chips.replace(/ ◷ \w+/g, ''))
  expect(await at(20)).toBe('Ferris Lv.3')
  expect(f.reads.length > 0).toBe(true)
})

test('the band keeps today\'s row when the quota directory is absent', async ($, on) => {
  mirror(on, {}, [], false)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(band(100))
  expect(await flat(ui)).toBe('Ferris Lv.3')
  await ui.unmount()
})

test('the band with an empty mirror directory draws today\'s row too', async ($, on) => {
  mirror(on, {}, [LIVE + '/quota'])
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(band(100))
  expect(await flat(ui)).toBe('Ferris Lv.3')
  await ui.unmount()
})

test('the timer re-reads the mirror every 15 s while the band is drawn, and a view that appears shows up', async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work']), ...NS })
  const clock = mock.clock(on, { now: Date.now() })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const ui = await $.ui.mount(band(200))
  expect(await flat(ui)).not.toContain('kimi:main')
  await ui.unmount()
  Object.assign(f.files, viewFiles(['kimi:main']))
  await clock.advance(15_000)
  const again = await $.ui.mount(band(200))
  expect(await flat(again)).toContain('kimi:main 429 冷卻')
  await again.unmount()
})

// ---- a hostile or changing mirror (reader safety) ----
test('namespace.json inside a provider directory is the alias `namespace`', async ($, on) => {
  const fx = mkFX(SESSION_NOW)
  const rec = JSON.parse(JSON.stringify(fx['claude:work'])) as O[]
  ;((rec[0] as O).record as O).alias = 'namespace'
  mirror(on, { ...viewFiles(['claude:home']), [`${LIVE}/quota/claude/namespace.json`]: view(rec), ...NS })
  const ui = await openUsage($)
  const text = await flat(ui)
  expect(text).toContain('claude:namespace')
  expect(text).toContain('claude:home')
  expect(text).not.toContain('筆無法讀取')
  await ui.unmount()
})

test('the reader rejects a view that grew past the cap, or changed size, or turned into a link between the listing and the read, and counts it', async ($, on) => {
  const named = (alias: string) => {
    const r = JSON.parse(JSON.stringify(mkFX(SESSION_NOW)['claude:home'])) as O[]
    ;((r[0] as O).record as O).alias = alias
    return view(r)
  }
  const f = mirror(on, { ...viewFiles(['claude:work']), ...Object.fromEntries(['grow', 'swap', 'pad'].map(n => [`${LIVE}/quota/claude/${n}.json`, named(n)])), ...NS })
  // each refresh lists clean files, then each is changed as it is read
  const pristine = { ...f.files }
  f.onList = d => {
    if (d !== LIVE + '/quota/claude') return
    for (const n of ['grow', 'swap', 'pad']) {
      const p = `${LIVE}/quota/claude/${n}.json`
      f.files[p] = pristine[p] as string
      delete f.over[p]
    }
  }
  f.onRead = p => {
    if (p.endsWith('/claude/grow.json')) f.files[p] = ' '.repeat(40000) + (f.files[p] as string)
    if (p.endsWith('/claude/swap.json')) f.over[p] = { isLink: true, kind: 'other' }
    if (p.endsWith('/claude/pad.json')) f.files[p] = (f.files[p] as string) + ' '
  }
  const ui = await openUsage($)
  const text = await flat(ui)
  expect(text).toContain('claude:work')
  expect(text).not.toContain('claude:grow')
  expect(text).not.toContain('claude:swap')
  expect(text).not.toContain('claude:pad')
  expect(text).toContain('3 筆無法讀取')
  await ui.unmount()
})

test('the reader rejects a namespace file that changed after the listing, and a provider directory that turned into a link', async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work', 'kimi:main']), ...NS })
  f.onRead = p => {
    if (p === LIVE + '/quota/namespace.json') f.files[p] = (f.files[p] as string) + ' '
  }
  // each refresh: the directory is clean when listed, then swapped for a link before it is entered
  f.onList = d => {
    if (d === LIVE + '/quota') delete f.over[LIVE + '/quota/kimi']
  }
  f.afterList = d => {
    if (d === LIVE + '/quota') f.over[LIVE + '/quota/kimi'] = { isLink: true, kind: 'other' }
  }
  const ui = await openUsage($)
  const text = await flat(ui)
  // the namespace is not trusted, the swapped provider is not listed
  expect(text).toContain('2 筆無法讀取')
  expect(text).toContain('claude:work')
  expect(text).not.toContain('kimi:main')
  await ui.unmount()
})

// ---- B45: shape c (token state only) is "no usage data", not "expired or reset" ----
const b45Rec = (alias: string, o: O): O => ({
  schema: 'fleet.quota/1', provider: 'b45', alias, visibility: 'L0', windows: [], refusal: null, token_state: 'unknown', token_state_at: null,
  host_role: 'agent', token_issued_at: null, token_expires_at: null, ...o,
})
const b45Row = (record: O): O => ({ writer_courier: 'local', host_role: 'agent', observed_at_hub: null, skewed: false, record })
const B45_FX: Record<string, O[]> = {
  'b45:only': [b45Row(b45Rec('only', { token_state: 'ok', token_state_at: N - 100 }))],
  'b45:out': [b45Row(b45Rec('out', { visibility: 'L3', windows: [win0(N, 'rolling_5h', 0.4, N - 25200, { resets_at: N + 3000 })] }))],
}
test('B45 pane and band: a shape-c record shows the neutral no-usage wording, an aged-out record keeps quotaDropped', () => {
  const st = stateOf(['b45:only', 'b45:out'], {}, B45_FX)
  const lines = paneText(st, 100)
  expect(block(lines, 'b45:only')).toEqual(['b45:only  L0', '  沒有用量資料'])
  expect(block(lines, 'b45:out')).toEqual(['b45:out  L0', '  過期或已重置，等新觀察'])
  expect(paneText(st, 100, en).join('\n')).toContain('no usage data')
  // compact lines stay the short word for both; the band counts both as 無資料
  expect(paneText(st, 40).some(l => l.startsWith('b45:only') && l.includes('無資料'))).toBe(true)
  expect(bandText(st)).toEqual(['無資料 2'])
})

// ---- the integrated panel (codeforge.panel/1, design v2): forge-stats writes `panel`, autopilot's shell draws it ----
const TONES = ['subtle', 'inactive', 'dim', 'bold', 'mono', 'success']
type PLine = { text: string; tone?: string }[]
const tierText = (lines: PLine[]) => lines.map(l => l.map(x => x.text).join(''))
// an inline autopilot with its panel open: it stamps `panel_open_at_ms` at session start (a register runs in its own environment, so the value is computed there)
// an inline autopilot whose panel shell reads `forge-stats.panel` while drawing, as the integrated panel will, and draws the value and its version as one Text
const AUTOPILOT_READER = {
  name: 'autopilot',
  register: ((on: any) => {
    on('ui.render', { component: 'Pane', requestId: 'live' }, async ($: any, e: any) => {
      const { Text } = $.ui.resolve(e)
      const got = await $.state.get({ plugin: 'forge-stats', key: 'panel' })
      return <Text key="v">{JSON.stringify({ version: got.version, value: got.value ?? null })}</Text>
    })
  }) as never,
}
const LIVE_PANE = { plugin: 'autopilot', component: 'Pane', requestId: 'live', surface: 'terminal', props: { title: 'live', isFocused: false, bodyColumns: 100, placement: 'dock', scroll: { offset: 0, bodyRows: 40 }, view: {} } } as const
const readPanel = async ($: Parameters<TestBody>[0]) => {
  const ui = await $.ui.mount(LIVE_PANE)
  const t = (await ui.find({ type: 'Text' })) as { text: string } | undefined
  await ui.unmount()
  return JSON.parse(t?.text ?? 'null') as { version: number; value: any }
}
const AUTOPILOT_PANEL_OPEN = {
  name: 'autopilot',
  register: ((on: any) => {
    on('session.start', async ($: any, e: any, next: any) => {
      await $.state.set({ plugin: 'autopilot', key: 'panel_open_at_ms' }, Date.now())
      return next(e)
    })
  }) as never,
}

test('panel state: the schema, one set of lines per tier width, the pane\'s own lines and tones, the sprite as pet.json has it', { plugins: [AUTOPILOT_READER] }, async ($, on) => {
  const withSprite = { ...PETSNAP, village: { ...PETSNAP.village, sprite: FERRIS } }
  mirror(on, { ...viewFiles(['claude:work', 'claude:home', 'kimi:main', 'agy:main']), ...NS, [LIVE + '/codeforge/pet.json']: JSON.stringify(withSprite) })
  mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const { value } = await readPanel($)
  expect(value.schema).toBe('codeforge.panel/1')
  expect(typeof value.updated_at_ms).toBe('number')
  expect(Object.keys(value.usage.tiers)).toEqual(['40', '60', '80', '100', '120'])
  expect(Object.keys(value.pet.tiers)).toEqual(['40', '60', '80', '100', '120'])
  // the usage lines are the pane's: the mirror blocks and today's models; below 60 columns the compact block form (one line per account, no blank separator rows)
  const wide = tierText(value.usage.tiers['100'])
  expect(wide.join('\n')).toContain('claude:work')
  expect(wide.join('\n')).toContain('claude-opus-5-5')
  expect(tierText(value.usage.tiers['40']).length).toBeLessThan(wide.length)
  for (const [w, lines] of Object.entries(value.usage.tiers) as [string, PLine[]][]) {
    for (const t of tierText(lines)) expect(displayWidth(t)).toBeLessThanOrEqual(Number(w))
  }
  // pet: the stats lines without the ASCII art; the sprite as forge-stats validated it from pet.json (palette as 0xRRGGBB, the shape of SpriteData)
  expect(tierText(value.pet.tiers['80']).join('\n')).toContain('Ferris')
  expect(tierText(value.pet.tiers['80']).join('\n')).not.toContain('_~^~^~_')
  expect(value.pet.sprite).toEqual(parseSprite(FERRIS))
  expect(value.pet.sprite).not.toBe(null)
  // tones: only the six of the contract
  const tones = new Set<string>()
  for (const lines of [...Object.values(value.usage.tiers), ...Object.values(value.pet.tiers)] as PLine[][]) for (const l of lines) for (const x of l) if (x.tone !== undefined) tones.add(x.tone)
  expect([...tones].every(t => TONES.includes(t))).toBe(true)
  expect(tones.has('dim')).toBe(true)
})

test('panel state is written again only when it changed (or every 30 s): an unchanged 15 s tick leaves the version', { plugins: [AUTOPILOT_READER] }, async ($, on) => {
  mirror(on, { ...viewFiles(['claude:work']), ...NS })
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const first = await readPanel($)
  expect(first.version > 0).toBe(true)
  expect(first.value.schema).toBe('codeforge.panel/1')
  await clock.advance(15_000)
  expect((await readPanel($)).version).toBe(first.version)
})

test('the quota mirror keeps refreshing while autopilot\'s panel is open, with forge-stats\' own band and pane never drawn', { plugins: [AUTOPILOT_PANEL_OPEN] }, async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work']), ...NS })
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const before = f.lists.filter(d => d === LIVE + '/quota').length
  await clock.advance(15_000)
  expect(f.lists.filter(d => d === LIVE + '/quota').length).toBeGreaterThan(before)
})

const AUTOPILOT_FUTURE_STAMP = {
  name: 'autopilot',
  register: ((on: any) => {
    on('session.start', async ($: any, e: any, next: any) => {
      await $.state.set({ plugin: 'autopilot', key: 'panel_open_at_ms' }, Number.MAX_SAFE_INTEGER)
      return next(e)
    })
  }) as never,
}

test('a panel_open_at_ms from the future counts as no panel: the undrawn 15 s tick reads no quota mirror', { plugins: [AUTOPILOT_FUTURE_STAMP] }, async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work']), ...NS })
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const before = f.lists.filter(d => d === LIVE + '/quota').length
  await clock.advance(15_000)
  expect(f.lists.filter(d => d === LIVE + '/quota').length).toBe(before)
})

test('without autopilot\'s panel and nothing drawn, the 15 s tick reads no quota mirror', async ($, on) => {
  const f = mirror(on, { ...viewFiles(['claude:work']), ...NS })
  const clock = mock.clock(on, { now: 1_000_000 })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  await clock.settle()
  const before = f.lists.filter(d => d === LIVE + '/quota').length
  await clock.advance(15_000)
  expect(f.lists.filter(d => d === LIVE + '/quota').length).toBe(before)
})
