import { mock } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'
import { byteLength } from '../../hooks/quota'
import { FERRIS } from '../ferris-sprite'
import { BRIEF_SAMPLE } from './brief-sample'
import { NOW, pools } from './pane-sample'

export const LIVE = '/state/test-live'
export const BRIEF = '/home/test/.local/state/fleet/brief.json'
export const QUOTA = LIVE + '/codeforge/usage/fleet-quota.json'
export const SUMMARY = LIVE + '/codeforge/usage/summary.json'
export const PET = LIVE + '/codeforge/pet.json'
export const petFixture = {
  schema: 'codeforge.pet-view/1', written_at: new Date(NOW).toISOString(),
  pet: { name: 'Ferris', level: 3, xp: 25, xp_to_next: 100, hp: 16, atk: 19, def: 11, sup: 18, ver: 13, mood: null, strategy: 'explorer', message: null },
  village: { id: 'rust', display_name: '鍛造遺跡', language: 'Rust', tagline: '', rgb: [215, 95, 0], art_small: [' _~^~^~_'], art_full: [' _~^~^~_', '/ o o \\', '  -  -'], sprite: FERRIS },
}
export const quotaFixture = () => ({ schema: 'codeforge.fleet-quota/1', generated_at_ms: NOW - 18000, use_target: 0.85, skipped: {}, accounts: pools().map(a => ({
  alias: a.alias, name: a.name, provider: 'claude', hosts: a.hosts, observed_at_ms: a.observedAtMs, age_s: 0, state: a.state,
  weekly: a.weekly === null ? null : { pct: a.weekly.pct, resets_at: a.weekly.resetsAt, reset: a.weekly.reset, warn: a.weekly.warn, projected: null, exhaust_at: a.weekly.exhaustAt },
  five_hour: a.fiveHour === null ? null : { pct: a.fiveHour.pct, resets_at: a.fiveHour.resetsAt, reset: a.fiveHour.reset },
  ...(a.sourceNote ? { source_note: a.sourceNote } : {}), ...(a.note ? { note: a.note } : {}), plan: a.plan,
})) })
export const usageFixture = (n = 8) => ({ schema: 'codeforge.usage-summary/1', generated_at_ms: NOW, host: 'host-aaaaaaaa', prices_as_of: '2026-10-06', accounts: [], days: Array.from({ length: n }, (_, i) => ({ day: new Date(NOW).toISOString().slice(0, 10), source: 'claude', account: 'pool-0', model: 'model-' + i, speed: 'standard', input: 100, output: 50, cache_write_5m: 0, cache_write_1h: 0, cache_read: 0, requests: 2, cost_usd: i + 1 })) })
export const runtimeFiles = () => ({ [BRIEF]: JSON.stringify(BRIEF_SAMPLE), [QUOTA]: JSON.stringify(quotaFixture()), [SUMMARY]: JSON.stringify(usageFixture()), [PET]: JSON.stringify(petFixture) })
export const paneTarget = (cols = 160, rows = 40, maxRows = 30, placement: 'dock' | 'inline' = 'dock') => ({ plugin: 'forge-stats', component: 'Pane', requestId: 'forge-stats', surface: 'terminal', viewport: { columns: cols, rows, isFullscreen: true }, props: { title: '鍛造總覽', bodyColumns: cols, isFocused: true, placement, scroll: { offset: 0, bodyRows: maxRows }, view: {} } }) as const
export type RenderNode = { type: string; text?: string; props?: Record<string, any>; children?: (RenderNode | string)[] }
export const textOf = (n: RenderNode | string): string => typeof n === 'string' ? n : (n.children ?? []).map(textOf).join('')
export const topRows = (tree: unknown) => ((tree as RenderNode).children ?? []).map(textOf)
export function runtimeWorld(on: Parameters<TestBody>[1], files: Record<string, string> = runtimeFiles(), stat: Record<string, unknown> = {}) {
  const log: string[] = []
  mock.clock(on, { now: NOW })
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('session.id', () => ({ value: 'test-session' }))
  on('ui.open', (_, e) => { log.push('open:' + e.id + ':' + e.focus); return { value: { isPlaced: true } } })
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/test' : e.name === 'AUTOPILOT_LIVE_DIR' ? LIVE : e.name === 'UID' ? '1000' : undefined }))
  on('fs.stat', (_, e) => { log.push('stat:' + e.path); if (!(e.path in files)) throw new Error('ENOENT'); return { value: { kind: 'file', isLink: false, size: byteLength(files[e.path]), mtimeMs: NOW, ...(e.path === BRIEF ? stat : {}) } } })
  on('fs.read', (_, e) => { log.push('read:' + e.path); if (!(e.path in files)) throw new Error('ENOENT'); return { value: files[e.path] } })
  on('fs.write', () => ({ value: undefined }))
  on('process.run', () => { throw new Error('A mod must not spawn a process') })
  return { files, log }
}
export const start = ($: Parameters<TestBody>[0]) => $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
export const command = ($: Parameters<TestBody>[0], args: string) => $.command.run({ command: 'forge-stats', args, origin: { kind: 'composer' }, presentation: { isFullscreen: true, columns: 160 } } as never)
