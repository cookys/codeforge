// The pet tab's icon (PM 01M7MD5Z): every candidate is exactly 2 cells as emoji and as fallback, the fallback is taken where the terminal cannot draw emoji,
// and forge-stats hands the decided icon to autopilot in codeforge.panel/1 `pet.icon`.
import { expect, test } from 'claude-code/testing'
import type { TestBody } from 'claude-code/testing'

import { displayWidth } from '../hooks/model'
import { DEFAULT_PET_ICON, PET_ICON_CANDIDATES, emojiOk, petIcon } from '../hooks/pet-icon'
import type { IconEnv } from '../hooks/pet-icon'

const ENV: IconEnv = { term: 'xterm-256color', lcAll: undefined, lcCtype: undefined, lang: 'zh_TW.UTF-8', pick: undefined }
const cps = (s: string) => [...s].map(c => c.codePointAt(0) as number)

test('every candidate: the emoji is one Emoji_Presentation code point (no U+FE0F) of 2 cells; the fallback is two printable ASCII characters', () => {
  expect(PET_ICON_CANDIDATES.length).toBe(3)
  expect(new Set(PET_ICON_CANDIDATES.map(c => c.id)).size).toBe(3)
  for (const c of PET_ICON_CANDIDATES) {
    expect(cps(c.emoji).length).toBe(1)
    expect(/^\p{Emoji_Presentation}$/u.test(c.emoji)).toBe(true)
    expect(c.emoji.includes('️')).toBe(false)
    expect(displayWidth(c.emoji)).toBe(2)
    expect(/^[\x21-\x7e]{2}$/.test(c.fallback)).toBe(true)
    expect(displayWidth(c.fallback)).toBe(2)
  }
  expect(PET_ICON_CANDIDATES.some(c => c.id === DEFAULT_PET_ICON)).toBe(true)
})

test('the drawn text is 2 cells in every env, emoji or fallback', () => {
  const envs: IconEnv[] = [ENV, { ...ENV, term: 'linux' }, { ...ENV, lang: 'C' }, { ...ENV, pick: 'plain' }, { ...ENV, pick: 'crab' }, { ...ENV, pick: 'nope' }]
  for (const env of envs) expect(displayWidth(petIcon(env).text)).toBe(2)
})

test('emoji where the terminal can draw it: xterm-family TERM with a UTF-8 locale, or no locale set at all', () => {
  expect(emojiOk(ENV)).toBe(true)
  expect(emojiOk({ ...ENV, term: 'tmux-256color' })).toBe(true)
  expect(emojiOk({ ...ENV, term: 'xterm-kitty', lang: 'en_US.utf8' })).toBe(true)
  expect(emojiOk({ term: undefined, lcAll: undefined, lcCtype: undefined, lang: undefined, pick: undefined })).toBe(true)
  expect(petIcon(ENV).text).toBe('\u{1F431}')
})

test('fallback: the Linux console and other no-emoji TERMs, a non-UTF-8 locale (LC_ALL over LC_CTYPE over LANG), or CODEFORGE_PET_ICON=plain', () => {
  for (const term of ['linux', 'dumb', 'vt100', 'vt220', 'ansi', 'cons25']) expect(petIcon({ ...ENV, term }).text).toBe(':3')
  expect(petIcon({ ...ENV, lang: 'C' }).text).toBe(':3')
  expect(petIcon({ ...ENV, lang: 'zh_TW.Big5' }).text).toBe(':3')
  expect(petIcon({ ...ENV, lcAll: 'POSIX' }).text).toBe(':3') // LC_ALL wins over a UTF-8 LANG
  expect(petIcon({ ...ENV, lcAll: undefined, lcCtype: 'C', lang: 'en_US.UTF-8' }).text).toBe(':3')
  expect(petIcon({ ...ENV, lcAll: 'en_US.UTF-8', lang: 'C' }).text).toBe('\u{1F431}')
  expect(petIcon({ ...ENV, pick: 'plain' })).toEqual({ id: DEFAULT_PET_ICON, emoji: '\u{1F431}', fallback: ':3', text: ':3' })
})

test('CODEFORGE_PET_ICON picks a candidate; an unknown id is the default', () => {
  expect(petIcon({ ...ENV, pick: 'chick' }).text).toBe('\u{1F423}')
  expect(petIcon({ ...ENV, pick: 'crab' }).text).toBe('\u{1F980}')
  expect(petIcon({ ...ENV, pick: 'crab', term: 'linux' }).text).toBe('}{')
  expect(petIcon({ ...ENV, pick: 'nope' }).id).toBe(DEFAULT_PET_ICON)
})

// ---- the contract: panel.pet.icon, read by autopilot's tab row ----
const READER = {
  name: 'autopilot',
  register: ((on: any) => {
    on('ui.render', { component: 'Pane', requestId: 'live' }, async ($: any, e: any) => {
      const { Text } = $.ui.resolve(e)
      const got = await $.state.get({ plugin: 'forge-stats', key: 'panel' })
      return <Text key="v">{JSON.stringify(got.value?.pet?.icon ?? null)}</Text>
    })
  }) as never,
}
const LIVE_PANE = { plugin: 'autopilot', component: 'Pane', requestId: 'live', surface: 'terminal', props: { title: 'live', isFocused: false, bodyColumns: 100, placement: 'dock', scroll: { offset: 0, bodyRows: 40 }, view: {} } } as const
const world = (on: Parameters<TestBody>[1], env: Record<string, string>) => {
  on('command.register', () => ({ value: { command: 'forge-stats' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('env.get', (_, e) => ({ value: e.name === 'HOME' ? '/home/t' : env[e.name] }))
  on('fs.read', (_, e) => {
    throw new Error('ENOENT ' + e.path)
  })
}
const iconOf = async ($: Parameters<TestBody>[0]) => {
  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const ui = await $.ui.mount(LIVE_PANE)
  const t = (await ui.find({ type: 'Text' })) as { text: string } | undefined
  await ui.unmount()
  return JSON.parse(t?.text ?? 'null')
}

test('panel.pet.icon: the default emoji under an xterm with a UTF-8 locale', { plugins: [READER] }, async ($, on) => {
  world(on, { TERM: 'xterm-256color', LANG: 'zh_TW.UTF-8' })
  expect(await iconOf($)).toEqual({ id: 'cat', emoji: '\u{1F431}', fallback: ':3', text: '\u{1F431}' })
})

test('panel.pet.icon: the Linux console gets the fallback; CODEFORGE_PET_ICON chooses the candidate', { plugins: [READER] }, async ($, on) => {
  world(on, { TERM: 'linux', LANG: 'en_US.UTF-8', CODEFORGE_PET_ICON: 'chick' })
  expect(await iconOf($)).toEqual({ id: 'chick', emoji: '\u{1F423}', fallback: 'o<', text: 'o<' })
})
