// UI strings, en + zh-TW. Locale order mirrors codeforge's own: CODEFORGE_LOCALE,
// then Claude Code's `language` setting, then LC_ALL / LANG, else en.

export type Locale = 'en' | 'zh-TW'

const EN = {
  title: 'Forge stats',
  pet: 'Pet',
  fleet: 'fleet',
  name: 'name',
  village: 'village',
  xp: 'XP',
  stats: 'stats',
  noPet: 'no pet yet',
  noPetAdopt: 'run `codeforge adopt` to pick a village',
  noSnapshot: 'no snapshot yet: it appears after the codeforge statusline renders',
  badSnapshot: 'snapshot unreadable (other schema?)',
  strategy: 'strategy',
  says: 'says',
  staleNote: 'snapshot from {n} min ago',
  opened: 'Forge stats pane opened.',
  cmdDesc: 'Open the codeforge pet pane',
  memory: 'memory',
  entries: 'entries',
  brain: 'brain',
  brainOk: 'connected',
  brainDegraded: 'degraded',
  brainOffline: 'offline',
  brainPending: 'pending',
  layoutSet: 'Forge stats layout: {layout}.',
  layoutUsage: 'Usage: /forge-stats layout band|pane (now: {layout}).',
} as const

export type StringId = keyof typeof EN

const ZH_TW: Record<StringId, string> = {
  title: 'Forge stats',
  pet: '寵物',
  fleet: '艦隊',
  name: '名字',
  village: '村落',
  xp: 'XP',
  stats: '屬性',
  noPet: '還沒有寵物',
  noPetAdopt: '執行 `codeforge adopt` 選村落',
  noSnapshot: '還沒有快照：codeforge statusline 畫過一次之後就會出現',
  badSnapshot: '快照讀不懂（schema 不同？）',
  strategy: '策略',
  says: '說',
  staleNote: '快照是 {n} 分鐘前的',
  opened: 'Forge stats 面板已開啟。',
  cmdDesc: '開啟 codeforge 寵物面板',
  memory: '記憶',
  entries: '條',
  brain: '大腦',
  brainOk: '已連線',
  brainDegraded: '降級',
  brainOffline: '離線',
  brainPending: '等待中',
  layoutSet: 'Forge stats 版面：{layout}。',
  layoutUsage: '用法：/forge-stats layout band|pane（目前：{layout}）。',
}

export type T = (id: StringId, vars?: Record<string, string | number>) => string

export function makeT(locale: Locale): T {
  const table: Record<StringId, string> = locale === 'zh-TW' ? ZH_TW : EN
  return (id, vars) => {
    let s = table[id] ?? EN[id] ?? id
    for (const [k, v] of Object.entries(vars ?? {})) s = s.replace(`{${k}}`, String(v))
    return s
  }
}

export function localeFrom(input: { env?: string | undefined; language?: string | null | undefined; lang?: string | undefined }): Locale {
  const pick = (s: string | null | undefined): Locale | null => {
    if (!s || s.trim() === '') return null
    if (/正體|繁體|台灣|zh[-_]?tw|zh[-_]?hant|traditional/i.test(s)) return 'zh-TW'
    if (/^en\b|english/i.test(s)) return 'en'
    return null
  }
  return pick(input.env) ?? pick(input.language) ?? pick(input.lang) ?? 'en'
}
