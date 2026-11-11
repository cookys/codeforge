// G40 display of the quota mirror (pure: no `$`, no I/O). The model is ./quota.ts; the words come from ./strings.ts.
// Spec: doc/plans/2026-11-03-g40-usage-accounts.md §3-§6 and §10 (v4 deltas), TUI guideline §3.1-§3.5.

import { CHIP_GAP, CHIP_SPACE, chipParts, countdown, displayWidth, meter, padRight } from './model'
import type { SummaryPart } from './model'
import { QUOTA_CONTRACT, isTight, mergeQuota, tightest, tokenWarn } from './quota'
import type { MergedQuota, MergedWindow, QuotaView } from './quota'
import type { T } from './strings'

const Q = QUOTA_CONTRACT
const DAY_S = 86400
/** Show a window's age only past this (PM ruling 3). */
export const AGE_SHOW_S = 30 * 60
/** Below this pane width a block is one line (§6.2). */
export const COMPACT_COLS = 60
const GAUGE_KINDS: readonly string[] = ['rolling_5h', 'weekly', 'daily', 'monthly', 'credits']
const LABEL_W = 6
const METER_W = 10

// ---------------------------------------------------------------------------------------------
// the mirror root (`<live_dir>`, contract §21.3: reproduce autopilot's resolver)

export type QuotaEnv = { AUTOPILOT_LIVE_DIR?: string | undefined; XDG_RUNTIME_DIR?: string | undefined; HOME?: string | undefined }
export type LiveCandidate = { dir: string; source: 'override' | 'xdg' | 'xdg-inferred' | 'shm' | 'fallback' }

const trimSlash = (p: string) => (p.length > 1 ? p.replace(/\/+$/, '') : p)

/**
 * The directories `<live_dir>` can be, in autopilot's order: `$AUTOPILOT_LIVE_DIR` alone (an override is honoured, never replaced);
 * else `$XDG_RUNTIME_DIR/autopilot`, or (XDG unset) `/run/user/<uid>/autopilot`; then `/dev/shm/autopilot-<uid>`; last `~/.autopilot`.
 * A `/tmp` candidate is never listed: the mod cannot tell whether /tmp is RAM-backed, and a mirror found only there is not a mirror.
 * Every candidate but the override and the `~/.autopilot` fallback must exist to be chosen. An unknown uid drops the uid-named candidates.
 */
export function quotaLiveCandidates(env: QuotaEnv, uid: number | null): LiveCandidate[] {
  const over = env.AUTOPILOT_LIVE_DIR
  if (over) return [{ dir: trimSlash(over), source: 'override' }]
  const out: LiveCandidate[] = []
  const xdg = env.XDG_RUNTIME_DIR
  if (xdg) out.push({ dir: trimSlash(xdg) + '/autopilot', source: 'xdg' })
  else if (uid !== null) out.push({ dir: `/run/user/${uid}/autopilot`, source: 'xdg-inferred' })
  if (uid !== null) out.push({ dir: `/dev/shm/autopilot-${uid}`, source: 'shm' })
  if (env.HOME) out.push({ dir: trimSlash(env.HOME) + '/.autopilot', source: 'fallback' })
  return out
}

/** The first candidate that exists (the override and the fallback need no check); null when there is none. */
export function quotaLiveDir(env: QuotaEnv, uid: number | null, exists: (dir: string) => boolean): string | null {
  for (const c of quotaLiveCandidates(env, uid)) {
    if (c.source === 'override' || c.source === 'fallback' || exists(c.dir)) return c.dir
  }
  return null
}

// ---------------------------------------------------------------------------------------------
// the mirror listing (pure part of the reader: which names are views)

/** One `$.fs.list` row, as the engine gives it. */
export type DirEntry = { name: string; kind: 'file' | 'dir' | 'other'; size: number; isLink: boolean }
export type ViewFile = { provider: string; alias: string; name: string; size: number }
const PROVIDER_RE = new RegExp(Q.provider)
const ALIAS_RE = new RegExp(Q.alias)

export const QUOTA_MAX_PROVIDERS = 64
export const QUOTA_NS_MAX_BYTES = 4096

