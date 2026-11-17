// Read-only fleet.brief/1 contract and priority selection. Unknown kinds are forward compatible.
import { byteLength } from './quota'

export const BRIEF_MAX_BYTES = 512 * 1024
export const BRIEF_STALE_MS = 5 * 60_000
export const briefPath = (home: string) => `${home}/.local/state/fleet/brief.json`
const KINDS = ['needs_human', 'waiting_pm', 'stuck', 'offline', 'high_context', 'waiting', 'info'] as const
export type BriefKind = typeof KINDS[number]
export type Assignment = { who: string; title: string; state: 'need_you' | 'waiting' | 'running'; sinceMs: number }
export type BriefItem = {
  id: string; kind: BriefKind; level: 'red' | 'amber' | 'info'; subject: { display: string }
  title: string; text: string; since_ms: number | null; next_step_zh: string; ids: { task_id?: string; card_id?: string }
}
export type BriefView = { status: 'ok'; generatedAt: number; counts: Record<BriefKind, number>; items: BriefItem[]; degraded: boolean } | { status: 'missing' | 'bad' }
const obj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
const uint = (v: unknown): v is number => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0
const label = (v: unknown): v is string => typeof v === 'string' && v.trim() !== '' && !/[\u0000-\u001f\u007f-\u009f]/u.test(v)
const prose = (v: unknown): v is string => typeof v === 'string'

export function parseBrief(text: string | null): BriefView {
  if (text === null) return { status: 'missing' }
  const bad: BriefView = { status: 'bad' }
  if (byteLength(text) > BRIEF_MAX_BYTES) return bad
  let raw: unknown
  try { raw = JSON.parse(text) } catch { return bad }
  if (!obj(raw) || raw.schema !== 'fleet.brief/1' || !uint(raw.generated_at_ms) || !obj(raw.counts) || !KINDS.every(k => uint((raw.counts as Record<string, unknown>)[k]))) return bad
  if (!Array.isArray(raw.items) || raw.items.length > 500) return bad
  if (raw.error !== undefined && raw.error !== null && !prose(raw.error)) return bad
  if (raw.sources_down !== undefined && (!Array.isArray(raw.sources_down) || !raw.sources_down.every(label))) return bad
  const items: BriefItem[] = []
  for (const it of raw.items) {
    if (!obj(it) || typeof it.kind !== 'string') return bad
    if (!(KINDS as readonly string[]).includes(it.kind)) continue
    if (!label(it.id) || !obj(it.subject) || !label(it.subject.display) || !prose(it.title) || !prose(it.text) || !prose(it.next_step_zh)) return bad
    if (it.level !== 'red' && it.level !== 'amber' && it.level !== 'info') return bad
    if (it.since_ms !== null && !uint(it.since_ms)) return bad
    if (!obj(it.ids) || (it.ids.task_id !== undefined && !label(it.ids.task_id)) || (it.ids.card_id !== undefined && !label(it.ids.card_id))) return bad
    items.push({ id: it.id, kind: it.kind as BriefKind, level: it.level, subject: { display: it.subject.display }, title: it.title, text: it.text, since_ms: it.since_ms, next_step_zh: it.next_step_zh, ids: { ...(typeof it.ids.task_id === 'string' ? { task_id: it.ids.task_id } : {}), ...(typeof it.ids.card_id === 'string' ? { card_id: it.ids.card_id } : {}) } })
  }
  return { status: 'ok', generatedAt: raw.generated_at_ms, counts: raw.counts as Record<BriefKind, number>, items, degraded: raw.error != null || (Array.isArray(raw.sources_down) && raw.sources_down.length > 0) }
}

const oldest = (a: BriefItem, b: BriefItem) => (a.since_ms ?? Infinity) - (b.since_ms ?? Infinity)
const severity: Record<BriefKind, number> = { offline: 0, stuck: 1, waiting_pm: 2, high_context: 3, needs_human: 4, waiting: 5, info: 6 }
const urgency = { need_you: 0, waiting: 1, running: 2 }

export function selectBrief(view: BriefView, input: Assignment[]) {
  const seen = new Set<string>()
  const items = (view.status === 'ok' ? view.items : []).filter(i => {
    if (seen.has(i.id)) return false
    seen.add(i.id)
    return true
  })
  const decisions = items.filter(i => i.kind === 'needs_human').sort(oldest)
  const assignments = input.slice().sort((a, b) => urgency[a.state] - urgency[b.state] || a.sinceMs - b.sinceMs)
  const blocked = items.filter(i => ['offline', 'stuck', 'waiting_pm', 'high_context'].includes(i.kind))
    .sort((a, b) => Number(b.level === 'red') - Number(a.level === 'red') || severity[a.kind] - severity[b.kind] || oldest(a, b))
    .filter(i => {
      // Only a card for the same thing on the same lane supersedes a blocked record.
      return !decisions.some(d => d.subject.display === i.subject.display && (
        (i.ids.task_id !== undefined && d.ids.task_id === i.ids.task_id) ||
        (i.ids.card_id !== undefined && d.ids.card_id === i.ids.card_id)
      ))
    })
  return { decisions, assignments, blocked, background: view.status === 'ok' ? view.counts.waiting + view.counts.info : 0 }
}
