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
  assignments: Assignment[]; offsets: Record<Group, number>; usage?: UsageView; info?: boolean; buttons?: boolean
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
const trimEndSegs = (segs: PaneSeg[]) => {
  while (segs.length && segs[segs.length - 1].text.trimEnd() === '') segs.pop()
  if (segs.length) segs[segs.length - 1] = { ...segs[segs.length - 1], text: segs[segs.length - 1].text.trimEnd() }
  return segs
}
const row = (key: string, label: string, body: PaneSeg[], cols: number, color: Tone = 'inactive', group?: Group): PaneRow => ({ key, segs: trimEndSegs(fitSegs([...gutter(label, color), ...body], cols)), ...(group ? { group } : {}) })

function blockedBody(i: BriefItem, cols: number, now: number): PaneSeg[] {
  const inner = cols - 9
  const who = cell('⊘ ' + i.subject.display, i.kind === 'offline' ? Math.min(20, displayWidth('⊘ ' + i.subject.display) + 2) : Math.min(24, Math.floor(inner * 0.32)), 'error')
  const tail = ' · ' + elapsed(i.since_ms, now)
  const state = (cells: number) => cut(i.text, Math.max(0, cells - displayWidth(tail))) + tail
  if (i.kind === 'offline') {
    const stateW = Math.min(inner - displayWidth(who.text), displayWidth(oneLine(i.text) + tail) + 2)
    return [who, s(padRight(state(stateW - (cols >= 110 ? 2 : 0)), stateW), 'error'), s(cut('→ ' + i.next_step_zh, inner - displayWidth(who.text) - stateW))]
  }
  if (cols >= 150) {
    const titleW = Math.max(20, inner - 24 - 36 - 40)
    return [who, cell(i.title, titleW), cell(state(34), 36, 'error'), s(cut('→ ' + i.next_step_zh, 40))]
  }
  const stateW = cols >= 110 ? 34 : 25
  return [who, cell(i.title, inner - displayWidth(who.text) - stateW), s(state(stateW), 'error')]
}

const PLAN = { pro: 'Pro', max5x: 'Max 5x', max20x: 'Max 20x', team: 'Team', enterprise: 'Enterprise', api: 'API' }
const pct = (value: number) => `${Math.round(value * 100)}%`
function poolBody(a: FleetAccount, cols: number, now: number, noteReserve = 0): PaneSeg[] {
  const inner = cols - 9
  let nameW = cols >= 110 ? 24 : 20
  let plan = a.plan === null ? '' : PLAN[a.plan]
  const w = a.weekly
  const tone: Tone = attention(a) ? 'warning' : 'inactive'
  let barW = cols < 110 ? 6 : 10
  const warn = w === null || w.reset || w.warn === null ? '' : w.warn === 'waste' ? ' · 用不完' : w.exhaustAt === null ? ' · 不夠' : ` · 不夠，預估 ${countdown(w.exhaustAt * 1000, now)} 耗盡`
  let reset = w === null || w.reset ? '' : `${cols < 110 ? '週' : '週重置 '}${countdown(w.resetsAt * 1000, now)}`
  const state = a.state === 'stale' ? '舊讀數 · ' : a.state === 'expired' ? '過期 · ' : ''
  let five = a.fiveHour === null ? (a.fiveNoSource ? '5h 無來源' : '') : a.fiveHour.reset ? '5h 已重置' : `5h ${pct(a.fiveHour.pct)} 重置 ${countdown(a.fiveHour.resetsAt * 1000, now)}`
  const noteW = Math.max(noteReserve, displayWidth(oneLine(a.note ?? '')))
  let fiveW = cols < 110 ? 0 : five !== '' || noteW > 0 ? 24 : 0
  const need = () => nameW + barW + 7 + 48 + fiveW + noteW
  if (noteW && need() > inner) {
    nameW = cols >= 110 ? 16 : 12
    plan = ''
  }
  if (noteW && need() > inner) barW = cols < 110 ? 0 : Math.max(4, barW - (need() - inner))
  if (noteW && nameW + barW + 7 + fiveW + noteW > inner) barW = 0
  if (noteW && nameW + barW + 7 + fiveW + noteW > inner) fiveW = 0
  if (noteW && need() > inner) {
    reset = reset.replace('週重置 ', '週')
    five = five.replace(' 重置 ', ' ')
  }
  const name = s(padRight(cut(a.name, Math.max(4, nameW - displayWidth(plan) - 3)), nameW - displayWidth(plan) - 1) + plan + ' ', 'inactive', true)
  const bar = w === null || w.reset ? [s(padRight(w?.reset ? '已重置' : '週 —', barW + 7), tone)] : [s(meter(w.pct, barW).on, tone), s(meter(w.pct, barW).off, 'subtle'), s(padLeft(pct(w.pct), 5) + '  ', tone)]
  const lead = [name, ...bar]
  const rest = inner - width(lead)
  const detail = (cells: number) => fitSegs([s(state + reset), s(warn, w?.warn === 'short' ? 'warning' : 'inactive')], cells)
  if (!a.note && cols < 110) return [...lead, ...detail(rest)]
  const detailW = Math.max(0, Math.min(48, rest - fiveW - noteW))
  const detailSegs = detail(Math.max(0, detailW - 2))
  const fields = [...lead, ...detailSegs, s(' '.repeat(Math.max(0, detailW - width(detailSegs))))]
  if (fiveW) fields.push(cell(five, fiveW))
  // The note owns its width before optional plan, bar and reset descriptions are fitted.
  if (a.note) fields.push(s(cut(a.note, inner - width(fields))))
  return fields
}

