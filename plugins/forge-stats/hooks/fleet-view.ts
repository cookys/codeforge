// The 全部帳號 / all-accounts pane tab: reads `codeforge.fleet-quota/1` (written by `codeforge usage collect` when CODEFORGE_FLEET_USAGE_DIR is
// set), pure: no `$`, no I/O. The JSON already carries the warnings and projections; nothing is recomputed here, a stale file is shown with its age.
// Spec: doc/specs/codeforge-usage.md (全帳號配額表), doc/specs/codeforge-mod-ui.md.

import { countdown, displayWidth, padRight } from './model'
import { clipCells } from './pane-view'
import { AGE_SHOW_S } from './quota-view'
import type { QLine, QSeg } from './quota-view'
import { isTight } from './quota'
import type { T } from './strings'

export const FLEET_SCHEMA = 'codeforge.fleet-quota/1'
export const FLEET_MAX_BYTES = 512 * 1024
export const FLEET_MAX_ACCOUNTS = 256
export const fleetQuotaPath = (base: string) => `${base}/codeforge/usage/fleet-quota.json`

export type FleetWin = { pct: number; resetsAt: number }
export type FleetWeekly = FleetWin & { warn: 'waste' | 'short' | null; exhaustAt: number | null }
export type FleetState = 'ok' | 'stale' | 'expired' | 'no_source'
export type FleetAccount = {
  alias: string
  /** `personal` or an org name; null when unknown. */
  org: string | null
  hosts: string[]
  observedAtMs: number | null
  state: FleetState
  weekly: FleetWeekly | null
  fiveHour: FleetWin | null
}
export type FleetQuotaView =
  | { status: 'ok'; generatedAt: number; useTarget: number; skipped: number; accounts: FleetAccount[] }
  | { status: 'missing' }
  | { status: 'bad' }

const BAD: FleetQuotaView = { status: 'bad' }
const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
const onlyKeys = (o: Record<string, unknown>, keys: string[]) => Object.keys(o).every(k => keys.includes(k))
const isInt = (v: unknown): v is number => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0
const isPct = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 1.5
// printable, bounded: a label that reaches the screen
const isLabel = (v: unknown): v is string => typeof v === 'string' && /^[^\u0000-\u001f\u007f]{1,64}$/.test(v)

function win(v: unknown): FleetWin | null | undefined {
  if (v === null) return null
  if (!isObj(v) || !onlyKeys(v, ['pct', 'resets_at']) || !isPct(v.pct) || !isInt(v.resets_at)) return undefined
  return { pct: v.pct, resetsAt: v.resets_at }
}

function weekly(v: unknown): FleetWeekly | null | undefined {
  if (v === null) return null
  if (!isObj(v) || !onlyKeys(v, ['pct', 'resets_at', 'projected', 'warn', 'exhaust_at'])) return undefined
  const w = win({ pct: v.pct, resets_at: v.resets_at })
  if (!w) return undefined
  if (v.projected !== null && !(typeof v.projected === 'number' && Number.isFinite(v.projected) && v.projected >= 0)) return undefined
  if (v.warn !== null && v.warn !== 'waste' && v.warn !== 'short') return undefined
  if (v.exhaust_at !== null && !isInt(v.exhaust_at)) return undefined
  return { ...w, warn: v.warn, exhaustAt: v.exhaust_at }
}

/** Strict parse of `codeforge.fleet-quota/1`: anything else is `bad`; `null` text is `missing`. Never throws. */
export function parseFleetQuota(text: string | null): FleetQuotaView {
  if (text === null) return { status: 'missing' }
  if (text.length > FLEET_MAX_BYTES) return BAD
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return BAD
  }
  if (!isObj(raw) || raw.schema !== FLEET_SCHEMA || !onlyKeys(raw, ['schema', 'generated_at_ms', 'use_target', 'skipped', 'accounts'])) return BAD
  if (!isInt(raw.generated_at_ms) || typeof raw.use_target !== 'number' || !(raw.use_target > 0 && raw.use_target <= 1)) return BAD
  if (!isObj(raw.skipped) || !Object.values(raw.skipped).every(isInt)) return BAD
  if (!Array.isArray(raw.accounts) || raw.accounts.length > FLEET_MAX_ACCOUNTS) return BAD
  const accounts: FleetAccount[] = []
  for (const a of raw.accounts as unknown[]) {
    if (!isObj(a) || !onlyKeys(a, ['alias', 'provider', 'hosts', 'observed_at_ms', 'age_s', 'state', 'weekly', 'five_hour', 'org'])) return BAD
    if (a.org !== undefined && a.org !== null && !isLabel(a.org)) return BAD
    if (!isLabel(a.alias) || !isLabel(a.provider)) return BAD
    if (!Array.isArray(a.hosts) || a.hosts.length > 64 || !a.hosts.every(isLabel)) return BAD
    if (a.observed_at_ms !== null && !isInt(a.observed_at_ms)) return BAD
    if (a.age_s !== null && !isInt(a.age_s)) return BAD
    if (a.state !== 'ok' && a.state !== 'stale' && a.state !== 'expired' && a.state !== 'no_source') return BAD
    const w = weekly(a.weekly)
    const f = win(a.five_hour)
    if (w === undefined || f === undefined) return BAD
    accounts.push({ alias: a.alias, org: typeof a.org === 'string' ? a.org : null, hosts: a.hosts as string[], observedAtMs: a.observed_at_ms, state: a.state, weekly: w, fiveHour: f })
  }
  return {
    status: 'ok',
    generatedAt: raw.generated_at_ms,
    useTarget: raw.use_target,
    skipped: Object.values(raw.skipped).reduce((n, c) => n + (c as number), 0),
    accounts,
  }
}

