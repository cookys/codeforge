// The G40 real-data check, forge-stats half. Not part of the plugin's own tests: scripts/g40-real-data/check.sh copies the
// plugin into a scratch dir, writes `real-data.input.ts` (the live dir's files, keyed by their path under `LIVE`) next to this
// file, and runs `claude plugin test` there. A plugin test may import only relative paths and "claude-code", so the data
// comes in as a module. Real data never enters the repo.
import { expect, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'
import { displayWidth } from '../hooks/model'
import { FILES, LIVE } from './real-data.input'

type PLine = { text: string; tone?: string }[]
type Tiers = Record<string, PLine[]>

const isDir = (p: string) => Object.keys(FILES).some(k => k.startsWith(p + '/'))

// a virtual file system over FILES, the env pointing at LIVE (as AUTOPILOT_LIVE_DIR would)
function live(on: Parameters<TestBody>[1]) {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('session.id', () => ({ value: 'sid' }))
  on('env.get', (_, e) => ({ value: e.name === 'AUTOPILOT_LIVE_DIR' ? LIVE : e.name === 'HOME' ? '/home/t' : e.name === 'CODEFORGE_LOCALE' ? 'zh-TW' : undefined }))
  on('process.run', () => ({ value: { exitCode: 0, stdout: '1000\n', stderr: '' } }))
  on('store.get', () => ({ value: undefined }))
  on('store.set', () => ({ value: undefined }))
  on('fs.write', () => ({ value: undefined }))
  on('ui.invalidate', () => ({ value: undefined }))
  on('fs.stat', (_, e) => {
    if (e.path in FILES) return { value: { kind: 'file', size: (FILES[e.path] as string).length, mtimeMs: 0, isLink: false } }
    if (isDir(e.path)) return { value: { kind: 'dir', size: 0, mtimeMs: 0, isLink: false } }
    throw new Error('ENOENT ' + e.path)
  })
  on('fs.list', (_, e) => {
    const dir = e.path as string
    if (!isDir(dir)) throw new Error('ENOENT ' + dir)
    const names = new Set<string>()
    for (const k of Object.keys(FILES)) if (k.startsWith(dir + '/')) names.add(k.slice(dir.length + 1).split('/')[0] as string)
    return {
      value: [...names].map(name => {
        const p = dir + '/' + name
        const file = p in FILES
        return { name, kind: file ? 'file' : 'dir', size: file ? (FILES[p] as string).length : 0, mtimeMs: 0, isLink: false }
      }),
    }
  })
  on('fs.read', (_, e) => {
    if (e.path in FILES) return { value: FILES[e.path] }
    throw new Error('ENOENT ' + e.path)
  })
}

// an inline autopilot that reads `forge-stats.panel` while drawing, as the band does
const READER = {
  name: 'autopilot',
  register: ((on: any) => {
    on('ui.render', { component: 'Pane', requestId: 'live' }, async ($: any, e: any) => {
      const { Text } = $.ui.resolve(e)
      const got = await $.state.get({ plugin: 'forge-stats', key: 'panel' })
      return <Text key="v">{JSON.stringify(got.value ?? null)}</Text>
    })
  }) as never,
}
const LIVE_PANE = { plugin: 'autopilot', component: 'Pane', requestId: 'live', surface: 'terminal', props: { title: 'live', isFocused: false, bodyColumns: 100, placement: 'dock', scroll: { offset: 0, bodyRows: 40 }, view: {} } } as const

const text = (l: PLine) => l.map(x => x.text).join('')

// every line of every tier: within the tier's width; `$` and `acct-` never shown where the contract forbids them
function problems(where: string, tiers: Tiers, noMoney: boolean): string[] {
  const out: string[] = []
  for (const [w, lines] of Object.entries(tiers)) {
    for (const l of lines) {
      const t = text(l)
      if (displayWidth(t) > Number(w)) out.push(`${where}[${w}] too wide (${displayWidth(t)}): ${t}`)
      if (t.includes('acct-')) out.push(`${where}[${w}] shows an acct- alias: ${t}`)
      if (noMoney && t.includes('$')) out.push(`${where}[${w}] shows money: ${t}`)
    }
  }
  return out
}

test('G40 real data: panel/1 usage tiers and side fit their widths, show no acct- alias, and side shows no money', { plugins: [READER] }, async ($, on) => {
  live(on)
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(LIVE_PANE)
  const t = (await ui.find({ type: 'Text' })) as { text: string } | undefined
  await ui.unmount()
  const panel = JSON.parse(t?.text ?? 'null')
  expect(panel?.schema).toBe('codeforge.panel/1')
  // the data must have reached the panel: no side, or no quota line at all, means the input was not read
  expect(panel.side).toBeDefined()
  expect(Object.values(panel.side.usage.tiers as Tiers).some(lines => lines.length > 0)).toBe(true)
  const found = [
    ...problems('usage', panel.usage.tiers, false),
    ...problems('side.usage', panel.side.usage.tiers, true),
    ...problems('side.usage_full', panel.side.usage_full.tiers, true),
    ...problems('side.memory', panel.side.memory.tiers, true),
  ]
  expect(found).toEqual([])
})
