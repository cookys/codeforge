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


SGR = re.compile(r'\x1b\[([0-9;:]*)m')
ESC = re.compile(r'\x1b\[[0-9;:?]*[A-Za-z]')


def sgr_apply(st, params):
    """Fold one SGR sequence into the state {bold, rev, fg, bg}; the off codes tmux emits (22/27/39/49) clear their part."""
    ps = [int(x) if x.isdigit() else 0 for x in re.split('[;:]', params)] if params else [0]
    i = 0
    while i < len(ps):
        p = ps[i]
        if p == 0:
            st.clear()
        elif p == 1:
            st['bold'] = 1
        elif p == 22:
            st.pop('bold', None)
        elif p == 7:
            st['rev'] = 1
        elif p == 27:
            st.pop('rev', None)
        elif p in (38, 48):
            n = 3 if i + 1 < len(ps) and ps[i + 1] == 5 else 5
            st['fg' if p == 38 else 'bg'] = tuple(ps[i:i + n]); i += n - 1
        elif p == 39:
            st.pop('fg', None)
        elif p == 49:
            st.pop('bg', None)
        elif 30 <= p <= 37 or 90 <= p <= 97:
            st['fg'] = p
        elif 40 <= p <= 47 or 100 <= p <= 107:
            st['bg'] = p
        i += 1


def sgr_at(line, col_text):
    """The style {bold, rev, fg, bg} in force at the first character of col_text on an ANSI line."""
    plain, st, pos = '', {}, 0
    for m in SGR.finditer(line + '\x1b[m'):
        chunk = ESC.sub('', (line + '\x1b[m')[pos:m.start()])
        j = (plain + chunk).find(col_text)
        if j >= 0 and j >= len(plain) - len(col_text) + 1 and j < len(plain) + len(chunk):
            if j >= len(plain):
                return dict(st)
        plain += chunk
        sgr_apply(st, m.group(1))
        pos = m.end()
    return dict(st)


