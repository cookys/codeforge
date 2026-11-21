#!/usr/bin/env bash
# parse.test.sh : the v74-keys screen parser (run.py's Screen) against stored captures — a red one from autopilot 94100764e
# (the PM's 01M7XAJ7 failures) and a hand-made green frame (PM 01M7XB0B strip-in-border, 01M7XBE2 reading rules),
# and the ux-v7.4 design S3 capture (colour: current page marked, ★ option emphasised).
# Hermetic (no tmux, no claude); check-all runs it. The live gate itself is run.py, attached to every pane delivery.
set -euo pipefail
cd "$(dirname "$0")"
python3 -I - <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location('run', 'run.py'); r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
def sc(n): return r.Screen(open('fixtures/' + n, encoding='utf-8').read())
red, b, green = sc('red-11-project-decide.txt'), sc('red-13-project-pick-b.txt'), sc('green-decide.txt')
d3 = r.Screen.from_ansi(open('fixtures/design-S3.ans', encoding='utf-8').read())
a_line = next(l for l in d3.ansi if '80 欄' in l and 'a: ' in r.ESC.sub('', l))
want = [
    ('red title', red.title(), '待決'), ('red decide n', red.decide_count(), 3), ('red cursor', red.cursor(), 1), ('red send n', red.sendn(), 0),
    ('red prompt empty', red.prompt(), ''), ('red strip on its own row', red.strip_in_border(), False), ('red no blank between options', red.blank_between_options(), False),
    ('red band', '待決 3' in (red.band() or ''), True),
    ('b went to the prompt', b.prompt(), 'b'),
    ('green title', green.title(), '待決'), ('green decide n', green.decide_count(), 4), ('green cursor', green.cursor(), 4), ('green send n', green.sendn(), 3),
    ('green strip in border', green.strip_in_border(), True), ('green blank between options', green.blank_between_options(), True),
    ('green no cut', green.no_cut_with_room(), True), ('green width', green.text_width_ok(), True), ('green prompt empty', green.prompt(), ''),
    ('green band tok/s', 'tok/s' in (green.band() or ''), True),
    ('negative: unwrapped long line fails width', r.Screen(green.text.replace('放得', '放得' + '長' * 60)).text_width_ok(), False),
    ('design S3 page 2 marked', d3.active_tab_marked('2', '待決'), True), ('design S3 page 5 not marked', d3.active_tab_marked('5', '花費'), False),
    ('design S3 ★ emphasised', d3.rec_emphasized(), True), ('design S3 band under frame', d3.band_below_frame(), True),
    ('design S3 strip fits', d3.strip_fits('待決'), True), ('red strip row fails strip_fits', red.strip_fits('待決'), False),
    ('old design: strip row between frame and band', sc('design-r3-S3.txt').band_below_frame(), False), ('old design strip_fits', sc('design-r3-S3.txt').strip_fits('待決'), False),
    ('negative: ★ line unstyled', r.Screen.from_ansi('\n'.join(r.ESC.sub('', l) if '★' in l else l for l in d3.ansi)).rec_emphasized(), False),
    ('negative: ★ line styled like a:', r.Screen.from_ansi('\n'.join(a_line.replace('80 欄', '120 欄 ★ 建議') if '★' in l else l for l in d3.ansi)).rec_emphasized(), False),
    ('negative: plain capture has no style', r.Screen(d3.text).active_tab_marked('2', '待決'), False),
    ('negative: … with free rows fails', r.Screen(green.text.replace('明顯。', '明…').replace('│ s: 送出', '│\n│\n│ s: 送出')).no_cut_with_room(), False),
]
# v7.5 region parser (autopilot b52aaa491 layout): a hand-made docked frame, Pane at the right with a left border only, band at the bottom-left
def docked(cur, pick, band_sgr='', pane_sgr=''):
    L = 60
    def row(left, right, st=''):
        plain = r.ESC.sub('', left)
        return left + ' ' * (L - r.dw(plain)) + st + '│' + ('\x1b[m' if st else '') + right
    cards = ['合併 plan', '外掛更新']
    rows = [row('對話 ...', '✕'), row('', '現況  ✗ 額度預測 無來源', pane_sgr), row('', '▾ 要你決定 3 張', pane_sgr),
            row('', ('▸' if cur == 1 else ' ') + '  1. ' + cards[0], pane_sgr), row('', '      b: 再審一輪' + (' →' if pick else ''), pane_sgr),
            row('', ('▸' if cur == 2 else ' ') + '  2. ' + cards[1], pane_sgr), row('', '      · 今晚'),
            row(band_sgr + '↑ %d/3 %s' % (cur, cards[cur - 1]) + ('\x1b[m' if band_sgr else ''), '╰─ ↑↓ 換張 · Esc 還給輸入框'),
            row('合併 plan ▸ a 合併 ★ · b 再審一輪' + ('  → b' if pick else ''), '資料'), row('n 附註', ''), row('ctrl+x tab 拿鍵盤', '1: 現況 2: 待決'),
            row('▲ 要你決定 │ 待決 4 │ ⓘ   ▾ 收起', ''), '─' * 80, '❯', '─' * 80]
    return '\n'.join(rows)
d1, d2 = r.V75(r.Screen.from_ansi(docked(1, 'b'))), r.V75(r.Screen.from_ansi(docked(2, '')))
f1 = r.V75(r.Screen.from_ansi(docked(1, 'b', band_sgr='\x1b[38;5;174m', pane_sgr='\x1b[38;5;153m')))
want += [
    ('v75 layout docked', d1.layout, 'docked'), ('v75 both', d1.both(), True), ('v75 pane title', d1.pane_title(), '現況'),
    ('v75 cursor no', d1.cursor_no(), 1), ('v75 cursor moves', d2.cursor_no(), 2), ('v75 band k', (d1.band_k, d2.band_k), (1, 2)),
    ('v75 band title', d1.band_title, '合併 plan'),
    ('v75 pick in pane and band', (d1.picked('b', 'pane'), d1.picked('b', 'band')), (True, True)),
    ('v75 no pick without the mark', (d2.picked('b', 'pane'), d2.picked('b', 'band')), (False, False)),
    ('v75 band hint', d1.band_hint(), 'ctrl+x tab 拿鍵盤'),
    ('v75 colours differ only where styled', (d1.band_style() == f1.band_style(), d1.pane_style() == f1.pane_style()), (False, False)),
    ('v75 pane split foot starts at the hint row', d1.pane_foot()[0].startswith('╰─'), True),
    ('v75 markers: card numbers inside the Pane edge', [m[2] for m in d1.markers()], [1, 2]),
    ('v75 fold: mod toggle found', (d1.fold()['mod'] or (0, 0, None))[2], 'open'),
    ('negative: no Pane in the v7.4 single band', r.V75(green).both(), False),
]
bad = [(n, got, exp) for n, got, exp in want if got != exp]
for n, got, exp in bad: print('FAIL %s: got %r want %r' % (n, got, exp))
print('parse.test: %d/%d ok' % (len(want) - len(bad), len(want)))
sys.exit(1 if bad else 0)
PY
