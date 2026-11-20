#!/usr/bin/env python3
# run.py <autopilot-checkout> [--out DIR] [--only project|noproject] : v7.4 pane key-acceptance gate (PM 01M7XAJ7, 01M7XARR).
# Opens a real `claude --plugin-dir <checkout>` in a private tmux server at 209x49, exactly as the operator would (/live-pane),
# then presses the PM's key sequence one key at a time. After every key it captures the screen (NN-name.txt plain, .ans colour)
# and asserts what must have changed: frame title, the ▸ card, 送出全部（N 張）, and that the prompt got no stray characters.
# Data is all fake (gen-data.py + bin/fleet); HOME is a sandbox that links only ~/.claude and ~/.claude.json so the session can log in.
# The submit step uses the store-only test card d-keys0004, never a real card. Exit 0 = every step passed; 1 = a step failed; 2 = setup.
import argparse, json, os, re, shutil, subprocess, sys, tempfile, time, unicodedata, uuid

HERE = os.path.dirname(os.path.realpath(__file__))
TITLES = ['現況', '待決', '派工', '審查', '花費', '殘留', '艦隊', '待辦', '說明']


def dw(t):
    return sum(2 if unicodedata.east_asian_width(c) in 'WF' else 1 for c in t)


class Screen:
    def __init__(self, text):
        self.text = text
        self.lines = text.split('\n')

    def title(self):  # the pane frame's title: text after '╭─ ' up to ' · ' or the rule
        for l in self.lines:
            i = l.find('╭─ ')
            if i >= 0:
                return re.split(r' · | ─', l[i + 3:])[0].strip()
        return None

    def prompt(self):  # what is typed in the prompt box: the last '❯' line; the placeholder counts as empty
        for l in reversed(self.lines):
            s = l.strip()
            if s.startswith('❯'):
                v = s[1:].strip()
                return '' if v.startswith('Try "') else v
        return None

    def cursor(self):  # the card number on the ▸ line
        for l in self.lines:
            m = re.search(r'▸\s+(\d+)\.', l)
            if m:
                return int(m.group(1))
        return None

    def sendn(self):
        m = re.search(r'送出全部（(\d+) 張）', self.text)
        return int(m.group(1)) if m else None

    def decide_count(self):
        m = re.search(r'要你決定 (\d+) 張', self.text)
        return int(m.group(1)) if m else None

    def strip_in_border(self):  # PM 01M7XB0B: the page strip sits at the right of the frame's bottom border, never on a row of its own
        bottom = next((l for l in self.lines if '╰─' in l), None)
        alone = any('1 現況' in l and '9 說明' in l and '╰─' not in l for l in self.lines)
        return bottom is not None and '1 現況' in bottom and '9 說明' in bottom and not alone

    def body(self):  # the frame's inner rows: between ╭ and ╰, without the │ edges and trailing blanks
        top = next((i for i, l in enumerate(self.lines) if '╭─' in l), None)
        bot = next((i for i, l in enumerate(self.lines) if '╰─' in l), None)
        if top is None or bot is None:
            return []
        return [re.sub(r'\s*│?\s*$', '', re.sub(r'^\s*│ ?', '', l)) for l in self.lines[top + 1:bot]]

    # PM 01M7XBE2 reading rules: no cut while rows are free, text column 70–90 wide, a blank row between options
    def no_cut_with_room(self):
        b = self.body()
        cut = any('…' in l for l in b)
        free = sum(1 for l in b if l.strip() == '')
        return not cut or (free < 2 and any('↓ 還有' in l for l in b))

    def text_width_ok(self):
        w = [dw(l) for l in self.body() if l.strip() and not re.match(r'^[▾▸─\s]*$', l) and '─' not in l]
        return bool(w) and max(w) <= 92

    def blank_between_options(self):
        b = self.body()
        idx = [i for i, l in enumerate(b) if re.match(r'^\s*[ab]: ', l)]
        return len(idx) == 2 and any(b[j].strip() == '' for j in range(idx[0] + 1, idx[1]))

    def has(self, s):
        return s in self.text

    def line_with(self, s):
        return next((l for l in self.lines if s in l), None)

    def band(self):  # the band row: holds 艦隊 and 待決 with │ separators, below the page strip
        rows = [l for l in self.lines if '│' in l and '艦隊' in l and '待決' in l and not l.lstrip().startswith('│')]
        return rows[-1].strip() if rows else None


