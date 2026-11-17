// Content-sized Fable rows. This planner is shared by the pane and the UX harness.
import { countdown, displayWidth, meter, padLeft, padRight } from './model'
import type { UsageView } from './model'
import { clipCells } from './pane-view'
import { BRIEF_STALE_MS, selectBrief } from './brief-view'
import type { Assignment, BriefItem, BriefView } from './brief-view'
import type { FleetAccount, FleetQuotaView } from './fleet-view'

export type Tone = 'inactive' | 'subtle' | 'merged' | 'warning' | 'error' | 'success'
export type PaneSeg = { text: string; color?: Tone; bold?: boolean }
export type Group = 'blocked' | 'pools' | 'assignments'
export type PaneRow = { key: string; segs: PaneSeg[]; group?: Group; more?: boolean }
export type GroupWindow = { total: number; visible: number; top: number; above: number; below: number }
export type OverviewInput = {
  cols: number; rows: number; maxRows: number; now: number; brief: BriefView; fleet: FleetQuotaView
  assignments: Assignment[]; offsets: Record<Group, number>; usage?: UsageView; info?: boolean
}
export const paneCap = (rows: number, maxRows: number) => Math.max(0, Math.min(maxRows, Math.floor(rows * 0.6)))
export const s = (text: string, color: Tone = 'inactive', bold = false): PaneSeg => ({ text, color, ...(bold ? { bold } : {}) })
export const oneLine = (text: string) => text.replace(/[\u0000-\u001f\u007f-\u009f]/gu, ' ').replace(/\s+/gu, ' ').trim()
export const cut = (text: string, cells: number) => {
  const clean = oneLine(text)
  return displayWidth(clean) <= cells ? clean : cells > 0 ? clipCells(clean, cells - 1) + '…' : ''
}
const width = (segs: PaneSeg[]) => segs.reduce((n, x) => n + displayWidth(x.text), 0)
export function fitSegs(segs: PaneSeg[], cols: number): PaneSeg[] {
  const out: PaneSeg[] = []
  let room = Math.max(0, cols)
  for (const seg of segs) {
    if (room === 0) break
    const text = displayWidth(seg.text) <= room ? seg.text : cut(seg.text, room)
    if (text) out.push({ ...seg, text })
    room -= displayWidth(text)
    if (text !== seg.text) break
  }
  return out
}
export const gutter = (label: string, color: Tone = 'inactive') => [s(padRight(label, 7), color), s('│ ', 'subtle')]
const cell = (text: string, cells: number, color: Tone = 'inactive') => s(padRight(cut(text, Math.max(0, cells - 2)), cells), color)
const elapsed = (since: number | null, now: number) => since === null ? '時間未知' : countdown(now + Math.max(0, now - since), now)
const row = (key: string, label: string, body: PaneSeg[], cols: number, color: Tone = 'inactive', group?: Group): PaneRow => ({ key, segs: fitSegs([...gutter(label, color), ...body], cols), ...(group ? { group } : {}) })

function blockedBody(i: BriefItem, cols: number, now: number): PaneSeg[] {
  const inner = cols - 9
  const who = cell('⊘ ' + i.subject.display, Math.min(24, Math.floor(inner * 0.32)), 'error')
  const state = i.text + ' · ' + elapsed(i.since_ms, now)
  if (cols >= 150) {
    const titleW = Math.max(20, inner - 24 - 36 - 40)
    return [who, cell(i.title, titleW), cell(state, 36, 'error'), s(cut('→ ' + i.next_step_zh, 40))]
  }
  const stateW = cols >= 110 ? 34 : 25
  return [who, cell(i.title, inner - displayWidth(who.text) - stateW), s(cut(state, stateW), 'error')]
}

const PLAN = { pro: 'Pro', max5x: 'Max 5x', max20x: 'Max 20x', team: 'Team', enterprise: 'Enterprise', api: 'API' }
const pct = (value: number) => `${Math.round(value * 100)}%`
function poolBody(a: FleetAccount, cols: number, now: number): PaneSeg[] {
  const inner = cols - 9
  const nameW = cols >= 110 ? 24 : 20
  const plan = a.plan === null ? '方案未知' : PLAN[a.plan]
  const name = s(padRight(cut(a.name, Math.max(4, nameW - displayWidth(plan) - 3)), nameW - displayWidth(plan) - 1) + plan + ' ')
  const w = a.weekly
  const tone: Tone = a.state === 'stale' || a.state === 'expired' ? 'warning' : w !== null && !w.reset && w.pct >= 0.95 ? 'error' : 'inactive'
  const barW = cols < 110 ? 6 : 10
  const bar = w === null || w.reset ? s(padRight(w?.reset ? '已重置' : '週 —', barW + 7), tone) : s(meter(w.pct, barW).on + meter(w.pct, barW).off + padLeft(pct(w.pct), 5) + '  ', tone)
  const warn = w === null || w.reset || w.warn === null ? '' : w.warn === 'waste' ? ' · 用不完' : w.exhaustAt === null ? ' · 不夠' : ` · 不夠，預估 ${countdown(w.exhaustAt * 1000, now)} 耗盡`
  const reset = w === null || w.reset ? '' : `${cols < 110 ? '週' : '週重置 '}${countdown(w.resetsAt * 1000, now)}`
  const state = a.state === 'stale' ? '舊讀數 · ' : a.state === 'expired' ? '過期 · ' : ''
  const detail = state + reset + warn
  const five = a.fiveHour === null ? (a.fiveNoSource ? '5h 無來源' : '') : a.fiveHour.reset ? '5h 已重置' : `5h ${pct(a.fiveHour.pct)} 重置 ${countdown(a.fiveHour.resetsAt * 1000, now)}`
  const lead = [name, bar]
  const rest = inner - width(lead)
  if (cols < 110) return [...lead, s(cut(detail, rest), tone)]
  const detailW = Math.min(48, rest - (five ? 24 : 0))
  const fields = [...lead, cell(detail, detailW, tone)]
  if (five) fields.push(cell(five, 24))
  if (a.note && inner > width(fields)) fields.push(s(cut(a.note, inner - width(fields))))
  return fields
}

