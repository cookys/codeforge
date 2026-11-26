#!/usr/bin/env python3
"""第三輪設計稿產生器（單 01M888MJ3D）：band ＋ 右側 Pane 一套，以 v7.4 為底。

輸出 out/<W>x<H>/{screen.txt, dark.ans, light.ans, roles.json}。
每列是 (文字, 角色, 類別) 片段；類別 text 要對比 4.5，glyph 要 3.0。
"""
import json, os, sys, unicodedata

OUT = sys.argv[1] if len(sys.argv) > 1 else 'out'

def cw(ch):
    return 2 if unicodedata.east_asian_width(ch) in ('W', 'F') else 1

def dw(s):
    return sum(cw(c) for c in s)

def cut(s, w):
    out, n = '', 0
    for c in s:
        if n + cw(c) > w:
            break
        out += c; n += cw(c)
    return out

T = lambda s, r='text': (s, r, 'text')
G = lambda s, r='inactive': (s, r, 'glyph')

def rowlen(row):
    return sum(dw(s) for s, _, _ in row)

def pad(row, w, role='text'):
    n = rowlen(row)
    assert n <= w, (n, w, ''.join(s for s, _, _ in row))
    return row + [(' ' * (w - n), role, 'text')]

def fit(row, w):
    """截到 w 格（只截最後一段文字）。"""
    out, n = [], 0
    for s, r, k in row:
        if n + dw(s) <= w:
            out.append((s, r, k)); n += dw(s)
        else:
            rest = w - n
            if rest > 1:
                out.append((cut(s, rest - 1) + '…', r, k)); n = w
            break
    return out

# ---------- 共用假資料（同 v7.4 fixture.md） ----------
CONV = [
    [T('  > '), T('把 tux-sync 的測試補上，跑完 check-all 再合併')],
    [],
    [G('  ● ', 'text'), T('8 個反例全過；拿掉 symlink 拒絕後測試會失敗。')],
    [T('    ⎿ ', 'inactive'), T('反向探針有效', 'inactive')],
    [],
    [G('  ● ', 'text'), T('check-all 結束碼 0，已 --no-ff 合進 develop。')],
    [],
    [T('  > '), T('好，下一張')],
    [],
    [G('  ● ', 'text'), T('第 2 張：check-cjk-safe 的測試，8 個反例。')],
    [T('    ⎿ ', 'inactive'), T('補上執行權限', 'inactive')],
    [],
]
CARDS = [
    ('合併 plan', '已選 a'),
    ('外掛更新時機', '未選'),
    ('預覽先拍幾欄', '未選'),
]
FOCUS = 2
Q = '預覽先拍幾欄：預覽先拍 80 欄還是 120 欄？'
OPTS = [('a', '80 欄', False, '筆電的問題先抓到；寬螢幕晚半天'),
        ('b', '120 欄', True, '平常畫面先過閘門；80 欄晚半天'),
        ('c', '待辦', False, '離開待決，到期日前不再出現')]
DETAIL = [('背景', '操作員平常用寬螢幕，外出用筆電只有 80 欄。'),
          ('卡住', '兩種寬度的閘門要排先後，同時拍會多佔一個審查席。'),
          ('來源', 'PM · 開卡 30 分前')]
AW_T = '時冶 PL02 模型的 8 個推定值照建議定案？'
AW_S = ['動作：打開 staging 頁 /?mode=staging&designer=shiye&variant=pl02-241115 走一圈；驗證：回 A 或 B']
AGENTS = [('codeforge--claude', 'opus', 41, '▆▆▄▃▆'), ('autopilot--claude', 'opus', 37, '▃▄▆▆▄'),
          ('hangar--claude', 'sonnet', 63, '▆▆▆▄▆'), ('codex 派工', 'gpt', 88, '▄▆▆▆▆')]

TABS = ['現況', '待決', '派工', '審查', '花費', '殘留', '艦隊', '待辦', '說明']

URLS_CUT = []  # 被截短的完整網址（Pane 底部提示 y 複製）

def is_url(t):
    return '://' in t or (t.startswith('/') and ('?' in t or '=' in t))

def _toks(s):
    toks, cur = [], ''
    for c in s:
        if c.isascii() and not c.isspace():
            cur += c; continue
        if cur:
            toks.append(cur); cur = ''
        if c in '，。；：、？！）」' and toks:
            toks[-1] += c
        else:
            toks.append(c)
    if cur:
        toks.append(cur)
    return toks