class Gate:
    def __init__(self, ap, out, scenario):
        self.ap, self.out, self.scenario = ap, out, scenario
        self.sock = 'v74keys-%d-%s' % (os.getpid(), scenario)
        self.sess = 'k'
        self.n = 0
        self.fails = 0
        self.results = open(os.path.join(out, 'results.jsonl'), 'a', encoding='utf-8')

    def tmux(self, *a, check=True):
        return subprocess.run(['tmux', '-L', self.sock] + list(a), capture_output=True, text=True, check=check)

    def snap(self):
        return Screen(self.tmux('capture-pane', '-p', '-J', '-t', self.sess).stdout)

    def keys(self, *ks, literal=False):
        for k in ks:
            self.tmux('send-keys', '-t', self.sess, *(['-l', k] if literal else [k]))
            time.sleep(0.35)

    def step(self, name, send, checks, wait=6.0):
        """send() presses the keys; checks is [(label, fn(Screen) -> bool)]. Polls until all hold or `wait` s pass."""
        self.n += 1
        tag = '%02d-%s-%s' % (self.n, self.scenario, name)
        send()
        deadline = time.time() + wait
        while True:
            time.sleep(0.5)
            sc = self.snap()
            res = [(lab, bool(fn(sc))) for lab, fn in checks]
            if all(ok for _, ok in res) or time.time() > deadline:
                break
        with open(os.path.join(self.out, tag + '.txt'), 'w', encoding='utf-8') as f:
            f.write(sc.text)
        with open(os.path.join(self.out, tag + '.ans'), 'w', encoding='utf-8') as f:
            f.write(self.tmux('capture-pane', '-p', '-e', '-J', '-t', self.sess).stdout)
        ok = all(o for _, o in res)
        self.fails += 0 if ok else 1
        seen = dict(title=sc.title(), prompt=sc.prompt(), cursor=sc.cursor(), send_n=sc.sendn(), decide_n=sc.decide_count(), band=sc.band())
        self.results.write(json.dumps(dict(step=tag, ok=ok, checks=[dict(check=l, ok=o) for l, o in res], seen=seen), ensure_ascii=False) + '\n')
        self.results.flush()
        mark = 'PASS' if ok else 'FAIL'
        print('%s %s' % (mark, tag))
        for lab, o in res:
            if not o:
                print('      ✗ %s' % lab)
        if not ok:
            print('      seen: %s' % json.dumps(seen, ensure_ascii=False))
            self.recover()
        return sc

    def recover(self):
        """After a failed step: close a dialog the stray keys opened and clear stray prompt text, so one failure does not fail every later step."""
        for _ in range(3):
            sc = self.snap()
            if sc.title() is None and sc.prompt() not in (None, '') and re.match(r'\d+\. ', sc.prompt()):
                self.keys('Escape')
            elif sc.prompt():
                self.keys('C-u')
            else:
                return
            time.sleep(0.5)

    def enter_if_clean(self):
        """Enter only when the prompt holds nothing: a stray key in the prompt plus Enter would send a turn to the model."""
        if self.snap().prompt() == '':
            self.keys('Enter')


def setup(ap, out, scenario):
    work = tempfile.mkdtemp(prefix='v74keys-')
    data, home, cwd = os.path.join(work, 'data'), os.path.join(work, 'data', 'home'), os.path.join(work, 'cwd')
    for d in (home, os.path.join(cwd, '.git'), os.path.join(cwd, '.claude'), os.path.join(home, '.local/state/fleet')):
        os.makedirs(d, exist_ok=True)
    open(os.path.join(cwd, '.git', 'HEAD'), 'w').write('ref: refs/heads/main\n')
    open(os.path.join(cwd, '.fleet-no-register'), 'w').close()
    real = os.path.expanduser('~')
    os.symlink(os.path.join(real, '.claude'), os.path.join(home, '.claude'))  # login only; the session writes its transcript there
    os.symlink(os.path.join(real, '.claude.json'), os.path.join(home, '.claude.json'))
    env = dict(os.environ, V74_FIX=os.path.join(ap, 'mods/live/fixtures'), V74_KEYS_LOG=os.path.join(out, 'fleet-calls-%s.jsonl' % scenario))
    with open(os.path.join(home, '.local/state/fleet/console.json'), 'w') as f:
        subprocess.run([os.path.join(HERE, 'bin/fleet'), 'console'], stdout=f, env=env, check=True)
    return work, data, home, cwd, env