const poolRank = (a: FleetAccount) => a.weekly?.warn === 'short' ? 0 : a.weekly?.warn === 'waste' ? 1 : 2
export function orderedPools(accounts: FleetAccount[]) {
  const seen = new Set<string>()
  return accounts.filter(a => a.state !== 'no_source').slice().sort((a, b) => poolRank(a) - poolRank(b) || (poolRank(a) === 1 ? (a.weekly?.resetsAt ?? Infinity) - (b.weekly?.resetsAt ?? Infinity) : (b.weekly?.pct ?? -1) - (a.weekly?.pct ?? -1))).filter(a => {
    if (seen.has(a.alias)) return false
    seen.add(a.alias)
    return true
  })
}

const sizeOf = (n: number, visible: number) => visible + (visible < n ? 1 : 0)
function windowRows(lines: PaneRow[], visible: number, offset: number, group: Group, cols: number): { lines: PaneRow[]; win: GroupWindow } {
  const top = Math.max(0, Math.min(offset, lines.length - visible))
  const below = lines.length - visible - top
  const shown = lines.slice(top, top + visible)
  if (visible < lines.length) {
    const more = [top > 0 ? `↑ 還有 ${top}` : '', below > 0 ? `↓ 還有 ${below}` : ''].filter(Boolean).join(' · ')
    shown.push({ ...row('more-' + group, shown.length ? '' : group === 'blocked' ? '卡住' : group === 'pools' ? '各池' : '交辦', [s(more, 'merged')], cols, 'inactive', group), more: true })
  }
  return { lines: shown, win: { total: lines.length, visible, top, above: top, below } }
}

