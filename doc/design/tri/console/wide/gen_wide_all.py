"""console 寬螢幕全步驟（PM 01M8A18Z N+2）：舊稿 68 步的資料（gen.py 的 S）照寬版面重排。

版面（209x49，操作員實測；宿主上限見 gen_wide.py 開頭）：
- 右側停靠 pane 94 欄（宿主上限，含第 115 欄宿主拖拉條，內容 93 欄），整個高度到 prompt 區上方。
  上半（唯讀）：回覆區放不下而移過來的步驟、固定警告、來龍去脈（待決卡照 R7 收成一列）、艦隊每條產線一列。
  下半（可按）：其他 8 頁每頁一列摘要，前面是跳頁的數字鍵。
- 底部 band 17 列（宿主上限）：左欄清單、右欄回覆區（問題、選項、說明），下面全域狀態列、頁籤列、鍵列。
- 上方對話區、底部 band 寬 110 欄（主欄 115 欄，右側 5 欄留給宿主；實機量測）：宿主對話，唯讀。
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
LEFT = W - DOCK            # 115：宿主主欄（對話與 band）的 viewport 寬
# 實機量測（CC 2.1.332，209x49 tmux，PM 裁定 A 6193db92）：主欄右側留 5 欄給宿主，band 實際可用 110 欄（0–109）；
# pane 的 94 欄含第 115 欄的宿主拖拉條，內容 93 欄（116–208）
BANDW = LEFT - 5           # 110
HOSTBAR = LEFT             # 115：宿主畫的拖拉條
PANE0 = LEFT + 1           # 116：pane 內容第一欄
PROMPT_TOP = H - 7         # 42
BAND_TOP = PROMPT_TOP - 17  # 25
DIV = PROMPT_TOP - 4       # 38：清單／回覆兩欄收在這條線
SPLIT = 36                 # 清單欄右邊的直線
BAND_N = DIV - BAND_TOP - 1  # 12：兩欄的內容列數
RW = BANDW - 1 - SPLIT - 3  # 70：回覆區寬
PX, PW = PANE0 + 2, W - PANE0 - 4  # pane 內文起點與寬（118、89）
PANE_TOP_N = 30  # pane 上半（唯讀）第 1–30 列；第 31 列留空
OTHERS_TOP = PROMPT_TOP - 1 - 9  # 32：下半「其他頁」標題 + 8 列摘要，貼齊 pane 底（R1：可按的在下半）

TABS = g.TABS
CW = lambda ch: 2 if unicodedata.east_asian_width(ch) in 'WF' else 1


def dw(s):
    return sum(CW(c) for c in s)


# 艦隊 12 條產線：和狀態列預設（執行 8、失敗 2、卡住 2）與 1 現況的來龍去脈同一份
LANES = [
    ('◉', 'success', 'codeforge', 'ϟ 41 tok/s  ▆▆▄▃▆', 'tux-sync 測試'),
    ('◉', 'success', 'autopilot', 'ϟ 37 tok/s  ▄▅▆▆▅', 'band 對比修正'),
    ('◉', 'success', 'hangar', 'ϟ 63 tok/s  ▆▇▆▅▆', '盤點 client 尺寸'),
    ('◉', 'success', 'fleet', 'ϟ 30 tok/s  ▃▄▄▅▄', 'courier 重試佇列'),
    ('◉', 'success', 'hippo', 'ϟ 22 tok/s  ▂▃▃▄▃', '安裝 P1.5'),
    ('◉', 'success', 'lab', 'ϟ 18 tok/s  ▂▂▃▂▂', 'gate 掃描'),
    ('◉', 'success', 'alfred', 'ϟ 15 tok/s  ▁▂▂▂▃', '部署前檢查'),
    ('◉', 'success', 'mnemos', 'ϟ 11 tok/s  ▁▁▂▁▂', '索引重建'),
    ('✗', 'error', 'hangar 換池', '失敗 3 輪', '等 NAS 回應逾時'),
    ('✗', 'error', 'docs 建置', '失敗 1 輪', 'mdbook 版本不符'),
    ('⋯', 'warning', 'pinball 美術', '卡住', '等操作員選版'),
    ('⋯', 'warning', '308 地圖', '卡住', '等素材'),
]

# 每條產線最近一件事和下一步（7 艦隊頁攤開用；PM 01M8A52M）
LANE_EV = {
    'codeforge': ('09:38 check-all 綠，合 develop', '補 tux-sync 反例'),
    'autopilot': ('09:35 band 淺色對比 4.7', '等 codeforge 審查'),
    'hangar': ('09:40 量到 client 209x51', '交實拍單 01M825HR'),
    'fleet': ('09:31 重試佇列清到 0', 'courier 逾時改 30 秒'),
    'hippo': ('09:29 P1.5 裝好 3/5 台', '裝剩下 2 台'),
    'lab': ('09:26 gate 掃完 41 個 repo', '寫掃描報告'),
    'alfred': ('09:20 部署前檢查 7/9 過', '修 2 項設定'),
    'mnemos': ('09:18 索引重建 62%', '跑完驗查詢'),
    'hangar 換池': ('09:22 NAS stat 逾時 600 秒', '第 4 輪前先查 NAS'),
    'docs 建置': ('09:05 mdbook 0.5 不認新語法', '釘 mdbook 版本'),
    'pinball 美術': ('昨天 18:10 交 3 版封面', '等你選版'),
    '308 地圖': ('昨天 16:40 缺第 3 區素材', '等素材進來'),
}
# 各頁自己的歷史清單：放在資料之後填滿 pane 上半，只記已經結束的事，不和這一步的計數衝突
HIST = {
    1: ('今天結束的事', ['09:38 codeforge 合進 develop', '09:12 autopilot 審查通過', '08:55 hangar 盤點完成', '08:31 待決 2 張已送出',
                        '08:15 換池 team → pool-b', '07:58 lab gate 放行 1 個 repo', '07:41 docs 第 2 版退件', '07:12 fleet 合進 develop']),
    2: ('今天已決的卡', ['08:31 合併 plan → a（照建議）', '08:31 外掛更新時機 → b', '08:02 預覽先拍幾欄 → a（照建議）', '07:40 hippo 安裝批次 → a',
                        '07:15 mnemos 匯入時段 → c（附註：晚上再跑）', '06:50 alfred 上線日 → a（照建議）']),
    3: ('今天結束的輪次', ['09:05 codex · codeforge tux-sync · 交件', '08:40 agy · hangar 盤點 · 交件', '08:12 claude · autopilot band · 交件',
                          '07:55 codex · lab gate · 逾時（已重派）', '07:30 claude · docs 版本 · 交件', '07:02 agy · fleet 重試 · 交件']),
    4: ('今天的審查紀錄', [f'{t} {x}' for t, x in [('08:31', '合併 develop codeforge 05e2'), ('08:22', 'codex PASS autopilot e931'), ('08:02', 'agy PASS hangar 5cab'),
                                                ('07:55', '合併 develop autopilot 7d1c'), ('07:41', '退件 grok 第 2 版'), ('07:30', 'codex PASS lab 93fa'), ('07:12', '合併 develop fleet 2b6e')]]),
    5: ('今天各席用量', ['codeforge · 92k tokens · 5h 的 10%', 'autopilot · 81k tokens · 9%', 'hangar · 64k tokens · 7%', 'fleet · 48k tokens · 5%',
                        'hippo · 37k tokens · 4%', 'lab · 29k tokens · 3%', 'alfred · 18k tokens · 2%', 'mnemos · 11k tokens · 1%']),
    6: ('最近自動清理的', ['昨天 23:10 worktree tri-old（已合併）', '昨天 23:10 分支 fix/band-rgb（已合併）', '昨天 03:00 過期暫存 1.4 GB',
                          '11-24 23:10 worktree pane-b2（已合併）', '11-24 03:00 handoff 6 個（7 天前）']),
    7: ('今天結束的產線', ['09:05 codeforge · tux-sync 交件', '08:40 hangar · 盤點交件', '08:12 autopilot · band 交件', '07:30 docs · 版本交件',
                          '07:02 fleet · 重試交件', '06:41 hippo · P1.4 交件']),
    8: ('最近完成的待辦', ['11-25 盤點 client 尺寸', '11-24 外掛版本對齊', '11-23 r1 rev4 示意', '11-22 v7.6 第 1 批', '11-21 band 對比']),
    9: ('顏色怎麼讀', ['綠 ◉ 正常、執行中', '紅 ✗ ⊘ 失敗、讀不到', '黃 ~ ⋯ 過時、在等人', '紫 可按的鍵', '灰 說明文字，不能按']),
}
# 本機事件紀錄：最後一塊，補滿 pane 上半剩下的列（來源是本機，不受 pm／console 讀不到影響）
EVENTS = [f'{t} {x}' for t, x in [('09:41', 'pm status 更新'), ('09:40', 'hangar 回報 client 尺寸'), ('09:38', 'codeforge check-all 綠'), ('09:35', 'autopilot 送審'),
                                  ('09:31', 'fleet 重試佇列清空'), ('09:29', 'hippo 裝好第 3 台'), ('09:26', 'lab gate 掃描完'), ('09:22', 'hangar 換池第 3 輪逾時'),
                                  ('09:20', 'alfred 檢查 7/9'), ('09:18', 'mnemos 索引 62%'), ('09:12', 'autopilot 審查通過'), ('09:05', 'docs 建置失敗'),
                                  ('09:01', 'codex 席額度 47%'), ('08:55', 'hangar 盤點完成'), ('08:40', 'agy 交件 hangar'), ('08:31', '待決 2 張送出'),
                                  ('08:22', 'codex PASS autopilot'), ('08:15', '換池 team → pool-b'), ('08:02', 'agy PASS hangar'), ('07:58', 'lab gate 放行'),
                                  ('07:41', 'docs 第 2 版退件'), ('07:30', 'codex PASS lab'), ('07:12', 'fleet 合進 develop'), ('07:02', 'agy 交件 fleet'),
                                  ('06:50', 'alfred 上線日已決'), ('06:41', 'hippo P1.4 交件'), ('06:21', 'hippo 合進 develop'), ('06:05', 'codex 第 1 版退件')]]
# 7 艦隊頁：band 清單改列 12 條產線（和狀態列的 執行 8 · 失敗 2 · 卡住 2 同一份；舊稿只列 3 條）
FLEET12 = {'7-N': 'codeforge', '7-L': 'hangar 換池', '7-S': 'codeforge', '7-X': 'codeforge', 'C2': 'codeforge'}


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

    def row_clip(self, y, x, segs, end=W):
        """和 row 一樣，但放不下時跟實作（AP wide/band.ts clipTo）同規則截斷：留到 寬度−1，補「…」，「…」沿用被截那段的角色。"""
        if sum(dw(s[0]) for s in segs) <= end - x:
            return self.row(y, x, segs, end)
        room = end - x - 1
        for s in segs:
            text, role = s[0], s[1]
            keep = ''
            for ch in text:
                if dw(keep) + CW(ch) > room:
                    break
                keep += ch
            x = self.put(y, x, keep, role, end)
            room -= dw(keep)
            if len(keep) < len(text):
                return self.put(y, x, '…', role, end)
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


def lane_row(mark, role, name, a, b, speed=True, wide=False):
    if not speed and a.startswith('ϟ'):
        a = 'ϟ —'
    r = [('  ', 'text'), (mark, role), (' ' + name + ' ' * (13 - dw(name)), 'inactive'), (a + ' ' * (20 - dw(a)), 'inactive')]
    if wide:  # 90 欄：名稱 14 · 速度 18 · 最近一件事 32 · 下一步
        ev, nx = LANE_EV[name]
        r[3] = (a + ' ' * (18 - dw(a)), 'inactive')
        return r + [(ev + ' ' * (32 - dw(ev)), 'inactive'), ('→ ' + nx, 'text')]
    return r + [(b, 'inactive')]


def fill(top, blocks):
    """blocks：[(標題, 列, 可截)]，依序放進 pane 上半；可截的塊放到剩下的列為止（至少 3 列才放）。"""
    for head, rows, cut in blocks:
        room = PANE_TOP_N - len(top) - (2 if top else 1)
        if not rows or room < (1 if cut else len(rows)):
            continue
        rows = rows[:room]
        if top:
            top.append([])
        top.append([(head(len(rows)) if callable(head) else head, 'text')])
        top += rows
    return top


def compose(n, s):
    G_ = Grid()
    tab = s['tab']
    tabname = f'{tab} {TABS[tab - 1]}'
    # 對話區
    for i, parts in enumerate(TALK):
        G_.row(1 + i, 0, parts, BANDW)

    # ── 回覆區：放不下 12 列時，題目後面的步驟移到 pane 上半，band 留題目與選項 ──
    rows, first_opt = g.render_items(s['reply'], RW)
    moved = []
    if len(rows) > BAND_N:
        assert first_opt is not None, f'{s["id"]}: 回覆 {len(rows)} 列放不下，又沒有選項可留'
        moved = rows[1:first_opt]
        rows = rows[:1] + [[('步驟在右邊，不用捲', 'inactive', 'text')]] + rows[first_opt:]
    assert len(rows) <= BAND_N, f'{s["id"]}: 回覆區 {len(rows)} 列 > {BAND_N}'
    ix = [segs_of(x) for x in s['ix']]
    speed = s['status'].get('speed') != '—'
    if s['id'] in FLEET12:
        ix = [[('▸ ' if name == FLEET12[s['id']] else '  ', 'text'), (mark, role),
               (f' {name} · ' + (('ϟ —' if not speed else a.split('  ')[0]) if mark == '◉' else a.replace('卡住', '等人')), 'text')]
              for mark, role, name, a, b in LANES]
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
    bl, br = 0, BANDW - 1
    hints = [h for h in (s['hints'] or g.DEFAULT_HINTS[tab]) if h[0] not in NO_SCROLL]
    # 缺源而且清單沒有可選的列：j/k、o 按了沒反應，提示列就不列（和實作一致，PM 01M8C4DY B 類）
    if s['state'].startswith('X') and not any('▸' in t for row in ix for t, *_ in row):
        hints = [h for h in hints if h[0] not in ('j/k', 'o')]
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
        G_.row_clip(BAND_TOP + 1 + i, 2, r, SPLIT)
    for i, r in enumerate(rows):
        G_.row_clip(BAND_TOP + 1 + i, SPLIT + 2, strip_row(r), br)
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
    dl, dr = PANE0, W - 1
    for y in range(0, PROMPT_TOP):
        G_.put(y, HOSTBAR, '│', 'inactive')  # 宿主的拖拉條，不是 mod 畫的
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
    stale = ' · 11 分鐘前的資料' if 'fleet' in s['status'].get('stale', set()) else ''
    blocks = []
    if fl == (8, 2, 2):
        wide = tab == 7
        head = '12 條產線 · 最近一件事 → 下一步' if wide else '艦隊 · 12 條產線 · 執行 8 · 失敗 2 · 卡住 2'
        blocks.append((head + stale, [lane_row(*l, speed=speed, wide=wide) for l in LANES], False))
    # 艦隊沒有失敗、卡住的狀態（1-E、7-E），歷史和事件也不放失敗、逾時、退件，免得和「全部正常」打架
    calm = isinstance(fl, tuple) and fl[1] == fl[2] == 0
    ok = (lambda x: not any(w in x for w in ('失敗', '逾時', '退件'))) if calm else (lambda x: True)
    if s['state'] not in ('X', 'X2'):  # 讀不到時，這一頁自己的歷史也讀不到，不放
        h, rows = HIST[tab]
        blocks.append((lambda k, h=h: f'{h} · 最近 {k} 筆', [[('  ' + x, 'inactive')] for x in rows if ok(x)], True))
    blocks.append((lambda k: f'本機事件 · 最近 {k} 筆', [[('  ' + x, 'inactive')] for x in EVENTS if ok(x)], True))
    assert len(top) <= PANE_TOP_N, f'{s["id"]}: pane 上半 {len(top)} 列 > {PANE_TOP_N}'
    top = fill(top, blocks)
    for i, r in enumerate(top):
        G_.row_clip(1 + i, PX, r, dr)
    G_.row(OTHERS_TOP, PX, [('其他頁', 'inactive')], dr)
    summ = summaries(s)
    y = OTHERS_TOP + 1
    for k, name in enumerate(TABS, 1):
        if k == tab:
            continue
        G_.row_clip(y, PX, [(f'{k} {name}', 'merged'), ('  ' + summ[k], 'inactive')], dr)
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