def run(ap, out, scenario):
    work, data, home, cwd, env = setup(ap, out, scenario)
    sid = str(uuid.uuid4())
    gen = [sys.executable, '-I', os.path.join(HERE, 'gen-data.py'), data, sid, cwd] + (['noproject'] if scenario == 'noproject' else [])
    subprocess.run(gen, check=True)
    regen = subprocess.Popen(['bash', '-c', 'while sleep 25; do "$@"; done', 'regen'] + gen)
    g = Gate(ap, out, scenario)
    store = os.path.join(data, 'demo/.git/autopilot/decisions')
    try:
        senv = ['env', 'HOME=' + home, 'PATH=' + os.path.join(HERE, 'bin') + ':' + os.environ['PATH'], 'V74_FIX=' + env['V74_FIX'],
                'V74_KEYS_LOG=' + env['V74_KEYS_LOG'], 'AUTOPILOT_LIVE_DIR=' + os.path.join(data, 'live'), 'CODEFORGE_DATA_DIR=' + os.path.join(data, 'cfdata'),
                'AUTOPILOT_CARD_MIRROR_DIR=' + os.path.join(data, 'mirror'), 'AUTOPILOT_ROOMS_DIR=' + os.path.join(data, 'rooms'),
                'AUTOPILOT_LOCALE=zh-TW', 'LANG=zh_TW.UTF-8', 'claude', '--session-id', sid, '--plugin-dir', ap, '--model', 'haiku']
        g.tmux('new-session', '-d', '-s', g.sess, '-x', '209', '-y', '49', '-c', cwd, ' '.join("'%s'" % a.replace("'", "'\\''") for a in senv))
        t0 = time.time()
        while time.time() - t0 < 40:  # trust prompt, then the input box
            time.sleep(1)
            sc = g.snap()
            if 'trust this folder' in sc.text.lower():
                g.keys('Down', 'Enter')
            elif sc.prompt() is not None and time.time() - t0 > 12:
                break
        time.sleep(8)  # first band draw + feeds
        if scenario == 'noproject':
            g.step('band-noproject', lambda: None, [
                ('band shows 艦隊 and 待決', lambda s: s.band() is not None),
                ('band has ⓘ', lambda s: s.band() is not None and 'ⓘ' in s.band()),
                ('no 沒有專案 prompt in the band', lambda s: not s.has('沒有專案')),
                ('band shows tok/s', lambda s: s.band() is not None and 'tok/s' in s.band()),
            ], wait=20)
            return g.fails
        g.step('open', lambda: (g.keys('/live-pane', literal=True), g.keys('Enter')), [
            ('title 現況', lambda s: s.title() == '現況'),
            ('band shows 艦隊 + 待決 4', lambda s: s.band() is not None and '待決 4' in s.band()),
            ('prompt empty', lambda s: s.prompt() == ''),
        ], wait=15)
        for i, t in enumerate(TITLES, 1):
            g.step('digit-%d' % i, lambda i=i: g.keys(str(i), literal=True), [
                ('title %s' % t, lambda s, t=t: s.title() == t),
                ('prompt empty', lambda s: s.prompt() == ''),
                ('page strip in the bottom border (no strip row)', lambda s: s.strip_in_border()),
            ])
        g.step('decide', lambda: g.keys('2', literal=True), [
            ('title 待決', lambda s: s.title() == '待決'),
            ('要你決定 4 張 (fleet + card store)', lambda s: s.decide_count() == 4),
            ('test card d-keys0004 / 驗收測試卡 listed', lambda s: s.has('驗收測試卡')),
            ('▸ on card 1', lambda s: s.cursor() == 1),
            ('送出 0 張', lambda s: s.sendn() == 0),
            ('no … cut while rows are free', lambda s: s.no_cut_with_room()),
            ('text column ≤ 90 wide (not the full 209)', lambda s: s.text_width_ok()),
            ('blank row between options a: and b:', lambda s: s.blank_between_options()),
        ])
        before = {}
        def grab():
            before['b'] = g.snap().line_with('b: 再審一輪')
            g.keys('C-x'); g.keys('Tab')
        g.step('ctrlx-tab', grab, [('prompt empty', lambda s: s.prompt() == ''), ('title 待決', lambda s: s.title() == '待決')])
        g.step('pick-b', lambda: g.keys('b', literal=True), [
            ('prompt empty (b did not go to the prompt)', lambda s: s.prompt() == ''),
            ('送出 ≥1 張', lambda s: (s.sendn() or 0) >= 1),
            ('option b line changed (shown as picked)', lambda s: s.line_with('再審一輪') is not None and s.line_with('再審一輪') != before.get('b')),
        ])
        for name, key, want in (('down-1', 'Down', 2), ('down-2', 'Down', 3), ('up-1', 'Up', 2)):
            g.step(name, lambda key=key: g.keys(key), [('▸ on card %d' % want, lambda s, w=want: s.cursor() == w), ('prompt empty', lambda s: s.prompt() == '')])
        g.step('pick-a', lambda: g.keys('a', literal=True), [
            ('prompt empty', lambda s: s.prompt() == ''),
            ('送出 2 張', lambda s: s.sendn() == 2),
        ])
        g.step('note', lambda: (g.keys('m', literal=True), g.keys('test note', literal=True), g.enter_if_clean()), [
            ('附註 shows test note', lambda s: s.has('test note') and s.line_with('test note') is not None and '附註' in s.line_with('test note')),
            ('prompt empty', lambda s: s.prompt() == ''),
        ])
        g.step('focused-4', lambda: g.keys('4', literal=True), [('title 審查', lambda s: s.title() == '審查'), ('prompt empty', lambda s: s.prompt() == '')])
        g.step('focused-2', lambda: g.keys('2', literal=True), [('title 待決', lambda s: s.title() == '待決'), ('送出 2 張 kept', lambda s: s.sendn() == 2)])
        # submit: move to the test card, pick 收到, send. The PM asked for the store-only test card, so the earlier picks are cleared first
        # is not possible from keys; the send carries every pick, all fake. The check is that d-keys0004 reaches the queue.
        def to_test():
            for _ in range(6):
                if g.snap().cursor() == 4:
                    break
                g.keys('Down')
            g.keys('a', literal=True)
        g.step('pick-test', to_test, [
            ('▸ on card 4', lambda s: s.cursor() == 4), ('送出 3 張', lambda s: s.sendn() == 3),
            ('long 背景 shown whole (its last words on screen)', lambda s: s.has('建議的選項要比其他選項明顯')),
            ('no … cut while rows are free', lambda s: s.no_cut_with_room()),
            ('text column ≤ 90 wide (long 背景 wrapped)', lambda s: s.text_width_ok()),
        ])
        g.step('send', lambda: g.keys('s', literal=True), [
            ('[待決回覆] submitted', lambda s: s.has('[待決回覆]')),
            ('d-keys0004 in the submitted reply', lambda s: s.has('d-keys0004')),
        ], wait=30)
        g.keys('Escape')  # stop the model turn the reply started
        time.sleep(2)
        g.step('queued', lambda: None, [
            ('card store / fleet saw d-keys0004 answered', lambda s: queued(store, env['V74_KEYS_LOG'])),
        ], wait=15)
        g.step('esc-returns', lambda: (g.keys('Escape'), g.keys('z', literal=True)), [('prompt is z', lambda s: s.prompt() == 'z')])
        g.step('clear', lambda: g.keys('C-u'), [('prompt empty', lambda s: s.prompt() == '')])
        g.step('dirty-digit', lambda: g.keys('x3', literal=True), [
            ('prompt is x3 (platform: a typed prompt keeps digits)', lambda s: s.prompt() == 'x3'),
            ('title unchanged', lambda s: s.title() == '待決'),
        ])
        g.step('dirty-ctrlx-3', lambda: (g.keys('C-x'), g.keys('Tab'), g.keys('3', literal=True)), [
            ('title 派工', lambda s: s.title() == '派工'),
            ('prompt still x3', lambda s: s.prompt() == 'x3'),
        ])
        return g.fails
    finally:
        regen.kill()
        g.tmux('kill-server', check=False)
        shutil.rmtree(work, ignore_errors=True)


