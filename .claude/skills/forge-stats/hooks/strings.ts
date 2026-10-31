// UI strings, en + zh-TW. Locale order mirrors codeforge's own: CODEFORGE_LOCALE,
// then Claude Code's `language` setting, then LC_ALL / LANG, else en.

export type Locale = 'en' | 'zh-TW'

const EN = {
  title: 'Forge stats',
  speed: 'Speed',
  recent: 'Recent requests',
  pet: 'Pet',
  main: 'main',
  firstToken: 'first token',
  median: 'median',
  requests: 'requests',
  reqUnit: '',
  out: 'output',
  withSub: 'incl. subagents',
  avg: 'avg',
  lastN: 'last {n}',
  best: 'best',
  noRequests: 'no requests timed yet',
  colTps: 'tok/s',
  colTtft: '1st tok',
  colOut: 'out',
  colModel: 'model',
  sub: 'sub',
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
  cmdDesc: 'Open the tok/s + codeforge pet pane',
} as const

export type StringId = keyof typeof EN

const ZH_TW: Record<StringId, string> = {
  title: 'Forge stats',
  speed: '速度',
  recent: '最近請求',
  pet: '寵物',
  main: '主對話',
  firstToken: '首個 token',
  median: '中位數',
  requests: '請求',
  reqUnit: '次',
  out: '輸出',
  withSub: '含 subagent',
  avg: '平均',
  lastN: '近 {n} 次',
  best: '最快',
  noRequests: '還沒有計時到任何請求',
  colTps: 'tok/s',
  colTtft: '首 token',
  colOut: '輸出',
  colModel: '模型',
  sub: 'sub',
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
  cmdDesc: '開啟 tok/s 與 codeforge 寵物面板',
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