class Screen:
    def __init__(self, text, ansi=None):
        self.text = text
        self.lines = text.split('\n')
        self.ansi = ansi.split('\n') if ansi is not None else None

    @classmethod
    def from_ansi(cls, ansi):
        return cls('\n'.join(ESC.sub('', l) for l in ansi.split('\n')), ansi)

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
        m = re.search(r'送出(?:全部（|已選 )(\d+) 張', self.text)  # r10: 送出已選 N 張; before it 送出全部（N 張）
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

    def band_below_frame(self):  # PM 01M7XB0B: under the frame only the band row; nothing between ╰ and the band
        bot = next((i for i, l in enumerate(self.lines) if '╰─' in l), None)
        nxt = self.lines[bot + 1] if bot is not None and bot + 1 < len(self.lines) else ''
        return ('艦隊' in nxt or '產線' in nxt) and '待決' in nxt and '│' in nxt and '1 現況' not in nxt

    def strip_fits(self, title):  # narrow width: one bottom-border row ending in ╯, current page named, no page row elsewhere
        bot = [l for l in self.lines if '╰─' in l]
        return (len(bot) == 1 and bot[0].rstrip().endswith('╯') and title in bot[0]
                and not any(re.match(r'^\s*[1-9] \S', l) for l in self.lines if '╰─' not in l and '│' not in l))

    def active_tab_marked(self, k, title):  # the current page in the strip is reverse video or has a background colour
        if self.ansi is None:
            return False
        line = next((l for l in self.ansi if '╰─' in ESC.sub('', l)), None)
        if line is None:
            return False
        st = sgr_at(line, '%s %s' % (k, title))
        return 'rev' in st or 'bg' in st

    def rec_emphasized(self):  # PM 01M7XBE2: the ★ option differs from the others by bold or colour, not by the star alone
        if self.ansi is None:
            return False
        opt = [(ESC.sub('', l), l) for l in self.ansi if re.search(r'(^|\s)[abc]: ', ESC.sub('', l)) and '│' in ESC.sub('', l)]
        star = [(p, a) for p, a in opt if '★' in p]
        rest = [(p, a) for p, a in opt if '★' not in p]
        if len(star) != 1 or not rest:
            return False
        label = lambda p: re.search(r'[abc]: (\S)', p).group(1)
        mine = sgr_at(star[0][1], label(star[0][0]))
        return all(('bold' in mine and 'bold' not in o) or (mine.get('fg') is not None and mine.get('fg') != o.get('fg'))
                   for o in (sgr_at(a, label(p)) for p, a in rest))

    def has(self, s):
        return s in self.text

    def line_with(self, s):
        return next((l for l in self.lines if s in l), None)

    def band(self):  # the band row: holds 艦隊 (r10: 產線, same source as 現況) and 待決 with │ separators, below the page strip
        rows = [l for l in self.lines if '│' in l and ('艦隊' in l or '產線' in l) and '待決' in l and not l.lstrip().startswith('│')]
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
        return Screen.from_ansi(self.tmux('capture-pane', '-p', '-e', '-J', '-t', self.sess).stdout)

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
            f.write('\n'.join(sc.ansi))
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
                self.keys('Escape')  # leave a band input (e.g. an open 附註) first, so C-u reaches the prompt
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
    data, home, cwd = os.path.join(work, 'data'), os.path.join(work, 'data', 'home'), os.path.join(work, 'demo')  # the design's project name (S2: 本席 專案 demo)
    for d in (home, cwd, os.path.join(cwd, '.claude'), os.path.join(home, '.local/state/fleet')):
        os.makedirs(d, exist_ok=True)
    subprocess.run(['git', 'init', '-q', '-b', 'main', cwd], check=True)  # a real repo: open-decision.js derives the card store's key from it
    open(os.path.join(cwd, '.fleet-no-register'), 'w').close()
    real = os.path.expanduser('~')
    os.symlink(os.path.join(real, '.claude'), os.path.join(home, '.claude'))  # login only; the session writes its transcript there
    os.symlink(os.path.join(real, '.claude.json'), os.path.join(home, '.claude.json'))
    env = dict(os.environ, V74_FIX=os.path.join(ap, 'mods/live/fixtures'), V74_KEYS_LOG=os.path.join(out, 'fleet-calls-%s.jsonl' % scenario))
    with open(os.path.join(home, '.local/state/fleet/console.json'), 'w') as f:
        subprocess.run([os.path.join(HERE, 'bin/fleet'), 'console'], stdout=f, env=env, check=True)
    return work, data, home, cwd, env


