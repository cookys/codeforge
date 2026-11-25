#!/usr/bin/env python3
"""v2 版面（單 01M89PEH1Y7FS0TMAGNMXYZDB1）：合併稿 29a1b1c 照操作員要求重排。

操作員原話：「bottom-left 可以是 index，bottom-right 是回復區，然後右邊往上漲的是來龍去脈」。
- 寬螢幕（≥110 欄）：底部一個框，中間一條直線。左格 index（全部卡：已選哪個字母、未選、要動手、目前這張），
  右格回覆區（只放目前這張的問題、動手步驟、選項、★ 建議、m 附註、s 送出）。
- 來龍去脈：回覆區正上方、貼右邊的另一個框，由下往上長，高度跟著內容，跟著選中的卡換。只有文字。沒有 v 鍵。
- 80 欄：index 縮成回覆框裡的一列，來龍去脈疊在回覆框正上方（放不下時從最上面收起）。
- 最底下那一行（狀態＋分頁）、選項字母、m 附註有寫才出現、送出後不能改、沒有 u 收回：照合併稿。

用法：python3 gen.py out
輸出 out/<W>x<H>-s<N>/{screen.txt, roles.json, dark.ans, light.ans[, urls.json]} 和 out/flow.json。
片段模型、框線寫法沿用 brief/tools/gen3.py。
"""
import json, os, sys, unicodedata

OUT = sys.argv[1] if len(sys.argv) > 1 else 'out'
HERE = os.path.dirname(os.path.abspath(__file__))

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

def pad(row, w):
    n = rowlen(row)
    assert n <= w, (n, w, ''.join(s for s, _, _ in row))
    return row + [T(' ' * (w - n))]

def fit(row, w):
    """截到 w 格：放不下的那一段截短加 …。"""
    out, n = [], 0
    for s, r, k in row:
        if n + dw(s) <= w:
            out.append((s, r, k)); n += dw(s)
        else:
            rest = w - n
            if rest > 1:
                out.append((cut(s, rest - 1) + '…', r, k))
            break
    return out

def wrap(s, w):
    """中文逐字、ASCII 整段不拆；全形標點黏前字。"""
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
    lines, line = [], ''
    for t in toks:
        if dw(line) + dw(t) > w and line:
            lines.append(line.rstrip()); line = t.lstrip()
        else:
            line += t
    if line:
        lines.append(line.rstrip())
    return lines

# ---------- 假資料 ----------
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
URL = '/?mode=staging&designer=shiye&variant=pl02-241115'
CARDS = [
    dict(name='合併 plan', q='要不要把 tui 共用計畫合進 develop？', steps=[],
         opts=[('a', '合併', True, '計畫已凍結兩輪；今天就能開工'),
               ('b', '再審一輪', False, '換一家再看一次；晚一天'),
               ('c', '待辦', False, '離開待決，到期日前不再出現')],
         detail=[('來源', 'PM · 開卡 3 小時前'), ('背景', '共用 tui 計畫審了兩輪，兩家都給過。')]),
    dict(name='外掛更新時機', q='forge-stats 外掛今晚還是明早更新？', steps=[],
         opts=[('a', '今晚', False, '今晚就用到新版；出事沒人在線上'),
               ('b', '明早', True, '有人在線上看著換'),
               ('c', '待辦', False, '離開待決，到期日前不再出現')],
         detail=[('來源', 'PM · 開卡 1 小時前'), ('背景', 'forge-stats 新版改了艦隊頁的欄位。'),
                 ('卡住', '更新要重載外掛，進行中的 session 會閃一下。')]),
    dict(name='預覽先拍幾欄', q='預覽先拍 80 欄還是 120 欄？', steps=[],
         opts=[('a', '80 欄', False, '筆電的問題先抓到；寬螢幕晚半天'),
               ('b', '120 欄', True, '平常畫面先過閘門；80 欄晚半天'),
               ('c', '待辦', False, '離開待決，到期日前不再出現')],
         detail=[('來源', 'PM · 開卡 30 分前'),
                 ('背景', '操作員平常用寬螢幕，外出用筆電只有 80 欄。'),
                 ('卡住', '兩種寬度的閘門要排先後，同時拍會多佔一個審查席。')]),
    dict(name='時冶 PL02 定案', q='時冶 PL02 模型的 8 個推定值照建議定案？',
         steps=['打開 staging 頁 ' + URL, '走一圈，核對 8 個推定值'],
         opts=[('a', '照建議定案', True, '8 個值都和 staging 看到的一致'),
               ('b', '不照建議', False, '按 m 寫哪一個值要改'),
               ('c', '待辦', False, '還沒空做，到期日前不再出現')],
         note='staging 走過一圈，8 個都對',
         detail=[('來源', 'PM · 開卡 2 小時前'),
                 ('前因', '11-22 時冶交出 PL02 第一版，8 個值裡有 3 個是估的。'),
                 ('', '11-23 PM 退回，要求附上 staging 對照。'),
                 ('', '11-24 時冶補上對照，改了第 5、7 兩個值。'),
                 ('', '11-25 審查席看過數字沒問題，但沒有人親眼走過 staging。'),
                 ('背景', '時冶把 PL02 的 8 個推定值填好了，要你親眼看過 staging 才能定案。'),
                 ('卡住', '沒定案，PL02 後面的派工開不了。')]),
]
NDEC = sum(1 for c in CARDS if not c['steps'])
NACT = len(CARDS) - NDEC
AGENTS = [('codeforge--claude', 'opus', 41, '▆▆▄▃▆'), ('autopilot--claude', 'opus', 37, '▃▄▆▆▄'),
          ('hangar--claude', 'sonnet', 63, '▆▆▆▄▆'), ('codex 派工', 'gpt', 88, '▄▆▆▆▆')]
