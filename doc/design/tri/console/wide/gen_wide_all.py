"""console 寬螢幕全步驟（PM 01M8A18Z N+2）：舊稿 68 步的資料（gen.py 的 S）照寬版面重排。

版面（209x49，操作員實測；宿主上限見 gen_wide.py 開頭）：
- 右側停靠 pane 94 欄（宿主上限），整個高度到 prompt 區上方。
  上半（唯讀）：回覆區放不下而移過來的步驟、固定警告、來龍去脈（待決卡照 R7 收成一列）、艦隊每條產線一列。
  下半（可按）：其他 8 頁每頁一列摘要，前面是跳頁的數字鍵。
- 底部 band 17 列（宿主上限）：左欄清單、右欄回覆區（問題、選項、說明），下面全域狀態列、頁籤列、鍵列。
- 上方對話區 115 欄：宿主對話，唯讀。
120x40、80x24 是退化檢查：沿用已過三輪審查的舊稿版面（out/），不在這裡產生。

用法：python3 gen_wide_all.py <輸出目錄>  → <輸出目錄>/209x49-s<N>/{screen.txt, roles.json[, urls.json]}、flow.json
畫面裡不放任何設計稿註記。
"""
import importlib.util
import json
import os
import sys
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location('cgen', os.path.join(HERE, '..', 'gen.py'))
g = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(g)
v = g.v

W, H = 209, 49
DOCK = 94
LEFT = W - DOCK            # 115
PROMPT_TOP = H - 7         # 42
BAND_TOP = PROMPT_TOP - 17  # 25
DIV = PROMPT_TOP - 4       # 38：清單／回覆兩欄收在這條線
SPLIT = 38                 # 清單欄右邊的直線
BAND_N = DIV - BAND_TOP - 1  # 12：兩欄的內容列數
RW = LEFT - 1 - SPLIT - 3  # 73：回覆區寬
PX, PW = LEFT + 2, DOCK - 4  # pane 內文起點與寬（90）
PANE_TOP_N = BAND_TOP - 2  # 23：pane 上半可用列（第 1–23 列；第 24 列留空）

TABS = g.TABS
CW = lambda ch: 2 if unicodedata.east_asian_width(ch) in 'WF' else 1


def dw(s):
    return sum(CW(c) for c in s)


# 艦隊 12 條產線：和狀態列預設（執行 8、失敗 2、卡住 2）與 1 現況的來龍去脈同一份
LANES = [
    ('●', 'success', 'codeforge', 'ϟ 41 tok/s  ▆▆▄▃▆', 'tux-sync 測試'),
    ('●', 'success', 'autopilot', 'ϟ 37 tok/s  ▄▅▆▆▅', 'band 對比修正'),
    ('●', 'success', 'hangar', 'ϟ 63 tok/s  ▆▇▆▅▆', '盤點 client 尺寸'),
    ('●', 'success', 'fleet', 'ϟ 30 tok/s  ▃▄▄▅▄', 'courier 重試佇列'),
    ('●', 'success', 'hippo', 'ϟ 22 tok/s  ▂▃▃▄▃', '安裝 P1.5'),
    ('●', 'success', 'lab', 'ϟ 18 tok/s  ▂▂▃▂▂', 'gate 掃描'),
    ('●', 'success', 'alfred', 'ϟ 15 tok/s  ▁▂▂▂▃', '部署前檢查'),
    ('●', 'success', 'mnemos', 'ϟ 11 tok/s  ▁▁▂▁▂', '索引重建'),
    ('✗', 'error', 'hangar 換池', '失敗 3 輪', '等 NAS 回應逾時'),
    ('✗', 'error', 'docs 建置', '失敗 1 輪', 'mdbook 版本不符'),
    ('⋯', 'warning', 'pinball 美術', '卡住', '等操作員選版'),
    ('⋯', 'warning', '308 地圖', '卡住', '等素材'),
]

