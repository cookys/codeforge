#!/usr/bin/env python3
# run.py <autopilot-checkout> [--out DIR] [--only project|noproject] [--v75] : v7.4 pane key-acceptance gate (PM 01M7XAJ7, 01M7XARR).
# Opens a real `claude --plugin-dir <checkout>` in a private tmux server at 209x49, exactly as the operator would (/live-pane),
# then presses the PM's key sequence one key at a time. After every key it captures the screen (NN-name.txt plain, .ans colour)
# and asserts what must have changed: frame title, the ▸ card, 送出全部（N 張）, and that the prompt got no stray characters.
# Data is all fake (gen-data.py + bin/fleet); HOME is a sandbox that links only ~/.claude and ~/.claude.json so the session can log in.
# The submit step uses the store-only test card d-keys0004, never a real card. Exit 0 = every step passed; 1 = a step failed; 2 = setup.
import argparse, json, os, re, shutil, subprocess, sys, tempfile, time, unicodedata, uuid

HERE = os.path.dirname(os.path.realpath(__file__))
TITLES = ['現況', '待決', '派工', '審查', '花費', '殘留', '艦隊', '待辦', '說明']

# ---- v7.5 (band + right Pane, one shared state) : every string the gate matches lives here, so a wording change is one edit.
# Calibrated on autopilot b52aaa491 (mods/live/locales/zh-TW.json v75.*, v75.ts, band75.tsx). Structure first: which region carries the
# marker, which region's colour changed; the strings below only name things.
V75_OPEN = '/live-pane'                      # typed + Enter to open the band/Pane, as the operator does (the Pane opens with the keyboard on it)
V75_FOCUS_KEY = ('C-x', 'Tab')               # input -> band (host key); from the Pane it goes to the input
BAND_HEAD_RE = r'[↑▲]\s*(\d+)/(\d+)\s+(\S.*?)(?:\s{2,}|$)'  # first band row: "↑ 1/3 <card title>   [-]" (position k of n, then the title)
CURSOR_RE = r'▸\s+(\d+)\.'                   # the cursor card's row in the Pane: "▸  1. <title>"
MARKER_RE = r'^[▸ ]\s*(\d+)\.'                # a card-number marker at the start of a Pane row (after its left border)
PICK_BAND_RE = r'→\s*%s'                      # band option row, picked: "... b 再審一輪  → b"
PICK_PANE_RE = r'(?m)^\s*%s: .*→\s*$'         # Pane option row, picked: "b: 再審一輪 →"
PAGE_PENDING, PAGE_DISPATCH, PAGE_NOW = '待決', '派工', '現況'  # Pane titles for pages 2, 3, 1
PREVIEW_MARK, STALE_MARK, SENT_MARK = '將送出', '已變更，請重新預覽', '[待決回覆]'
HINT_INPUT = 'ctrl+x tab 拿鍵盤'              # band hint line while the input has the keyboard (v75.hint.prompt)
HINT_BAND = 'a/b/c 選 · n 附註'               # ... while the band has it (v75.hint.band starts with this)
HINT_TO_PANE = '進 Pane'                      # the band hint row after a digit: the keyboard stayed in the band (ctrl+x tab 進 Pane)
HINT_PANE = '鍵盤在 Pane'                     # ... while the Pane has it (v75.hint.pane)
PANE_FOOT_RE = r'╰─'                          # the Pane's own hint row (above its page strip)
LONG_CARD = 3                                # the fixture card with the long 背景 (scrolls the Pane)
CLICK_MARK_COLS = 8                          # a marker's digit sits within this many columns right of the Pane's left border
FOLD_OPEN = ('▾', '▼')                        # mod toggle glyph while the Pane is open ("▾ 收起" ends the quick row)
FOLD_TOGGLE_OPEN, FOLD_TOGGLE_SHUT = '收起', '展開'  # v75.toggle_close / v75.toggle_open words
FOLD_HOST = ('[-]', '[−]')                    # the host's own collapse button, end of the band's first row
FOLD_SHUT = ('▸', '▶', '[+]')                 # the folded row's leading glyph
FOLD_KEYS = ('C-x', 'C-a')                    # the host's keyboard fold (claude-code.d.ts ~:12723: "ctrl+x ctrl+a, [-]")
RELOAD_CMD = '/reload-plugins'
COLLAPSED_ROW = 'plugin panel hidden'         # the host's folded band row: "▸ plugin panel hidden · ctrl+x ctrl+a or click to show"
MIN_DOCK_COLS = 110                          # Pane docks beside the chat from this width in full screen


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

    def step(self, name, send, checks, wait=6.0, soft=None):
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
        if getattr(self, 'work', None):  # v7.5: the host's band-geometry.log as it stood at this shot
            for root, _, files in os.walk(self.work):
                if any(l + '.log' in files for l in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload')):
                    for lg in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload'):
                        if lg + '.log' in files:
                            shutil.copy(os.path.join(root, lg + '.log'), os.path.join(self.out, tag + '.' + lg + '.log'))
                    break
        self.last_ok = ok
        self.fails += 0 if ok or soft else 1
        seen = dict(title=sc.title(), prompt=sc.prompt(), cursor=sc.cursor(), send_n=sc.sendn(), decide_n=sc.decide_count(), band=sc.band())
        self.results.write(json.dumps(dict(step=tag, ok=ok, checks=[dict(check=l, ok=o) for l, o in res], seen=seen), ensure_ascii=False) + '\n')
        self.results.flush()
        mark = 'PASS' if ok else 'KNOWN-LIMIT' if soft else 'FAIL'
        if soft and not ok:
            self.results.write(json.dumps(dict(step=tag, known_limit=soft), ensure_ascii=False) + '\n')
        print('%s %s' % (mark, tag))
        for lab, o in res:
            if not o:
                print('      ✗ %s' % lab)
        if not ok:
            print('      seen: %s' % json.dumps(seen, ensure_ascii=False))
            if soft:
                print('      known limit: %s' % soft)
            self.recover()
        return sc

    def recover(self):
        """After a failed step: close a dialog the stray keys opened and clear stray prompt text, so one failure does not fail every later step."""
        for _ in range(3):
            sc = self.snap()
            if sc.has('Nothing to rewind') or sc.has('Rewind'):
                self.keys('Escape')  # the host's Rewind dialog (a double Esc opens it)
            elif sc.title() is None and sc.prompt() not in (None, '') and re.match(r'\d+\. ', sc.prompt()):
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


def launch(g, ap, out, scenario, data, home, cwd, env, sid, alias, cols=209, rows=49, extra_env=()):
    """Open the real `claude --plugin-dir` in the gate's tmux session and get past the trust prompt to the input box."""
    senv = ['env', 'HOME=' + home, 'PATH=' + os.path.join(HERE, 'bin') + ':' + os.environ['PATH'], 'V74_FIX=' + env['V74_FIX'],
            'V74_KEYS_LOG=' + env['V74_KEYS_LOG'], 'AUTOPILOT_LIVE_DIR=' + os.path.join(data, 'live'), 'CODEFORGE_DATA_DIR=' + os.path.join(data, 'cfdata'),
            'AUTOPILOT_CARD_MIRROR_DIR=' + os.path.join(data, 'mirror'), 'AUTOPILOT_ROOMS_DIR=' + os.path.join(data, 'rooms'),
            'AUTOPILOT_V74_KEYLOG=' + os.path.join(os.path.abspath(out), 'keylog-%s.jsonl' % scenario),
            *extra_env, 'AUTOPILOT_LOCALE=zh-TW', 'LANG=zh_TW.UTF-8', 'claude', '--session-id', sid, '--plugin-dir', ap, '--model', 'haiku']
    cmd = ' '.join("'%s'" % a.replace("'", "'\\''") for a in senv)
    if alias is not None:
        # --token-alias: the session logs in with a pool's existing token instead of the default login (a pool at its
        # weekly limit never answers [待決回覆]). The file is read inside the tmux shell, so the value is in the
        # session's environment only: never in argv, the gate's output or a log. No token is copied, no login made.
        tok = os.path.join(os.path.expanduser('~'), '.config/autopilot/accounts', alias, 'token')
        cmd = 'unset CLAUDE_CONFIG_DIR; CLAUDE_CODE_OAUTH_TOKEN="$(cat %s)" exec %s' % ("'%s'" % tok.replace("'", "'\\''"), cmd)
        cmd = "bash -c '%s'" % cmd.replace("'", "'\\''")
    g.tmux('new-session', '-d', '-s', g.sess, '-x', str(cols), '-y', str(rows), '-c', cwd, cmd)
    t0 = time.time()
    while time.time() - t0 < 40:  # trust prompt, then the input box
        time.sleep(1)
        sc = g.snap()
        if 'trust this folder' in sc.text.lower():
            g.keys('Down', 'Enter')
        elif sc.prompt() is not None and time.time() - t0 > 12:
            break


def run(ap, out, scenario, alias=None, hold=False):
    work, data, home, cwd, env = setup(ap, out, scenario)
    sid = str(uuid.uuid4())
    gen = [sys.executable, '-I', os.path.join(HERE, 'gen-data.py'), data, sid, cwd] + (['noproject'] if scenario == 'noproject' else [])
    subprocess.run(gen, check=True)
    regen = subprocess.Popen(['bash', '-c', 'while sleep 25; do "$@"; done', 'regen'] + gen)
    g = Gate(ap, out, scenario)
    store = os.path.join(cwd, '.git/autopilot/decisions')
    try:
        launch(g, ap, out, scenario, data, home, cwd, env, sid, alias)
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
        if hold:
            return hold_session(g, store, env['V74_KEYS_LOG'])
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
        g.step('send-preview', lambda: g.keys('s', literal=True), [  # r10: the first s previews, the second sends (opus r8-4 Major)
            ('preview 將送出 shown', lambda s: s.has('將送出')),
            ('nothing submitted yet', lambda s: not s.has('[待決回覆]')),
            ('preview names y as the confirm key', lambda s: s.has('y 確認送出')),
        ])
        g.step('send', lambda: g.keys('y', literal=True), [  # r11 (PM 01M7YDY4): y confirms the preview; a second s only redraws it
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
        fold_steps(g, os.path.abspath(out), 'normal', 'v74')  # PM 01M7YWXW: the band's fold button, before and after /reload-plugins (ordinary screen: known limit if no click arrives)
        return g.fails
    finally:
        regen.kill()
        g.tmux('kill-server', check=False)
        # keep what the host gave the band (maxRows x bodyColumns per change) next to the shots: the band-vs-Pane call
        # (PM 01M7Y9150) needs the measured rows, not the d.ts reading
        for root, _, files in os.walk(work):
            if any(l + '.log' in files for l in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload')):
                for lg in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload'):
                    if lg + '.log' in files:
                        shutil.copy(os.path.join(root, lg + '.log'), os.path.join(out, lg + '-%s.log' % scenario))
                break
        shutil.rmtree(work, ignore_errors=True)


# ---------------------------------------------------------------------------------------------------------------------
# v7.5 : band (bottom-left) + Pane (docked right, or a full-width box above the band on an ordinary screen), one shared state.
# Region parsing is by structure (the Pane's left border column, the band head row), never by page wording.

def col_of(line, idx):
    return dw(line[:idx])


def slice_cols(line, c0, c1=None):
    """The part of a line between display columns c0 and c1 (wide characters count 2)."""
    out, c = '', 0
    for ch in line:
        w = dw(ch)
        if c >= c0 and (c1 is None or c + w <= c1):
            out += ch
        c += w
    return out


def sgr_col(ansi_line, col):
    """The style in force at display column col of an ANSI line."""
    st, c, pos = {}, 0, 0
    for m in SGR.finditer(ansi_line + '\x1b[m'):
        for ch in ESC.sub('', ansi_line[pos:m.start()]):
            if c >= col:
                return dict(st)
            c += dw(ch)
        sgr_apply(st, m.group(1))
        pos = m.end()
    return dict(st)


def sig(st):
    return tuple(sorted((k, str(v)) for k, v in (st or {}).items()))


class V75:
    """Read-only structural view of one captured screen: where the Pane is, where the band is, and their colours."""

    def __init__(self, sc):
        self.sc = sc
        counts = {}
        for i, l in enumerate(sc.lines):
            for m in re.finditer('│', l):
                counts.setdefault(col_of(l, m.start()), []).append(i)
        best = max(((len(v), c) for c, v in counts.items() if c > 0), default=(0, 0))
        self.pane_row = self.pane_bottom = None
        self.pane_col = 0
        self.layout = 'none'
        inline_top = next((i for i, l in enumerate(sc.lines) if l.startswith('╭─')), None)
        if inline_top is None and best[0] >= 6:  # a left border down the screen: docked beside the chat
            self.pane_col, self.layout = best[1], 'docked'
            self.pane_row = counts[best[1]][0]
        else:             # an ordinary screen: a ╭─…─╮ box above the band
            top = inline_top
            if top is not None:
                self.pane_row, self.layout = top, 'inline'
                self.pane_bottom = next((i for i in range(top + 1, len(sc.lines)) if sc.lines[i].startswith('╰─')), len(sc.lines))
        self.band_row = self.band_k = self.band_title = None
        for i, l in enumerate(sc.lines):
            if self.layout == 'inline' and self.pane_bottom is not None and i <= self.pane_bottom:
                continue
            m = re.search(BAND_HEAD_RE, self.left(l))
            if m and self.left(l).lstrip().startswith(('↑', '▲')):
                self.band_row, self.band_k, self.band_title = i, int(m.group(1)), m.group(3).strip()
                self.band_col = col_of(l, l.index(m.group(0)[0]))
                break

    def left(self, line):  # the part of a row that is not the docked Pane
        return slice_cols(line, 0, self.pane_col) if self.layout == 'docked' else line

    def both(self):  # a Pane and a v7.5 band head on the same screen (what the single-band fallback must not show)
        return self.layout != 'none' and self.band_row is not None

    def prompt_row(self):
        return next((i for i in range(len(self.sc.lines) - 1, -1, -1) if self.left(self.sc.lines[i]).strip().startswith('❯')), None)

    def band_lines(self):
        if self.band_row is None:
            return []
        end = self.prompt_row()
        end = end - 1 if end is not None and end > self.band_row else self.band_row + 5
        return [self.left(l).rstrip() for l in self.sc.lines[self.band_row:end]]

    def band_text(self):
        return '\n'.join(self.band_lines())

    def band_hint(self):  # the band line above the quick row: the keys that work for the current focus
        bl = self.band_lines()
        q = next((i for i, l in enumerate(bl) if l.lstrip().startswith('▲') and '│' in l), None)
        return bl[q - 1].strip() if q else None

    def pane_lines(self):
        """The Pane's inner rows: docked = right of the border, inline = inside the box; the ✕ close-button row is dropped."""
        if self.layout == 'docked':
            rows = [slice_cols(l, self.pane_col + 1).rstrip() for l in self.sc.lines[self.pane_row:]]
        elif self.layout == 'inline':
            rows = [re.sub(r'^│ ?', '', l.rstrip()).rstrip('│ ').rstrip() for l in self.sc.lines[self.pane_row + 1:self.pane_bottom]]
        else:
            return []
        return [r_ for r_ in rows if r_.strip() != '✕']

    def pane_text(self):
        return '\n'.join(self.pane_lines())

    def pane_title(self):
        for l in self.pane_lines():
            if l.strip():
                return re.split(r'\s{2,}| · ', l.strip())[0]
        return None

    def pane_split(self):
        """(head, body, foot): the title row, the scrolling middle, and the foot from the Pane's own hint row down."""
        pl = self.pane_lines()
        foot = next((i for i, l in enumerate(pl) if re.search(PANE_FOOT_RE, l)), len(pl))
        return pl[:1], pl[1:foot], pl[foot:]

    def pane_head(self):
        return self.pane_split()[0]

    def pane_body(self):
        return self.pane_split()[1]

    def pane_foot(self):
        return self.pane_split()[2]

    def cursor_no(self):
        for l in self.pane_lines():
            m = re.search(CURSOR_RE, l)
            if m:
                return int(m.group(1))
        return None

    def band_style(self):
        if self.band_row is None or self.sc.ansi is None:
            return None
        return sig(sgr_col(self.sc.ansi[self.band_row], self.band_col))

    def pane_has_keys(self):
        """True when the Pane's own foot hint says the keyboard is on it (鍵盤在 pane), False when it offers ctrl+x tab 拿鍵盤, None if neither shows."""
        foot = ' '.join(self.pane_foot()[:2]).lower()
        if '鍵盤在 pane' in foot:
            return True
        if 'ctrl+x tab 拿鍵盤' in foot:
            return False
        return None

    def band_rev(self):
        """True if any cell of the band rows (left of a docked Pane) is drawn in reverse video: v7.5 shows no focus highlight on the band."""
        if self.band_row is None or self.sc.ansi is None:
            return False
        end = self.prompt_row()
        end = end if end is not None and end > self.band_row else self.band_row + 5
        width = self.pane_col if self.layout == 'docked' else 400
        for i in range(self.band_row, end):
            line = self.sc.ansi[i]
            for c in range(min(width, dw(self.sc.lines[i]))):
                if 'rev' in dict(sgr_col(line, c)):
                    return True
        return False

    def pane_style(self):  # the colour of the Pane's left border (docked, one body row down) or its box corner (inline)
        if self.layout == 'none' or self.sc.ansi is None:
            return None
        return sig(sgr_col(self.sc.ansi[self.pane_row + (2 if self.layout == 'docked' else 0)], self.pane_col))

    def picked(self, letter, region):
        if region == 'band':
            return re.search(PICK_BAND_RE % letter, self.band_text()) is not None
        return re.search(PICK_PANE_RE % letter, self.pane_text()) is not None

    def markers(self):
        """(screen_row, display_col, number, row_has_cursor) of each card-number marker just inside the Pane's left border."""
        out = []
        if self.layout == 'none':
            return out
        base = self.pane_col + 1 if self.layout == 'docked' else 1
        end = len(self.sc.lines) if self.layout == 'docked' else self.pane_bottom
        for i in range(self.pane_row + 1, end):
            seg = slice_cols(self.sc.lines[i], base, base + CLICK_MARK_COLS)
            m = re.match(MARKER_RE, seg)
            if m:
                out.append((i, base + col_of(seg, m.start(1)), int(m.group(1)), seg.startswith('▸')))
        return out

    def fold(self):
        """What the fold controls show: host [-] on the band head row, the folded row, and the mod's ▾ 收起 / ▸ 展開 on the quick row.
        {'host': (row, col) | None, 'folded': (row, col) | None, 'mod': (row, col, 'open'|'shut') | None}"""
        f = dict(host=None, folded=None, mod=None)
        for i, l in enumerate(self.sc.lines):
            t = self.left(l).rstrip()
            if COLLAPSED_ROW in t and t.lstrip().startswith(FOLD_SHUT):
                f['folded'] = (i, dw(t[:len(t) - len(t.lstrip())]))
            for b in FOLD_HOST:
                if (self.band_row == i or '待決' in t or '▲' in t) and t.endswith(b):
                    f['host'] = (i, dw(t) - dw(b))
            for g_ in FOLD_OPEN:
                if t.endswith(g_ + ' ' + FOLD_TOGGLE_OPEN) and '待決' in t:
                    f['mod'] = (i, dw(t) - dw(g_ + ' ' + FOLD_TOGGLE_OPEN), 'open')
            if t.endswith(' ' + FOLD_TOGGLE_SHUT) and '待決' in t:
                glyph = t.split()[-2]
                f['mod'] = (i, dw(t) - dw(glyph + ' ' + FOLD_TOGGLE_SHUT), 'shut')
        return f


def v75(sc):
    return V75(sc)


def sgr_click(g, x, y):
    """One left click as the SGR mouse report the host reads: press then release, 1-based cell."""
    for tail in ('M', 'm'):
        g.tmux('send-keys', '-t', g.sess, '-l', '\x1b[<0;%d;%d%s' % (x, y, tail))
        time.sleep(0.1)


def keylog_count(out, scenario, pat='press'):
    try:
        return sum(1 for l in open(os.path.join(out, 'keylog-%s.jsonl' % scenario), encoding='utf-8') if pat in l)
    except OSError:
        return 0


def fold_cycle(g, out, tag, soft_msg):
    """Click each fold control with SGR and read the result off the screen (PM 01M7YWXW): the host [-] folds the band to one row, the folded
    row's ▸ opens it, the mod's ▾ 收起 closes the Pane and its ▸ 展開 opens it. A host click that does nothing is retried as the host's
    keyboard fold (ctrl+x ctrl+a) and recorded as a known limit. Run again after /reload-plugins."""
    memo = {}

    def click(pick):
        def f():
            fo = v75(g.snap()).fold()
            memo['pos'] = pick(fo)
            if memo['pos']:
                sgr_click(g, memo['pos'][1] + 1, memo['pos'][0] + 1)
        return f
    plan = (
        ('host-fold', lambda fo: fo['host'], lambda fo: fo['folded'] is not None, 'band folded to one row', True),
        ('host-unfold', lambda fo: fo['folded'], lambda fo: fo['folded'] is None and fo['host'] is not None, 'band back out of the folded row', True),
        ('mod-close', lambda fo: fo['mod'] if fo['mod'] and fo['mod'][2] == 'open' else None, lambda fo: bool(fo['mod']) and fo['mod'][2] == 'shut', 'toggle now offers 展開', False),
        ('mod-open', lambda fo: fo['mod'] if fo['mod'] and fo['mod'][2] == 'shut' else None, lambda fo: bool(fo['mod']) and fo['mod'][2] == 'open', 'toggle offers 收起 again', False),
    )
    for name, pick, want, what, key_fallback in plan:
        g.step('%s-click-%s' % (tag, name), click(pick), [
            ('the control is on screen', lambda s, pick=pick: pick(v75(s).fold()) is not None or memo.get('pos') is not None),
            (what, lambda s, want=want: want(v75(s).fold())),
        ], wait=5, soft=soft_msg)
        print('      %s control at (row, col)=%s' % (name, memo.get('pos')))
        if not g.last_ok and key_fallback:
            g.step('%s-key-%s' % (tag, name), lambda: (g.keys(FOLD_KEYS[0]), g.keys(FOLD_KEYS[1])), [
                ('keyboard fold ctrl+x ctrl+a: ' + what, lambda s, want=want: want(v75(s).fold())),
            ], wait=5, soft='keyboard fold ctrl+x ctrl+a did not change the band either')


def fold_steps(g, out, mode, tag):
    soft = None if mode == 'fs' else 'ordinary screen: clicks may not arrive (claude-code.d.ts :6786, :9906 say full screen only); the v74 session on the ordinary screen did get them'
    if 'Rewind' in g.snap().text:  # a stray Esc Esc opened the rewind dialog
        g.keys('Escape')
    g.keys('C-u')
    fold_cycle(g, out, tag, soft)
    g.step('%s-reload' % tag, lambda: (g.keys(RELOAD_CMD, literal=True), g.keys('Enter')), [('prompt empty after /reload-plugins', lambda s: s.prompt() == '')], wait=15)
    time.sleep(3)
    if getattr(g, 'work', None):  # v7.5: what the reopen did (band-reload.log): retried and placed, or gave up
        rows = []
        for root, _, files in os.walk(g.work):
            if 'band-reload.log' in files:
                with open(os.path.join(root, 'band-reload.log'), encoding='utf-8') as f:
                    rows = [json.loads(l) for l in f if l.strip()]
                break
        opens = [r for r in rows if r.get('step') in ('open', 'retry', 'gave-up')]
        print('      band-reload.log: ' + ' | '.join(json.dumps({k: v for k, v in r.items() if k != 'ts'}, ensure_ascii=False) for r in rows[-6:]))
        g.step('%s-reload-log' % tag, lambda: None, [
            ('reopen after reload placed the Pane (a retry may be needed, gave-up is a fail)', lambda s: bool(opens) and opens[-1].get('step') != 'gave-up' and (opens[-1].get('result') or {}).get('isPlaced', True) is True),
        ], wait=1)
    fold_cycle(g, out, tag + '-after-reload', soft)
    fo = v75(g.snap()).fold()  # leave the band expanded for whatever runs next
    if fo['folded'] is not None:
        g.keys(*FOLD_KEYS)


def run_v75_steps(g, mode, out):
    """The v7.5 key steps on an open sandbox session. mode 'fs' = full screen (Pane docks, mouse reports), 'normal' = ordinary screen."""
    memo = {}
    focus = lambda: g.keys(*V75_FOCUS_KEY)
    # v7.5 focus rules (host limits, d.ts :8724-8731, :2913-2927): the band draws no focus colour and keeps one fixed hint row; the mod cannot hand the
    # keyboard from the band to the Pane. Only the Pane shows its focus (isFocused): frame colour and its own foot hint.
    # measured on 5e3dfcc6c (cyc2-fs): ctrl+x tab 1 -> band, presses 2-4 change nothing, press 5 -> Pane (frame fg 153); the Pane foot hint stays 'ctrl+x tab 拿鍵盤'
    pane_focused = lambda s: v75(s).pane_style() != memo['pane']
    pane_unfocused = lambda s: v75(s).pane_style() == memo['pane']
    no_band_rev = lambda s: not v75(s).band_rev()
    to_band_hint = lambda s: HINT_TO_PANE in (v75(s).band_hint() or '')

    def to_input():  # one Esc (two in a row open the host's Rewind dialog), then probe with z: the prompt must take it
        g.keys('Escape')
        g.keys('z', literal=True)
        if g.snap().prompt() != 'z':
            g.keys('Escape')
            time.sleep(1.5)
        g.keys('C-u')

    def press_to_pane():  # ctrl+x tab until the Pane frame shows focus (at most 10: anchor, 7 band buttons, pane-toggle, then the Pane on e8d47d478)
        for _ in range(10):
            if pane_focused(g.snap()):
                return
            focus()
            time.sleep(1.5)

    def to_pane():
        if not pane_focused(g.snap()):
            to_input()
            press_to_pane()

    g.step('open', lambda: (g.keys(V75_OPEN, literal=True), g.keys('Enter')), [
        ('a Pane and the band head are both on screen', lambda s: v75(s).both()),
        ('layout %s' % ('docked beside the chat' if mode == 'fs' else 'is docked or inline'), lambda s: v75(s).layout == 'docked' if mode == 'fs' else v75(s).layout in ('docked', 'inline')),
        ('the Pane foot hint is on screen', lambda s: v75(s).pane_has_keys() is not None),
        ('prompt empty', lambda s: s.prompt() == ''),
    ], wait=20)
    memo['pane_open'] = v75(g.snap()).pane_style()
    g.step('esc-from-pane', lambda: g.keys('Escape'), [
        ('Pane foot hint switched to ctrl+x tab 拿鍵盤 (isFocused false)', lambda s: v75(s).pane_has_keys() is False),
    ])
    memo['pane'] = v75(g.snap()).pane_style()  # the Pane-not-focused frame colour
    g.step('esc-types', lambda: g.keys('z', literal=True), [
        ('prompt is z (the input has the keyboard)', lambda s: s.prompt() == 'z'),
        ('no reverse video on the band', no_band_rev),
    ])
    g.step('clear-1', lambda: g.keys('C-u'), [('prompt empty', lambda s: s.prompt() == '')])
    g.step('band-focus', focus, [
        ('Pane frame still unfocused', pane_unfocused),
        ('the band hint row is there (one fixed row)', lambda s: v75(s).band_hint() is not None),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.step('band-pick-b', lambda: g.keys('b', literal=True), [
        ('band shows →b', lambda s: v75(s).picked('b', 'band')),
        ('prompt empty (b did not go to the prompt)', lambda s: s.prompt() == ''),
    ])
    g.step('band-digit-2', lambda: g.keys('2', literal=True), [
        ('Pane is on 待決', lambda s: v75(s).pane_title() == PAGE_PENDING),
        ('Pane shows →b on option b (set from the band)', lambda s: v75(s).picked('b', 'pane')),
        ('the keyboard stays in the band: Pane frame still unfocused', pane_unfocused),
        ('band hint row offers ctrl+x tab 進 Pane', to_band_hint),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    no_pane = None if mode == 'fs' else 'ordinary screen: ctrl+x tab never reaches the Pane (m75c measured 10 stops), host limit'
    memo['j0'] = v75(g.snap()).cursor_no()
    g.step('band-j', lambda: g.keys('j', literal=True), [  # j/k change card from the band (e6d47b40c)
        ('Pane cursor 1 -> 2', lambda s: memo['j0'] == 1 and v75(s).cursor_no() == 2),
        ('band k/n follows', lambda s: v75(s).band_k == 2),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.step('band-k', lambda: g.keys('k', literal=True), [('Pane cursor back to 1', lambda s: v75(s).cursor_no() == 1)])
    g.step('pane-take-keys', press_to_pane, [
        ('Pane frame focused after ctrl+x tab presses (count recorded in the log)', pane_focused),
        ('prompt empty', lambda s: s.prompt() == ''),
    ], soft=no_pane)
    g.step('pane-pick-a', lambda: g.keys('a', literal=True), [
        ('Pane shows →a', lambda s: v75(s).picked('a', 'pane')),
        ('band shows →a (set from the Pane)', lambda s: v75(s).picked('a', 'band')),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.step('pane-j', lambda: g.keys('j', literal=True), [  # j/k with the keyboard in the Pane
        ('Pane cursor 1 -> 2', lambda s: v75(s).cursor_no() == 2),
        ('band k/n follows', lambda s: v75(s).band_k == 2),
    ], soft=no_pane)
    g.step('pane-k', lambda: g.keys('k', literal=True), [('Pane cursor back to 1', lambda s: v75(s).cursor_no() == 1)], soft=no_pane)
    memo['cur'] = v75(g.snap()).cursor_no()
    g.step('pane-down', lambda: g.keys('Down'), [
        ('Pane cursor moved to another card', lambda s: v75(s).cursor_no() not in (None, memo['cur'])),
        ('band follows: its k/n is the Pane cursor card', lambda s: v75(s).band_k is not None and v75(s).band_k == v75(s).cursor_no()),
    ], soft=no_pane)
    g.step('esc-pane', lambda: g.keys('Escape'), [
        ('Pane frame and foot hint back to unfocused', pane_unfocused),
    ])
    g.step('esc-pane-types', lambda: g.keys('z', literal=True), [
        ('prompt is z (the input has the keyboard)', lambda s: s.prompt() == 'z'),
        ('no reverse video on the band', no_band_rev),
    ])
    g.step('clear-2', lambda: g.keys('C-u'), [('prompt empty', lambda s: s.prompt() == '')])
    g.step('band-digit-3', lambda: (focus(), g.keys('3', literal=True)), [
        ('Pane jumped to 派工', lambda s: v75(s).pane_title() == PAGE_DISPATCH),
        ('the keyboard stays in the band: Pane frame unfocused', pane_unfocused),
        ('band hint row offers ctrl+x tab 進 Pane', to_band_hint),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.step('esc-band', lambda: g.keys('Escape'), [('Pane frame unfocused', pane_unfocused)])
    g.step('esc-band-types', lambda: g.keys('z', literal=True), [
        ('prompt is z (Esc in the band gave the keyboard to the input)', lambda s: s.prompt() == 'z'),
        ('no reverse video on the band', no_band_rev),
    ])
    g.step('clear-3', lambda: g.keys('C-u'), [('prompt empty', lambda s: s.prompt() == '')])
    # preview: Esc cancels only the preview, pressed where the keyboard is (the band); a change after it opens makes y refuse
    g.step('preview-open', lambda: (focus(), g.keys('2', literal=True), g.keys('a', literal=True), g.keys('s', literal=True)), [
        ('preview shown', lambda s: s.has(PREVIEW_MARK)),
        ('band hint offers x 取消預覽 while the preview is open', lambda s: 'x 取消預覽' in v75(s).band_text()),
        ('nothing sent', lambda s: not s.has(SENT_MARK)),
    ])
    g.step('preview-x', lambda: g.keys('x', literal=True), [  # the band hotkey takes one digit or one lowercase letter (d.ts:1391) and never sees Esc: x cancels the preview
        ('preview gone', lambda s: not s.has(PREVIEW_MARK)),
        ('only the preview closed: Pane and band still up', lambda s: v75(s).both()),
        ('nothing sent', lambda s: not s.has(SENT_MARK)),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.step('preview-reopen', lambda: g.keys('s', literal=True), [('preview shown again', lambda s: s.has(PREVIEW_MARK)), ('prompt empty', lambda s: s.prompt() == '')])
    g.step('preview-stale-y', lambda: (g.keys('b', literal=True), g.keys('y', literal=True)), [
        ('screen says the preview is stale', lambda s: s.has(STALE_MARK)),
        ('y did not send', lambda s: not s.has(SENT_MARK)),
        ('prompt empty', lambda s: s.prompt() == ''),
    ])
    g.keys('Escape')
    to_pane()
    # scrolling: PgUp/PgDn move only the Pane's middle section
    def to_long():
        for _ in range(6):
            if v75(g.snap()).cursor_no() == LONG_CARD:
                break
            g.keys('Down')
    g.step('to-long-card', to_long, [('cursor on the long card', lambda s: v75(s).cursor_no() == LONG_CARD)], soft=no_pane)
    memo['pre'] = v75(g.snap())
    more = re.search(r'還有\s*(\d+)\s*列', ' '.join(memo['pre'].pane_foot()) + ' ' + ' '.join(memo['pre'].pane_body()))
    body_n, room_n = len(memo['pre'].pane_body()), len(memo['pre'].pane_lines())
    print('      pgdn precondition: body rows %d, pane rows %d, overflow %s' % (body_n, room_n, more.group(1) if more else 'none'))
    no_scroll = None if more else 'no long card found: the card fits the Pane (body %d rows, pane %d rows), a clamp at 0 is not an implementation fault' % (body_n, room_n)
    g.step('pgdn', lambda: g.keys('PageDown'), [
        ('Pane body scrolled', lambda s: v75(s).pane_body() != memo['pre'].pane_body()),
        ('Pane title row and foot did not move', lambda s: (v75(s).pane_head(), v75(s).pane_foot()) == (memo['pre'].pane_head(), memo['pre'].pane_foot())),
        ('band did not move at all', lambda s: v75(s).band_lines() == memo['pre'].band_lines()),
        ('prompt empty', lambda s: s.prompt() == ''),
    ], soft=no_pane or no_scroll)
    g.step('pgup', lambda: g.keys('PageUp'), [
        ('Pane body back where it was', lambda s: v75(s).pane_body() == memo['pre'].pane_body()),
        ('band did not move', lambda s: v75(s).band_lines() == memo['pre'].band_lines()),
    ])
    # mouse: SGR click on a card-number marker on the Pane's left border. fs = must work; normal = known limit if the host sends no reports
    def click():
        v = v75(g.snap())
        mk = [m for m in v.markers() if not m[3]]
        memo['click_cur'] = v.cursor_no()
        memo['marker'] = mk[0] if mk else None
        memo['press0'] = keylog_count(out, g.scenario)
        if mk:
            sgr_click(g, mk[0][1] + 1, mk[0][0] + 1)
    soft = None if mode == 'fs' else 'ordinary screen: the host reports clicks only in full screen (claude-code.d.ts :6786, :9906); the help page says so (v75.help.click)'
    g.step('mouse-click-marker', click, [
        ('a card-number marker exists on the Pane left border', lambda s: memo.get('marker') is not None),
        ('cursor jumped to the clicked card', lambda s: memo.get('marker') is not None and v75(s).cursor_no() == memo['marker'][2]),
        ('band follows the click', lambda s: v75(s).band_k == v75(s).cursor_no()),
    ], wait=6, soft=soft)
    probe = dict(mode=mode, marker=memo.get('marker'), cursor_before=memo.get('click_cur'), cursor_after=v75(g.snap()).cursor_no(),
                 ui_press_logged=keylog_count(out, g.scenario) > memo.get('press0', 0))
    with open(os.path.join(out, 'mouse-probe-%s.json' % mode), 'w', encoding='utf-8') as f:
        json.dump(probe, f, ensure_ascii=False)
    print('MOUSE-PROBE %s' % json.dumps(probe, ensure_ascii=False))
    to_input()
    # spend page (5): capacity's pool forecast from AUTOPILOT_POOL_ETA (a fixture with made-up pool names): % / burn / reset / exhaust columns; an old generated_at marks the page 過期
    g.step('spend-fresh', lambda: (to_input(), focus(), g.keys('1', literal=True), g.keys('5', literal=True)), [
        ('Pane shows the spend page with the fixture pools', lambda s: 'pool-alpha' in v75(s).pane_text() and 'pool-beta' in v75(s).pane_text()),
        ('percent column', lambda s: re.search(r'pool-alpha.*\b54%', v75(s).pane_text()) is not None),
        ('burn column (%/h)', lambda s: re.search(r'pool-alpha.*\d+(\.\d+)?%/h', v75(s).pane_text()) is not None),
        ('reset column (重置)', lambda s: re.search(r'pool-alpha.*重置', v75(s).pane_text()) is not None),
        ('exhaust column (用完)', lambda s: re.search(r'pool-alpha.*用完', v75(s).pane_text()) is not None),
        ('a pool that never exhausts says so', lambda s: re.search(r'pool-beta.*重置前不會用完', v75(s).pane_text()) is not None),
        ('a fresh feed is not marked 過期', lambda s: '過期' not in v75(s).pane_text()),
    ], wait=10)
    write_eta(g.eta, 3 * 3600)
    g.step('spend-stale', lambda: (g.keys('1', literal=True), g.keys('5', literal=True)), [
        ('the page says the pool forecast is stale (池預估已過期)', lambda s: '池預估已過期' in v75(s).pane_text()),
        ('each pool row carries 過期', lambda s: len(re.findall('過期', v75(s).pane_text())) >= 3),
    ], wait=10)
    write_eta(g.eta, 0)
    to_input()
    if mode == 'fs':  # sizes (full screen session): 110 docks, 109 does not, 30 rows falls back to the single band
        def resize(c, r):
            return lambda: g.tmux('resize-window', '-t', g.sess, '-x', str(c), '-y', str(r))
        g.step('w110', resize(110, 49), [
            ('Pane docked beside the chat at 110 columns', lambda s: v75(s).layout == 'docked'),
            ('band present', lambda s: v75(s).band_row is not None),
        ], wait=10)
        g.step('w109', resize(109, 49), [
            ('Pane not docked at 109 columns', lambda s: v75(s).layout != 'docked'),
            ('band present', lambda s: v75(s).band_row is not None or s.band() is not None),
        ], wait=10)
        g.step('h30', resize(209, 30), [
            ('no Pane next to a band head: single band fallback', lambda s: not v75(s).both()),
            ('a band is still drawn', lambda s: v75(s).band_row is not None or s.band() is not None),
            ('prompt still on screen', lambda s: s.prompt() is not None),
        ], wait=10)
        g.step('h30-focus-not-stranded', lambda: (focus(), g.keys('3', literal=True)), [
            ('the digit reached a visible page or the band, not a hidden Pane', lambda s: s.prompt() == '' and (s.title() is not None or v75(s).band_row is not None or s.band() is not None)),
        ])
        to_input()
        g.tmux('resize-window', '-t', g.sess, '-x', '209', '-y', '49')
        time.sleep(3)
    focus(); g.keys('2', literal=True); to_input(); time.sleep(1)  # back on 待決 with the band cards showing: the fold button lives on the band head
    fold_steps(g, out, mode, 'v75')
    to_input()


def write_eta(path, age_s):
    """A capacity.pool-eta/1 fixture with made-up pool names, generated age_s seconds ago (stale_after_s 1200). Times are relative to now so the rows read as live."""
    now = int(time.time() * 1000)
    h = 3600 * 1000

    def pool(name, seven, burn, resets_h, exhaust_h):
        return dict(pool=name, **{'class': 'claude'}, state='ok', seen_at_ms=now - 600000, seven_day_pct=seven, five_hour_pct=None, resets_at_ms=now + resets_h * h,
                    burn_pct_per_h=burn, burn_window_h=5.8, burn_1h=None, burn_6h=None, exhaust_at_ms=(now + exhaust_h * h) if exhaust_h else None,
                    early_by_ms=None, burn_reason=None, disposition='OK', reason_code='ahead_no_seat', disposition_source='verdicts', reset_kind='fixed',
                    reset_kind_confidence='asserted', alerts=[])
    doc = dict(schema='capacity.pool-eta/1', generated_at_ms=now - age_s * 1000, generated_at='2026-01-01T00:00:00Z', stale_after_s=1200,
               pace_generated_at_ms=now - age_s * 1000, pools=[pool('pool-alpha', 54, 1.03, 90, 40), pool('pool-beta', 68, 0.52, 60, None)])
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(doc, f)


def run_v75(ap, out, mode, alias=None):
    """One v7.5 session: mode 'fs' (CLAUDE_CODE_NO_FLICKER=1, 209x49) or 'normal' (NO_FLICKER=0, 209x49)."""
    scenario = 'v75' + mode
    work, data, home, cwd, env = setup(ap, out, scenario)
    sid = str(uuid.uuid4())
    gen = [sys.executable, '-I', os.path.join(HERE, 'gen-data.py'), data, sid, cwd]
    subprocess.run(gen, check=True)
    regen = subprocess.Popen(['bash', '-c', 'while sleep 25; do "$@"; done', 'regen'] + gen)
    g = Gate(ap, out, scenario)
    g.work = work
    g.eta = os.path.join(work, 'pool-eta.json')
    write_eta(g.eta, 0)
    try:
        launch(g, ap, out, scenario, data, home, cwd, env, sid, alias, extra_env=('CLAUDE_CODE_NO_FLICKER=%d' % (1 if mode == 'fs' else 0), 'AUTOPILOT_POOL_ETA=' + g.eta))
        time.sleep(8)
        run_v75_steps(g, mode, out)
        return g.fails
    finally:
        regen.kill()
        g.tmux('kill-server', check=False)
        for root, _, files in os.walk(work):
            if any(l + '.log' in files for l in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload')):
                for lg in ('band-geometry', 'band-focus', 'band-scroll', 'band-reload'):
                    if lg + '.log' in files:
                        shutil.copy(os.path.join(root, lg + '.log'), os.path.join(out, lg + '-%s.log' % scenario))
                break
        shutil.rmtree(work, ignore_errors=True)


def hold_session(g, store, log):
    """--hold (PM 01M7YN17): leave the opened sandbox session for a person to press by hand. Every card is a fixture
    (d-keys000N, 驗收測試卡); fleet is the gate's fake bin and the card store sits in the sandbox repo, so a y here
    answers nothing outside the sandbox. Ends when the tmux session is gone."""
    g.keys('2', literal=True)
    t = "tmux -L %s" % g.sock
    print('HOLD · sandbox session open (fixture cards only; nothing reaches a real card queue)')
    print('  watch:    %s attach -t %s     (detach: ctrl+b d)' % (t, g.sess))
    print('  press:    %s send-keys -t %s a      then  s  (preview)  then  y  (confirm)' % (t, g.sess))
    print('  capture:  %s capture-pane -p -e -J -t %s > shot.ans' % (t, g.sess))
    print('  finish:   %s kill-session -t %s' % (t, g.sess))
    sys.stdout.flush()
    while g.tmux('has-session', '-t', g.sess, check=False).returncode == 0:
        time.sleep(2)
    done = []
    try:
        for fn in os.listdir(store):
            done += [c.get('id') for c in json.load(open(os.path.join(store, fn), encoding='utf-8')).get('decisions', []) if c.get('status') != 'open']
    except OSError:
        pass
    try:
        calls = sum(1 for _ in open(log, encoding='utf-8'))
    except OSError:
        calls = 0
    print('HOLD END · sandbox store, cards no longer open: %s · fake-fleet calls: %d' % (' '.join(done) or 'none', calls))
    return 0


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
    ap.add_argument('--hold', action='store_true', help='open the project sandbox on the 待決 tab and wait for a person to press keys (no steps run)')
    ap.add_argument('--v75', action='store_true', help='run the v7.5 band+Pane steps instead of the v7.4 ones: a full-screen session (209x49, NO_FLICKER=1: docked Pane, mouse, 110/109 columns, 30 rows) and an ordinary-screen one (mouse is a known limit there)')
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
    if a.v75:
        for mode in ('fs', 'normal'):
            fails += run_v75(chk, out, mode, a.token_alias)
        print('RESULT %s · %d failed step(s) · %s' % ('GREEN' if fails == 0 else 'RED', fails, out))
        return 0 if fails == 0 else 1
    for sc in (['project'] if a.hold else [a.only] if a.only else ['project', 'noproject']):
        fails += run(chk, out, sc, a.token_alias, a.hold)
    print('RESULT %s · %d failed step(s) · %s' % ('GREEN' if fails == 0 else 'RED', fails, out))
    return 0 if fails == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
