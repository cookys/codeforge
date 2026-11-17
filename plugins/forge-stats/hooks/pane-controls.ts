// The two operation rows, shared verbatim by the pane and screenshot harness.
import { displayWidth } from './model'
import { cut, fitSegs, gutter, s } from './overview-view'
import type { Group, GroupWindow, PaneSeg } from './overview-view'

export type ActionSeg = PaneSeg & { action?: string; hotkey?: string; key?: string }
export function paneChrome(o: { cols: number; tab: 'overview' | 'pet'; statusTail: PaneSeg[]; groups: Record<Group, GroupWindow>; decisions: number; info: boolean; autopilotHint: boolean; buttons: boolean; petWindow?: { above: number; below: number } }) {
  const tabs: ActionSeg[] = [...gutter('分頁', 'merged')]
  for (const [i, id] of (['overview', 'pet'] as const).entries()) {
    if (i) tabs.push(s('  '))
    const active = id === o.tab
    tabs.push({ ...s((active ? '▸' : '') + (id === 'overview' ? '總覽' : '寵物'), 'merged'), ...(!active && o.buttons ? { action: id, hotkey: 'p', key: 'tab-' + id } : {}) })
  }
  const tail = fitSegs(o.statusTail, Math.max(0, o.cols - 27))
  const tailW = tail.reduce((n, seg) => n + displayWidth(seg.text), 0)
  if (tailW) tabs.push(s(' '.repeat(Math.max(2, o.cols - 20 - tailW))), ...tail)
  const controls: ActionSeg[] = []
  const control = (hotkey: string, text: string, action = hotkey) => controls.push({ ...s(hotkey + ' ' + text, 'merged'), hotkey, action, key: 'key-' + hotkey })
  if (o.tab === 'overview') {
    if (o.decisions > 0 && !o.autopilotHint) control('d', '開啟方式')
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
  const fallback = o.tab === 'overview' && o.autopilotHint ? '輸入 /autopilot 開決策卡 · ' : ''
  let room = o.cols - 9 - displayWidth(fallback + end)
  const hint: ActionSeg[] = [...gutter('按鍵', 'merged')]
  if (o.buttons) for (const c of controls) {
    const cells = displayWidth(c.text) + 2
    if (cells > room) continue
    hint.push(c, s('  '))
    room -= cells
  }
  hint.push(s(cut(fallback + end, Math.max(0, displayWidth(fallback + end) + room)), 'merged'))
  return { tabs, hint }
}