# 「很多」狀態：舊稿清單最後一列是「⋯ 下面還有 N」，要捲才看得到。寬版把這 N 項全部攤開：
# band 左欄放得下的放左欄，其餘移到 pane 上半「清單（續）」。項數照舊稿那一列寫的數字。
D = lambda t: (t, 'inactive')
EXTRA = {
    '2-M': ['  ·8 lab gate 放行', '  ·9 docs 版本對齊', '  ·10 mnemos 索引重建', '  ·11 308 地圖素材', '  ·12 pinball 選版', '  ·13 revival GPU slot', '  ·14 twgame 音效授權'],
    '3-M': [f'  {n}線 {lane} · {eng} · {m} 分' for n, lane, eng, m in zip('己庚辛壬癸子丑寅卯辰巳午未申亥', ['lab', 'alfred', 'mnemos', 'docs', 'fleet', 'hangar', 'codeforge', 'autopilot', 'hippo', 'lab', 'alfred', 'pinball', '308', 'revival', 'twgame'], ['codex', 'claude', 'agy', 'codex', 'claude'] * 3, range(7, 52, 3))]
           + [D('失敗 3 輪'), '  酉線 hangar · codex · 逾時', '  戌線 docs · claude · 建置失敗', '  亥線 lab · agy · private-leaks'],
    '4-M': [f'  {t} {x}' for t, x in [('08:31', '合併 develop codeforge 05e2'), ('08:22', 'codex PASS autopilot e931'), ('08:15', '換池 team → pool-b'), ('08:02', 'agy PASS hangar 5cab'),
                                       ('07:55', '合併 develop autopilot 7d1c'), ('07:41', '退件 grok 第 2 版'), ('07:30', 'codex PASS lab 93fa'), ('07:12', '合併 develop fleet 2b6e'),
                                       ('06:58', '換池 cookys → pool-a'), ('06:40', 'agy PASS docs 44c1'), ('06:21', '合併 develop hippo 81d0'), ('06:05', '退件 codex 第 1 版'),
                                       ('05:50', 'codex PASS mnemos 0f3a'), ('05:31', '合併 develop alfred c2e9'), ('05:10', '換池 lab → pool-c')]],
    '5-M': ['  kimi 席 · 31%', '  glm 席 · 12%', '  agy 席 · 54%', '  codex 席 · 47%', '  fable 席 · 6%'],
    '6-M': ['  worktree tri-fable（5 天沒動）', '  worktree tri-brief（5 天沒動）', '  worktree tri-v2（6 天沒動）', '  分支 fix/shot-lazy（已合併）', '  分支 docs/bl-atat（已合併）',
            '  分支 design/tri-merge（已合併）', '  過期暫存：handoff 4 個', '  過期暫存：scratch 2.1 GB', '  stash WIP band 對比', '  stash WIP r1 rev4', '  archive tag 3 個沒備份'],
    '7-M': [f'  {a}--claude · {b} tok/s' for a, b in [('hippo', 33), ('lab', 29), ('alfred', 27), ('mnemos', 24), ('game-lead', 21), ('recovery-lead', 19), ('ops', 17),
                                                     ('pinball', 15), ('308', 14), ('revival', 12), ('twgame', 11), ('docs', 9)]]
           + [D('失敗 4'), '  codex 派工 · 測試逾時', '  agy 派工 · 認證失敗', '  grok 審查 · 額度用完', '  hangar 換池 · NAS 逾時',
              D('完成 9'), '  docs 建置 · lab gate · fleet 重試', '  hippo 安裝 · alfred 部署 · mnemos 索引', '  codex 審查 · agy 審查 · 換池 pool-c'],
    '8-M': ['  12-07 hippo 第二批', '  12-09 盤點報告', '  12-10 外掛清理', '  12-12 pool 續約檢查', '  12-15 年度備份', '  12-18 r1 重做'],
}
# 2-M 清單上方被捲掉的 3 張（舊稿「⋯ 上面還有 3 張」）：寬版也攤開
ABOVE = {'2-M': ['  ✓1 合併 plan：a', '  ✓2 外掛更新時機：b', '  ✓3 預覽先拍幾欄：a']}
# 9 說明的按鍵表：寬版不捲，拿掉 ↑↓、PgUp/PgDn，補上寬版新增的鍵
KEYS_WIDE = {'↑↓ ＊      捲回覆區（待決以外的頁，原本是換筆）': 'Tab ＊      換到下一頁',
             'PgUp/PgDn ＊ 捲來龍去脈（原本只有待決有）': 'e ＊        展開來龍去脈（待決卡預設收起）'}