def run(ap, out, scenario, alias=None):
    work, data, home, cwd, env = setup(ap, out, scenario)
    sid = str(uuid.uuid4())
    gen = [sys.executable, '-I', os.path.join(HERE, 'gen-data.py'), data, sid, cwd] + (['noproject'] if scenario == 'noproject' else [])
    subprocess.run(gen, check=True)
    regen = subprocess.Popen(['bash', '-c', 'while sleep 25; do "$@"; done', 'regen'] + gen)
    g = Gate(ap, out, scenario)
    store = os.path.join(cwd, '.git/autopilot/decisions')
    try:
        senv = ['env', 'HOME=' + home, 'PATH=' + os.path.join(HERE, 'bin') + ':' + os.environ['PATH'], 'V74_FIX=' + env['V74_FIX'],
                'V74_KEYS_LOG=' + env['V74_KEYS_LOG'], 'AUTOPILOT_LIVE_DIR=' + os.path.join(data, 'live'), 'CODEFORGE_DATA_DIR=' + os.path.join(data, 'cfdata'),
                'AUTOPILOT_CARD_MIRROR_DIR=' + os.path.join(data, 'mirror'), 'AUTOPILOT_ROOMS_DIR=' + os.path.join(data, 'rooms'),
                'AUTOPILOT_V74_KEYLOG=' + os.path.join(os.path.abspath(out), 'keylog-%s.jsonl' % scenario),
                'AUTOPILOT_LOCALE=zh-TW', 'LANG=zh_TW.UTF-8', 'claude', '--session-id', sid, '--plugin-dir', ap, '--model', 'haiku']
        cmd = ' '.join("'%s'" % a.replace("'", "'\\''") for a in senv)
        if alias is not None:
            # --token-alias: the session logs in with a pool's existing token instead of the default login (a pool at its
            # weekly limit never answers [待決回覆]). The file is read inside the tmux shell, so the value is in the
            # session's environment only: never in argv, the gate's output or a log. No token is copied, no login made.
            tok = os.path.join(os.path.expanduser('~'), '.config/autopilot/accounts', alias, 'token')
            cmd = 'unset CLAUDE_CONFIG_DIR; CLAUDE_CODE_OAUTH_TOKEN="$(cat %s)" exec %s' % ("'%s'" % tok.replace("'", "'\\''"), cmd)
            cmd = "bash -c '%s'" % cmd.replace("'", "'\\''")
        g.tmux('new-session', '-d', '-s', g.sess, '-x', '209', '-y', '49', '-c', cwd, cmd)
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
        time.sleep(1.0)  # r8-5c: the frame drew before its hotkeys were live and digit 1 reached the prompt; r8-5d green on rerun
        for i, t in enumerate(TITLES, 1):
            g.step('digit-%d' % i, lambda i=i: g.keys(str(i), literal=True), [
                ('title %s' % t, lambda s, t=t: s.title() == t),
                ('prompt empty', lambda s: s.prompt() == ''),
                ('page strip in the bottom border (no strip row)', lambda s: s.strip_in_border()),
                ('current page %d marked (reverse or background)' % i, lambda s, i=i, t=t: s.active_tab_marked(str(i), t)),
                ('band right under the frame', lambda s: s.band_below_frame()),
            ])
        g.step('decide', lambda: g.keys('2', literal=True), [
            ('title 待決', lambda s: s.title() == '待決'),
            ('要你決定 4 張 (fleet + card store)', lambda s: s.decide_count() == 4),
            ('▸ on card 1', lambda s: s.cursor() == 1),
            ('送出 0 張', lambda s: s.sendn() == 0),
            ('no … cut while rows are free', lambda s: s.no_cut_with_room()),
            ('text column ≤ 90 wide (not the full 209)', lambda s: s.text_width_ok()),
            ('blank row between options a: and b:', lambda s: s.blank_between_options()),
            ('★ option bold or coloured apart from the others', lambda s: s.rec_emphasized()),
        ])
        # PM 01M7XB0B width rule: at 100 columns the hint and the strip shrink, never wrap, never overlap; the current page keeps its name
        g.step('narrow-100', lambda: g.tmux('resize-window', '-t', g.sess, '-x', '100', '-y', '49'), [
            ('one bottom border ending in ╯, 待決 named, no strip row', lambda s: s.strip_fits('待決')),
            ('band right under the frame', lambda s: s.band_below_frame()),
        ], wait=8)
        g.tmux('resize-window', '-t', g.sess, '-x', '209', '-y', '49')
        time.sleep(2)
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
    ap.add_argument('--token-alias', default=None, help='log the session in with ~/.config/autopilot/accounts/<alias>/token (an existing pool token; read in the session shell, never printed)')
    a = ap.parse_args()
    chk = os.path.realpath(a.checkout)
    if not os.path.isdir(os.path.join(chk, 'mods/live')):
        print('setup: %s has no mods/live' % chk); return 2
    for tool in ('tmux', 'claude'):
        if shutil.which(tool) is None:
            print('setup: %s not on PATH' % tool); return 2
    if a.token_alias is not None:
        if not re.fullmatch(r'[A-Za-z0-9._-]+', a.token_alias):
            print('setup: --token-alias must be a plain alias name'); return 2
        tok = os.path.join(os.path.expanduser('~'), '.config/autopilot/accounts', a.token_alias, 'token')
        if not os.path.isfile(tok) or os.stat(tok).st_mode & 0o077:
            print('setup: no 0600 token file for alias %s' % a.token_alias); return 2
    out = os.path.realpath(a.out or tempfile.mkdtemp(prefix='v74keys-out-'))
    os.makedirs(out, exist_ok=True)
    head = subprocess.run(['git', '-C', chk, 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()
    print('v74-keys gate · autopilot %s · out %s' % (head[:10] or '?', out))
    fails = 0
    for sc in ([a.only] if a.only else ['project', 'noproject']):
        fails += run(chk, out, sc, a.token_alias)
    print('RESULT %s · %d failed step(s) · %s' % ('GREEN' if fails == 0 else 'RED', fails, out))
    return 0 if fails == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
