// The integrated panel's right-hand summary columns (doc/specs/panel-band-contract.md §2, O4 width tiers): `side` of codeforge.panel/1. autopilot's band picks the tier from
// its bodyColumns and draws the column; forge-stats only lays the lines out once per column width (the reader takes the widest key that fits). Ratios and counts only:
// never an amount of money (sideState refuses a `$` anywhere).
import type { PanelLine, PanelSide, SessionView } from '../types'
import { GLYPH, countdown, displayWidth, meter, type UsageView } from './model'
import type { QEntry } from './quota-view'
import type { T } from './strings'

export const SIDE_USAGE_W = [30, 40, 50] as const
export const SIDE_FULL_W = [40] as const
export const SIDE_MEMORY_W = [30, 40, 50] as const
const FULL_MODELS = 3

/** One quota window of one account: 0..1 used, the reset on the local clock (ms) when known. */
export type SideWin = { label: '5h' | '7d'; pct: number; resetsAtMs: number | null }
export type SideAccount = { alias: string; wins: SideWin[] }

const peak = (a: SideAccount) => Math.max(0, ...a.wins.map(w => w.pct))

/** The accounts with a 5h or 7d window: from the quota mirror when its directory exists (design §7 rule 7, the only source then), else from the usage summary. */
export function sideAccounts(view: UsageView, mirror: readonly QEntry[] | null): SideAccount[] {
  const out: SideAccount[] = []
  if (mirror !== null) {
    for (const e of mirror) {
      const wins: SideWin[] = []
      for (const w of e.m.windows) {
        const label = w.kind === 'rolling_5h' ? '5h' : w.kind === 'weekly' ? '7d' : null
        if (label !== null && w.utilization !== null) wins.push({ label, pct: w.utilization, resetsAtMs: w.resetsAtLocal === null ? null : w.resetsAtLocal * 1000 })
      }
      if (wins.length > 0) out.push({ alias: e.alias, wins })
    }
  } else if (view.status === 'ok') {
    for (const a of view.accounts) {
      const wins: SideWin[] = []
      if (a.fiveHour !== null) wins.push({ label: '5h', pct: a.fiveHour.pct, resetsAtMs: a.fiveHour.resetsAtMs })
      if (a.sevenDay !== null) wins.push({ label: '7d', pct: a.sevenDay.pct, resetsAtMs: a.sevenDay.resetsAtMs })
      if (wins.length > 0) out.push({ alias: a.alias, wins })
    }
  }
  // the most constrained account first: that is the one the column shows
  return out.sort((a, b) => peak(b) - peak(a))
}

const pctOf = (r: number) => `${Math.floor(Math.max(0, r) * 100)}%`
const barW = (w: number) => (w >= 50 ? 10 : w >= 40 ? 8 : 6)

// `5h ▰▰▱▱▱▱ 32%`, then `· 2h47m 後重置` when it fits; a part is dropped whole, never cut
function winLine(win: SideWin, w: number, now: number, tr: T): PanelLine {
  const m = meter(win.pct, barW(w))
  const line: PanelLine = [{ text: win.label + ' ', tone: 'dim' }, { text: m.on }, { text: m.off, tone: 'inactive' }, { text: ' ' + pctOf(win.pct) }]
  if (win.resetsAtMs !== null && win.resetsAtMs > now) {
    const reset = { text: ' · ' + tr('sideReset', { t: countdown(win.resetsAtMs, now) }), tone: 'dim' as const }
    if (width([...line, reset]) <= w) line.push(reset)
  }
  return line
}

const width = (l: PanelLine) => l.reduce((n, p) => n + displayWidth(p.text), 0)

// `text` in exactly `w` display cells: cut with … when wider (a wide character never split), padded with spaces when narrower
function fitCells(text: string, w: number): string {
  if (displayWidth(text) <= w) return text + ' '.repeat(w - displayWidth(text))
  let out = ''
  for (const ch of text) {
    if (displayWidth(out + ch) > w - 1) break
    out += ch
  }
  return out + '…' + ' '.repeat(w - 1 - displayWidth(out))
}