def _greedy(toks, w):
    lines, line = [], ''
    for t in toks:
        if dw(line) + dw(t) > w and line:
            lines.append(line.rstrip()); line = t.lstrip()
        else:
            line += t
    if line:
        lines.append(line.rstrip())
    return lines

def wrap(s, w):
    """詞邊界斷行：ASCII 字串整段不拆；全形標點黏前字；網址不從中間斷，
    放不下一整行就截短加 …（完整網址記在 URLS_CUT）；最後一行不留只有一兩個字（PM 01M89CD1）。"""
    toks = []
    for t in _toks(s):
        if dw(t) > w:
            if is_url(t.rstrip('，。；：')):
                URLS_CUT.append(t); t = cut(t, w - 1) + '…'
            else:
                while dw(t) > w:
                    toks.append(cut(t, w)); t = t[len(cut(t, w)):]
        toks.append(t)
    # 先以子句（，。；：結尾）為單位排；子句比一行還長才拆回單字
    units, cur = [], []
    for t in toks:
        cur.append(t)
        if t[-1:] in '，。；：！？':
            units.append(cur); cur = []
    if cur:
        units.append(cur)
    flat = []
    for u in units:
        j = ''.join(u)
        if dw(j.strip()) <= w:
            flat.append(j); continue
        # 子句太長：在空白處斷，空白之間的一段（例如「走一圈；」）不拆，除非比一行還長
        run = ''
        for t in u + [' ']:
            if t == ' ':
                if run:
                    flat += [run] if dw(run) <= w else list(_toks(run)) if not is_url(run) else [run]
                flat.append(' '); run = ''
            else:
                run += t
        flat.pop()
    toks = flat
    lines = _greedy(toks, w)
    tw = w
    short = max(5, w // 4)
    while len(lines) > 1 and dw(lines[-1]) < short and tw > w * 2 // 3:
        tw -= 1
        cand = _greedy(toks, tw)
        if len(cand) == len(lines) and dw(cand[-1]) >= short:
            return cand
    return lines

# ---------- 框 ----------
def frame_top(title, w, tail='', ctrl=''):
    """╭─ title ───╮ 加 tail（例如 ' [-]'）；總寬 w。"""
    inner = w - dw(tail) - 2
    head = [G('╭─ '), T(title, 'text')]
    n = 3 + dw(title)
    if ctrl:
        head.append(G(' ' + '─' * (inner - n - dw(ctrl) - 3) + ' '))
        head.append(T(ctrl, 'inactive'))
        head.append(G(' ─╮'))
    else:
        head.append(G(' ' + '─' * (inner - n) + '╮'))
    return head

def frame_row(content, w, tail=''):
    fw = w - dw(tail)
    body = fit(content, fw - 4)
    return [G('│ ')] + pad(body, fw - 4) + [G(' │')] + ([T(' ' * dw(tail))] if tail else [])

def frame_bottom(left, right, w, tail=''):
    fw = w - dw(tail)
    l = [G('╰─  ')] + left + [G('  ')]
    r = [G('  ')] + right + [G('  ─╯')]
    gap = fw - rowlen(l) - rowlen(r)
    assert gap >= 2, (gap, w)
    return l + [G('─' * gap)] + r + ([T(' ' * dw(tail))] if tail else [])

def tabs(width_class, cur=1):
    out = []
    for i, name in enumerate(TABS):
        if i:
            out.append(T('  '))
        show = width_class >= 120 or i == cur
        out.append(T(f'{i+1}', 'merged' if i != cur else 'warning'))
        if show:
            out.append(T(' ' + name, 'text' if i != cur else 'warning'))
    return out

def keys(width_class):
    if width_class >= 200:
        s = ['a/b/c 選', 'p/n 換張', 'm 附註', 's 送出全部', 'ctrl+x tab 拿鍵盤', 'Esc 還給輸入框']
    elif width_class >= 120:
        s = ['a/b/c 選', 'p/n 換張', 's 送出', 'Esc']
    else:
        s = ['a/b/c', 'p/n', 's', 'v 細節', 'Esc']
    out = []
    for i, k in enumerate(s):
        if i:
            out.append(G(' · '))
        out.append(T(k, 'text'))
    return out

# ---------- band（底部操作熱區） ----------
def band(W, aw_mode, width_class, cur=1):
    """aw_mode: 'line' 一行摘要；'none' 沒有等你動手卡。回傳列（最上到最下）。"""
    rows = [frame_top(f'待決 · 要你決定 {len(CARDS)} 張 · 第 {FOCUS+1} 張 · 10 秒前', W, '', '[-]')]
    if aw_mode == 'line':
        tail = '步驟在右側 Pane' if width_class >= 110 else 'v 看步驟'
        rows.append(frame_row([G('▲ ', 'warning'), T(f'等你動手 1 張', 'warning'), T(' · ' + AW_T + ' · ', 'text'), T(tail, 'inactive')], W))
    rows.append(frame_row([G('▸ ', 'warning'), T(f'{FOCUS+1}. ' + Q)], W))
    for k, lab, rec, why in OPTS:
        r = [T('    '), T(k + ':', 'merged'), T(' ' + lab)]
        r.append(T(' ' * max(1, 10 - dw(lab))))
        if rec:
            r += [G('★', 'warning'), T(' 建議 ', 'warning')]
        else:
            r.append(T('       '))
        if width_class >= 80:
            r.append(T(why, 'inactive'))
        rows.append(frame_row(r, W))
    if width_class >= 120:
        rows.append(frame_row([T('    '), T('m', 'merged'), T(' 附註：（空）', 'inactive')], W))
    done = sum(1 for _, s in CARDS if s.startswith('已選'))
    rows.append(frame_row([T('s', 'merged'), T(f': 送出全部 · 已選 {done} 張 · 另 {len(CARDS)-done} 張未選')], W))
    rows.append(frame_bottom(keys(width_class), tabs(width_class, cur), W))
    status = [G(' ● ', 'suggestion'), T('進行中'), G(' │ '), T('艦隊 '), G('⊘', 'error'), T(' 4 '),
              G('●', 'suggestion'), T(' 3 '), G('✗', 'error'), T(' 2'), G(' │ '), T('待決 3'), G(' │ '),
              T('ϟ 41 tok/s '), G('▆▆▄▃▆', 'suggestion')]
    rows.append(pad(fit(status, W - 2), W - 2) + [G('▾', 'merged'), T(' ')])
    return rows

def prompt(W):
    line = [G('─' * W)]
    st = [T(' codeforge'), G(' │ '), T('develop'), G(' │ '), T('5h '), G('▰▰▰▰▱▱▱▱▱▱', 'success'), T(' 42% 重置 2h10m'),
          G(' │ '), T('7d '), G('▰▰▱▱▱▱▱▱▱▱', 'success'), T(' 18%'), G(' │ '), T('fleet 3 lane 執行中'), G(' │ '), T('記憶 L1 214 條')]
    return [line, pad([T('> ')], W), line, pad(fit(st, W), W)]

# ---------- Pane（右側，只放 Text，跟著 band 的焦點卡） ----------
def pane_decide(pw):
    tail = ''
    fw = pw - dw(tail)
    iw = fw - 4
    rows = [frame_top(f'待決 · 第 {FOCUS+1} 張的細節', pw, tail, '[-]')]
    rows.append(frame_row([T(Q)], pw, tail))
    for k, v in DETAIL:
        lines = wrap(v, iw - 7)
        for i, ln in enumerate(lines):
            rows.append(frame_row([T((k + '   ') if i == 0 else ' ' * 7, 'inactive'), T(ln)], pw, tail))
    rows.append(frame_row([G('── '), T(f'清單 · 要你決定 {len(CARDS)} 張', 'inactive')], pw, tail))
    for i, (name, st) in enumerate(CARDS):
        mark = G('▸ ', 'warning') if i == FOCUS else T('  ')
        r = [mark, T(f'{i+1}. {name}')]
        r.append(T(' ' * max(1, 18 - dw(name))))
        r.append(T(st, 'success' if st.startswith('已選') else 'inactive'))
        rows.append(frame_row(r, pw, tail))
    rows.append(frame_row([G('── '), T('等你動手 1 張（照做後回覆卡片）', 'inactive')], pw, tail))
    rows.append(frame_row([G('▲ ', 'warning'), T(AW_T, 'warning')], pw, tail))
    for j, s in enumerate(AW_S):
        lines = wrap(s, iw - 5)
        for i, ln in enumerate(lines):
            rows.append(frame_row([T(f'  {j+1}. ' if i == 0 else '     '), T(ln)], pw, tail))
    if URLS_CUT:
        rows.append(frame_row([T('網址截短處：y 複製完整網址', 'inactive')], pw, tail))
    rows.append(frame_bottom([T('只能看 · 選項在下方 band', 'inactive')], [T('↑↓ 捲 · ctrl+x tab', 'inactive')], pw, tail)
                if pw >= 60 else frame_bottom([T('選項在 band', 'inactive')], [T('↑↓', 'inactive')], pw, tail))
    return rows

def pane_fleet(pw):
    """forge-stats 併入：7 艦隊 頁的每個 agent tok/s。"""
    tail = ''
    rows = [frame_top('艦隊 · 每個 agent 的速度 · 10 秒前', pw, tail, '[-]')]
    rows.append(frame_row([T('agent' + ' ' * 17, 'inactive'), T('模型    ', 'inactive'), T('tok/s  ', 'inactive'), T('近 5 分', 'inactive')], pw, tail))
    for name, m, v, sp in AGENTS:
        rows.append(frame_row([T(name + ' ' * (22 - dw(name))), T(m + ' ' * (8 - dw(m))), T(f'{v:>5}  '), G(sp, 'suggestion')], pw, tail))
    rows.append(frame_row([T('合計 229 tok/s · 4 個 agent 執行中', 'inactive')], pw, tail))
    rows.append(frame_bottom([T('只能看', 'inactive')], [T('↑↓ 捲', 'inactive')], pw, tail))
    return rows

# ---------- 組畫面 ----------
def compose(W, H, page='decide', aw='line'):
    URLS_CUT.clear()
    wc = 200 if W >= 200 else 120 if W >= 120 else 80
    b = band(W, aw, wc, 1 if page == 'decide' else 6)
    p = prompt(W)
    top_h = H - len(b) - len(p)
    docked = W >= 110
    pw = 78 if W >= 200 else 52 if W >= 120 else 0
    pane = (pane_decide(pw) if page == 'decide' else pane_fleet(pw)) if docked else []
    cwid = W - pw - (1 if docked else 0)
    conv = [fit(r, cwid) for r in (CONV * 6)][:top_h]
    conv = conv[len(conv) - top_h:] if len(conv) > top_h else [[]] * (top_h - len(conv)) + conv
    rows = []
    off = top_h - len(pane)
    assert off >= 0, (W, H, len(pane), top_h)
    for i in range(top_h):
        left = pad(conv[i], cwid)
        if docked:
            right = pane[i - off] if i >= off else [T(' ' * pw)]
            right = pad(right, pw)
            rows.append(left + [T(' ')] + right)
        else:
            rows.append(left)
    rows += b + p
    assert len(rows) == H, (len(rows), H)
    for r in rows:
        assert rowlen(r) == W, (rowlen(r), W, ''.join(s for s, _, _ in r))
    return rows

THEMES = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cc-themes.json')))
TH = {'light': THEMES[0], 'dark': THEMES[2]}
BG = {'light': (255, 255, 255), 'dark': (0, 0, 0)}

def rgb(s):
    return tuple(int(x) for x in s[4:-1].split(','))

def ans(rows, theme):
    t = TH[theme]
    bg = BG[theme]
    out = []
    for r in rows:
        line = f'\x1b[48;2;{bg[0]};{bg[1]};{bg[2]}m'
        for s, role, _ in r:
            c = rgb(t[role])
            line += f'\x1b[38;2;{c[0]};{c[1]};{c[2]}m' + s
        out.append(line + '\x1b[0m')
    return '\n'.join(out) + '\n'

def main():
    plan = [('209x49', 'decide', 'line'), ('120x40', 'decide', 'line'), ('80x24', 'decide', 'line'),
            ('209x49-fleet', 'fleet', 'none'), ('80x24-noaw', 'decide', 'none')]
    for name, page, aw in plan:
        W, H = (int(x) for x in name.split('-')[0].split('x'))
        rows = compose(W, H, page, aw)
        d = os.path.join(OUT, name)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, 'screen.txt'), 'w').write('\n'.join(''.join(s for s, _, _ in r) for r in rows) + '\n')
        for th in ('dark', 'light'):
            open(os.path.join(d, th + '.ans'), 'w').write(ans(rows, th))
        json.dump([[[s, r, k] for s, r, k in row] for row in rows], open(os.path.join(d, 'roles.json'), 'w'), ensure_ascii=False)
        json.dump(sorted({t.rstrip('，。；：') for s in AW_S for t in s.split() if is_url(t.rstrip('，。；：'))}), open(os.path.join(d, 'urls.json'), 'w'), ensure_ascii=False)
        print(name, 'ok', W, H)

if __name__ == '__main__':
    main()
