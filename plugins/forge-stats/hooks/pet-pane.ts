// Pane-only copy. The band and integrated-panel text keep their existing contracts.
import type { PetView, SpriteData } from '../types'
import type { UsageView } from './model'
import { compactCount, displayWidth, halfBlockCells, meter, padRight, staleMinutes } from './model'
import { cut, fitSegs, gutter, s } from './overview-view'
import type { PaneRow } from './overview-view'

// Crop transparent outer rows across all frames so animation keeps a stable geometry.
// The band still uses the uncropped source sprite.
export function paneSpriteFrame(sprite: SpriteData, index: number) {
  const frames = sprite.frames.map((_, i) => halfBlockCells(sprite, i))
  let first = frames[0].rows
  let last = -1
  for (const frame of frames) {
    const bytes = Uint8Array.from(atob(frame.cells), c => c.charCodeAt(0))
    const view = new DataView(bytes.buffer)
    for (let y = 0; y < frame.rows; y++) for (let x = 0; x < frame.columns; x++) {
      if (view.getUint32((y * frame.columns + x) * 12, true) === 32) continue
      first = Math.min(first, y)
      last = Math.max(last, y)
    }
  }
  if (last < first) return null
  const frame = frames[index % frames.length]
  const bytes = atob(frame.cells).slice(first * frame.columns * 12, (last + 1) * frame.columns * 12)
  return { columns: frame.columns, rows: last - first + 1, cells: btoa(bytes) }
}

export function petPaneRows(view: PetView, usage: UsageView, cols: number, now: number) {
  const line = (key: string, label: string, text: string, warning = false): PaneRow => ({ key, segs: fitSegs([...gutter(label), s(text, warning ? 'warning' : 'inactive')], cols) })
  const stats: PaneRow[] = []
  if (view.status !== 'ok' || !view.pet || !view.village) stats.push(line('no-pet', '寵物', view.status === 'bad' ? '寵物快照無法讀取' : view.status === 'no-pet' ? '還沒有寵物，輸入 codeforge adopt 領養' : '寵物快照還沒出現，等待 codeforge statusline', view.status === 'bad'))
  else {
    const p = view.pet
    stats.push({ key: 'pet-name', segs: fitSegs([...gutter('寵物'), { ...s(p.name), bold: true }, s(`  等級 ${p.level}`)], cols) })
    stats.push(line('pet-village', '村落', view.village.display_name + ' · ' + view.village.language))
    if (p.xp_to_next > 0) {
      const bar = meter(p.xp / p.xp_to_next, Math.max(4, Math.min(16, cols - 30)))
      stats.push(line('pet-xp', '經驗', `${bar.on}${bar.off} ${Math.floor(p.xp / p.xp_to_next * 100)}% · ${p.xp}/${p.xp_to_next}`))
    }
    stats.push(line('pet-stats', '能力', `攻擊 ${p.atk} · 防禦 ${p.def} · 生命 ${p.hp}`), line('pet-support', '', `支援 ${p.sup} · 驗證 ${p.ver}`))
    const strategies: Record<string, string> = { aggressive: '強攻', defensive: '穩守', explorer: '探索', scholar: '學習' }
    if (p.strategy) stats.push(line('pet-strategy', '策略', strategies[p.strategy] ?? p.strategy))
    if (p.message) stats.push(line('pet-message', '說話', p.message))
    const age = staleMinutes(view, now)
    if (age !== null) stats.push(line('pet-age', '來源', `寵物舊資料 ${age}m`, true))
  }
  const models: PaneRow[] = []
  if (usage.status !== 'ok') models.push(line('models-unavailable', '本機', usage.status === 'bad' ? '本機用量摘要無法讀取' : '本機用量摘要還沒出現', true))
  else for (const [i, model] of usage.models.slice(0, 6).entries()) {
    const tail = ` ${compactCount(model.tokens)} 詞元 · ${model.requests} 次 · ${model.cost === null ? '未定價' : (model.unpriced ? '≥' : '') + '$' + model.cost.toFixed(2)}`
    models.push(line('model-' + model.model, i ? '' : '本機', padRight(cut(model.model, Math.max(4, cols - 9 - displayWidth(tail))), Math.max(4, cols - 9 - displayWidth(tail))) + tail))
  }
  return { stats, models }
}
