import { expect, test } from 'claude-code/testing'
import { BRIEF, paneTarget, runtimeWorld, start, topRows } from './fixtures/pane-runtime'

test('pane v2 defaults to overview, has two Traditional Chinese tabs and no blank padding', async ($, on) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/test' : undefined }))
  on('fs.read', () => { throw new Error('ENOENT') })
  on('fs.stat', () => { throw new Error('ENOENT') })
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount({ plugin: 'forge-stats', component: 'Pane', requestId: 'forge-stats', surface: 'terminal', viewport: { columns: 160, rows: 40, isFullscreen: true }, props: { title: '鍛造總覽', bodyColumns: 160, isFocused: true, placement: 'dock', scroll: { offset: 0, bodyRows: 30 }, view: {} } })
  const texts = await ui.findAll({ type: 'Text' })
  const all = texts.map(n => n.text).join('')
  expect(all).toContain('▸總覽')
  expect(all).toContain('寵物')
  expect(all).toContain('fleet 摘要檔還沒出現')
  expect(all).not.toMatch(/Usage|全部帳號|Esc.*PgUp/)
  expect(texts.some(n => n.text === ' ')).toBe(false)
  await ui.press({ key: 'tab-pet' })
  expect((await ui.findAll({ type: 'Text' })).map(n => n.text).join('')).toContain('▸寵物')
  await ui.unmount()
})

test('d explains the supported autopilot command in the last row; i opens and closes source help', async ($, on) => {
  runtimeWorld(on)
  await start($)
  const ui = await $.ui.mount(paneTarget())
  await ui.press({ key: 'key-d' })
  expect(topRows(await ui.drawn()).slice(-1)[0]).toContain('輸入 /autopilot 開決策卡')
  expect(await ui.find({ type: 'Button', key: 'key-d' })).toBe(undefined)
  await ui.press({ key: 'key-i' })
  expect(topRows(await ui.drawn()).join('')).toContain('fleet.brief/1')
  await ui.press({ key: 'key-i' })
  expect(topRows(await ui.drawn()).join('')).toContain('待決 3')
})

test('brief size and missing states preserve quota', async ($, on) => {
  const stat: Record<string, unknown> = { size: 512 * 1024 + 1 }
  const w = runtimeWorld(on, undefined, stat)
  await start($)
  const ui = await $.ui.mount(paneTarget())
  expect(w.log.includes('read:' + BRIEF)).toBe(false)
  expect(topRows(await ui.drawn()).join('')).toContain('fleet 摘要檔無法讀取')
  expect(topRows(await ui.drawn()).join('')).toContain('pool-0')
  delete w.files[BRIEF]
  await ui.redraw()
  expect(topRows(await ui.drawn()).join('')).toContain('fleet 摘要檔還沒出現')
  expect(topRows(await ui.drawn()).join('')).toContain('pool-0')
})