function compactPoolBody(a: FleetAccount, cells: number, noteCells = 8): PaneSeg[] {
  const w = a.weekly
  const reading = w !== null && !w.reset ? pct(w.pct) : w?.reset ? '已重置' : '週 —'
  let bar = w !== null && !w.reset ? meter(w.pct, 4) : null
  const note = a.note ? cut(a.note, noteCells) : ''
  if (displayWidth(oneLine(a.name) + reading + note) + 7 > cells) bar = null
  const tone = attention(a) ? 'warning' : 'inactive'
  const nameW = Math.max(1, cells - displayWidth(reading + note) - (bar ? 4 : 0) - 3)
  return [s(cut(a.name, nameW), 'inactive', true), s(' '), ...(bar ? [s(bar.on, tone), s(bar.off, 'subtle')] : []), s(' ' + reading + ' ', tone), s(note)]
}

const attention = (a: FleetAccount) => a.weekly !== null && !a.weekly.reset && (a.weekly.warn === 'short' || a.weekly.pct >= 0.95)
const poolRank = (a: FleetAccount) => attention(a) ? 0 : a.weekly?.warn === 'waste' ? 1 : 2
export function orderedPools(accounts: FleetAccount[]) {
  const seen = new Set<string>()
  return accounts.filter(a => a.state !== 'no_source').slice().sort((a, b) => poolRank(a) - poolRank(b) || (poolRank(a) === 1 ? (a.weekly?.resetsAt ?? Infinity) - (b.weekly?.resetsAt ?? Infinity) : (b.weekly?.pct ?? -1) - (a.weekly?.pct ?? -1))).filter(a => {
    if (seen.has(a.alias)) return false
    seen.add(a.alias)
    return true
  })
}

function groupWindow(total: number, visible: number, offset: number): GroupWindow {
  const top = Math.max(0, Math.min(offset, total - visible))
  return { total, visible, top, above: top, below: total - visible - top }
}
const remainder = (w: GroupWindow, key: string) => [w.above ? `↑ 還有 ${w.above}（${key === 'j' ? 'k' : key}）` : '', w.below ? `↓ 還有 ${w.below}（${key}）` : ''].filter(Boolean).join(' · ')
const withTail = (body: PaneSeg[], tail: PaneSeg[], cells: number) => [...fitSegs(body, Math.max(0, cells - width(tail) - 2)), s('  '), ...tail]