TABS = ['現況', '待決', '派工', '審查', '花費', '殘留', '艦隊', '待辦', '說明']

# ---------- 框 ----------
def frame_top(title, w, ctrl='[-]'):
    head = [G('╭─ ')] + title
    n = rowlen(head)
    tail = [G(' '), T(ctrl, 'inactive'), G(' ─╮')] if ctrl else [G('─╮')]
    gap = w - n - rowlen(tail) - 1
    assert gap >= 1, (gap, w)
    return head + [G(' ' + '─' * gap)] + tail

def hline(lc, items, w):
    """框線的一段：lc 起頭、items 嵌在線上，補 ─ 到 w 格。"""
    row = [G(lc + '─ ')] + fit(items, w - 4) + [G(' ')]
    return row + [G('─' * (w - rowlen(row)))]

def frame_row(content, w):
    return [G('│ ')] + pad(fit(content, w - 4), w - 4) + [G(' │')]

def frame_bottom(hint, w):
    l = [G('╰─ ')] + fit(hint, w - 7) + [G(' ')]
    return l + [G('─' * (w - rowlen(l) - 1) + '╯')]

def hints(items):
    out = []
    for i, (k, d) in enumerate(items):
        if i:
            out.append(G(' · '))
        out.append(T(k))
        if d:
            out.append(T(' ' + d, 'inactive'))
    return out

# ---------- 最底下那一行：左狀態、右分頁（永遠在） ----------
def tabs(cur, mode):
    """mode: full 全部有名字；cur 只有目前頁有名字。"""
    out = []
    for i, name in enumerate(TABS):
        if i:
            out.append(T('  ' if mode == 'full' else ' '))
        on = i + 1 == cur
        out.append(T(str(i + 1), 'warning' if on else 'merged'))
        if mode == 'full' or on:
            out.append(T(' ' + name, 'warning' if on else 'text'))
    return out

