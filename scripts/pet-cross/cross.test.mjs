// pet-cross, codeforge side (shared-TUI plan §4.3): the panel/1 forge-stats writes, read by autopilot's
// pet-tab reader (mods/live/panel.ts readPet). Run by scripts/pet-cross.sh with PET_CROSS_AP=<autopilot dir>.
import test from 'node:test'
import assert from 'node:assert/strict'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const here = path.dirname(new URL(import.meta.url).pathname)
const fs = path.join(here, '..', '..', 'plugins', 'forge-stats')
const ap = process.env.PET_CROSS_AP
const imp = p => import(pathToFileURL(p).href)

const { panelState, panelLine } = await imp(path.join(fs, 'hooks', 'panel.ts'))
const { PET_ICON_CANDIDATES, petIcon } = await imp(path.join(fs, 'hooks', 'pet-icon.ts'))
const { FERRIS } = await imp(path.join(fs, 'tests', 'ferris-sprite.ts'))
const { readPet } = await imp(path.join(ap, 'mods', 'live', 'panel.ts'))

const tiers = { 40: [panelLine([{ text: 'Lv 3 ' }, { text: 'Ferris', role: 'bold' }])] }
// The panel exactly as writePanel builds it (register.tsx): JSON round trip, as it crosses $.state.
const written = icon => JSON.parse(JSON.stringify(panelState(1_000_000, {}, { title: 'Pet', sprite: FERRIS, tiers, icon })))

test('every icon forge-stats can pick is accepted whole by autopilot', () => {
  for (const c of PET_ICON_CANDIDATES) {
    for (const text of [c.emoji, c.fallback]) {
      const got = readPet(written({ ...c, text })).pet
      assert.ok(got, `icon ${c.id} (${text}) refused`)
      assert.deepEqual(got.icon, { ...c, text })
    }
  }
})

test('the default icon (no env) is accepted', () => {
  const icon = petIcon({ term: undefined, lcAll: undefined, lcCtype: undefined, lang: undefined, pick: undefined })
  assert.ok(readPet(written(icon)).pet)
})

test('the stats tiers arrive with their tones', () => {
  const got = readPet(written({ ...PET_ICON_CANDIDATES[0], text: PET_ICON_CANDIDATES[0].fallback })).pet
  assert.deepEqual(got.tiers, { 40: [[{ text: 'Lv 3 ' }, { text: 'Ferris', tone: 'bold' }]] })
})

test('the sprite forge-stats sends (village.sprite: palette + frames) is drawn, not dropped', () => {
  const got = readPet(written({ ...PET_ICON_CANDIDATES[2], text: PET_ICON_CANDIDATES[2].fallback })).pet
  assert.ok(got.sprite, 'autopilot read the sprite as null: forge-stats sends {width,height,transparent,palette,frames,frame_ms}')
  assert.equal(got.sprite.length, FERRIS.height / 2, 'one half-block row per two pixel rows')
})