NO_SCROLL = ('↑↓', 'PgUp/PgDn')  # 寬版內容都放得下，不用捲

TALK = [
    [('  > ', 'inactive'), ('把 tux-sync 的測試補上，跑完 check-all 再合併', 'text')],
    [],
    [('  ● ', 'success'), ('8 個反例全過；拿掉 symlink 拒絕那段，測試會紅', 'text')],
    [('    ⎿ ', 'inactive'), ('反向探針有效', 'inactive')],
    [],
    [('  ● ', 'success'), ('check-all 結束碼 0，已 --no-ff 合進 develop', 'text')],
    [],
    [('  > ', 'inactive'), ('好，下一張', 'text')],
    [],
    [('  ● ', 'success'), ('第 2 張：check-cjk-safe 的測試少了執行權限', 'text')],
    [('    ⎿ ', 'inactive'), ('補上執行權限，重跑通過', 'inactive')],
]


class Grid:
    def __init__(self):
        self.c = [[[' ', 'text'] for _ in range(W)] for _ in range(H)]

    def put(self, y, x, text, role='text', end=W):
        for ch in text:
            w = CW(ch)
            assert x + w <= end, f'第 {y} 列超出 {end} 欄：{text!r}'
            self.c[y][x] = [ch, role]
            if w == 2:
                self.c[y][x + 1] = [None, role]
            x += w
        return x

    def row(self, y, x, segs, end=W):
        """segs：[(文字, 角色[, 種類])] 或 gen.py 的 T/G 列。"""
        for s in segs:
            x = self.put(y, x, s[0], s[1], end)
        return x

    def box(self, top, left, bottom, right, title, foot=(), role='inactive'):
        self.put(top, left, '╭─', role)
        x = self.row(top, left + 2, title, right)
        self.put(top, x, '─' * (right - x), role)
        self.put(top, right, '╮', role)
        for y in range(top + 1, bottom):
            self.put(y, left, '│', role)
            self.put(y, right, '│', role)
        self.put(bottom, left, '╰─', role)
        x = self.row(bottom, left + 2, foot, right)
        self.put(bottom, x, '─' * (right - x), role)
        self.put(bottom, right, '╯', role)


def segs_of(item):
    t, r = g.seg(item)
    return [(t, r)]


def strip_row(r):
    """gen.py 的列 → 去掉右邊補的空白（寬度由這裡的框決定）。"""
    out = list(r)
    while out and not out[-1][0].strip():
        out.pop()
    if out:
        t = out[-1]
        out[-1] = (t[0].rstrip(), t[1]) + tuple(t[2:])
    return out


def summaries(s):
    """其他頁摘要：數字跟這一步的狀態列同源。"""
    st = s['status']
    dec, act = st.get('dec', '3'), st.get('act', '1')
    fl = st.get('fleet', (8, 2, 2))
    stale = st.get('stale', set())
    fleet = '讀不到' if fl == '—' else f'執行 {fl[0]} · 失敗 {fl[1]} · 卡住 {fl[2]}' + (' · 過時' if 'fleet' in stale else '')
    return {
        1: f'待決 {dec} · 動手 {act} · 殘留 2 項',
        2: '讀不到' if dec == '—' else f'{dec} 張要你選 · {act} 張要先動手',
        3: '執行中 2 輪 · 失敗 1 輪 · 完成 1 輪',
        4: '欠審 1 · 我替你決定的事 4',
        5: '5h 42% · 7d 18% · 今日 380k tokens',
        6: '要你處理 2 · 自動清理 1',
        7: fleet,
        8: '3 張 · 最近到期 11-30',
        9: '狀態圖示與按鍵表',
    }