/** The provider directories to read (at most 64, in name order) and how many more there were. A name off the provider pattern, a link or a file is no provider. */
export function pickProviders(entries: readonly DirEntry[]): { names: string[]; skipped: number } {
  const all = entries.filter(e => e.kind === 'dir' && !e.isLink && PROVIDER_RE.test(e.name)).map(e => e.name).sort()
  return { names: all.slice(0, QUOTA_MAX_PROVIDERS), skipped: Math.max(0, all.length - QUOTA_MAX_PROVIDERS) }
}

/**
 * The view files of one provider directory. Silently skipped: anything not `<alias>.json` (lock files, temp files, dot files). `namespace.json` is metadata only at `quota/namespace.json`; inside a provider directory it is the alias `namespace`.
 * Counted as unreadable: a `<alias>.json` that is a link, not a regular file, over the view cap, or an alias off the pattern.
 */
export function pickViews(provider: string, entries: readonly DirEntry[]): { views: ViewFile[]; bad: number } {
  const views: ViewFile[] = []
  let bad = 0
  for (const e of [...entries].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
    if (!e.name.endsWith('.json') || e.name.startsWith('.')) continue
    const alias = e.name.slice(0, -'.json'.length)
    if (!ALIAS_RE.test(alias) || e.isLink || e.kind !== 'file' || e.size > Q.view_max_bytes) {
      bad++
      continue
    }
    views.push({ provider, alias, name: e.name, size: e.size })
  }
  return { views, bad }
}

/** `quota/namespace.json` (`fleet.quota-ns/1`): the namespace string, or null when it is anything else. */
export function parseQuotaNs(text: string | null): string | null {
  if (text === null || text.length > QUOTA_NS_MAX_BYTES) return null
  try {
    const v = JSON.parse(text) as unknown
    if (typeof v !== 'object' || v === null || Array.isArray(v)) return null
    const o = v as Record<string, unknown>
    if (o.schema !== Q.ns_schema || typeof o.namespace !== 'string' || o.namespace === '') return null
    return o.namespace
  } catch {
    return null
  }
}

// ---------------------------------------------------------------------------------------------
// what the reader keeps, and what a draw makes of it

export type QuotaRaw = { provider: string; alias: string; view: QuotaView }
export type QuotaState = { present: boolean; ns: string | null; raw: QuotaRaw[]; bad: number; skipped: number }
export const NO_QUOTA: QuotaState = { present: false, ns: null, raw: [], bad: 0, skipped: 0 }
export type QEntry = { provider: string; alias: string; key: string; m: MergedQuota }

/** Merge every view (§5.2) at `nowS`. A view with no row left (all rejected, or every peer row behind the namespace guard) is no entry. */
export function quotaEntries(state: QuotaState, nowS: number): QEntry[] {
  const out: QEntry[] = []
  for (const r of state.raw) {
    if (r.view.rows.length === 0) continue
    const m = mergeQuota(r.view, state.ns, nowS)
    if (!m.hadObservation && !m.placeholder) continue // only peer rows we may not use
    out.push({ provider: r.provider, alias: r.alias, key: `${r.provider}:${r.alias}`, m })
  }
  return out.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))
}

// ---------------------------------------------------------------------------------------------
// facts about one alias

type Token = { text: string; flag: boolean; invalid: boolean }
type Kind = 'live' | 'refusal' | 'never' | 'nousage' | 'dropped' | 'tokenonly'
export type Fact = { e: QEntry; kind: Kind; tok: Token | null; main: MergedWindow | null; tight: boolean; group: number }

function tokenOf(m: MergedQuota, nowS: number, tr: T): Token | null {
  if (m.tokenState === 'invalid') return { text: tr('quotaTokenInvalid'), flag: true, invalid: true }
  if (m.tokenExpiresAt === null) return null
  const w = tokenWarn(m.tokenExpiresAt, nowS)
  if (w?.level === 'expired') return { text: tr('quotaTokenExpired'), flag: true, invalid: false }
  if (w?.level === 'urgent') return { text: tr('quotaTokenExpires', { n: w.days }), flag: true, invalid: false }
  const days = w?.days ?? Math.ceil((m.tokenExpiresAt - nowS) / DAY_S)
  return { text: tr('quotaTokenExpires', { n: days }), flag: false, invalid: false }
}