export function overviewPlan(inp: OverviewInput) {
  const { cols, now, brief, fleet } = inp
  const cap = paneCap(inp.rows, inp.maxRows)
  const actionTone: Tone = inp.buttons === false ? 'subtle' : 'merged'
  const selected = selectBrief(brief, inp.assignments)
  const status: PaneSeg[][] = []
  if (selected.background > 0) status.push([s('⋯' + selected.background)])
  if (inp.usage?.status === 'ok' && inp.usage.models.some(m => m.cost !== null)) {
    const cost = inp.usage.models.reduce((n, m) => n + (m.cost ?? 0), 0)
    status.push([s((inp.usage.models.some(m => m.unpriced > 0) ? '≥' : '') + '$' + cost.toFixed(2))])
  }
  const ages: PaneSeg[] = [s('ⓘ ')]
  if (brief.status === 'ok') ages.push(s(now - brief.generatedAt > BRIEF_STALE_MS ? `摘要過舊 ${Math.floor((now - brief.generatedAt) / 60000)}m` : '摘要 ' + elapsed(brief.generatedAt, now), now - brief.generatedAt > BRIEF_STALE_MS || brief.degraded ? 'warning' : 'inactive'))
  if (fleet.status === 'ok') ages.push(s((brief.status === 'ok' ? ' · ' : '') + (now - fleet.generatedAt > BRIEF_STALE_MS ? `各池過舊 ${Math.floor((now - fleet.generatedAt) / 60000)}m` : '各池 ' + elapsed(fleet.generatedAt, now)), now - fleet.generatedAt > BRIEF_STALE_MS ? 'warning' : 'inactive'))
  if (brief.status === 'ok' || fleet.status === 'ok') status.push(ages)
  const statusSegs = status.flatMap((ss, i) => i ? [s(' · ', 'subtle'), ...ss] : ss)
  const statusTail = statusSegs
  const head: PaneRow[] = []
  if (inp.info) {
    head.push(row('info-brief', '來源', [s('fleet.brief/1：每分鐘更新；超過 5 分鐘為舊資料')], cols), row('info-quota', '', [s('codeforge.fleet-quota/1：讀取來源的用量、重置、預估與備註')], cols), row('info-bg', '圖例', [s('⋯ 背景等待與資訊；ⓘ 資料年齡；金額為本機當日成本')], cols))
    return { lines: head.slice(0, Math.max(0, cap - 2)), statusTail, groups: Object.fromEntries(['blocked', 'pools', 'assignments'].map(g => [g, { total: 0, visible: 0, top: 0, above: 0, below: 0 }])) as Record<Group, GroupWindow>, decisions: selected.decisions.length }
  }
  if (brief.status !== 'ok') head.push(row('brief-unavailable', '摘要', [s('fleet 摘要檔還沒出現（待決、卡住無法顯示）', 'warning')], cols, 'warning'))
  else {
    const oldest = selected.decisions[0]
    const count = brief.counts.needs_human
    const lead = `待決 ${count}`
    const tail = oldest ? ` · ${elapsed(oldest.since_ms, now)}` : ''
    const stale = now - brief.generatedAt > BRIEF_STALE_MS ? `（摘要過舊 ${Math.floor((now - brief.generatedAt) / 60000)}m）` : ''
    const budget = cols - 9 - displayWidth(lead + tail + stale) - 3
    head.push(row('decisions', '待決', [s(lead + (oldest ? ' ' + cut(`${oldest.subject.display}：${oldest.title}`, budget) + tail : count > 0 ? ' · 決策明細尚未提供' : ' · 目前沒有待決'), count > 0 ? 'warning' : 'inactive'), ...(stale ? [s(' '), s(stale, 'warning')] : [])], cols, count > 0 ? 'warning' : 'inactive'))
    if (brief.degraded) head.push(row('brief-degraded', '來源', [s('fleet 摘要部分來源無法讀取', 'warning')], cols, 'warning'))
  }
  const accounts = fleet.status === 'ok' ? fleet.accounts : []
  const pools = orderedPools(accounts)
  const noSource = accounts.filter(a => a.state === 'no_source')
  const assignments = selected.assignments
  const wanted = { blocked: selected.blocked.length, pools: Math.max(1, pools.length + Number(noSource.length > 0)), assignments: assignments.length }
  const slots = { blocked: Math.min(1, wanted.blocked), pools: 1, assignments: Math.min(1, wanted.assignments) }
  let spare = Math.max(0, cap - 2 - head.length - slots.blocked - slots.pools - slots.assignments)
  // Allocate every spare row to blocked lanes first, then pools, then assignments.
  for (const g of ['blocked', 'pools', 'assignments'] as Group[]) {
    const extra = Math.min(spare, wanted[g] - slots[g])
    slots[g] += extra
    spare -= extra
  }
  const blockVisible = slots.blocked
  const poolVisible = slots.pools < wanted.pools || noSource.length ? Math.max(1, slots.pools - 1) : slots.pools
  const groups = {
    blocked: groupWindow(wanted.blocked, blockVisible, inp.offsets.blocked),
    pools: groupWindow(pools.length, Math.min(pools.length, poolVisible), inp.offsets.pools),
    assignments: groupWindow(assignments.length, slots.assignments, inp.offsets.assignments),
  }
  const out = [...head]
  const bw = groups.blocked
  for (const [index, it] of selected.blocked.slice(bw.top, bw.top + bw.visible).entries()) {
    const more = bw.total > bw.visible && index === bw.visible - 1
    const tail = more ? [s(remainder(bw, 'j'), actionTone)] : []
    const body = blockedBody(it, cols - (more ? width(tail) + 2 : 0), now)
    out.push({ ...row(it.id, index ? '' : '卡住', more ? withTail(body, tail, cols - 9) : body, cols, 'error', 'blocked'), ...(more ? { more: true } : {}) })
  }

  const poolNoteW = Math.max(0, ...pools.map(a => displayWidth(oneLine(a.note ?? ''))))
  const pw = groups.pools
  const hiddenPools = pools.filter((_, i) => i < pw.top || i >= pw.top + pw.visible)
  const poolSummary: PaneSeg[] = []
  const leadReserve = slots.pools === 1 && pools.length ? cols >= 110 ? displayWidth(oneLine(pools[pw.top].name)) + 7 + displayWidth(oneLine(pools[pw.top].note ?? '')) : 24 : 0
  const summaryBudget = cols - 9 - (leadReserve ? leadReserve + 2 : 0) - (pw.total > pw.visible ? displayWidth(' · （u）') : 0)
  const add = (text: string, tone: Tone = 'inactive', optional = false) => {
    if (optional && width(poolSummary) + (poolSummary.length ? 3 : 0) + displayWidth(text) > summaryBudget) return
    if (poolSummary.length) poolSummary.push(s(' · ', 'subtle'))
    poolSummary.push(s(text, tone))
  }
  const tightPools = hiddenPools.filter(attention)
  const tight = tightPools.length
  const waste = hiddenPools.filter(a => !attention(a) && a.weekly?.warn === 'waste').length
  const normal = hiddenPools.length - tight - waste
  const tokens = noSource.filter(a => a.sourceNote === 'needs_token').length
  if (slots.pools > 1 || cols >= 110) for (const a of tightPools) add(`${oneLine(a.name)} ${pct(a.weekly!.pct)}`, 'warning')
  else if (tight) add(`另 ${tight} 池吃緊`, 'warning')
  if (noSource.length) add(`無來源 ×${noSource.length}${tokens ? `（缺權杖 ${tokens}）` : ''}`, 'inactive', true)
  if (waste) add(`用不完 ×${waste}`, 'inactive', true)
  if (normal) add(`正常 ×${normal}`, 'inactive', true)
  if (pw.total > pw.visible) add('（u）', actionTone)
  for (const [index, account] of pools.slice(pw.top, pw.top + pw.visible).entries()) {
    const body = slots.pools === 1 && poolSummary.length ? compactPoolBody(account, cols - 9 - width(poolSummary) - 2, cols >= 110 ? displayWidth(oneLine(account.note ?? '')) : 8) : poolBody(account, cols, now, poolNoteW)
    out.push({ ...row('pool-' + account.alias, index ? '' : '各池', slots.pools === 1 && poolSummary.length ? withTail(body, poolSummary, cols - 9) : body, cols, 'inactive', 'pools'), ...(slots.pools === 1 && pw.total > pw.visible ? { more: true } : {}) })
  }
  if (poolSummary.length && (slots.pools > 1 || !pools.length)) out.push({ ...row('pool-summary', pools.length ? '' : '各池', poolSummary, cols, 'inactive', 'pools'), ...(pw.total > pw.visible ? { more: true } : {}) })
  if (fleet.status !== 'ok') out.push(row('quota-unavailable', '各池', [s(fleet.status === 'missing' ? '各池配額檔還沒出現' : '各池配額檔無法讀取', 'warning')], cols, 'warning', 'pools'))
  else if (!accounts.length) out.push(row('quota-empty', '各池', [s('配額檔沒有帳號')], cols, 'inactive', 'pools'))

  const aw = groups.assignments
  for (const [index, a] of assignments.slice(aw.top, aw.top + aw.visible).entries()) {
    const tail = index === aw.visible - 1 && aw.total > aw.visible ? [s(` · +${aw.total - aw.visible} 件`), s('（a）', actionTone)] : []
    const state = (a.state === 'need_you' ? '▲ 等操作員' : a.state === 'waiting' ? '等回報' : '進行中') + ' · ' + elapsed(a.sinceMs, now)
    const nameW = cols >= 110 ? 24 : 20
    out.push({ ...row('assignment-' + a.who, index ? '' : '交辦', [{ ...cell(a.who, nameW), bold: true }, cell(a.title, Math.max(0, cols - 9 - nameW - displayWidth(state) - width(tail))), s(state, a.state === 'need_you' ? 'warning' : 'inactive'), ...tail], cols, 'inactive', 'assignments'), ...(tail.length ? { more: true } : {}) })
  }
  // A host may allocate fewer rows than the supported terminal minimum. Never overrun it.
  return { lines: out.slice(0, Math.max(0, cap - 2)), statusTail, groups, decisions: brief.status === 'ok' ? brief.counts.needs_human : 0 }
}