def queued(store, log):
    """d-keys0004 left the open queue: the store marks it answered/closed, or the reply path called fleet with it."""
    try:
        for fn in os.listdir(store):
            doc = json.load(open(os.path.join(store, fn), encoding='utf-8'))
            for c in doc.get('decisions', []):
                if c.get('id') == 'd-keys0004' and c.get('status') != 'open':
                    return True
    except OSError:
        pass
    try:
        return any('d-keys0004' in l for l in open(log, encoding='utf-8'))
    except OSError:
        return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('checkout')
    ap.add_argument('--out', default=None)
    ap.add_argument('--only', choices=['project', 'noproject'])
    a = ap.parse_args()
    chk = os.path.realpath(a.checkout)
    if not os.path.isdir(os.path.join(chk, 'mods/live')):
        print('setup: %s has no mods/live' % chk); return 2
    for tool in ('tmux', 'claude'):
        if shutil.which(tool) is None:
            print('setup: %s not on PATH' % tool); return 2
    out = os.path.realpath(a.out or tempfile.mkdtemp(prefix='v74keys-out-'))
    os.makedirs(out, exist_ok=True)
    head = subprocess.run(['git', '-C', chk, 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()
    print('v74-keys gate · autopilot %s · out %s' % (head[:10] or '?', out))
    fails = 0
    for sc in ([a.only] if a.only else ['project', 'noproject']):
        fails += run(chk, out, sc)
    print('RESULT %s · %d failed step(s) · %s' % ('GREEN' if fails == 0 else 'RED', fails, out))
    return 0 if fails == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