def status_line(W, st):
    pending = st['mode'] in ('alert', 'card')
    counts = ([T(f'待決 {NDEC} · 動手 {NACT}', 'warning')] if pending else [T('待決 0', 'inactive')])
    counts_s = ([T(f'待決 {NDEC} ', 'warning'), G('▲', 'warning'), T(f' {NACT}', 'warning')] if pending else [T('待決 0', 'inactive')])
    sep = [G(' │ ')]
    head = [G(' ● ', 'suggestion'), T('進行中')]
    fleet = [T('艦隊 '), G('⊘', 'error'), T(' 4 '), G('●', 'suggestion'), T(' 3 '), G('✗', 'error'), T(' 2')]
    speed = [T('ϟ 41 tok/s '), G('▆▆▄▃▆', 'suggestion')]
    toggle = [T(' '), G('▾' if st['mode'] in ('card', 'receipt') else '▴', 'merged'), T(' ')]
    cur = {'ctx': 2, 'fleet': 7, 'spend': 5}.get(st['pane'], 0)
    if st['mode'] == 'alert':
        c = CARDS[0]
        alert = [G(' ▲ ', 'warning'), T(f'要你處理 {len(CARDS)} 張', 'warning'), T(' · ctrl+x tab 打開'),
                 T(f' · 第 1 張：{c["q"]}', 'inactive')]
        for tm in ('full', 'cur'):
            right = tabs(cur, tm) + toggle
            room = W - rowlen(right) - 2
            if room >= 34:
                return pad(fit(alert, room), W - rowlen(right)) + right
    lefts = [head + sep + fleet + sep + counts + sep + speed,
             head + sep + counts + sep + speed,
             head + sep + counts_s + sep + [T('ϟ 41 tok/s')]]
    for left, tm in ((lefts[0], 'full'), (lefts[1], 'full'), (head + sep + counts_s + sep + speed, 'full'), (lefts[1], 'cur'), (lefts[2], 'cur')):
        right = tabs(cur, tm) + toggle
        if rowlen(left) + 2 + rowlen(right) <= W:
            return pad(left, W - rowlen(right)) + right
    raise AssertionError(W)

def prompt(W):
    line = [G('─' * W)]
    st = [T(' codeforge'), G(' │ '), T('develop'), G(' │ '), T('5h '), G('▰▰▰▰▱▱▱▱▱▱', 'success'), T(' 42% 重置 2h10m'),
          G(' │ '), T('7d '), G('▰▰▱▱▱▱▱▱▱▱', 'success'), T(' 18%'), G(' │ '), T('fleet 3 lane 執行中'), G(' │ '), T('記憶 L1 214 條')]
    return [line, pad([T('> ')], W), line, pad(fit(st, W), W)]

# ---------- band：一次一張卡 ----------
def strip(st, iw):
    """佇列條：每張卡一格。✓ 已選、· 沒選、▲ 要動手、▸ 目前這張。"""
    def build(names):
        out = []
        for i, c in enumerate(CARDS):
            if i:
                out.append(T(' '))
            foc = i == st['focus']
            ans = st['ans'].get(i)
            out.append(G('▸', 'warning') if foc else T(' '))
            if ans:
                out.append(G('✓', 'success'))
            elif c['steps']:
                out.append(G('▲', 'warning'))
            else:
                out.append(G('·', 'inactive'))
            role = 'warning' if foc else 'text' if ans else 'inactive'
            label = f'{i+1}'
            if names == 'all' or (names == 'focus' and foc):
                label += ' ' + c['name']
            out.append(T(label, role))
            if ans:
                out.append(T('：' + ans, 'success'))
        return out
    for names in ('all', 'focus', 'none'):
        r = build(names)
        if rowlen(r) <= iw:
            return r
    raise AssertionError(iw)