export function factsOf(entries: readonly QEntry[], nowS: number, tr: T): Fact[] {
  const facts = entries.map((e): Fact => {
    const m = e.m
    const tok = tokenOf(m, nowS, tr)
    let kind: Kind
    if (tok?.invalid) kind = 'tokenonly'
    else if (m.windows.length > 0) kind = 'live'
    else if (m.refusal) kind = 'refusal'
    else if (m.placeholder) kind = 'never'
    else if (!m.hadWindow) kind = 'nousage'
    else kind = 'dropped'
    const tw = kind === 'live' ? tightest(m) : null
    const main = tw ?? (kind === 'live' ? (m.windows.find(w => w.kind !== 'rpm' && w.kind !== 'tpm') ?? m.windows[0] ?? null) : null)
    const tight = tw !== null && tw.utilization !== null && isTight(tw.utilization)
    // §6.2 block order: ▲, tight, 429, the rest; L0 and dropped last
    const group = tok?.flag ? 0 : tight ? 1 : m.refusal ? 2 : kind === 'never' || kind === 'nousage' || kind === 'dropped' ? 4 : 3
    return { e, kind, tok, main, tight, group }
  })
  return facts.sort((a, b) => a.group - b.group || (a.e.key < b.e.key ? -1 : a.e.key > b.e.key ? 1 : 0))
}

// ---------------------------------------------------------------------------------------------
// shared text

const pctOf = (u: number) => `${Math.round(u * 100)}%`
const withCommas = (s: string) => s.replace(/\B(?=(\d{3})+(?!\d))/g, ',')
export function fmtNum(n: number): string {
  if (Number.isInteger(n)) return withCommas(String(n))
  const [i, f] = n.toFixed(2).split('.')
  return withCommas(i as string) + '.' + f
}
const leftOf = (w: MergedWindow): number | null => (w.remaining !== null ? w.remaining : w.used !== null && w.limit !== null ? Math.max(0, w.limit - w.used) : null)
const KIND_KEY = {
  rolling_5h: 'quotaW5h', weekly: 'quotaWWeekly', daily: 'quotaWDaily', monthly: 'quotaWMonthly', credits: 'quotaWCredits', rpm: 'quotaWRpm', tpm: 'quotaWTpm',
} as const
const labelOf = (w: MergedWindow, tr: T) => tr(KIND_KEY[w.kind])
const isGauge = (w: MergedWindow) => GAUGE_KINDS.includes(w.kind)
const winTight = (w: MergedWindow) => isGauge(w) && w.utilization !== null && isTight(w.utilization)
const ageText = (ageS: number, nowMs: number, tr: T) => tr('quotaAge', { t: countdown(nowMs + ageS * 1000, nowMs) })

/** `62%`, `估 41%`; the tight word is a separate field. */
const pctText = (w: MergedWindow, tr: T) => (w.estimated ? tr('quotaEst') + ' ' : '') + pctOf(w.utilization as number)

// ---------------------------------------------------------------------------------------------
// pane lines

export type QSeg = { text: string; role?: 'structure' | 'idle' | 'dim' | 'bold' | 'mono'; color?: string }
export type QLine = QSeg[]
const seg = (text: string, role?: QSeg['role'], color?: string): QSeg => ({ text, ...(role ? { role } : {}), ...(color ? { color } : {}) })
const width = (l: QLine) => l.reduce((n, s) => n + displayWidth(s.text), 0)
const fit = (forms: QLine[], cols: number): QLine => forms.find(f => width(f) <= cols) ?? (forms[forms.length - 1] as QLine)

type WinForm = { age: boolean; fullReset: boolean; meter: boolean }
const WIN_FORMS: WinForm[] = [
  { age: true, fullReset: true, meter: true },
  { age: false, fullReset: true, meter: true },
  { age: false, fullReset: false, meter: true },
  { age: false, fullReset: false, meter: false },
]

function winLine(w: MergedWindow, nowMs: number, tr: T, f: WinForm): QLine {
  const out: QLine = [seg('  '), seg(padRight(labelOf(w, tr), LABEL_W), 'dim')]
  if (w.utilization === null) {
    // credits, remaining only: a number and nothing to gauge (§3)
    out.push(seg(tr('quotaLeft', { n: fmtNum(w.remaining ?? 0), unit: w.unit ?? '' }).trim()))
  } else {
    if (f.meter && isGauge(w)) {
      const m = meter(w.utilization, METER_W)
      out.push(seg(m.on), seg(m.off, 'idle'), seg('  '))
    }
    out.push(seg(pctText(w, tr)))
    if (winTight(w)) out.push(seg(' ' + tr('quotaTight')))
    const left = w.kind === 'credits' ? leftOf(w) : null
    if (left !== null) out.push(seg('  ' + tr('quotaLeft', { n: fmtNum(left), unit: w.unit ?? '' })))
  }
  if (w.resetsAtLocal !== null) out.push(seg('  ◷ ' + countdown(w.resetsAtLocal * 1000, nowMs) + (f.fullReset ? ' ' + tr('quotaReset') : ''), 'dim'))
  else if (w.utilization !== null && isGauge(w)) out.push(seg('  ' + tr('quotaResetUnknown'), 'dim'))
  if (f.age && w.ageS > AGE_SHOW_S) out.push(seg(' · ' + ageText(w.ageS, nowMs, tr), 'dim'))
  return out
}

