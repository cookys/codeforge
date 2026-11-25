#!/usr/bin/env python3
"""三家競稿合併稿：以 fable 版為底，借 neon 的右欄佇列清單（單 01M89GKEWA5P9EJJSPVC1QHMA5）。

理念：一個佇列、一張卡、一條底線。
- band 一次只放一張卡（待決卡和「等你動手」卡同一個形狀），選了自動跳下一張。
- 最底下那一行永遠在：左邊狀態、右邊分頁，不隨狀態搬家。
- 寬螢幕（≥110 欄）：band 開著卡時，右側 Pane 跟著開在「2 待決」頁，列出全部卡（已選哪個字母、未選、目前這張）；
  v 在清單下面加開目前這張的細節。Pane 只放文字。清單在 Pane 時 band 不再放佇列條（同一件事只講一次）。
- 80 欄：放不下右欄，佇列留在 band 的佇列條；v 的細節框疊在 band 上面。
- 送出後是純回條，沒有收回（後端沒有延後送出）。

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
         detail=[('來源', 'PM · 開卡 3 小時前')]),
    dict(name='外掛更新時機', q='forge-stats 外掛今晚還是明早更新？', steps=[],
         opts=[('a', '今晚', False, '今晚就用到新版；出事沒人在線上'),
               ('b', '明早', True, '有人在線上看著換'),
               ('c', '待辦', False, '離開待決，到期日前不再出現')],
         detail=[('來源', 'PM · 開卡 1 小時前')]),
    dict(name='預覽先拍幾欄', q='預覽先拍 80 欄還是 120 欄？', steps=[],
         opts=[('a', '80 欄', False, '筆電的問題先抓到；寬螢幕晚半天'),
               ('b', '120 欄', True, '平常畫面先過閘門；80 欄晚半天'),
               ('c', '待辦', False, '離開待決，到期日前不再出現')],
         detail=[('背景', '操作員平常用寬螢幕，外出用筆電只有 80 欄。'),
                 ('卡住', '兩種寬度的閘門要排先後，同時拍會多佔一個審查席。'),
                 ('來源', 'PM · 開卡 30 分前')]),
    dict(name='時冶 PL02 定案', q='時冶 PL02 模型的 8 個推定值照建議定案？',
         steps=['打開 staging 頁 ' + URL, '走一圈，核對 8 個推定值'],
         opts=[('a', '照建議定案', True, '8 個值都和 staging 看到的一致'),
               ('b', '不照建議', False, '按 m 寫哪一個值要改'),
               ('c', '待辦', False, '還沒空做，到期日前不再出現')],
         detail=[('背景', '時冶把 PL02 的 8 個推定值填好了，要你親眼看過 staging 才能定案。'),
                 ('卡住', '沒定案，PL02 後面的派工開不了。'),
                 ('來源', 'PM · 開卡 2 小時前')]),
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
    tail = [G(' '), T(ctrl, 'inactive'), G(' ─╮')]
    gap = w - n - rowlen(tail) - 1
    assert gap >= 1, (gap, w)
    return head + [G(' ' + '─' * gap)] + tail

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
    cur = {'queue': 2, 'detail': 2, 'fleet': 7, 'spend': 5}.get(st['pane'], 0)
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

def band_card(W, H, st):
    c = CARDS[st['focus']]
    ans = st['ans']
    mine = ans.get(st['focus'])
    title = [T(f'要你處理 {len(CARDS)} 張', 'text'), T(f' · 第 {st["focus"]+1} 張', 'inactive')]
    rows = [frame_top(title, W)]
    if not st['docked']:  # 清單在右欄時不重複；放不下右欄才用佇列條
        rows.append(frame_row(strip(st, W - 4), W))
    if c['steps']:
        rows.append(frame_row([G('▲ ', 'warning'), T(c['q']), T('  先照 1、2 做，再選', 'warning')], W))
        for j, s in enumerate(c['steps']):
            rows.append(frame_row([T(f'    {j+1}. ', 'inactive'), T(s)], W))
    else:
        rows.append(frame_row([G('▸ ', 'warning'), T(c['q'])], W))
    lw = max(dw(o[1]) for o in c['opts']) + 2
    for k, lab, rec, why in c['opts']:
        r = [T('    '), T(k + ':', 'merged'), T(' ' + lab + ' ' * (lw - dw(lab)))]
        r += [G('★', 'warning'), T(' 建議  ', 'warning')] if rec else [T(' ' * 8)]
        if mine:
            r += [G('✓', 'success'), T(' 已選  ', 'success')] if mine == k else [T(' ' * 8)]
        r.append(T(why, 'inactive'))
        rows.append(frame_row(r, W))
    n = len(ans)
    if n == len(CARDS):
        send = [T('s:', 'merged'), T(f' 送出 {n} 張', 'warning'), T(' · 都選好了', 'warning'), T(' · 送出後不能改', 'inactive')]
    elif n:
        send = [T('s:', 'merged'), T(f' 送出已選 {n} 張'), T(f' · 還有 {len(CARDS)-n} 張沒選 · 送出後不能改', 'inactive')]
    else:
        send = [T('s:', 'merged'), T(' 送出', 'inactive'), T(f' · {len(CARDS)} 張都還沒選', 'inactive')]
    rows.append(frame_row(send, W))
    hs = [('a/b/c', '選'), ('p/n', '換張'), ('v', '關細節' if st['pane'] == 'detail' else '細節'), ('m', '附註')]
    if c['steps']:
        hs.append(('y', '複製網址'))
    hs += [('s', '送出'), ('1-9', '換頁'), ('Esc', '回輸入框')]
    if W < 100:
        hs = [h for h in hs if h[0] != '1-9']
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
    card = st['mode'] == 'card'
    vk = {'queue': 'v 看細節', 'detail': 'v 關細節' if st['docked'] else 'v 關'}.get(st['pane'], 'v 關')
    h = [T('只能看', 'inactive'), G(' · '), T('選項在下面' if card else '1-9 換頁', 'inactive'),
         G(' · '), T(vk if card else 'Esc 關', 'inactive'), G(' · '), T('↑↓ 捲', 'inactive')]
    return frame_bottom(h, pw)

def pane_detail(pw, st):
    i = st['focus']
    c = CARDS[i]
    iw = pw - 4
    rows = [frame_top([T('2 待決', 'warning'), T(f' · 第 {i+1} 張的細節', 'text')], pw)]
    rows.append(frame_row([T(c['name'])], pw))
    for k, v in c['detail']:
        for j, ln in enumerate(wrap(v, iw - 7)):
            rows.append(frame_row([T((k + '   ') if j == 0 else ' ' * 7, 'inactive'), T(ln)], pw))
    rows.append(frame_row([T('附註   ', 'inactive'), T('（空）', 'inactive')], pw))
    rows.append(pane_bottom(pw, st))
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

def pane_queue(pw, st):
    """寬螢幕的「2 待決」頁：全部卡的清單（借 neon）。只放文字，沒有可按的鍵。
    每張兩列：狀態（已選哪個字母／未選／目前這張）＋問題。v 之後，目前這張的細節接在清單下面。"""
    iw = pw - 4
    nw = max(dw(c['name']) for c in CARDS) + 2
    n = len(st['ans'])
    rows = [frame_top([T('2 待決', 'warning'), T(f' · 全部 {len(CARDS)} 張 · 已選 {n} 張', 'text')], pw)]
    def entry(i, c):
        foc = i == st['focus']
        k = st['ans'].get(i)
        role = 'warning' if foc else 'text'
        r = [G('▸', 'warning') if foc else T(' ')]
        r.append(G('✓', 'success') if k else G('▲', 'warning') if c['steps'] else G('·', 'inactive'))
        r.append(T(f' {i+1} ' + c['name'] + ' ' * (nw - dw(c['name'])), role))
        if k:
            lab = next(o[1] for o in c['opts'] if o[0] == k)
            r += [T('已選 ' + k, 'success'), T(' ' + lab)]
        else:
            r.append(T('未選', 'inactive'))
        if foc:
            r.append(T(' · 目前', 'warning'))
        rows.append(frame_row(r, pw))
        rows.append(frame_row([T('     ' + c['q'], 'inactive')], pw))
    rows.append(frame_row([T(f'待決 {NDEC} 張', 'inactive')], pw))
    for i, c in enumerate(CARDS):
        if not c['steps']:
            entry(i, c)
    rows.append(frame_row([T(f'等你動手 {NACT} 張', 'inactive')], pw))
    for i, c in enumerate(CARDS):
        if c['steps']:
            entry(i, c)
    if st['pane'] == 'detail':
        i = st['focus']
        head = [G('── ', 'inactive'), T(f'第 {i+1} 張的細節 ')]
        rows.append(frame_row(head + [G('─' * (iw - rowlen(head)), 'inactive')], pw))
        for k, v in CARDS[i]['detail']:
            for j, ln in enumerate(wrap(v, iw - 7)):
                rows.append(frame_row([T((k + '   ') if j == 0 else ' ' * 7, 'inactive'), T(ln)], pw))
        rows.append(frame_row([T('附註   ', 'inactive'), T('（空）', 'inactive')], pw))
    rows.append(pane_bottom(pw, st))
    return rows

def pane_for(pw, st):
    p = st['pane']
    if p in ('queue', 'detail'):
        if st['docked']:
            return pane_queue(pw, st)
        return pane_detail(pw, st) if p == 'detail' else []  # 80 欄：清單留在 band 的佇列條
    return {'fleet': pane_fleet, 'spend': pane_spend}[p](pw, st) if p else []

# ---------- flow ----------
A1, A2, A3 = {0: 'a'}, {0: 'a', 1: 'b'}, {0: 'a', 1: 'b', 2: 'b'}
A4 = {**A3, 3: 'a'}
STEPS = [
    dict(title='平常沒事：band 只有一行', key='（等卡片進來）', next=2, mode='idle', pane=None),
    dict(title='卡片進來：那一行變提醒，不搶鍵盤', key='ctrl+x tab', next=3, mode='alert', pane=None),
    dict(title='待決卡：一次一張；寬螢幕右欄列出全部卡', key='a', next=4, mode='card', pane='queue', focus=0, ans={}),
    dict(title='選了就打勾，自動跳到下一張沒選的', key='b', next=5, mode='card', pane='queue', focus=1, ans=A1),
    dict(title='第 3 張：想先看細節', key='v', next=6, mode='card', pane='queue', focus=2, ans=A2),
    dict(title='看細節：接在清單下面（80 欄疊在 band 上），選項留在原位', key='b', next=7, mode='card', pane='detail', focus=2, ans=A2),
    dict(title='等你動手卡：照 1、2 做完再選', key='a（照 1、2 做完之後）', next=8, mode='card', pane='detail', focus=3, ans=A3),
    dict(title='都選好了：送出列亮起來', key='s', next=9, mode='card', pane='detail', focus=3, ans=A4),
    dict(title='送出後的回條：已送出，每張送了什麼', key='7', next=10, mode='receipt', pane=None, ans=A4),
    dict(title='艦隊頁：每個 agent 的 tok/s', key='5', next=11, mode='idle', pane='fleet'),
    dict(title='花費頁：額度和今天用量', key='Esc', next=1, mode='idle', pane='spend'),
]

def caption(W, n, s):
    nxt = f'按 {s["key"]} → 步驟 {s["next"]}' if not s['key'].startswith('（') else f'{s["key"]} → 步驟 {s["next"]}'
    row = [T(f' 設計稿註記 · 步驟 {n}/{len(STEPS)} · ', 'inactive'), T(nxt, 'warning'), T(' · ' + s['title'], 'inactive')]
    return pad(fit(row, W), W)

def compose(W, H, n):
    s = STEPS[n - 1]
    docked = W >= 110
    st = dict(mode=s['mode'], pane=s['pane'], focus=s.get('focus', 0), ans=s.get('ans', {}), docked=docked)
    b = band_card(W, H, st) if st['mode'] == 'card' else band_receipt(W, st) if st['mode'] == 'receipt' else []
    bottom = b + [status_line(W, st)] + prompt(W)
    top_h = H - len(bottom)
    pw = (78 if W >= 200 else 52) if docked else W
    pane = pane_for(pw, st)
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
    else:  # 窄：Pane 疊在 band 正上方，同一個框、同一份內容
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
