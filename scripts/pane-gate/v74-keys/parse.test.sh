#!/usr/bin/env bash
# parse.test.sh : the v74-keys screen parser (run.py's Screen) against stored captures — a red one from autopilot 94100764e
# (the PM's 01M7XAJ7 failures) and a hand-made green frame (PM 01M7XB0B strip-in-border, 01M7XBE2 reading rules).
# Hermetic (no tmux, no claude); check-all runs it. The live gate itself is run.py, attached to every pane delivery.
set -euo pipefail
cd "$(dirname "$0")"
python3 -I - <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location('run', 'run.py'); r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
def sc(n): return r.Screen(open('fixtures/' + n, encoding='utf-8').read())
red, b, green = sc('red-11-project-decide.txt'), sc('red-13-project-pick-b.txt'), sc('green-decide.txt')
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
    ('negative: … with free rows fails', r.Screen(green.text.replace('明顯。', '明…').replace('│ s: 送出', '│\n│\n│ s: 送出')).no_cut_with_room(), False),
]
bad = [(n, got, exp) for n, got, exp in want if got != exp]
for n, got, exp in bad: print('FAIL %s: got %r want %r' % (n, got, exp))
print('parse.test: %d/%d ok' % (len(want) - len(bad), len(want)))
sys.exit(1 if bad else 0)
PY
