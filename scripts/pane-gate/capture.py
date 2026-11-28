#!/usr/bin/env python3
# capture.py <ap-checkout> <out-dir> : drive the pane-gate harness (v74-keys/run.py) against an autopilot checkout and capture pane screens. Sandbox only; never touches live.
# Env knobs: MODE=fs|keys, SZ=<WxH>, CAPTURE_THEME=light|dark, V75OFF=1 (v7.4 frame), KEEP=1 (leave the tmux server + work dir behind).
# --check-env: print the sandbox environment (tmux, harness, TERM) and exit; used by toolchain.test.sh.
import importlib.util, os, subprocess, sys, time, uuid, shutil, json
HERE = os.path.dirname(os.path.abspath(__file__))
H = os.path.join(HERE, 'v74-keys')
sys.path.insert(0, H)
spec = importlib.util.spec_from_file_location('gaterun', H + '/run.py'); R = importlib.util.module_from_spec(spec); spec.loader.exec_module(R)
R.HERE = H
PATCH = os.path.join(HERE, 'patch_awaiting.py')
if sys.argv[1] == '--check-env':
    tmux = shutil.which('tmux')
    ok = bool(tmux) and os.path.isfile(os.path.join(H, 'run.py')) and os.path.isfile(PATCH)
    print('tmux', tmux or 'MISSING'); print('harness', H, os.path.isfile(os.path.join(H, 'run.py')))
    print('patch', PATCH, os.path.isfile(PATCH))
    print('TERM', os.environ.get('TERM'), 'COLORTERM', os.environ.get('COLORTERM'), 'FORCE_COLOR', os.environ.get('FORCE_COLOR'))
    sys.exit(0 if ok else 1)
ap, out = os.path.realpath(sys.argv[1]), os.path.realpath(sys.argv[2])
os.makedirs(out, exist_ok=True)
MODE = os.environ.get('MODE', 'fs'); SZ = os.environ.get('SZ', '120x22')
PATCH = os.path.join(HERE, 'patch_awaiting.py')
scenario = 'v75' + MODE
work, data, home, cwd, env = R.setup(ap, out, scenario)
sid = str(uuid.uuid4())
gen = [sys.executable, '-I', os.path.join(H, 'gen-data.py'), data, sid, cwd, 'longcard']
def regen_cmd(): return gen
subprocess.run(gen, check=True); subprocess.run([sys.executable, PATCH, cwd, '21'], check=True)
regen = subprocess.Popen(['bash', '-c', 'while sleep 25; do "$@" && python3 "%s" "%s" 21; done' % (PATCH, cwd), 'regen'] + gen)
# light 版：把 sandbox 的 .claude symlink 換成實體目錄，寫入 light 主題（PM 01M8FJQEQ0 對比裁示驗證）
cl = os.path.join(home, '.claude')
if os.path.islink(cl):
    tgt = os.readlink(cl)
    os.remove(cl); os.makedirs(cl, exist_ok=True)
    json.dump({"theme": os.environ.get("CAPTURE_THEME", "light")}, open(os.path.join(cl, 'settings.json'), 'w'))
    for d in ('projects',):
        os.symlink(os.path.join(tgt, d), os.path.join(cl, d))
g = R.Gate(ap, out, scenario); g.work = work
import atexit, signal
atexit.register(lambda: g.tmux('kill-server', check=False))  # leave no tmux server behind, even after a crash
for _s in (signal.SIGTERM, signal.SIGHUP): signal.signal(_s, lambda *_: sys.exit(1))
g.eta = os.path.join(work, 'pool-eta.json'); R.write_eta(g.eta, 0)
shots = []
def shot(name, note=''):
    sc = g.snap()
    cols = int(g.tmux('display-message', '-p', '-t', g.sess, '#{window_width}').stdout)
    rows = int(g.tmux('display-message', '-p', '-t', g.sess, '#{window_height}').stdout)
    open(os.path.join(out, name + '.txt'), 'w', encoding='utf-8').write(sc.text)
    open(os.path.join(out, name + '.ans'), 'w', encoding='utf-8').write('\n'.join(sc.ansi))
    shots.append(dict(name=name, cols=cols, rows=rows, note=note)); print('SHOT', name, cols, rows, note, flush=True)
    return sc
def resize(c, r):
    g.tmux('resize-window', '-t', g.sess, '-x', str(c), '-y', str(r)); time.sleep(3)
def k(*ks, lit=False, wait=1.2):
    g.keys(*ks, literal=lit); time.sleep(wait)
try:
    R.launch(g, ap, out, scenario, data, home, cwd, env, sid, None, extra_env=('CLAUDE_CODE_NO_FLICKER=%d' % (1 if MODE == 'fs' else 0), 'AUTOPILOT_POOL_ETA=' + g.eta, 'V74_LONGCARD=1', 'TERM=xterm-256color') + (('AUTOPILOT_LIVE_V75=0',) if os.environ.get('V75OFF') else ()))
    time.sleep(8)
    k('/live-pane', lit=True); k('Enter', wait=8)
    shot('00-open-209x49')
    k("!env | grep -E '^(TERM|COLORTERM|FORCE_COLOR)='", lit=True); k('Enter', wait=4)
    shot('env-probe')
    import re
    def cardnum():
        t = g.snap().text
        m = re.search(r'目前第 (\d+) 張', t) or re.search(r'^↑ (\d+)/', t, re.M)
        return m.group(1) if m else None
    log = []
    def note(s): log.append(s); print('NOTE', s, flush=True)
    PROBE = ('Down', 'Up') if MODE == 'fs' else ('j', 'k')
    norm = lambda: re.sub(r'· \d+ 秒', '', g.snap().text)
    def focus():
        """Press ctrl+x tab until the probe key reacts, then undo it. fs: Down scrolls/changes the Pane text; normal: band j changes the card."""
        k('Escape'); k('C-u')
        for i in range(12):
            if MODE == 'fs':
                b = cardnum(); k('n', lit=True, wait=1.2); a = cardnum()
                if a != b: k('p', lit=True, wait=1.2); return i
            else:
                b = cardnum(); k('j', wait=1.2); a = cardnum()
                if a != b: k('k', wait=1.2); return i
            k('C-u', wait=0.3); k('C-x', 'Tab', wait=1.3)
        return None
    def press(key, lit=False, wait=1.5):
        b = cardnum(); k(key, lit=lit, wait=wait); a = cardnum()
        p = g.snap().prompt()
        if p:
            note('%s: card %s -> %s, but the input box now holds %r (key went to the prompt)' % (key, b, a, p)); k('C-u'); f = focus(); note('refocus took %s ctrl+x tab presses' % f)
        else:
            note('%s: card %s -> %s, prompt empty' % (key, b, a))
    k('C-x', 'Tab'); k('2', lit=True, wait=2)
    for (c, r) in ((200, 50), (120, 40), (80, 24)):
        resize(c, r); time.sleep(4)
        shot('A-%dx%d' % (c, r), 'decide page, awaiting block, pin under test')
    open(os.path.join(out, 'notes.log'), 'w').write('\n'.join(log))

finally:
    json.dump(dict(shots=shots), open(os.path.join(out, 'shots.json'), 'w'), ensure_ascii=False, indent=1)
    if os.environ.get('KEEP') != '1':
        regen.kill(); g.tmux('kill-server', check=False); shutil.rmtree(work, ignore_errors=True)
    else:
        print('KEPT', 'tmux -L', g.sock, 'work', work, 'regen pid', regen.pid)
