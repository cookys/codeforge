// The 全部帳號 / all-accounts pane tab: reads `codeforge.fleet-quota/1` (written by `codeforge usage collect` when CODEFORGE_FLEET_USAGE_DIR is
// set), pure: no `$`, no I/O. The JSON already carries the warnings and projections; nothing is recomputed here, a stale file is shown with its age.
// Spec: doc/specs/codeforge-usage.md (全帳號配額表), doc/specs/codeforge-mod-ui.md.

import { countdown, displayWidth, keyDefault, padRight } from './model'
import { clipCells } from './pane-view'
import { AGE_SHOW_S } from './quota-view'
import type { QLine, QSeg } from './quota-view'
import { isTight } from './quota'
import type { T } from './strings'

export const FLEET_SCHEMA = 'codeforge.fleet-quota/1'
export const FLEET_MAX_BYTES = 512 * 1024
export const FLEET_MAX_ACCOUNTS = 256
export const fleetQuotaPath = (base: string) => `${base}/codeforge/usage/fleet-quota.json`

/** `reset`: the window's reset time has passed; its pct is not current and it is never warned on. */
export type FleetWin = { pct: number; resetsAt: number; reset: boolean }
export type FleetWeekly = FleetWin & { warn: 'waste' | 'short' | null; exhaustAt: number | null }
export type FleetState = 'ok' | 'stale' | 'expired' | 'no_source'
/** Why a `no_source` row has no data (a closed set). */
export type FleetSourceNote = 'web_only' | 'needs_token' | 'no_window'
export type FleetAccount = {
  /** Internal key only: never drawn. */
  alias: string
  /** What the tab shows: codeforge's display name (the operator's name, else the org short form). */
  name: string
  /** `personal` or an org name; null when unknown. */
  org: string | null
  /** The login email; null when unknown (the alias then names the row). */
  email: string | null
  hosts: string[]
  /** The hosts whose data lost the merge (an older, disagreeing capture); a subset of `hosts`. */
  oldHosts: string[]
  observedAtMs: number | null
  state: FleetState
  weekly: FleetWeekly | null
  fiveHour: FleetWin | null
  /** Only on a `no_source` row; null from a writer before the key. */
  sourceNote: FleetSourceNote | null
  /** The operator's local note for the row; null when none. */
  note: string | null
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
// the login email, the same rule as codeforge's `valid_email` (and the summary contract): 3..=254, one `@` with text both sides, no space or control char
const isEmail = (v: unknown): v is string => typeof v === 'string' && /^(?=.{3,254}$)[^@\s\u0000-\u001f\u007f-\u009f\uFEFF]+@[^@\s\u0000-\u001f\u007f-\u009f\uFEFF]+$/u.test(v)
// printable, bounded: a label that reaches the screen
const isLabel = (v: unknown): v is string => typeof v === 'string' && /^[^\u0000-\u001f\u007f]{1,64}$/.test(v)
const SOURCE_NOTES: readonly string[] = ['web_only', 'needs_token', 'no_window']
const isSourceNote = (v: unknown): v is FleetSourceNote => typeof v === 'string' && SOURCE_NOTES.includes(v)
/** The widest an operator note may be, the same rule as codeforge's `valid_note`. */
export const NOTE_MAX = 40
// non-empty, trimmed, at most NOTE_MAX cells, no control char, no `@`, no BOM, no whitespace but the space
const isNote = (v: unknown): v is string =>
  typeof v === 'string' && v !== '' && v === v.trim() && !/[\u0000-\u001f\u007f-\u009f@\uFEFF]|[^\S ]/u.test(v) && displayWidth(v) <= NOTE_MAX

function win(v: unknown): FleetWin | null | undefined {
  if (v === null) return null
  if (!isObj(v) || !onlyKeys(v, ['pct', 'resets_at', 'reset']) || !isPct(v.pct) || !isInt(v.resets_at)) return undefined
  if (v.reset !== undefined && typeof v.reset !== 'boolean') return undefined
  return { pct: v.pct, resetsAt: v.resets_at, reset: v.reset === true }
}

function weekly(v: unknown): FleetWeekly | null | undefined {
  if (v === null) return null
  if (!isObj(v) || !onlyKeys(v, ['pct', 'resets_at', 'projected', 'warn', 'exhaust_at', 'reset'])) return undefined
  const w = win({ pct: v.pct, resets_at: v.resets_at, reset: v.reset })
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
    if (!isObj(a) || !onlyKeys(a, ['alias', 'provider', 'hosts', 'observed_at_ms', 'age_s', 'state', 'weekly', 'five_hour', 'org', 'email', 'old_hosts', 'name', 'source_note', 'note'])) return BAD
    if (a.source_note !== undefined && (a.state !== 'no_source' || !isSourceNote(a.source_note))) return BAD
    if (a.note !== undefined && !isNote(a.note)) return BAD
    if (a.name !== undefined && !isLabel(a.name)) return BAD
    if (a.org !== undefined && a.org !== null && !isLabel(a.org)) return BAD
    if (a.email !== undefined && a.email !== null && !isEmail(a.email)) return BAD
    if (!isLabel(a.alias) || !isLabel(a.provider)) return BAD
    if (!Array.isArray(a.hosts) || a.hosts.length > 64 || !a.hosts.every(isLabel)) return BAD
    if (a.old_hosts !== undefined && (!Array.isArray(a.old_hosts) || a.old_hosts.length > 64 || !a.old_hosts.every(isLabel) || !a.old_hosts.every(h => (a.hosts as string[]).includes(h as string)))) return BAD
    if (a.observed_at_ms !== null && !isInt(a.observed_at_ms)) return BAD
    if (a.age_s !== null && !isInt(a.age_s)) return BAD
    if (a.state !== 'ok' && a.state !== 'stale' && a.state !== 'expired' && a.state !== 'no_source') return BAD
    const w = weekly(a.weekly)
    const f = win(a.five_hour)
    if (w === undefined || f === undefined) return BAD
    // a writer from before `name`: the email, else the org, else the alias's default name (`codex` for
    // `codex-<hex>`); an `acct-` hash with none of these is an identity from before the login-derived alias: not shown
    const name = typeof a.name === 'string' ? keyDefault(a.name)
      : typeof a.email === 'string' ? a.email
      : typeof a.org === 'string' ? a.org
      : keyDefault(a.alias)
    if (name === null) continue
    accounts.push({ alias: a.alias, name, org: typeof a.org === 'string' ? a.org : null, email: typeof a.email === 'string' ? a.email : null, hosts: a.hosts as string[], oldHosts: (a.old_hosts ?? []) as string[], observedAtMs: a.observed_at_ms, state: a.state, weekly: w, fiveHour: f, sourceNote: isSourceNote(a.source_note) ? a.source_note : null, note: isNote(a.note) ? a.note : null })
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
/** The widest the name column (name · email) may be; a narrow pane leaves it less, so the numbers stay on the line. */
const NAME_MAX = 40

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

/** The display name, then the email when known; never the internal alias. */
const label = (a: FleetAccount, _tr: T) => (a.email === null || a.email === a.name ? a.name : `${a.name} · ${a.email}`)

function warnLine(a: FleetAccount, useTarget: number, cols: number, tr: T, now: number): QLine | null {
  const w = a.weekly
  if (w === null || w.reset || w.warn === null) return null
  const head: QLine = [seg(tr(w.warn === 'waste' ? 'fleetWaste' : 'fleetShort'), 'bold', 'warning'), seg(SEP, 'dim'), seg(clipCells(label(a, tr), Math.min(NAME_MAX, Math.max(10, cols - 26))), 'bold'), seg(SEP, 'dim'), seg(`${tr('quotaWWeekly')} ${pctText(w.pct)}`)]
  const runout: QLine = w.warn === 'short' && w.exhaustAt !== null ? [seg(SEP, 'dim'), seg(tr('fleetRunout', { t: countdown(w.exhaustAt * 1000, now) }))] : []
  const reset: QLine = [seg(SEP, 'dim'), seg(`${tr('fleetReset')} ◷ ${countdown(w.resetsAt * 1000, now)}`, 'dim')]
  const target: QLine = w.warn === 'waste' ? [seg(SEP, 'dim'), seg(tr('fleetTarget', { p: pctText(useTarget) }), 'dim')] : []
  return fit([[...head, ...runout, ...reset, ...target], [...head, ...runout, ...reset], [...head, ...runout], head], cols)
}

const SOURCE_TEXT = { web_only: 'fleetSrcWebOnly', needs_token: 'fleetSrcNeedsToken', no_window: 'fleetSrcNoWindow' } as const
/** A note is shown only with at least this many cells (or all of it, when shorter). */
const NOTE_MIN_CELLS = 8

/** `line` with the operator's note dim at its end, cut with `…` to the room left; `line` alone when too little is left. */
function withNote(line: QLine, note: string | null, cols: number): QLine {
  if (note === null) return line
  const room = cols - width(line) - 2
  const w = displayWidth(note)
  if (room < Math.min(NOTE_MIN_CELLS, w)) return line
  return [...line, seg('  ' + (w <= room ? note : clipCells(note, room - 1) + '…'), 'dim')]
}

function accountLine(a: FleetAccount, aliasW: number, cols: number, tr: T, now: number): QLine {
  return withNote(accountBody(a, aliasW, cols, tr, now), a.note, cols)
}

function accountBody(a: FleetAccount, aliasW: number, cols: number, tr: T, now: number): QLine {
  const name = seg(padRight(clipCells(label(a, tr), aliasW), aliasW + 2), a.state === 'ok' ? undefined : 'idle')
  const age: QLine =
    a.observedAtMs !== null && (now - a.observedAtMs) / 1000 > AGE_SHOW_S
      ? [seg('  ' + tr('quotaAge', { t: countdown(now + (now - a.observedAtMs), now) }), 'dim')]
      : []
  // the chosen hosts first, then the old ones with the marker
  const shown = [...a.hosts.filter(h => !a.oldHosts.includes(h)), ...a.hosts.filter(h => a.oldHosts.includes(h)).map(h => tr('fleetOld', { h }))]
  const hosts: QLine = shown.length > 0 ? [seg('  ' + shown.join(','), 'dim')] : []
  const tight = (p: number): QLine => (isTight(p) ? [seg(' ' + tr('quotaTight'), 'bold', 'warning')] : [])
  const word = (id: 'fleetNoSource' | 'fleetExpired' | 'fleetStale'): QLine => [seg(tr(id), 'idle')]
  if (a.state !== 'ok') {
    const w = word(a.state === 'no_source' ? 'fleetNoSource' : a.state === 'expired' ? 'fleetExpired' : 'fleetStale')
    // the reason a row has no source, dim after the word; dropped before the word itself
    const why: QLine = a.sourceNote === null ? [] : [seg(SEP + tr(SOURCE_TEXT[a.sourceNote]), 'dim')]
    return fit([[name, ...w, ...why, ...hosts, ...age], [name, ...w, ...why, ...age], [name, ...w, ...why], [name, ...w]], cols)
  }
  const wk = a.weekly
  const five = a.fiveHour
  // a window past its reset says so instead of its old pct
  const W: QLine = wk === null ? [] : wk.reset ? [seg(`${tr('quotaWWeekly')} ${tr('fleetWinReset')}`, 'idle')] : [seg(`${tr('quotaWWeekly')} ${pctText(wk.pct)}`), ...tight(wk.pct)]
  const R: QLine = wk === null || wk.reset ? [] : [seg(` ◷ ${countdown(wk.resetsAt * 1000, now)}`, 'dim')]
  const F: QLine = five === null ? [] : [seg((wk === null ? '' : '  ') + `${tr('quotaW5h')} ${five.reset ? tr('fleetWinReset') : pctText(five.pct)}`, five.reset ? 'idle' : undefined), ...(five.reset ? [] : tight(five.pct))]
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
  const aliasW = Math.min(NAME_MAX, Math.max(10, cols - 16), Math.max(0, ...view.accounts.map(a => displayWidth(label(a, tr)))))
  for (const a of view.accounts) out.push(accountLine(a, aliasW, cols, tr, now))
  if (view.skipped > 0) out.push([seg(clipCells('  ' + tr('fleetSkipped', { n: view.skipped }), cols), 'dim')])
  return out
}