function refusalLine(m: MergedQuota, nowMs: number, tr: T, full: boolean): QLine {
  const r = m.refusal as NonNullable<MergedQuota['refusal']>
  return [seg('  '), seg(tr('quotaCooldown'), 'idle'), seg('  ◷ ' + countdown(r.endS * 1000, nowMs) + (full ? ' ' + tr('quotaRetry') : ''), 'dim')]
}

function headerLines(f: Fact, cols: number, tr: T): QLine {
  const m = f.e.m
  const name = seg(f.e.key, 'bold')
  const dot = () => seg(' · ', 'dim')
  const host = m.multiHost ? seg(tr('quotaHost', { roles: [...new Set(m.hosts)].join(tr('quotaRoleSep')) }), 'dim') : null
  const tok: QSeg[] =
    f.tok === null ? [] : f.tok.flag ? [seg('▲', undefined, 'warning'), seg(' ' + f.tok.text)] : [seg(f.tok.text, 'dim')]
  const join = (parts: (QSeg | QSeg[])[]): QLine => {
    const out: QLine = [name, seg('  '), seg(m.level, 'dim')]
    for (const p of parts) out.push(dot(), ...(Array.isArray(p) ? p : [p]))
    return out
  }
  const forms: QLine[] = []
  forms.push(join([...(host ? [host] : []), ...(tok.length ? [tok] : [])]))
  if (host) forms.push(join(tok.length ? [tok] : []))
  if (f.tok && !f.tok.flag) forms.push(join([]))
  else if (f.tok) forms.push(join([tok]))
  forms.push(join([]))
  return fit(forms, cols)
}

function compactLine(f: Fact, nowMs: number, cols: number, tr: T): QLine {
  const m = f.e.m
  const name = seg(f.e.key, 'bold')
  const flag: QLine = f.tok?.flag ? [seg(' '), seg('▲', undefined, 'warning'), seg(' ' + f.tok.text)] : []
  const state = (cd: boolean, age: boolean): QLine => {
    const w = f.main
    if (m.refusal) return [seg(' '), seg(tr('quotaCooldown'), 'idle'), ...(cd ? [seg(' ◷ ' + countdown(m.refusal.endS * 1000, nowMs), 'dim')] : [])]
    if (f.kind === 'never' || f.kind === 'nousage' || f.kind === 'dropped') return [seg(' '), seg(tr('quotaNoData'), 'idle')]
    if (f.kind === 'tokenonly' || w === null) return []
    if (w.utilization === null) return [seg(' ' + labelOf(w, tr) + ' ' + tr('quotaLeft', { n: fmtNum(w.remaining ?? 0), unit: w.unit ?? '' }))]
    return [
      seg(' ' + labelOf(w, tr) + ' ' + pctText(w, tr) + (winTight(w) ? ' ' + tr('quotaTight') : '')),
      ...(cd && w.resetsAtLocal !== null ? [seg(' ◷ ' + countdown(w.resetsAtLocal * 1000, nowMs), 'dim')] : []),
      ...(age && w.ageS > AGE_SHOW_S ? [seg(' · ' + ageText(w.ageS, nowMs, tr), 'dim')] : []),
    ]
  }
  return fit(
    [[name, ...state(true, true), ...flag], [name, ...state(true, false), ...flag], [name, ...state(false, false), ...flag], [name, ...state(false, false)], [name]],
    cols,
  )
}

