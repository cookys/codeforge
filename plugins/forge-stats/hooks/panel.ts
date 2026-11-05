// The integrated panel's contract (codeforge.panel/1, autopilot × codeforge design v2, operator d-fb45042e): forge-stats writes `$.state` key `panel`, autopilot's panel shell
// draws the 用量 / 寵物 tabs from it and computes nothing. The lines are the pane's own (usageLines / petLines), laid out once per tier width; the reader takes the widest
// tier that fits its columns (40 below 40) and clips each line to cols - 1.
import type { PanelLine, PanelState, PanelTone, SpriteData } from '../types'

export const PANEL_SCHEMA = 'codeforge.panel/1'
export const PANEL_TIERS = [40, 60, 80, 100, 120] as const
// rewritten at least this often while nothing changes, so the reader's 60 s staleness note never shows for live data
export const PANEL_REFRESH_MS = 30_000

type Seg = { text: string; role?: 'structure' | 'idle' | 'dim' | 'bold' | 'mono' }

// a pane role → a tone: the two colour roles become theme text-colour keys, the styles keep their names
const TONE: Record<NonNullable<Seg['role']>, PanelTone> = { structure: 'subtle', idle: 'inactive', dim: 'dim', bold: 'bold', mono: 'mono' }

export const panelLine = (line: readonly Seg[]): PanelLine => line.map(s => (s.role === undefined ? { text: s.text } : { text: s.text, tone: TONE[s.role] }))

export function panelTiers(lay: (cols: number) => readonly (readonly Seg[])[]): Record<string, PanelLine[]> {
  return Object.fromEntries(PANEL_TIERS.map(w => [String(w), lay(w).map(panelLine)]))
}

export function panelState(nowMs: number, usage: Record<string, PanelLine[]>, pet: { title: string; sprite: SpriteData | null; tiers: Record<string, PanelLine[]> }): PanelState {
  return { schema: PANEL_SCHEMA, updated_at_ms: nowMs, usage: { tiers: usage }, pet }
}
