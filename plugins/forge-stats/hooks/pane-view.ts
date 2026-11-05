// The chat-style pane window (TUI guideline §13): the operation area (tab strip + key hint) is the last CONTROL_ROWS rows of the tree, the content above it is a window the mod
// cuts itself, anchored at the bottom. Pure arithmetic, apart from the drawing in register.tsx.
import { displayWidth } from './model'

export const CONTROL_ROWS = 2

// per tab: stuck to the newest row, or standing at row `top` of the content
export type PaneScroll = { bottom: boolean; top: number }
export const newPaneScroll = (): PaneScroll => ({ bottom: true, top: 0 })

export type PaneWindow = {
  // content rows drawn (the indicator row not counted)
  area: number
  // first content row shown
  top: number
  // blank rows above the content (dock, short content only)
  pad: number
  // the content overflows (the mod windows it and the tree carries the spare row)
  overflow: boolean
  // the "n more" row shows: it overflows and the room holds the row and at least one content row
  indicator: boolean
  above: number
  below: number
}

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n))
const roomOf = (bodyRows: number) => Math.max(0, bodyRows - CONTROL_ROWS)

export function windowOf(total: number, bodyRows: number, dock: boolean, st: PaneScroll): PaneWindow {
  const room = roomOf(bodyRows)
  if (total <= room) return { area: total, top: 0, pad: dock ? room - total : 0, overflow: false, indicator: false, above: 0, below: 0 }
  // the operation area always fits: below two rows of room the indicator goes, below one the content too
  const indicator = room >= 2
  const area = indicator ? room - 1 : room
  const maxTop = total - area
  const top = st.bottom ? maxTop : clamp(st.top, 0, maxTop)
  return { area, top, pad: 0, overflow: true, indicator, above: top, below: maxTop - top }
}

// What one `ui.scroll` of the person does. `by` is the engine's: an arrow a row, a page key `bodyRows`, Home / End `contentRows` (signed), the wheel a few rows.
export function scrolled(st: PaneScroll, total: number, e: { by: number; bodyRows: number; contentRows: number }): PaneScroll {
  const w = windowOf(total, e.bodyRows, false, st)
  if (!w.overflow) return st
  const maxTop = total - w.area
  const sign = e.by < 0 ? -1 : 1
  const want = Math.abs(e.by) >= e.contentRows ? (sign < 0 ? 0 : maxTop) : Math.abs(e.by) >= e.bodyRows ? w.top + sign * w.area : w.top + e.by
  const top = clamp(want, 0, maxTop)
  return { bottom: top === maxTop, top }
}

// cut `text` to at most `cols` terminal cells (a CJK character is 2)
export function clipCells(text: string, cols: number): string {
  let used = 0
  let out = ''
  for (const ch of Array.from(text)) {
    const w = displayWidth(ch)
    if (used + w > cols) break
    out += ch
    used += w
  }
  return out
}