def reply_rows(st):
    """回覆區：只放目前這張卡。問題、（動手步驟）、選項、★ 建議、（m 附註，有寫才有）、s 送出。"""
    c = CARDS[st['focus']]
    ans = st['ans']
    mine = ans.get(st['focus'])
    rows = []
    if c['steps']:
        rows.append([G('▲ ', 'warning'), T(c['q']), T('  先照 1、2 做，再選', 'warning')])
        for j, s in enumerate(c['steps']):
            rows.append([T(f'    {j+1}. ', 'inactive'), T(s)])
    else:
        rows.append([G('▸ ', 'warning'), T(c['q'])])
    lw = max(dw(o[1]) for o in c['opts']) + 2
    assert [o[0] for o in c['opts']] == list('abc')
    for k, lab, rec, why in c['opts']:
        r = [T('    '), T(k + ':', 'merged'), T(' ' + lab + ' ' * (lw - dw(lab)))]
        r += [G('★', 'warning'), T(' 建議  ', 'warning')] if rec else [T(' ' * 8)]
        if mine:
            r += [G('✓', 'success'), T(' 已選  ', 'success')] if mine == k else [T(' ' * 8)]
        r.append(T(why, 'inactive'))
        rows.append(r)
    if st.get('note') and c.get('note'):
        rows.append([T('    m 附註  ', 'inactive'), T(c['note'])])
    n = len(ans)
    if n == len(CARDS):
        send = [T('s:', 'merged'), T(f' 送出 {n} 張', 'warning'), T(' · 都選好了', 'warning'), T(' · 送出後不能改', 'inactive')]
    elif n:
        send = [T('s:', 'merged'), T(f' 送出已選 {n} 張'), T(f' · 還有 {len(CARDS)-n} 張沒選 · 送出後不能改', 'inactive')]
    else:
        send = [T('s:', 'merged'), T(' 送出', 'inactive'), T(f' · {len(CARDS)} 張都還沒選 · 送出後不能改', 'inactive')]
    rows.append(send)
    return rows

def index_rows(st):
    """index：全部卡，一張一列。▸ 目前這張；✓ 已選（寫出字母和名字）、· 未選、▲ 要動手。只有文字。"""
    nw = max(dw(c['name']) for c in CARDS) + 2
    rows = []
    def entry(i, c):
        foc = i == st['focus']
        k = st['ans'].get(i)
        r = [G('▸', 'warning') if foc else T(' ')]
        r.append(G('✓', 'success') if k else G('▲', 'warning') if c['steps'] else G('·', 'inactive'))
        r.append(T(f' {i+1} ' + c['name'] + ' ' * (nw - dw(c['name'])), 'warning' if foc else 'text'))
        if k:
            r += [T(k, 'success'), T(' ' + next(o[1] for o in c['opts'] if o[0] == k))]
        elif c['steps']:
            r.append(T('要動手', 'warning'))
        else:
            r.append(T('未選', 'inactive'))
        rows.append(r)
    rows.append([T(f'待決 {NDEC} 張', 'inactive')])
    for i, c in enumerate(CARDS):
        if not c['steps']:
            entry(i, c)
    rows.append([T(f'等你動手 {NACT} 張', 'inactive')])
    for i, c in enumerate(CARDS):
        if c['steps']:
            entry(i, c)
    return rows

def key_hints(st, W):
    c = CARDS[st['focus']]
    hs = [('a/b/c', '選'), ('m', '附註')]
    if c['steps']:
        hs.append(('y', '複製網址'))
    hs += [('s', '送出'), ('1-9', '換頁'), ('Esc', '回輸入框')]
    if W < 150:
        hs = [h for h in hs if h[0] != '1-9']
    return hs

IDX = 34  # index 那一格的內寬

def band_split(W, st):
    """寬螢幕：一個底部框，左 index、右回覆區，中間一條直線。換張的鍵在 index 下面，回覆的鍵在回覆區下面。"""
    c = CARDS[st['focus']]
    lw_, rw_ = IDX + 3, W - IDX - 3   # 左段含左框線到直線前；右段從直線到右框線
    ltitle = [T(f'要你處理 {len(CARDS)} 張'), T(f' · 已選 {len(st["ans"])}', 'inactive')]
    rtitle = [T(f'第 {st["focus"]+1} 張', 'warning'), T(' · ' + c['name'])]
    tail = [G(' '), T('[-]', 'inactive'), G(' ─╮')]
    rows = [hline('╭', ltitle, lw_) + hline('┬', rtitle, rw_ - rowlen(tail)) + tail]
    L, R = index_rows(st), reply_rows(st)
    for i in range(max(len(L), len(R))):
        l = pad(fit(L[i] if i < len(L) else [], IDX), IDX)
        r = pad(fit(R[i] if i < len(R) else [], W - IDX - 7), W - IDX - 7)
        rows.append([G('│ ')] + l + [G(' │ ')] + r + [G(' │')])
    rows.append(hline('╰', hints([('p/n', '換張')]), lw_) + hline('┴', hints(key_hints(st, W)), rw_ - 1) + [G('╯')])
    return rows