def compose(n, s):
    G_ = Grid()
    tab = s['tab']
    tabname = f'{tab} {TABS[tab - 1]}'
    # 對話區
    for i, parts in enumerate(TALK):
        G_.row(1 + i, 0, parts, LEFT)

    # ── 回覆區：放不下 12 列時，題目後面的步驟移到 pane 上半，band 留題目與選項 ──
    rows, first_opt = g.render_items(s['reply'], RW)
    moved = []
    if len(rows) > BAND_N:
        assert first_opt is not None, f'{s["id"]}: 回覆 {len(rows)} 列放不下，又沒有選項可留'
        moved = rows[1:first_opt]
        rows = rows[:1] + [[('步驟在右邊，不用捲', 'inactive', 'text')]] + rows[first_opt:]
    assert len(rows) <= BAND_N, f'{s["id"]}: 回覆區 {len(rows)} 列 > {BAND_N}'
    ix = [segs_of(x) for x in s['ix']]
    more = []
    if s['id'] in ABOVE:
        k = next(i for i, r in enumerate(ix) if r[0][0].startswith('⋯ 上面還有'))
        ix[k:k + 1] = [segs_of(x) for x in ABOVE[s['id']]]
    if s['id'] in EXTRA:
        assert ix[-1][0][0].startswith('⋯ 下面還有'), s['id']
        ix.pop()
        extra = [segs_of(x) for x in EXTRA[s['id']]]
        room = BAND_N - len(ix)
        if len(extra) > room:  # 放不下的移到 pane；左欄最後一列告訴你在右邊
            room -= 1
            ix += extra[:room] + [[(f'另 {len(extra) - room} 列在右邊「清單（續）」', 'inactive')]]
            more = extra[room:]
        else:
            ix += extra
    assert len(ix) <= BAND_N, f'{s["id"]}: 清單 {len(ix)} 列'

    # ── 底部 band ──
    bl, br = 0, LEFT - 1
    hints = [h for h in (s['hints'] or g.DEFAULT_HINTS[tab]) if h[0] not in NO_SCROLL]
    if not any(k == 'Tab' for k, _ in hints):
        hints.append(g.H('Tab', '換頁'))
    foot = [(' ', 'inactive')] + [tuple(x[:2]) for x in g.hint_row(hints)] + [(' ', 'inactive')]
    # 標題分兩段：左欄是分頁名，右欄（┬ 之後）是這一步的狀態；[-] 固定在右上角，不和標題重疊
    G_.box(BAND_TOP, bl, PROMPT_TOP - 1, br, [(f' {tabname} · 清單 ', 'text')], foot)
    G_.put(BAND_TOP, SPLIT + 2, f' {s["title"]} ', 'text', br - 6)
    G_.put(BAND_TOP, br - 5, '[-]', 'merged')
    for y in range(BAND_TOP + 1, DIV):
        G_.put(y, SPLIT, '│', 'inactive')
    G_.put(BAND_TOP, SPLIT, '┬', 'inactive')
    G_.put(DIV, bl, '├' + '─' * (SPLIT - bl - 1) + '┴' + '─' * (br - SPLIT - 1) + '┤', 'inactive')
    for i, r in enumerate(ix):
        G_.row(BAND_TOP + 1 + i, 2, r, SPLIT)
    for i, r in enumerate(rows):
        G_.row(BAND_TOP + 1 + i, SPLIT + 2, strip_row(r), br)
    # 狀態列：取舊稿最完整的那一版（給很寬的寬度），右邊的頁籤另成一列，這裡只留左邊的全域狀態。
    # 「卡住 N」改成 ⋯ N（R7：band 裡不出現 卡住），和 pane 艦隊區的 ⋯ 同一個記號
    st = [tuple(x[:2]) for x in g.status_line(400, tab, s['status'])]
    st = strip_row(st[:next(i for i, x in enumerate(st) if x[0].strip() == '1')])
    st = [(t.replace('卡住 ', '⋯ '), r) for t, r in st]
    G_.row(DIV + 1, 1, st, br)
    tabs = []
    for k, name in enumerate(TABS, 1):
        tabs += [(f'{k} {name}', 'warning' if k == tab else 'merged'), ('  ', 'inactive')]
    G_.row(DIV + 2, 2, tabs[:-1], br)

    # ── 右側 pane ──
    dl, dr = LEFT, W - 1
    G_.box(0, dl, PROMPT_TOP - 1, dr, [(f' {tabname} · 詳情 ', 'text')],
           [(' ', 'inactive'), ('數字鍵', 'merged'), (' 跳到那一頁', 'inactive'), (' · ', 'inactive'), ('e', 'merged'), (' 展開來龍去脈 ', 'inactive')])
    top = [strip_row(r) for r in moved]
    if more:
        top += [[('清單（續）', 'inactive')]] + more + [[]]
    top += [segs_of(x) for x in s['pin']]
    detail = g.render_items([KEYS_WIDE.get(x, x) if isinstance(x, str) else x for x in s['detail']], PW)[0]
    if tab == 2 and s['id'] not in ('2-E', '2-X') and any('來源' in ''.join(t[0] for t in r) or '背景' in ''.join(t[0] for t in r) for r in detail):
        # R7：卡的來源、背景、卡住收起，第一眼只看問題、選項、建議
        top.append([(f'來龍去脈 {len(detail)} 列 · 收起', 'inactive')])
    else:
        top += [strip_row(r) for r in detail]
    fl = s['status'].get('fleet', (8, 2, 2))
    if tab != 7 and fl == (8, 2, 2) and len(top) + 2 + len(LANES) <= PANE_TOP_N:
        top += [[], [('艦隊 · 12 條產線 · 執行 8 · 失敗 2 · 卡住 2', 'text')]]
        for mark, role, name, a, b in LANES:
            top.append([('  ', 'text'), (mark, role), (' ' + name + ' ' * (13 - dw(name)), 'inactive'), (a + ' ' * (20 - dw(a)), 'inactive'), (b, 'inactive')])
    assert len(top) <= PANE_TOP_N, f'{s["id"]}: pane 上半 {len(top)} 列 > {PANE_TOP_N}'
    for i, r in enumerate(top):
        G_.row(1 + i, PX, r, dr)
    G_.row(BAND_TOP, PX, [('其他頁', 'inactive')], dr)
    summ = summaries(s)
    y = BAND_TOP + 1
    for k, name in enumerate(TABS, 1):
        if k == tab:
            continue
        G_.row(y, PX, [(f'{k} {name}', 'merged'), ('  ' + summ[k], 'inactive')], dr)
        y += 1

    # ── prompt 區（全寬，沿用舊稿的 statusline 狀態：過時、讀不到）──
    for i, r in enumerate(g.prompt(W, s['quota'], s['status'])):
        G_.row(PROMPT_TOP + i, 0, [tuple(x[:2]) for x in r])
    G_.row(PROMPT_TOP + 4, 2, [('⏵⏵ auto mode on (shift+tab to cycle)', 'inactive')])
    return G_


