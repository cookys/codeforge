// The two operation rows, shared verbatim by the pane and screenshot harness.
import { displayWidth } from './model'
import { fitSegs, gutter, s } from './overview-view'
import type { Group, GroupWindow, PaneSeg } from './overview-view'

export type ActionSeg = PaneSeg & { action?: string; hotkey?: string; key?: string }
export function paneChrome(o: { cols: number; tab: 'overview' | 'pet'; statusTail: PaneSeg[]; groups: Record<Group, GroupWindow>; decisions: number; info: boolean; autopilotHint: boolean; buttons: boolean; petWindow?: { above: number; below: number } }) {
  const tabs: ActionSeg[] = [...gutter('分頁')]
  for (const [i, id] of (['overview', 'pet'] as const).entries()) {
    if (i) tabs.push(s('  '))
    const active = id === o.tab
    tabs.push({ ...s((active ? '▸' : '') + (id === 'overview' ? '總覽' : '寵物'), !active && o.buttons ? 'merged' : 'inactive', active), ...(!active && o.buttons ? { action: id, hotkey: 'p', key: 'tab-' + id } : {}) })
  }
  const tail = fitSegs(o.statusTail, Math.max(0, o.cols - 27))
  const tailW = tail.reduce((n, seg) => n + displayWidth(seg.text), 0)
  if (tailW) tabs.push(s(' '.repeat(Math.max(2, o.cols - 20 - tailW))), ...tail)
  const controls: ActionSeg[] = []
  const control = (hotkey: string, text: string, action = hotkey) => controls.push({ ...s(hotkey + ' ' + text, 'merged'), hotkey, action, key: 'key-' + hotkey })
  if (o.tab === 'overview') {
    if (o.decisions > 0 && !o.autopilotHint) control('d', '決策卡')
    for (const g of ['blocked', 'pools', 'assignments'] as Group[]) {
      const w = o.groups[g]
      if (w.total <= w.visible) continue
      if (g === 'blocked') {
        if (w.below) control('j', '下列', 'blocked:down')
        if (w.above) control('k', '上列', 'blocked:up')
      } else control(g === 'pools' ? 'u' : 'a', g === 'pools' ? '捲各池' : '捲交辦', g + ':cycle')
    }
    control('i', o.info ? '返回' : '說明')
  } else if (o.petWindow) {
    if (o.petWindow.below) control('j', '下列', 'pet:down')
    if (o.petWindow.above) control('k', '上列', 'pet:up')
  }
  const end = (o.buttons ? 'p 換頁 · ' : '') + 'Esc 回輸入框'
  const fallback = o.tab === 'overview' && o.autopilotHint ? '待決卡在 autopilot pane：輸入 /autopilot · ' : ''
  const live = o.buttons ? controls : []
  const size = (parts: ActionSeg[], gap: number, tail: string) => 9 + parts.reduce((n, c) => n + displayWidth(c.text) + gap, 0) + displayWidth(tail)
  const short: Record<string, string> = { d: 'd 決策卡', j: 'j↓', k: 'k↑', u: 'u池', a: 'a辦', i: o.info ? 'i返回' : 'i說明' }
  let shown = live
  let gap = 2
  let tailText = fallback + end
  if (size(shown, gap, tailText) > o.cols) {
    shown = live.map(c => ({ ...c, text: short[c.hotkey as string] ?? c.text }))
    gap = 1
    tailText = (fallback ? '待決卡在 autopilot pane：輸入 /autopilot ' : '') + (o.buttons ? 'p換頁 ' : '') + 'Esc 回輸入框'
  }
  if (size(shown, gap, tailText) > o.cols) {
    // Keep every binding even when only keycaps fit. The supported 80-column layout uses the form above.
    shown = live.map(c => ({ ...c, text: c.hotkey as string }))
    tailText = (fallback ? '待決卡在 autopilot pane：輸入 /autopilot ' : '') + (o.buttons ? 'p ' : '') + 'Esc 回輸入框'
  }
  const hint: ActionSeg[] = [...gutter('按鍵')]
  for (const c of shown) hint.push(c, s(' '.repeat(gap)))
  const page = o.buttons ? /p(?: 換頁 · |換頁 | )(?=Esc)/u.exec(tailText) : null
  if (page) {
    if (page.index) hint.push(s(tailText.slice(0, page.index)))
    hint.push({ ...s(page[0].trimEnd(), 'merged'), action: o.tab === 'overview' ? 'pet' : 'overview', key: 'key-p' }, s(' ' + tailText.slice(page.index + page[0].length)))
  } else hint.push(s(tailText))
  return { tabs, hint }
}