def band_card(W, st):
    """80 欄：放不下左右兩格，index 縮成框裡第一列（佇列條），下面就是回覆區。"""
    title = [T(f'要你處理 {len(CARDS)} 張', 'text'), T(f' · 第 {st["focus"]+1} 張', 'inactive')]
    rows = [frame_top(title, W), frame_row(strip(st, W - 4), W)]
    rows += [frame_row(r, W) for r in reply_rows(st)]
    hs = key_hints(st, W)
    hs.insert(1, ('p/n', '換張'))
    hs[-1] = ('Esc', '')
    rows.append(frame_bottom(hints(hs), W))
    return rows

def band_receipt(W, st):
    rows = [frame_top([G('✓ ', 'success'), T(f'已送出 {len(CARDS)} 張', 'success'), T(' · 剛剛', 'inactive')], W)]
    nw = max(dw(c['name']) for c in CARDS) + 3
    for i, c in enumerate(CARDS):
        k = st['ans'][i]
        lab = next(o[1] for o in c['opts'] if o[0] == k)
        rows.append(frame_row([T(f'  {i+1} ', 'inactive'), T(c['name'] + ' ' * (nw - dw(c['name']))),
                               T(k, 'success'), T(' ' + lab)], W))
    rows.append(frame_bottom(hints([('1-9', '看別頁'), ('Esc', '回輸入框')]), W))
    return rows

# ---------- Pane：只放文字，不放按鍵 ----------
def pane_bottom(pw, st):
    h = [T('只能看', 'inactive'), G(' · '), T('1-9 換頁', 'inactive'), G(' · '), T('Esc 關', 'inactive'), G(' · '), T('↑↓ 捲', 'inactive')]
    return frame_bottom(h, pw)

def pane_ctx(pw, st, cap):
    """來龍去脈：目前這張卡的來源、前因、背景、卡住。由下往上長，高度跟著內容；只有文字，沒有鍵。
    順序是時間順（來源在最上，卡住貼著回覆區）。超過 cap 列時從最上面收起，留一列說收了幾列。"""
    i = st['focus']
    iw = pw - 4
    lines, lab = [], ''
    for k, v in CARDS[i]['detail']:
        lab = k or lab
        for j, ln in enumerate(wrap(v, iw - 7)):
            lines.append((lab, j == 0 and bool(k), ln))
    room = cap - 2
    cutn = max(0, len(lines) - (room - 1)) if len(lines) > room else 0
    lines = lines[cutn:]
    # 收起後第一列補回它的標籤，不讓一列字沒頭沒尾
    body = [[T((k + '   ') if (first or (cutn and n == 0)) else ' ' * 7, 'inactive'), T(ln)] for n, (k, first, ln) in enumerate(lines)]
    if cutn:
        body.insert(0, [G('⋯ ', 'inactive'), T(f'更早的 {cutn} 列收起來了', 'inactive')])
    rows = [frame_top([T('2 待決', 'warning'), T(f' · 第 {i+1} 張的來龍去脈')], pw, ctrl='')]
    rows += [frame_row(r, pw) for r in body]
    rows.append(frame_bottom([T('只能看 · 跟著下面那張卡換', 'inactive')], pw))
    return rows

def pane_fleet(pw, st):
    rows = [frame_top([T('7 艦隊', 'warning'), T(' · 每個 agent 的速度 · 10 秒前', 'text')], pw)]
    rows.append(frame_row([T('agent' + ' ' * 17, 'inactive'), T('模型    ', 'inactive'), T('tok/s  ', 'inactive'), T('近 5 分', 'inactive')], pw))
    for name, m, v, sp in AGENTS:
        rows.append(frame_row([T(name + ' ' * (22 - dw(name))), T(m + ' ' * (8 - dw(m))), T(f'{v:>5}  '), G(sp, 'suggestion')], pw))
    rows.append(frame_row([T('合計 229 tok/s · 4 個 agent 執行中', 'inactive')], pw))
    rows.append(frame_row([], pw))
    rows.append(frame_row([G('● ', 'suggestion'), T('執行中 1  '), T('甲線 codex · tux-sync 測試 · 12 分', 'inactive')], pw))
    rows.append(frame_row([G('✗ ', 'error'), T('失敗 1    ', 'error'), T('丙線 kimi · 整合測試逾時', 'inactive')], pw))
    rows.append(frame_row([G('✓ ', 'success'), T('已完成 1  '), T('乙線 kimi · README 盤點 · 40 分前', 'inactive')], pw))
    rows.append(pane_bottom(pw, st))
    return rows