/** The 用量 pane's quota section: one block per (provider, alias); the footer carries the unreadable count and the 估/緊 legend. */
export function quotaPaneLines(state: QuotaState, entries: readonly QEntry[], cols: number, nowMs: number, tr: T): QLine[] {
  const nowS = Math.floor(nowMs / 1000)
  const facts = factsOf(entries, nowS, tr)
  const out: QLine[] = []
  let usedEst = false
  let usedTight = false
  const compact = cols < COMPACT_COLS
  if (facts.length === 0) out.push([seg('  ' + tr('quotaEmpty'), 'dim')])
  facts.forEach((f, i) => {
    const m = f.e.m
    if (compact) {
      out.push(compactLine(f, nowMs, cols, tr))
    } else {
      if (i > 0) out.push([seg(' ')])
      out.push(headerLines(f, cols, tr))
      if (f.kind === 'never') out.push([seg('  ' + tr('quotaNever'), 'idle')])
      else if (f.kind === 'nousage') out.push([seg('  ' + tr('quotaNoUsage'), 'idle')])
      else if (f.kind === 'dropped') out.push([seg('  ' + tr('quotaDropped'), 'idle')])
      else if (f.kind !== 'tokenonly') {
        if (m.refusal) out.push(refusalLine(m, nowMs, tr, true))
        for (const w of m.windows) out.push(fit(WIN_FORMS.map(wf => winLine(w, nowMs, tr, wf)), cols))
      }
    }
    if (f.kind === 'live') {
      for (const w of m.windows) {
        if (w.estimated) usedEst = true
        if (winTight(w)) usedTight = true
      }
    }
  })
  if (state.bad > 0) out.push([seg('  ' + tr('quotaBad', { n: state.bad }), 'dim')])
  if (state.skipped > 0) out.push([seg('  ' + tr('quotaSkipped', { n: state.skipped }), 'dim')])
  const legend = [...(usedEst ? [tr('quotaLegendEst')] : []), ...(usedTight ? [tr('quotaLegendTight')] : [])]
  if (legend.length > 0) out.push([seg(' ')], [seg(legend.join(tr('quotaSep')), 'dim')])
  return out
}

// ---------------------------------------------------------------------------------------------
// band chips (§6.1)

export type QBandChip = {
  token: boolean
  key: string
  alias: string
  /** `5h 62%`, `週 91% 緊`, `429 冷卻`; for a ▲ chip, the token text. */
  head: string
  /** the alias-only form's value: `62%`, `估 41%`, `429 冷卻` */
  short: string
  inactive: boolean
  reset: string | null
  age: string | null
}
export type QBand = { chips: QBandChip[]; noData: number; noDataWord: string }

/** Chips in §6.1 order: ▲ token, tight, 429, the rest by utilization; plus the count of aliases with nothing to draw. */
export function quotaBand(entries: readonly QEntry[], nowMs: number, tr: T): QBand {
  const nowS = Math.floor(nowMs / 1000)
  const facts = factsOf(entries, nowS, tr)
  type Ranked = { rank: number; util: number; chip: QBandChip }
  const ranked: Ranked[] = []
  let noData = 0
  for (const f of facts) {
    const m = f.e.m
    if (f.tok?.flag) {
      ranked.push({ rank: 0, util: 0, chip: { token: true, key: f.e.key, alias: f.e.alias, head: f.tok.text, short: f.tok.text, inactive: false, reset: null, age: null } })
    }
    if (f.kind === 'tokenonly') continue
    if (m.refusal) {
      ranked.push({
        rank: f.tight ? 1 : 2, util: 2,
        chip: { token: false, key: f.e.key, alias: f.e.alias, head: tr('quotaCooldown'), short: tr('quotaCooldown'), inactive: true, reset: countdown(m.refusal.endS * 1000, nowMs), age: null },
      })
      continue
    }
    if (f.kind === 'never' || f.kind === 'nousage' || f.kind === 'dropped') {
      noData++
      continue
    }
    const w = tightest(m)
    if (w === null || w.utilization === null) continue // remaining-only credits, rpm/tpm: pane only
    // credits (design §4): what is left when the amounts are known, else what is used; other kinds show the percentage
    const left = w.kind === 'credits' ? leftOf(w) : null
    const value =
      w.kind !== 'credits' ? pctText(w, tr)
      : left !== null ? (w.estimated ? tr('quotaEst') + ' ' : '') + tr('quotaLeftBand', { n: fmtNum(left) })
      : tr('quotaUsedPct', { p: pctText(w, tr) })
    ranked.push({
      rank: f.tight ? 1 : 3, util: w.utilization,
      chip: {
        token: false, key: f.e.key, alias: f.e.alias,
        head: `${labelOf(w, tr)} ${value}${f.tight ? ' ' + tr('quotaTight') : ''}`,
        short: value, inactive: false,
        reset: w.resetsAtLocal === null ? null : countdown(w.resetsAtLocal * 1000, nowMs),
        age: w.ageS > AGE_SHOW_S ? ageText(w.ageS, nowMs, tr) : null,
      },
    })
  }
  ranked.sort((a, b) => a.rank - b.rank || b.util - a.util || (a.chip.key < b.chip.key ? -1 : a.chip.key > b.chip.key ? 1 : 0))
  return { chips: ranked.map(r => r.chip), noData, noDataWord: tr('quotaNoData') }
}