def need(ch):
    return 'text' if ch.isalnum() or CW(ch) == 2 else 'glyph'


def write(G_, d):
    os.makedirs(d, exist_ok=True)
    lines, roles = [], []
    for row in G_.c:
        lines.append(''.join(c for c, _ in row if c is not None))
        segs = []
        for c, r in row:
            if c is None:
                continue
            k = need(c)
            if segs and segs[-1][1] == r and segs[-1][2] == k:
                segs[-1][0] += c
            else:
                segs.append([c, r, k])
        roles.append(segs)
    open(os.path.join(d, 'screen.txt'), 'w').write('\n'.join(lines) + '\n')
    json.dump(roles, open(os.path.join(d, 'roles.json'), 'w'), ensure_ascii=False)
    if any(g.URL in l for l in lines):
        json.dump([g.URL], open(os.path.join(d, 'urls.json'), 'w'))


def main():
    out = sys.argv[1]
    for n, s in enumerate(g.S, 1):
        write(compose(n, s), os.path.join(out, f'{W}x{H}-s{n}'))
    flow = [dict(step=n, id=s['id'], tab=TABS[s['tab'] - 1], state=s['state'], title=s['title'], key_to_next=s['key'], next=g.NUM[s['next']])
            for n, s in enumerate(g.S, 1)]
    json.dump(flow, open(os.path.join(out, 'flow.json'), 'w'), ensure_ascii=False, indent=1)
    print('ok', len(g.S), 'steps')


if __name__ == '__main__':
    main()