def pane_spend(pw, st):
    rows = [frame_top([T('5 花費', 'warning'), T(' · 額度和今天用量 · 10 秒前', 'text')], pw)]
    rows.append(frame_row([T('5 小時  '), T(' 42%  '), G('▰▰▰▰▱▱▱▱▱▱', 'success'), T('  重置 2h10m', 'inactive')], pw))
    rows.append(frame_row([T('7 天    '), T(' 18%  '), G('▰▰▱▱▱▱▱▱▱▱', 'success'), T('  重置 4d', 'inactive')], pw))
    rows.append(frame_row([], pw))
    rows.append(frame_row([G('○ ', 'inactive'), T('A 池 42%   '), G('○ ', 'inactive'), T('B 池 18%')], pw))
    rows.append(frame_row([G('✗ ', 'error'), T('C 池 180 分沒更新', 'error')], pw))
    rows.append(frame_row([G('▲ ', 'warning'), T('G 池 約剩 91% 會浪費', 'warning')], pw))
    rows.append(frame_row([], pw))
    rows.append(frame_row([T('今日    '), T('380k tok'), T('  不顯示金額', 'inactive')], pw))
    rows.append(frame_row([T('近 24 小時  ', 'inactive'), G('▁▂▂▃▅▆▇█▆▄▃▂▁▁▂▃▄▅▆▇▆▅▃▂', 'suggestion')], pw))
    rows.append(pane_bottom(pw, st))
    return rows

def pane_for(pw, st, cap):
    p = st['pane']
    if p == 'ctx':
        return pane_ctx(pw, st, cap)
    return {'fleet': pane_fleet, 'spend': pane_spend}[p](pw, st) if p else []

# ---------- flow ----------
A1, A2, A3 = {0: 'a'}, {0: 'a', 1: 'b'}, {0: 'a', 1: 'b', 2: 'b'}
A4 = {**A3, 3: 'a'}
STEPS = [
    dict(title='平常沒事：只有最底下一行', key='（等卡片進來）', next=2, mode='idle', pane=None),
    dict(title='卡片進來：那一行變提醒，不搶鍵盤', key='ctrl+x tab', next=3, mode='alert', pane=None),
    dict(title='待決卡：左下 index、右下回覆區、右上來龍去脈', key='a', next=4, mode='card', pane='ctx', focus=0, ans={}),
    dict(title='選了自動跳下一張：index 打勾，來龍去脈跟著換', key='b', next=5, mode='card', pane='ctx', focus=1, ans=A1),
    dict(title='第 3 張：來龍去脈多一段，框往上長', key='b', next=6, mode='card', pane='ctx', focus=2, ans=A2),
    dict(title='等你動手卡：步驟在回覆區，來龍去脈很長', key='a（照 1、2 做完之後）', next=7, mode='card', pane='ctx', focus=3, ans=A3),
    dict(title='都選好了：送出列亮起來（這張按 m 寫了附註）', key='s', next=8, mode='card', pane='ctx', focus=3, ans=A4, note=True),
    dict(title='送出後的回條：已送出，每張送了什麼', key='7', next=9, mode='receipt', pane=None, ans=A4),
    dict(title='艦隊頁：每個 agent 的 tok/s', key='5', next=10, mode='idle', pane='fleet'),
    dict(title='花費頁：額度和今天用量', key='Esc', next=1, mode='idle', pane='spend'),
]

def caption(W, n, s):
    nxt = f'按 {s["key"]} → 步驟 {s["next"]}' if not s['key'].startswith('（') else f'{s["key"]} → 步驟 {s["next"]}'
    row = [T(f' 設計稿註記 · 步驟 {n}/{len(STEPS)} · ', 'inactive'), T(nxt, 'warning'), T(' · ' + s['title'], 'inactive')]
    return pad(fit(row, W), W)