export function overviewPlan(inp: OverviewInput) {
  const { cols, now, brief, fleet } = inp
  const cap = paneCap(inp.rows, inp.maxRows)
  const selected = selectBrief(brief, inp.assignments)
  const status: PaneSeg[][] = []
  if (selected.background > 0) status.push([s('⋯' + selected.background)])
  if (inp.usage?.status === 'ok' && inp.usage.models.some(m => m.cost !== null)) {
    const cost = inp.usage.models.reduce((n, m) => n + (m.cost ?? 0), 0)
    status.push([s((inp.usage.models.some(m => m.unpriced > 0) ? '≥' : '') + '$' + cost.toFixed(2))])
  }
  const ages: PaneSeg[] = [s('ⓘ ')]
  if (brief.status === 'ok') ages.push(s(now - brief.generatedAt > BRIEF_STALE_MS ? `舊資料 ${Math.floor((now - brief.generatedAt) / 60000)}m` : '摘要 ' + elapsed(brief.generatedAt, now), now - brief.generatedAt > BRIEF_STALE_MS || brief.degraded ? 'warning' : 'inactive'))
  if (fleet.status === 'ok') ages.push(s((brief.status === 'ok' ? ' · ' : '') + '各池 ' + elapsed(fleet.generatedAt, now), now - fleet.generatedAt > BRIEF_STALE_MS ? 'warning' : 'inactive'))
  if (brief.status === 'ok' || fleet.status === 'ok') status.push(ages)
  const statusSegs = status.flatMap((ss, i) => i ? [s(' · ', 'subtle'), ...ss] : ss)
  const statusTail = status.length < 2 ? statusSegs : []
  const head: PaneRow[] = status.length >= 2 ? [row('status', '狀態', statusSegs, cols)] : []
  if (inp.info) {
    head.push(row('info-brief', '來源', [s('fleet.brief/1：每分鐘更新；超過 5 分鐘為舊資料')], cols), row('info-quota', '', [s('codeforge.fleet-quota/1：讀取來源的用量、重置、預估與備註')], cols), row('info-bg', '圖例', [s('⋯ 背景等待與資訊；ⓘ 資料年齡；金額為本機當日成本')], cols))
    return { lines: head.slice(0, Math.max(0, cap - 2)), statusTail, groups: Object.fromEntries(['blocked', 'pools', 'assignments'].map(g => [g, { total: 0, visible: 0, top: 0, above: 0, below: 0 }])) as Record<Group, GroupWindow>, decisions: selected.decisions.length }
  }
  if (brief.status !== 'ok') head.push(row('brief-unavailable', '待決', [s(brief.status === 'missing' ? 'fleet 摘要檔還沒出現' : 'fleet 摘要檔無法讀取', 'warning')], cols, 'warning'))
  else {
    const oldest = selected.decisions[0]
    const count = brief.counts.needs_human
    const lead = `待決 ${count}`
    const tail = oldest ? ` · ${elapsed(oldest.since_ms, now)}` : ''
    const budget = cols - 9 - displayWidth(lead + tail) - 3
    head.push(row('decisions', '待決', [s(lead + (oldest ? ' ' + cut(`${oldest.subject.display}：${oldest.title}`, budget) + tail : count > 0 ? ' · 決策明細尚未提供' : ' · 目前沒有待決'), count > 0 ? 'warning' : 'inactive')], cols, count > 0 ? 'warning' : 'inactive'))
    if (brief.degraded) head.push(row('brief-degraded', '來源', [s('fleet 摘要部分來源無法讀取', 'warning')], cols, 'warning'))
  }
  const blocked = selected.blocked.map((it, i) => row(it.id, i ? '' : '卡住', blockedBody(it, cols, now), cols, 'error', 'blocked'))
  const accounts = fleet.status === 'ok' ? fleet.accounts : []
  const pools = orderedPools(accounts).map((a, i) => row('pool-' + a.alias, i ? '' : '各池', poolBody(a, cols, now), cols, 'inactive', 'pools'))
  const noSource = accounts.filter(a => a.state === 'no_source')
  const poolTail: PaneRow[] = []
  if (noSource.length) {
    const reasons = [['needs_token', '缺權杖'], ['web_only', '僅網頁'], ['no_window', '無視窗'], [null, '未提供原因']] as const
    const counts = reasons.map(([key, label]) => ({ label, n: noSource.filter(a => a.sourceNote === key).length, color: key === 'needs_token' ? 'warning' as const : 'inactive' as const })).filter(x => x.n)
    poolTail.push(row('no-source', pools.length ? '' : '各池', [s(`另 ${noSource.length} 池無來源：`), ...counts.flatMap((c, i) => [s((i ? ' · ' : '') + c.label + ' ×' + c.n, c.color)])], cols))
  }
  if (fleet.status !== 'ok') poolTail.push(row('quota-unavailable', '各池', [s(fleet.status === 'missing' ? '各池配額檔還沒出現' : '各池配額檔無法讀取', 'warning')], cols, 'warning'))
  else if (!accounts.length) poolTail.push(row('quota-empty', '各池', [s('配額檔沒有帳號')], cols))
  const assignmentRows = selected.assignments.map((a, i) => row('assignment-' + a.who, i ? '' : '交辦', [cell(a.who, cols >= 110 ? 24 : 20), cell(a.title, Math.max(8, cols - 9 - (cols >= 110 ? 24 : 20) - 24)), s((a.state === 'need_you' ? '▲ 等操作員' : a.state === 'waiting' ? '等回報' : '進行中') + ' · ' + elapsed(a.sinceMs, now), a.state === 'need_you' ? 'warning' : 'inactive')], cols, 'inactive', 'assignments'))
  const source = { blocked, pools, assignments: assignmentRows }
  const visible = { blocked: blocked.length, pools: pools.length, assignments: assignmentRows.length }
  const minimum = { blocked: Math.min(3, blocked.length), pools: Math.min(1, pools.length), assignments: Math.min(selected.assignments.some(a => a.state === 'need_you') ? 1 : 0, assignmentRows.length) }
  const size = () => head.length + poolTail.length + (Object.keys(source) as Group[]).reduce((n, g) => n + sizeOf(source[g].length, visible[g]), 0) + 2
  for (const g of ['assignments', 'pools', 'blocked'] as Group[]) while (size() > cap && visible[g] > minimum[g]) visible[g]--
  const groups = {} as Record<Group, GroupWindow>
  const out = [...head]
  for (const g of ['blocked', 'pools', 'assignments'] as Group[]) {
    const w = windowRows(source[g], visible[g], inp.offsets[g], g, cols)
    groups[g] = w.win
    if (w.lines.length && w.lines[0]?.segs[0]?.text.trim() === '') w.lines[0] = { ...w.lines[0], segs: [...gutter(g === 'blocked' ? '卡住' : g === 'pools' ? '各池' : '交辦', g === 'blocked' ? 'error' : 'inactive'), ...w.lines[0].segs.slice(2)] }
    out.push(...w.lines)
    if (g === 'pools') out.push(...poolTail)
  }
  // A host may allocate fewer rows than the supported terminal minimum. Never overrun it.
  return { lines: out.slice(0, Math.max(0, cap - 2)), statusTail, groups, decisions: brief.status === 'ok' ? brief.counts.needs_human : 0 }
}