type ChipForm = { age: boolean; countdown: boolean }
function chipPartsOf(c: QBandChip, label: string, f: ChipForm, aliasOnly: boolean): SummaryPart[] {
  if (c.token) return chipParts('▲', 'warning', `${label} ${c.head}`)
  const head: SummaryPart = c.inactive ? { text: aliasOnly ? c.short : c.head, color: 'inactive' } : { text: aliasOnly ? c.short : c.head }
  const out: SummaryPart[] = [{ text: label }, { text: CHIP_SPACE }, head]
  if (!aliasOnly && f.countdown && c.reset !== null) out.push({ text: CHIP_SPACE }, ...chipParts('◷', undefined, c.reset))
  if (!aliasOnly && f.age && c.age !== null) out.push({ text: ' · ', dim: true }, { text: c.age })
  return out
}
const noDataParts = (b: QBand): SummaryPart[] => [{ text: b.noDataWord, color: 'inactive' }, { text: CHIP_SPACE }, { text: String(b.noData), color: 'inactive' }]
const textOf = (parts: readonly SummaryPart[]) => parts.map(p => p.text).join('')

/**
 * The quota group, widest form first (§6.1 drop order): everything; no `無資料 N`; no ages; no countdowns; no `provider:` prefixes (where the alias is unique); whole chips from the tail until one is left;
 * that one as `alias 62%`. (The pet is the caller's: it goes before all of these.) The chips are CHIP_GAP apart; nothing is cut inside a chip.
 */
export function quotaBandForms(b: QBand): SummaryPart[][] {
  const forms: SummaryPart[][] = []
  const all = b.chips
  // a provider prefix may go only when no other chip of the whole band shares the alias (§6.1); a collision keeps `provider:`
  const keysOf = new Map<string, Set<string>>()
  for (const c of all) keysOf.set(c.alias, (keysOf.get(c.alias) ?? new Set<string>()).add(c.key))
  const labelOf2 = (c: QBandChip, bare: boolean) => (bare && (keysOf.get(c.alias)?.size ?? 0) === 1 ? c.alias : c.key)
  const group = (chips: QBandChip[], f: ChipForm, withNoData: boolean, bare = false): SummaryPart[] => {
    const parts: SummaryPart[][] = chips.map(c => chipPartsOf(c, labelOf2(c, bare), f, false))
    if (withNoData && b.noData > 0) parts.push(noDataParts(b))
    return parts.flatMap((p, i) => (i === 0 ? p : [{ text: CHIP_GAP }, ...p]))
  }
  forms.push(group(all, { age: true, countdown: true }, true))
  forms.push(group(all, { age: true, countdown: true }, false))
  forms.push(group(all, { age: false, countdown: true }, false))
  forms.push(group(all, { age: false, countdown: false }, false))
  // then the prefixes, before any whole chip goes; the chips that follow keep them off
  forms.push(group(all, { age: false, countdown: false }, false, true))
  for (let n = all.length - 1; n >= 1; n--) forms.push(group(all.slice(0, n), { age: false, countdown: false }, false, true))
  const first = all[0]
  if (first !== undefined) {
    // the alias always stays; the provider prefix goes only when no other chip shares the alias
    const unique = new Set(all.filter(c => c.alias === first.alias).map(c => c.key)).size === 1
    forms.push(chipPartsOf(first, unique ? first.alias : first.key, { age: false, countdown: false }, true))
  }
  const seen = new Set<string>()
  return forms.filter(f => {
    const t = textOf(f)
    if (t === '' || seen.has(t)) return false
    seen.add(t)
    return true
  })
}