def compose(W, H, n):
    s = STEPS[n - 1]
    docked = W >= 110
    st = dict(mode=s['mode'], pane=s['pane'], focus=s.get('focus', 0), ans=s.get('ans', {}), docked=docked, note=s.get('note'))
    if st['mode'] == 'card':
        b = band_split(W, st) if docked else band_card(W, st)
    else:
        b = band_receipt(W, st) if st['mode'] == 'receipt' else []
    bottom = b + [status_line(W, st)] + prompt(W)
    top_h = H - len(bottom)
    pw = (78 if W >= 200 else 52) if docked else W
    # 對話區不能被蓋到：右欄不佔對話的列；80 欄疊放時至少留 3 列（含註記列）
    pane = pane_for(pw, st, top_h - 1 if docked else top_h - 3)
    rows = []
    if docked or not pane:
        cwid = W - pw - 1 if pane else W
        conv = ([[]] + CONV * 6)[:top_h]
        off = top_h - len(pane)
        assert off >= 1, (W, H, n)
        for i in range(top_h):
            left = pad(fit(conv[i], cwid), cwid)
            if pane:
                left += [T(' ')] + (pane[i - off] if i >= off else [T(' ' * pw)])
            rows.append(left)
    else:  # 80 欄：右欄的框疊在回覆框正上方，同一份內容
        ch = top_h - len(pane)
        assert ch >= 1, (W, H, n, ch)
        conv = ([[]] + CONV * 6)[:ch]
        rows = [pad(fit(r, W), W) for r in conv] + pane
    rows[0] = caption(W, n, s)
    rows += bottom
    assert len(rows) == H, (len(rows), H)
    for r in rows:
        assert rowlen(r) == W, (rowlen(r), W, ''.join(x for x, _, _ in r))
    return rows

THEMES = json.load(open(os.path.join(HERE, '..', 'brief', 'tools', 'cc-themes.json')))
TH = {'light': THEMES[0], 'dark': THEMES[2]}
BG = {'light': (255, 255, 255), 'dark': (0, 0, 0)}

def rgb(s):
    return tuple(int(x) for x in s[4:-1].split(','))

def ans(rows, theme):
    t, bg = TH[theme], BG[theme]
    out = []
    for r in rows:
        line = f'\x1b[48;2;{bg[0]};{bg[1]};{bg[2]}m'
        for s, role, _ in r:
            c = rgb(t[role])
            line += f'\x1b[38;2;{c[0]};{c[1]};{c[2]}m' + s
        out.append(line + '\x1b[0m')
    return '\n'.join(out) + '\n'

def main():
    os.makedirs(OUT, exist_ok=True)
    for W, H in ((209, 49), (120, 40), (80, 24)):
        for n in range(1, len(STEPS) + 1):
            rows = compose(W, H, n)
            d = os.path.join(OUT, f'{W}x{H}-s{n}')
            os.makedirs(d, exist_ok=True)
            text = '\n'.join(''.join(s for s, _, _ in r) for r in rows) + '\n'
            open(os.path.join(d, 'screen.txt'), 'w').write(text)
            for th in ('dark', 'light'):
                open(os.path.join(d, th + '.ans'), 'w').write(ans(rows, th))
            json.dump([[[s, r, k] for s, r, k in row] for row in rows], open(os.path.join(d, 'roles.json'), 'w'), ensure_ascii=False)
            u = os.path.join(d, 'urls.json')
            if '/?mode' in text:
                json.dump([URL], open(u, 'w'), ensure_ascii=False)
            elif os.path.exists(u):
                os.remove(u)
    json.dump([{'step': i + 1, 'title': s['title'], 'key_to_next': s['key'], 'next': s['next']} for i, s in enumerate(STEPS)],
              open(os.path.join(OUT, 'flow.json'), 'w'), ensure_ascii=False, indent=1)
    print('ok', len(STEPS), 'steps x 3 sizes')

if __name__ == '__main__':
    main()