/** `side.usage`: ≤ 3 lines, the most constrained account's 5h and 7d, then how many other accounts there are and their highest window. */
export function sideUsageLines(accts: readonly SideAccount[], w: number, now: number, tr: T): PanelLine[] {
  if (accts.length === 0) return [[{ text: tr('sideNoUsage'), tone: 'dim' }]]
  const out = accts[0].wins.map(win => winLine(win, w, now, tr))
  if (accts.length > 1) out.push([{ text: tr('sideMore', { n: accts.length - 1, pct: pctOf(Math.max(...accts.slice(1).map(peak))) }), tone: 'dim' }])
  return out.slice(0, 3)
}

/** `side.usage_full`: ≤ 7 lines, the windows, a blank line, then today's top three models by share of tokens. */
export function sideFullLines(accts: readonly SideAccount[], view: UsageView, w: number, now: number, tr: T): PanelLine[] {
  const out = accts.length === 0 ? [[{ text: tr('sideNoUsage'), tone: 'dim' as const }]] : accts[0].wins.map(win => winLine(win, w, now, tr))
  const models = view.status === 'ok' ? view.models : []
  const total = models.reduce((n, m) => n + m.tokens, 0)
  out.push([])
  if (total === 0) out.push([{ text: tr('usageNone'), tone: 'dim' }])
  const nameW = w - barW(w) - 6
  for (const m of [...models].sort((a, b) => b.tokens - a.tokens).slice(0, FULL_MODELS)) {
    if (total === 0) break
    const name = fitCells(m.model, nameW)
    const bar = meter(m.tokens / total, barW(w))
    out.push([{ text: name + ' ', tone: 'dim' }, { text: bar.on }, { text: bar.off, tone: 'inactive' }, { text: ' ' + pctOf(m.tokens / total) }])
  }
  return out.slice(0, 7)
}

/** `side.memory`: ≤ 2 lines, this session's memory and brain lights as words (no state glyph: tui-guideline §3.4). */
export function sideMemoryLines(lights: SessionView | null, tr: T): PanelLine[] {
  if (lights === null) return [[{ text: `${tr('memory')} ${GLYPH.empty}`, tone: 'dim' }]]
  const out: PanelLine[] = []
  if (lights.memory !== 'hidden') out.push([{ text: tr('memory') + ' ', tone: 'dim' }, { text: `L1 ${lights.active} ${tr('entries')}` }])
  const brain: Record<Exclude<SessionView['brain'], 'hidden'>, string> = { ok: tr('brainOk'), degraded: tr('brainDegraded'), offline: tr('brainOffline'), pending: tr('brainPending') }
  if (lights.brain !== 'hidden') out.push([{ text: tr('brain') + ' ', tone: 'dim' }, { text: brain[lights.brain], tone: lights.brain === 'ok' ? 'success' : undefined }])
  return out
}

const tiers = (ws: readonly number[], lay: (w: number) => PanelLine[]) => Object.fromEntries(ws.map(w => [String(w), lay(w)]))

/** The whole `side`, or null when any line would carry a `$` (the contract carries ratios only; a stray amount must not reach the band). */
export function sideState(view: UsageView, mirror: readonly QEntry[] | null, lights: SessionView | null, now: number, tr: T): PanelSide | null {
  const accts = sideAccounts(view, mirror)
  const side: PanelSide = {
    usage: { tiers: tiers(SIDE_USAGE_W, w => sideUsageLines(accts, w, now, tr)) },
    usage_full: { tiers: tiers(SIDE_FULL_W, w => sideFullLines(accts, view, w, now, tr)) },
    memory: { tiers: tiers(SIDE_MEMORY_W, () => sideMemoryLines(lights, tr)) },
  }
  return JSON.stringify(side).includes('$') ? null : side
}