const seg = (text: string, role?: QSeg['role'], color?: string): QSeg => ({ text, ...(role ? { role } : {}), ...(color ? { color } : {}) })
const width = (l: QLine) => l.reduce((n, s) => n + displayWidth(s.text), 0)
const pctText = (p: number) => `${Math.round(p * 100)}%`
const SEP = ' · '

/** The widest form that fits; when none does, the last (shortest) cut to `cols` with `…`. */
function fit(forms: QLine[], cols: number): QLine {
  const hit = forms.find(f => width(f) <= cols)
  if (hit) return hit
  const last = forms[forms.length - 1] as QLine
  let room = Math.max(0, cols - 1)
  const out: QLine = []
  for (const s of last) {
    const text = clipCells(s.text, room)
    room -= displayWidth(text)
    if (text !== '') out.push({ ...s, text })
    if (text !== s.text) {
      out.push(seg('…', 'dim'))
      break
    }
  }
  return out
}

/** The alias, then the org (`personal` shown as 個人 / personal) when known. */
const label = (a: FleetAccount, tr: T) => (a.org === null ? a.alias : `${a.alias} ${a.org === 'personal' ? tr('fleetPersonal') : a.org}`)

function warnLine(a: FleetAccount, useTarget: number, cols: number, tr: T, now: number): QLine | null {
  const w = a.weekly
  if (w === null || w.warn === null) return null
  const head: QLine = [seg(tr(w.warn === 'waste' ? 'fleetWaste' : 'fleetShort'), 'bold', 'warning'), seg(SEP, 'dim'), seg(label(a, tr), 'bold'), seg(SEP, 'dim'), seg(`${tr('quotaWWeekly')} ${pctText(w.pct)}`)]
  const runout: QLine = w.warn === 'short' && w.exhaustAt !== null ? [seg(SEP, 'dim'), seg(tr('fleetRunout', { t: countdown(w.exhaustAt * 1000, now) }))] : []
  const reset: QLine = [seg(SEP, 'dim'), seg(`${tr('fleetReset')} ◷ ${countdown(w.resetsAt * 1000, now)}`, 'dim')]
  const target: QLine = w.warn === 'waste' ? [seg(SEP, 'dim'), seg(tr('fleetTarget', { p: pctText(useTarget) }), 'dim')] : []
  return fit([[...head, ...runout, ...reset, ...target], [...head, ...runout, ...reset], [...head, ...runout], head], cols)
}

function accountLine(a: FleetAccount, aliasW: number, cols: number, tr: T, now: number): QLine {
  const name = seg(padRight(clipCells(label(a, tr), aliasW), aliasW + 2), a.state === 'ok' ? undefined : 'idle')
  const age: QLine =
    a.observedAtMs !== null && (now - a.observedAtMs) / 1000 > AGE_SHOW_S
      ? [seg('  ' + tr('quotaAge', { t: countdown(now + (now - a.observedAtMs), now) }), 'dim')]
      : []
  const hosts: QLine = a.hosts.length > 0 ? [seg('  ' + a.hosts.join(','), 'dim')] : []
  const tight = (p: number): QLine => (isTight(p) ? [seg(' ' + tr('quotaTight'), 'bold', 'warning')] : [])
  const word = (id: 'fleetNoSource' | 'fleetExpired' | 'fleetStale'): QLine => [seg(tr(id), 'idle')]
  if (a.state !== 'ok') {
    const w = word(a.state === 'no_source' ? 'fleetNoSource' : a.state === 'expired' ? 'fleetExpired' : 'fleetStale')
    return fit([[name, ...w, ...hosts, ...age], [name, ...w, ...age], [name, ...w]], cols)
  }
  const wk = a.weekly
  const five = a.fiveHour
  const W: QLine = wk === null ? [] : [seg(`${tr('quotaWWeekly')} ${pctText(wk.pct)}`), ...tight(wk.pct)]
  const R: QLine = wk === null ? [] : [seg(` ◷ ${countdown(wk.resetsAt * 1000, now)}`, 'dim')]
  const F: QLine = five === null ? [] : [seg((wk === null ? '' : '  ') + `${tr('quotaW5h')} ${pctText(five.pct)}`), ...tight(five.pct)]
  // the widest form first; a field is dropped whole: hosts, the age, the 5h window, then the countdown
  return fit(
    [
      [name, ...W, ...R, ...F, ...hosts, ...age],
      [name, ...W, ...R, ...F, ...age],
      [name, ...W, ...R, ...F],
      [name, ...W, ...R],
      [name, ...(W.length > 0 ? W : F)],
    ],
    cols,
  )
}

/** The tab's lines (no header rule: the caller draws it with the file age). Every line is at most `cols` cells. */
export function fleetLines(view: FleetQuotaView, cols: number, tr: T, now: number): QLine[] {
  if (view.status !== 'ok') return [[seg('  ' + tr(view.status === 'bad' ? 'fleetBad' : 'fleetMissing'), 'dim')]]
  const out: QLine[] = []
  const warns = view.accounts.map(a => warnLine(a, view.useTarget, cols, tr, now)).filter((l): l is QLine => l !== null)
  if (warns.length === 0) out.push([seg(clipCells(tr('fleetNoWarn'), cols), 'dim')])
  else out.push(...warns)
  out.push([seg(' ')])
  const aliasW = Math.min(28, Math.max(0, ...view.accounts.map(a => displayWidth(label(a, tr)))))
  for (const a of view.accounts) out.push(accountLine(a, aliasW, cols, tr, now))
  if (view.skipped > 0) out.push([seg(clipCells('  ' + tr('fleetSkipped', { n: view.skipped }), cols), 'dim')])
  return out
}
