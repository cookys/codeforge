#!/usr/bin/env python3
"""R1 rev4 靜態示意產生器。畫面網格用資料結構定義，HTML 與檢查都從同一份網格產出。
用法：python3 gen.py   （輸出 r1-rev4-mockup.html、checks.txt 到本檔所在目錄）
資料來源（只讀）：
  寬度規則  autopilot mods/tux/width.ts（cellWidth 範圍照抄）
  主題 RGB  autopilot feat/tui-width-contrast:mods/tux/themes-cc.json（CC 2.1.331 深淺主題）
  角色對鍵  autopilot feat/tui-width-contrast:mods/tux/palette.ts（A1 重映射後）
  門檻      同分支 mods/tux/contrast.test.js（一般 4.5、dim 類 3、accent 類 3 且文字須粗體、SGR2 dim 3）
"""
import html, os, re, sys
from dataclasses import dataclass, replace

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------- 寬度（照 width.ts）
def cell_width(c):
    if c == 0 or 0x300 <= c <= 0x36f or 0x200b <= c <= 0x200f or 0xfe00 <= c <= 0xfe0f or 0x20d0 <= c <= 0x20ff:
        return 0
    wide = [(0x1100,0x115f),(0x2e80,0x303e),(0x3041,0x33ff),(0x3400,0x4dbf),(0x4e00,0x9fff),(0xa000,0xa4cf),
            (0xac00,0xd7a3),(0xf900,0xfaff),(0xfe30,0xfe4f),(0xff00,0xff60),(0xffe0,0xffe6),
            (0x1f300,0x1f64f),(0x1f900,0x1f9ff),(0x20000,0x3fffd)]
    for a, b in wide:
        if a <= c <= b:
            return 2
    return 1

def dw(s):
    return sum(cell_width(ord(ch)) for ch in s)

# ---------------------------------------------------------------- 主題與角色
THEMES = {
  "background": {"dark": [0,0,0], "light": [255,255,255]},
  "dark":  {"claude":[215,119,87],"warning":[255,193,7],"error":[255,107,128],"success":[78,186,101],"suggestion":[177,185,249],"inactive":[153,153,153],"subtle":[80,80,80],"merged":[175,135,255],"remember":[177,185,249],"text":[255,255,255]},
  "light": {"claude":[215,119,87],"warning":[150,108,30],"error":[171,43,63],"success":[44,122,57],"suggestion":[87,105,247],"inactive":[102,102,102],"subtle":[175,175,175],"merged":[135,0,255],"remember":[0,0,255],"text":[0,0,0]},
}
ROLE_KEY_NEW = {"attention":"warning","here":"claude","broken":"error","done":"success","running":"suggestion","idle":"inactive","structure":"inactive","action":"remember","stale":"inactive","overdue":"error","waiting_on":"inactive","needs_human":"merged","link":"suggestion","text":"text","host":"inactive"}
ROLE_KEY_OLD = dict(ROLE_KEY_NEW, structure="subtle", waiting_on="subtle")   # A1 重映射前（develop 現行）
DIM_ROLES = {"idle","stale","structure","waiting_on","host"}
ACCENT_ROLES = {"here","running","link"}

def lum(rgb):
    v = [x/255 for x in rgb]
    v = [x/12.92 if x <= 0.04045 else ((x+0.055)/1.055)**2.4 for x in v]
    return 0.2126*v[0]+0.7152*v[1]+0.0722*v[2]
def contrast(a, b):
    la, lb = lum(a), lum(b)
    return (max(la,lb)+0.05)/(min(la,lb)+0.05)
def mix(a, b, t):
    return [round(x+(y-x)*t) for x, y in zip(a, b)]
def role_rgb(theme, role, keymap):
    if role == "dim":   # SGR 2：主題文字色對背景各一半
        return mix(THEMES[theme]["text"], THEMES["background"][theme], 0.5)
    return THEMES[theme][keymap[role]]
def bar_of(role):
    if role == "dim" or role in DIM_ROLES or role in ACCENT_ROLES:
        return 3.0
    return 4.5

# ---------------------------------------------------------------- 網格資料
@dataclass
class S:
    t: str
    role: str = "text"
    press: str = None
    b: bool = False

def T(t, role="text", press=None, b=False):
    return S(t, role, press, b)

def rw(row):
    return sum(dw(s.t) for s in row)

def pad(row, n):
    d = n - rw(row)
    return row + [T(" " * d)] if d > 0 else row

def cut(s, w):
    """計畫 3.2 允許的截斷：依顯示寬度，右截加 …"""
    if dw(s) <= w:
        return s
    out = ""
    for ch in s:
        if dw(out + ch) > w - 1:
            break
        out += ch
    return out + "…"

@dataclass
class Fig:
    id: str
    title: str          # 圖上方：尺寸＋頁名
    W: int
    rows: list
    note: str = ""      # 模式／階
    kind: str = "pane"

    @property
    def H(self):
        return len(self.rows)

# ---------------------------------------------------------------- 邊框
def top_row(Wf, title, bold=True):
    d = Wf - 5 - dw(title)
    return [T("╭─", "structure"), T(" "), T(title, b=bold), T(" "), T("─" * d, "structure"), T("╮", "structure"), T(" "), T("[-]", "host")]

def mid(inner, Wf):
    return [T("│", "structure"), T(" ")] + pad(inner, Wf - 4) + [T(" "), T("│", "structure"), T(" " * 4)]

def bottom_row(Wf, hints, tabs, active):
    """照 frame.ts frameBottomFit：完整提示＋全名 → 短提示＋全名（至少 8 個橫線）→ 數字頁籤（目前頁保留名稱）→ 無提示"""
    full = [f"{k} {n}" for k, n in tabs]
    digits = [f"{k} {n}" if k == active else k for k, n in tabs]
    tw = lambda L: sum(dw(x) for x in L) + 2 * (len(L) - 1)
    cand = []
    if hints:
        cand.append((hints[0], full, 2))
        if len(hints) > 1:
            cand.append((hints[-1], full, 8))
        for h in hints:
            cand.append((h, digits, 2))
    cand.append(("", digits, 2))
    for h, tl, mind in cand:
        chrome = 12 if h else 8
        d = Wf - chrome - dw(h) - tw(tl)
        if d >= mind:
            break
    else:
        raise SystemExit("底線放不下")
    row = [T("╰─", "structure")]
    if h:
        row += [T("  "), T(h), T("  ")]
    row += [T("─" * d, "structure"), T("  ")]
    for i, ((k, n), txt) in enumerate(zip(tabs, tl)):
        if i:
            row.append(T("  "))
        on = (k == active)
        row.append(T(txt, "here" if on else "text", press=f"tab:{k}", b=on))
    row += [T("  "), T("─╯", "structure"), T(" " * 4)]
    return row

TABS = [("1","現況"),("2","待決"),("3","派工"),("4","審查"),("5","花費"),("6","殘留"),("7","艦隊"),("8","待辦"),("9","說明"),("0","寵物"),("_","帳號")]

# ---------------------------------------------------------------- 範例資料（全是示意，非真資料）
CARDS = [
  ("d-3a01", "合併 plan", "要不要把 shared-tui plan 合進 develop？", "合併"),
  ("d-3a02", "外掛更新時機", "forge-stats 外掛今晚還是明早更新？", "明早"),
  ("d-3a03", "預覽先拍幾欄", "預覽先拍 80 欄還是 120 欄？", "120 欄"),
  ("d-3a04", "驗收長卡", "這張卡的背景有八十行，用來驗證 PgDn 的步長", "收到"),
  ("d-3a05", "驗收測試卡", "這是驗收腳本的測試卡", "收到"),
  ("d-3a06", "捲動測試卡 01", "捲動測試 01：這張卡用來湊滿 6 張，選哪個？", "合併"),
]
CUR = 2          # 第 3 張
OPTS = [("a", "80 欄", False), ("b", "120 欄", True)]
BG_ALL = [f"第 {n:02d} 行：背景示意文字，用 PgUp/PgDn 捲動這一區。" for n in range(1, 31)]
AWAIT = [
  "等你動手 1 張 · d-1de9a5e9 接回 gentoo 的電源並登入",
  "  1. 到機房把 gentoo 的電源線插回 UPS（等你插電）",
  "  2. 開機後在 console 登入一次，再回來這裡看結果（等你登入）",
]
CTRL_ROLE_NOTE = {}

# ---------------------------------------------------------------- 待決頁區塊
def r_heading(iw):
    return [T("待決 6 張 · 第 3 張", b=True)]

def r_await(collapse=False):
    if collapse:
        return [[T("等你動手 1 張", "dim")]]
    return [[T(AWAIT[0], "dim")], [T(AWAIT[1])], [T(AWAIT[2])]]

def r_rec():
    return [T("建議", "dim"), T("   "), T("b 120 欄")]

def r_title(iw):
    c = CARDS[CUR]
    return [T("‣ ", "here"), T(f" {CUR+1}. {c[1]}", "here", b=True), T(f"：{c[2]}")]

def bg_rows(n_rows, width, scroll=True):
    """背景視窗：捲到底，顯示最後 n_rows 列；回傳 (列, 被截掉的列數)"""
    win = BG_ALL[-n_rows:]
    out = []
    for i, line in enumerate(win):
        lead = [T("背景", "dim"), T("   ")] if i == 0 else [T(" " * 7)]
        row = pad(lead + [T(line)], width - 1)
        if scroll:
            row.append(T("█" if i >= n_rows - 2 else "│", "here" if i >= n_rows - 2 else "structure"))
        out.append(row)
    return out, len(BG_ALL) - n_rows

def r_more(n):
    return [T(f"↑ 上面還有 {n} 列", "dim")]

def info_single(iw, n):
    """資訊區（單欄）由上往下：分類標題、等你動手、↑、背景、建議、▸ 標題。回傳 n 列。"""
    fixed = 1 + 3 + 1 + 1          # heading + await3 + rec + title
    rows = [r_heading(iw)]
    if n >= fixed + 1 + 1:          # 放得下 ↑ 與至少 1 列背景
        rows += r_await()
        k = n - fixed - 1
        bg, left = bg_rows(k, iw)
        rows.append(r_more(left))
        rows += bg
    elif n >= fixed + 1:            # 背景整個被截，只剩 ↑
        rows += r_await()
        rows.append(r_more(len(BG_ALL)))
    else:
        raise SystemExit("資訊區太小")
    rows += [r_rec(), r_title(iw)]
    assert len(rows) == n, (len(rows), n)
    return rows

def info_dual(iw, n):
    """boxed 雙欄：標題＋等你動手橫跨，下面左清單（不可點）右詳情。"""
    sep = 3
    usable = iw - sep
    lw = usable // 2
    rw_ = usable - lw
    rows = [r_heading(iw)] + r_await()
    m = n - len(rows)
    left = []
    for i, c in enumerate(CARDS):
        if i == CUR:
            left.append([T("‣ ", "here"), T(f" {i+1}. {c[1]}", "here", b=True), T(f"：{c[2]}")])
        else:
            left.append([T("   "), T(f"{i+1}. {c[1]}：{c[2]}")])
    left = (left + [[T("")]] * m)[:m]
    right = [r_more(len(BG_ALL) - (m - 3))]
    bg, _ = bg_rows(m - 3, rw_)
    right += bg + [r_rec(), r_title(rw_)]
    assert len(right) == m
    for a, b in zip(left, right):
        rows.append(pad(a, lw) + [T(" "), T("│", "structure"), T(" ")] + pad(b, rw_))
    assert len(rows) == n
    return rows

# ---------------------------------------------------------------- 控制區（每階）
def card_rows(lo, hi):
    out = []
    for i in range(lo, hi + 1):
        c = CARDS[i]
        if i == CUR:
            out.append([T("‣ ", "here", press=f"key:card:{c[0]}", b=False), T(f" {i+1}. {c[1]}", "here", press=f"key:card:{c[0]}", b=True)])
        else:
            out.append([T("   ", press=f"key:card:{c[0]}"), T(f" {i+1}. {c[1]}", press=f"key:card:{c[0]}")])
    return out

def opt_rows():
    out = []
    for k, label, rec in OPTS:
        row = [T(f"{k} {label}", press=f"key:{k}")]
        if rec:
            row.append(T(" ✧", "attention", press=f"key:{k}"))
        out.append(row)
    return out

def note_row():
    return [T("m 附註：", press="key:m"), T("（按 m 寫）", "dim", press="key:m")]

def send_row():
    return [T("[s] 送出已選 0 張 · 另 6 張未選", press="key:s")]

def pn_row():
    return [T("‹ ", press="key:p"), T("p", press="key:p"), T(" 3/6 "), T("n", press="key:n"), T(" ›", press="key:n")]

def ctrl_L0():       # 5 + 2 + 1 + 1 = 9
    return card_rows(0, 4) + opt_rows() + [note_row(), send_row()]
def ctrl_L0_dual():  # 換卡器 1 列
    return [pn_row()] + opt_rows() + [note_row(), send_row()]
def ctrl_L2():
    return [pn_row()] + opt_rows() + [[T("m 附註", press="key:m"), T(" · "), T("[s] 送出 0 張", press="key:s")]]
def ctrl_L4():
    return [
        [T("a 80 欄", press="key:a"), T(" · "), T("b 120 欄", press="key:b"), T(" ✧", "attention", press="key:b")],
        [T("p", press="key:p"), T("/"), T("n", press="key:n"), T(" 3/6 · "), T("m 附註", press="key:m"), T(" · "), T("[s] 送出 0 張", press="key:s")],
    ]

HINT_DECIDE = ["PgUp/PgDn 捲背景 · p/n 換張 · a/b 選 · m 附註 · s 送出 · 鍵盤在 pane · Esc", "p/n · a/b · m · s · Esc"]
TITLE_DECIDE = "待決 · 資料 fleet decisions list --json（schema fleet.decisions v1）· 10 秒"

def build_pane(Wf, info, ctrl, title, hints, tabs, active):
    rows = [top_row(Wf, title)]
    for r in info + ctrl:
        rows.append(mid(r, Wf))
    rows.append(bottom_row(Wf, hints, tabs, active))
    return rows

def decide_boxed(W, H, level):
    Wf = W - 5
    c = {"L0d": ctrl_L0_dual, "L2": ctrl_L2, "L4": ctrl_L4}[level]()
    n = H - 2 - len(c)
    if level == "L4":
        title = "待決 6 · 第 3 張 · " + CARDS[CUR][1]
        title = cut(title, Wf - 6)
        info = [[T(cut(CARDS[CUR][2], Wf - 4))]]
        assert n == 1
    else:
        title = TITLE_DECIDE
        info = info_dual(Wf - 4, n) if level == "L0d" else info_single(Wf - 4, n)
    return build_pane(Wf, info, c, title, HINT_DECIDE if level != "L4" else ["p/n · a/b · m · s · Esc"], TABS, "2")

def decide_docked(H):
    Wf = 76
    c = ctrl_L0()
    n = H - 2 - len(c)
    return build_pane(Wf, info_single(Wf - 4, n), c, "待決 · 資料 fleet decisions list --json · 10 秒", HINT_DECIDE, TABS, "2")

# ---------------------------------------------------------------- forge-stats 頁籤（live pane 的頁）
def kv(k, *segs):
    return [T(k + " " * (8 - dw(k)), "dim")] + list(segs)

def meter(frac, n):
    on = round(frac * n)
    return [T("▰" * on), T("▱" * (n - on), "idle")]

def pet_info(icon):
    return [
        kv("名字", T(icon + " "), T("小墨", b=True), T("  Lv.7")),
        kv("村落", T("鍛造村"), T(" · zh-TW", "dim")),
        kv("XP", *meter(0.62, 24), T("  62%"), T("  310/500", "dim")),
        kv("屬性", T("ATK ", "dim"), T("14"), T(" · ", "structure"), T("HP ", "dim"), T("42"), T(" · ", "structure"), T("DEF ", "dim"), T("9"),
           T(" · ", "structure"), T("SUP ", "dim"), T("11"), T(" · ", "structure"), T("VER ", "dim"), T("6")),
        kv("策略", T("穩健")),
        kv("說", T("今天的測試都綠了。")),
        [T("來源 forge-stats.panel · 12 秒前", "dim")],
    ]

def spend_info():
    return [
        [T("額度 依池", b=True)],
        [T("池      7 天   5 時   重置      預測", "dim")],
        [T("pool-a  92%    64%    2天3時    "), T("約 9 時後用完", "attention")],
        [T("pool-b  55%    12%    4天1時    會用到 80%")],
        [T("pool-c  31%     8%    1天4時    浪費 39%")],
        [T(" ")],
        [T("今日花費 ", b=True), T("只算 token，不顯示金額", "dim")],
        [T("本機今日  1.8M tok")],
        [T("  opus      1.2M tok · 38 次")],
        [T("  sonnet    520k tok · 61 次")],
        [T("  haiku      94k tok · 40 次")],
        [T(" ")],
        [T("近 24 小時 走勢", b=True)],
        [T("▁▂▃▅▆▇█▆▅▃▂▁▁▂▃▅▆▇▆▅▄▃▂▁")],
        [T("一格一小時，最右是這個小時", "dim")],
        [T(" ")],
        [T("本席速度（來源 turn.step）", b=True)],
        [T("最新 42 tok/s · 最後請求 3 分前")],
        [T("平均 38 · 最佳 61 tok/s · 25 次請求")],
        [T("來源 fleet seats --quota · 2 分前（>10 分過期）", "dim")],
    ]

def accounts_info():
    h = lambda s: [T(s, "dim")]
    return [
        [T("帳號 4 個 · 目標用到 70%", b=True)],
        [T("會不夠 估", "attention", b=False), T(" · pool-a · 週 92% · 約 9 時後用完")],
        [T("會浪費", "attention"), T(" · pool-c · 週 31% · 浪費 39% · 目標 70%")],
        [T(" ")],
        h("帳號    週    重置     5時  預測           主機          資料"),
        [T("pool-a  92%  ◷2天3時  64%  約 9 時後用完  host-1,host-2  2 分前")],
        [T("pool-b  55%  ◷4天1時  12%  會用到 80%     host-3         1 分前")],
        [T("pool-c  31%  ◷1天4時   8%  浪費 39%       host-4 舊      6 分前")],
        [T("pool-d  無來源 · 帳號沒有窗口資料        host-5         —", "idle")],
        [T(" ")],
        [T("資料 fleet-quota.json · 2 分前 · 略過 1 份", "dim")],
    ]

def page_pane(title, info, M, active, hints):
    Wf = 76
    room = M - 2
    if len(info) > room:
        extra = len(info) - (room - 1)
        info = info[:room - 1] + [[T(f"下方還有 {extra} 行", "dim")]]
    return build_pane(Wf, info, [], title, hints, TABS, active)

# ---------------------------------------------------------------- band（單列）
BW = dict(line=26, source=16, pend=10, qc=8, tps=14, spark=6, pet=12, toggle=6, sep=3)

def c3(n):
    return f"{n:>3}"

def band_segments(expanded):
    line = [T("3 "), T("產線"), T(" ")]
    items = [("⊘", 1, "broken"), ("◉", 3, None), ("✗", 0, None), ("⋯", 2, None)]
    line = [T("3 產線 ", press="page:dispatch")]
    for i, (g, v, r) in enumerate(items):
        if i:
            line.append(T(" ", press="page:dispatch"))
        line.append(T(g + c3(v), r or "text", press="page:dispatch"))
    segs = {
      "line": line,
      "source": [T("6 來源 開發 ⚑" + c3(3), press="page:hygiene")],
      "pend": [T("2 待決 ", press="page:decide"), T(c3(6), "needs_human", press="page:decide")],
      "qc": [T("4 QC ✓  ", press="page:review")],
      "tps": [T(("5 ϟ " + c3(42) + " tok/s ").ljust(14), press="page:spend")],
      "spark": [T("▃▅▆▇▅ ")],
      "pet": [T("小墨 Lv.7" + " " * (12 - dw("小墨 Lv.7")))],
    }
    return segs

def band_row(W, expanded, card_title):
    segs = band_segments(expanded)
    order = ["line", "source", "pend", "qc", "tps", "spark", "pet"]
    width = {k: BW[k] for k in order}
    active = list(order)
    toggle = " " + ("▴收合" if expanded else "▾展開")
    tw = dw(toggle)
    rem_text = "‣ 要你決定：" + card_title
    remW = dw(rem_text)
    target = min(60, remW)
    def right():
        return tw + sum(BW["sep"] + width[k] for k in active)
    def avail():
        return W - right()
    dropped = []
    def drop(k):
        if k in active:
            active.remove(k); dropped.append(k)
    if avail() < target: drop("pet")
    if avail() < target: drop("spark")
    if avail() < target: drop("qc")           # QC 非警示才可丟
    if avail() < target: drop("source")
    if avail() < target: drop("tps")
    for k in ["pet", "spark", "qc", "source", "tps", "line"]:
        if avail() >= 28: break
        drop(k)
    fin = max(0, avail())
    rem = rem_text if dw(rem_text) <= fin else cut(rem_text, fin)
    row = [T(rem, "attention", press="page:decide")]
    row = pad(row, fin) if dw(rem) < fin else row
    for k in order:
        if k in active:
            row += [T(" │ ", "structure")] + segs[k]
    row.append(T(toggle, press="band:toggle"))
    return row, dropped

CHAT = [
  ("● ", "示意先給你看，你說好才動。", "text"),
  ("", "", "text"),
  ("  ", "待決在右邊的 pane，", "dim"),
  ("  ", "選項和送出在 pane 下半部。", "dim"),
  ("  ", "band 在最底下一列，", "dim"),
  ("  ", "兩邊共用同一張卡。", "dim"),
  ("", "", "text"),
  ("● ", "要不要先拍 120 欄？", "text"),
]

def chat_row(r, w):
    if r < len(CHAT):
        a, b, role = CHAT[r]
        return pad([T(a), T(b, role)], w)
    return [T(" " * w)]

def composite(W, rows_n, pane_rows, expanded, pane_open=True):
    out = []
    Hp = len(pane_rows) if pane_open else 0
    pw = rw(pane_rows[0]) if pane_open else 0
    for r in range(rows_n - 2):
        if pane_open and r < Hp:
            out.append(chat_row(r, W - pw) + pane_rows[r])
        else:
            out.append(chat_row(r, W))
    b, _ = band_row(W, expanded, CARDS[CUR][1])
    out.append(b)
    out.append(pad([T("> ", "text"), T("（輸入框）", "dim")], W))
    return out

# ---------------------------------------------------------------- 流程圖（兩格）
def flow_cell(W, filled):
    rows = []
    rows.append(pad([T("● ", "text"), T("（你在輸入框打指令）", "dim")], W))
    rows.append(pad([], W))
    rows.append(pad([T("╭" + "─" * (W - 2) + "╮", "structure")], W))
    if filled:
        body = [T("│ ", "structure"), T("> "), T("/live-pane spend"), T("█", "here")]
    else:
        body = [T("│ ", "structure"), T("> "), T("/forge-stats spend"), T("█", "here")]
    body = pad(body, W - 1) + [T("│", "structure")]
    rows.append(body)
    rows.append(pad([T("╰" + "─" * (W - 2) + "╯", "structure")], W))
    if filled:
        rows.append(pad([T("  已填入、還沒執行；按 Enter 才打開 live pane 並切到「花費」", "dim")], W))
    else:
        rows.append(pad([T("  forge-stats 不再開自己的 Pane", "dim")], W))
    return rows

# ================================================================ 組圖
FIGS = []
def add(id, title, W, rows, note="", kind="pane"):
    FIGS.append(Fig(id, title, W, rows, note, kind))

# 1) 待決頁 boxed（宿主實際框高 17／13／5）
add("dec-b200", "200x50 · 待決頁 · boxed", 199, decide_boxed(200, 17, "L0d"), "boxed H＝17，ctrlCap＝8，bodyColumns≥83 且 17−2−8＝7≥6 → 雙欄 L0（換卡器 1 列）")
add("dec-b120", "120x40 · 待決頁 · boxed", 119, decide_boxed(120, 13, "L2"), "boxed H＝13，ctrlCap＝6，13−2−6＝5<6 → 單欄 L2")
add("dec-b80",  "80x24 · 待決頁 · boxed",  79, decide_boxed(80, 5, "L4"),  "boxed H＝5，ctrlCap＝2 → 單欄 L4（分類標題併進上框線）")
# 2) 待決頁 docked
add("dec-d80",  "80x24 · 待決頁 · docked",  80, decide_docked(22), "docked H≈22，ctrlCap＝10 → L0（<110 欄通常不 docked，計畫表仍列此格，照畫）")
add("dec-d120", "120x40 · 待決頁 · docked", 80, decide_docked(38), "docked H≈38，ctrlCap＝18 → L0")
add("dec-d200", "200x50 · 待決頁 · docked", 80, decide_docked(48), "docked H≈48，ctrlCap＝23 → L0")
# 3) band＋pane 整合
p120 = decide_docked(38)
p200 = decide_docked(48)
add("int-120c", "120x40 · band＋pane 整合 · pane 收合（只有 band）", 120, composite(120, 40, [], False, pane_open=False), "band 收起一列，展開鍵 ▾展開 在最右端", kind="screen")
add("int-120o", "120x40 · band＋pane 整合 · pane 展開", 120, composite(120, 40, p120, True), "pane docked 在右，band 在最底，兩者同一張卡（第 3 張）", kind="screen")
add("int-200o", "200x50 · band＋pane 整合 · pane 展開", 200, composite(200, 50, p200, True), "同上，band 欄位在 200 欄全部放得下", kind="screen")
# 4) forge-stats 頁籤
HP = ["↑↓ 捲動 · w 收合 · 鍵盤在 pane · Esc 還給輸入框", "↑↓ · w · Esc"]
add("pet-80",  "80x24 · live pane「寵物」頁（原 forge-stats pet）", 80, page_pane("寵物 · 來源 forge-stats.panel · 12 秒前", pet_info(":3"), 22, "0", HP), "docked M≈22；<80 欄用 2 字元替代圖示 :3")
add("pet-120", "120x40 · live pane「寵物」頁（原 forge-stats pet）", 80, page_pane("寵物 · 來源 forge-stats.panel · 12 秒前", pet_info("🐱"), 38, "0", HP), "docked M≈38")
add("spd-80",  "80x24 · live pane「花費」頁（補每模型明細，無金額）", 80, page_pane("花費 · 來源 fleet seats --quota · 2 分前", spend_info(), 22, "5", HP), "docked M≈22，內容超出時最後一列標「下方還有 N 行」")
add("spd-120", "120x40 · live pane「花費」頁（補每模型明細，無金額）", 80, page_pane("花費 · 來源 fleet seats --quota · 2 分前", spend_info(), 38, "5", HP), "docked M≈38")
add("acc-80",  "80x24 · live pane「帳號」頁（新頁）", 80, page_pane("帳號 · 資料 fleet-quota.json · 2 分前", accounts_info(), 22, "_", HP), "docked M≈22；帳號與主機名稱為占位字（遮罩規則照 live 現有，細節計畫未定）")
add("acc-120", "120x40 · live pane「帳號」頁（新頁）", 80, page_pane("帳號 · 資料 fleet-quota.json · 2 分前", accounts_info(), 38, "_", HP), "docked M≈38")
add("flow-1", "流程 ①　操作員打 /forge-stats spend", 80, flow_cell(80, False), "forge-stats 不再註冊 Pane", kind="flow")
add("flow-2", "流程 ②　輸入框被填成 /live-pane spend", 80, flow_cell(80, True), "D4：用 $.prompt.fill 填入，Enter 由 /live-pane 打開並切頁", kind="flow")

# ================================================================ 檢查
def check_widths(figs, verbose=True, out=None):
    bad = 0
    for f in figs:
        if verbose: out.append(f"[{f.id}] {f.title}  圖寬 {f.W}  列數 {f.H}")
        for i, r in enumerate(f.rows):
            w = rw(r)
            ok = (w == f.W)
            if not ok: bad += 1
            if verbose: out.append(f"  列 {i:02d} 寬 {w:3d} {'ok' if ok else 'FAIL (應為 %d)' % f.W}")
    return bad

def upper_clicks(f):
    up = f.H // 2
    return [(i, s.press) for i, r in enumerate(f.rows[:up]) for s in r if s.press]

def check_r1(figs, out=None):
    bad = 0
    for f in figs:
        up = f.H // 2
        n = len(upper_clicks(f))
        bad += n
        if out is not None:
            ctrl = sorted({i for i, r in enumerate(f.rows) if any(s.press for s in r)})
            out.append(f"[{f.id}] H={f.H} 上半部＝第 0 到 {up-1} 列 可點 seg 數={n} {'ok' if n == 0 else 'FAIL'}；有可點 seg 的列＝{fmt_ranges(ctrl)}")
    return bad

def fmt_ranges(L):
    if not L: return "無"
    out, a, p = [], L[0], L[0]
    for x in L[1:] + [None]:
        if x is not None and x == p + 1:
            p = x; continue
        out.append(str(a) if a == p else f"{a}–{p}")
        if x is not None: a = p = x
    return "、".join(out)

def roles_used(figs):
    s = {"text"}
    for f in figs:
        for r in f.rows:
            for t in r:
                if t.t.strip(): s.add(t.role)
    return sorted(s)

def check_contrast(figs, keymap, out=None, label=""):
    bad = []
    for theme in ("dark", "light"):
        bg = THEMES["background"][theme]
        if out is not None: out.append(f"-- {theme} 背景 rgb{tuple(bg)} {label}")
        for role in roles_used(figs):
            fg = role_rgb(theme, role, keymap)
            c = contrast(fg, bg); bar = bar_of(role)
            ok = c >= bar
            if not ok: bad.append((theme, role, c, bar))
            if out is not None:
                k = "(SGR2 半色)" if role == "dim" else keymap[role]
                out.append(f"  {role:12s} {k:12s} rgb{tuple(fg)!s:18s} 對比 {c:5.2f} 門檻 {bar} {'ok' if ok else 'FAIL'}")
    return bad

def check_accent_bold(figs):
    bad = []
    for f in figs:
        for i, r in enumerate(f.rows):
            for s in r:
                if s.role in ACCENT_ROLES and re.search(r"[A-Za-z0-9一-鿿]", s.t) and not s.b:
                    bad.append((f.id, i, s.t))
    return bad

def check_money(figs):
    bad = []
    pat = re.compile(r"\$|USD|NT\$|金額|\d+\.\d\d\b")
    for f in figs:
        if f.id.startswith(("spd", "acc")):
            for i, r in enumerate(f.rows):
                txt = "".join(s.t for s in r)
                # 「只算 token，不顯示金額」是聲明句，不算金額
                txt2 = txt.replace("不顯示金額", "")
                if pat.search(txt2): bad.append((f.id, i, txt.strip()))
    return bad

def run_checks():
    out = []
    P = lambda s="": out.append(s)
    summary = {}
    # ---- 第 1 節 寬度
    P("== 第 1 節　逐列 CJK 顯示寬度（規則：mods/tux/width.ts；CJK 與全形 2 格，框線與模糊寬度字元 1 格）==")
    bad1 = check_widths(FIGS, True, out)
    total_rows = sum(f.H for f in FIGS)
    P(f"結果：{len(FIGS)} 張圖、{total_rows} 列，不等於圖寬的列 {bad1}")
    summary["width"] = (total_rows - bad1, bad1)
    P()
    P("-- 反向探針 1：在 dec-b120 第 3 列尾端多塞一個中文字，檢查必須報錯")
    f0 = next(f for f in FIGS if f.id == "dec-b120")
    rows = [list(r) for r in f0.rows]
    rows[3] = rows[3] + [T("中")]
    probe = Fig("probe-width", "探針", f0.W, rows)
    tmp = []
    pb = check_widths([probe], True, tmp)
    for l in tmp:
        if "FAIL" in l or "探針" in l: P("  " + l)
    P(f"探針結果：報出 {pb} 列不合格 → {'探針有效（檢查會抓到）' if pb >= 1 else '探針失敗：檢查是壞的'}")
    summary["probe_width"] = pb
    P()
    # ---- 第 2 節 對比
    P("== 第 2 節　深淺雙主題對比（WCAG 2；門檻同 mods/tux/contrast.test.js：一般 4.5，dim 類／accent 類／SGR2 半色 3）==")
    P("角色對主題鍵：A1 重映射後（feat/tui-width-contrast palette.ts：structure、waiting_on → inactive）；host＝宿主畫的 [-]，比照 inactive")
    bad2 = check_contrast(FIGS, ROLE_KEY_NEW, out, "（A1 後）")
    ab = check_accent_bold(FIGS)
    P(f"accent 類（here／running／link）的文字須粗體（WCAG 粗體 3:1）：違規 {len(ab)}")
    for x in ab: P(f"  FAIL {x}")
    n_pairs = len(roles_used(FIGS)) * 2
    P(f"結果：用到 {len(roles_used(FIGS))} 個角色 × 2 主題 = {n_pairs} 組，不合格 {len(bad2) + len(ab)}")
    summary["contrast"] = (n_pairs - len(bad2), len(bad2) + len(ab))
    P()
    P("-- 反向探針 2：改用重映射前的舊對應（structure、waiting_on → subtle），檢查必須報出不合格")
    tmp = []
    old_bad = check_contrast(FIGS, ROLE_KEY_OLD, tmp, "（舊對應 subtle）")
    for l in tmp:
        if "FAIL" in l: P("  " + l)
    P(f"探針結果：報出 {len(old_bad)} 組不合格 → {'探針有效（檢查會抓到）' if len(old_bad) >= 1 else '探針失敗：檢查是壞的'}")
    summary["probe_contrast"] = len(old_bad)
    P()
    # ---- 第 3 節 R1
    P("== 第 3 節　R1 機械檢查：上半部（第 0 到 floor(H/2)−1 列）可點 seg 數必須是 0；H＝該圖的高（pane 圖是框高，整畫面圖是終端列數）==")
    bad3 = check_r1(FIGS, out)
    P(f"結果：{len(FIGS)} 張圖，上半部可點 seg 合計 {bad3}")
    summary["r1"] = (len(FIGS) - sum(1 for f in FIGS if upper_clicks(f)), sum(1 for f in FIGS if upper_clicks(f)))
    P()
    P("-- 反向探針 3：在 dec-d120 第 1 列種一個可點 seg，檢查必須報錯")
    f1 = next(f for f in FIGS if f.id == "dec-d120")
    rows = [list(r) for r in f1.rows]
    rows[1] = rows[1] + [S("", "text", "key:probe")]
    probe = Fig("probe-r1", "探針", f1.W, rows)
    tmp = []
    pb3 = check_r1([probe], tmp)
    for l in tmp: P("  " + l)
    P(f"探針結果：報出 {pb3} 個可點 seg → {'探針有效（檢查會抓到）' if pb3 >= 1 else '探針失敗：檢查是壞的'}")
    summary["probe_r1"] = pb3
    P()
    P("-- 反向探針 4：把 dec-d120 的換卡器（卡片列）搬進資訊區的第 1 列，等同舊版 V3 違規，檢查必須報錯")
    rows = [list(r) for r in f1.rows]
    rows[1] = mid(card_rows(0, 0)[0], 76)
    probe = Fig("probe-v3", "探針", f1.W, rows)
    tmp = []
    pb4 = check_r1([probe], tmp)
    for l in tmp: P("  " + l)
    P(f"探針結果：報出 {pb4} 個可點 seg → {'探針有效' if pb4 >= 1 else '探針失敗'}")
    summary["probe_r1b"] = pb4
    P()
    # ---- 第 4 節 其他
    P("== 第 4 節　補充檢查：花費與帳號頁不得出現金額 ==")
    bm = check_money(FIGS)
    nm = sum(1 for f in FIGS if f.id.startswith(("spd", "acc")))
    for x in bm: P(f"  FAIL {x}")
    P(f"結果：{nm} 張圖，金額字樣 {len(bm)} 處")
    summary["money"] = (nm - len({x[0] for x in bm}), len(bm))
    P()
    P("== 總結 ==")
    P(f"第 1 節 寬度：通過列 {summary['width'][0]}，失敗列 {summary['width'][1]}；探針報出 {summary['probe_width']}（需 ≥1）")
    P(f"第 2 節 對比：通過組 {summary['contrast'][0]}，失敗 {summary['contrast'][1]}；舊對應探針報出 {summary['probe_contrast']}（需 ≥1）")
    P(f"第 3 節 R1：通過圖 {summary['r1'][0]}，失敗圖 {summary['r1'][1]}；探針報出 {summary['probe_r1']}／{summary['probe_r1b']}（需 ≥1）")
    P(f"第 4 節 金額：通過圖 {summary['money'][0]}，金額字樣 {summary['money'][1]}")
    fail = summary["width"][1] + summary["contrast"][1] + summary["r1"][1] + summary["money"][1]
    probes_ok = summary["probe_width"] >= 1 and summary["probe_contrast"] >= 1 and summary["probe_r1"] >= 1 and summary["probe_r1b"] >= 1
    P(f"整體：{'全部通過，探針皆有效' if fail == 0 and probes_ok else 'FAIL'}")
    return out, summary, fail == 0 and probes_ok

# ================================================================ HTML
def css_vars():
    s = []
    for theme in ("dark", "light"):
        s.append(f".{theme}{{background:rgb({','.join(map(str,THEMES['background'][theme]))});color:rgb({','.join(map(str,THEMES[theme]['text']))});")
        for role in sorted(set(ROLE_KEY_NEW) | {"dim"}):
            s.append(f"--c-{role}:rgb({','.join(map(str, role_rgb(theme, role, ROLE_KEY_NEW)))});")
        s.append("}")
    return "".join(s)

def render_pre(f, theme):
    parts = []
    for r in f.rows:
        prev = None; buf = ""
        def flush():
            nonlocal buf
            if buf == "": return
            role, press, b = prev
            cls = f"c-{role}" + (" b" if b else "")
            attr = f' data-press="{html.escape(press)}"' if press else ""
            parts.append(f'<span class="{cls}"{attr}>{html.escape(buf)}</span>')
            buf = ""
        for s in r:
            k = (s.role, s.press, s.b)
            if k != prev:
                flush(); prev = k
            buf += s.t
        flush()
        parts.append("\n")
    return f'<pre class="grid {theme}" aria-label="{html.escape(f.title)}（{theme}）">{"".join(parts)}</pre>'

def rows_with(fid, prefix):
    f = next(x for x in FIGS if x.id == fid)
    L = sorted({i for i, r in enumerate(f.rows) if any(s.press and s.press.startswith(prefix) for s in r)})
    return fmt_ranges(L)

def fig_block(f):
    up = f.H // 2
    n = len(upper_clicks(f))
    ctrl = sorted({i for i, r in enumerate(f.rows) if any(s.press for s in r)})
    dim_note = f"{f.W} 欄 × {f.H} 列"
    note = (f'<ul class="facts"><li><b>上半部（第 0–{up-1} 列）可點元素：{n} 個</b></li>'
            f'<li>可點元素所在列（控制區、頁籤、band）：第 {fmt_ranges(ctrl)} 列</li>'
            f'<li>{html.escape(f.note)}</li></ul>')
    return (f'<section class="fig" id="{f.id}"><h3>{html.escape(f.title)} <small>{dim_note}</small></h3>{note}'
            f'<div class="theme-dark"><p class="tl">深色主題</p>{render_pre(f, "dark")}</div>'
            f'<div class="theme-light"><p class="tl">淺色主題</p>{render_pre(f, "light")}</div></section>')

def build_html(chk_text, ok):
    ids = lambda *a: [x for x in FIGS if x.id in a]
    def sect(title, intro, figs):
        return f'<h2>{title}</h2><p>{intro}</p>' + "".join(fig_block(x) for x in figs)
    body = []
    body.append(sect("一、待決頁（含「等你動手」）：三種尺寸，照退化階梯選樣式",
        "每張圖是 pane 的框（含框線）。boxed ＝ 宿主給的內嵌框，高度 17／13／5 列（單 01M889SR1X 實測），這是操作員 120x40、200x50、80x24 終端實際會落到的格，所以雙欄或單欄由計畫 3.4 的門檻決定。"
        "「等你動手」區塊畫的是計畫 3.2 的樣子（資訊區第 2 項，最多 3 列）；H＝5 時資訊區只有 1 列，計畫沒寫等你動手放哪裡（計畫未定），所以這格不畫。",
        ids("dec-b200","dec-b120","dec-b80")))
    body.append(sect("二、待決頁 docked（pane 停靠在右側，高度接近整個終端）",
        "docked 的待決頁一律單欄 L0，控制區 9 列（換卡器 5、選項 2、附註、送出），三種終端尺寸只差資訊區列數（計畫第 4 節）。pane 寬度畫 76（pane 預設寬度，tui-guideline §1.2），計畫沒寫 docked 寬度隨終端變不變（計畫未定）。",
        ids("dec-d80","dec-d120","dec-d200")))
    body.append(sect("三、band 與右側 pane 整合（R3、R4）",
        "band 是最底下的單列，左邊是提醒（‣ 要你決定：…），右邊是各欄位，最右端固定放展開鍵（▾展開／▴收合），和 pane 右上角宿主的 [-] 一個在最上、一個在最下，不會撞。"
        "band 的「待決」段、提醒和 pane 待決頁共用同一個選取游標：圖中 band 提醒的是第 3 張「預覽先拍幾欄」，pane 的 ‣ 也在第 3 張；在 pane 換卡，band 提醒跟著換（計畫 3.6）。"
        "band 展開後除了 ▴收合 之外還有沒有別的內容，計畫沒寫（計畫未定），本稿只畫鍵的變化。120 欄時 band 依 band-onerow.ts 的丟棄順序，放不下的欄位（來源、tok/s 等）會被丟掉。",
        ids("int-120c","int-120o","int-200o")))
    body.append(sect("四、forge-stats 併入 live pane 的頁籤（D4）",
        "forge-stats 自己的 Pane 退役；寵物、花費、帳號三頁由 live pane 提供（U3c，PM 01M88NXPCR A）。帳號是新頁，每帳號週與 5h 用量、預估、目標、主機、資料年齡，沒有任何金額；花費頁補每個模型的 token 與請求數，金額拿掉。"
        "這三頁計畫沒有為它們加控制區，所以只有下框線的頁籤可點。範例資料都是占位，帳號與主機名稱不是真的。",
        ids("pet-80","pet-120","spd-80","spd-120","acc-80","acc-120")))
    body.append('<h3>操作流程：<code>/forge-stats spend</code> → 輸入框被填成 <code>/live-pane spend</code></h3>'
                '<p>D4（PM 01M88NQREZ A）：宿主不允許外掛開另一個外掛的 Pane，所以 /forge-stats 只負責把 /live-pane 的指令填進輸入框，操作員按 Enter 後才由 autopilot 的 /live-pane 打開並切頁。下面兩格是操作前後；Enter 之後的畫面就是上面的「花費」頁。</p>'
                + "".join(fig_block(x) for x in ids("flow-1","flow-2")))

    # R1–R6 對照表
    r = lambda fid, pre: rows_with(fid, pre)
    table = f"""
<h2>五、R1–R6 逐條對照</h2>
<table>
<tr><th>條</th><th>準則（摘要）</th><th>畫面上在哪裡做到</th><th>怎麼驗（checks.txt）</th></tr>
<tr><td>R1</td><td>操作區在下方、上半部只放不可點的標題</td>
<td>所有待決圖的選項 a／b、附註、送出、換卡器都在下半部。例：dec-d120 可點列＝第 {rows_with('dec-d120','key:')} 列（全圖 38 列，上半部＝第 0–18 列）；dec-b200 第 {rows_with('dec-b200','key:')} 列（H＝17，上半部＝第 0–7 列）；dec-b80 第 {rows_with('dec-b80','key:')} 列（H＝5，上半部＝第 0–1 列）。左欄卡片清單（dec-b200 資訊區）不可點。</td>
<td>第 3 節：每張圖上半部可點 seg 數＝0；探針 3、4 證明檢查會報錯。注意：右上角 [-] 是宿主畫的，不是本 mod 的可點元素。</td></tr>
<tr><td>R2</td><td>從 v7.4 改進，不重畫；偏離列清單</td>
<td>框線、標題、頁籤、卡片文字、★／✧、背景捲軸都沿用 v7.4（現行畫面基準 209x50）；差異全在下面「偏離清單」。</td>
<td>不是機械檢查；偏離清單每條附計畫節號或 PM 裁定號。</td></tr>
<tr><td>R3</td><td>band 左右分區、收起一列、展開鍵固定、不和 [-] 衝突</td>
<td>int-120c／int-120o／int-200o 最底下第二列：提醒在左，欄位在右，展開鍵 ▾展開／▴收合 固定在最右端；[-] 在 pane 第 0 列最右。可點列：int-200o 第 {rows_with('int-200o','band:')} 列（展開鍵）。</td>
<td>第 1 節（band 列寬度＝圖寬）；第 3 節（band 在下半部）。</td></tr>
<tr><td>R4</td><td>band 與 Pane 一起設計、共用狀態、forge-stats 併入</td>
<td>int-120o／int-200o：band 提醒「預覽先拍幾欄」＝pane 第 3 張；section 四：寵物、花費、帳號三頁在 live pane；流程圖 flow-1、flow-2。</td>
<td>共用游標的測試在計畫第 5 節（本稿不驗）；本稿只驗畫面寬度、對比、R1。</td></tr>
<tr><td>R5</td><td>CJK 寬度、框線對齊、深淺對比都用腳本驗</td>
<td>全部圖：每列寬度＝圖寬；兩種主題都畫；顏色是 themes-cc.json 的真實 RGB。</td>
<td>第 1 節（逐列寬度，探針 1）、第 2 節（對比表，探針 2）。</td></tr>
<tr><td>R6</td><td>按 a 一定選 A</td>
<td>選項列 a 80 欄、b 120 欄（✧＝建議）：dec-d120 第 {rows_with('dec-d120','key:a')}、{rows_with('dec-d120','key:b')} 列，dec-b80 第 {rows_with('dec-b80','key:a')} 列（a、b 合成一列，各自可點）。字母是選項的順位字母，不是畫面位置。</td>
<td>本稿只能驗「畫面上字母對應順位」：人工對照；機械驗在 pickKeys 測試（計畫第 5 節），不在 checks.txt。</td></tr>
</table>"""
    body.append(table)

    dev = [
     ("D1","docked 待決頁：v7.4 S3 每張卡的 a/b/c、m 跟著卡片 → 卡片內容在資訊區，a/b、附註、送出固定在下半部控制區（dec-d80／d120／d200）","計畫第 6 節 D1；3.1、3.2"),
     ("D2","boxed C 雙欄：v7.4 左欄清單可點選換卡 → 左欄不可點（‣ 標目前這張），換卡改由下半部換卡器（dec-b200）","計畫 D2；3.4"),
     ("D3","ux-v7 把選單列放在 band 上方 → 不變，band 仍是最下面一列；只記錄 R1 優先（int-* 圖）","計畫 D3；準則 R1"),
     ("D4","forge-stats 自己的 Pane 退役；/forge-stats 改成把 /live-pane 填進輸入框，Enter 後切頁（流程圖）","計畫 D4；PM 01M88NQREZ A"),
     ("D5","C 雙欄門檻：不再用「maxRows＜10 收清單」，改成 bodyColumns≥83 且資訊區（控制區用滿後）≥6 列才雙欄 → H＝17 雙欄、H＝13 單欄","計畫 D5、3.4；PM 01M88NJE2C"),
     ("E1","新增換卡器：目前這張前後各 2 張一列一張，雙欄時縮成 1 列 ‹ p 3/6 n ›（v7.4 沒有）","計畫 3.2、3.3、3.4"),
     ("E2","新增退化階梯 L0–L5：選項、附註＋送出在窄框時合成一列（本稿畫 L0、L2、L4）","計畫 3.3"),
     ("E3","「等你動手」區塊從 v7.4 的底部 action 區搬到資訊區第 2 項，最多 3 列，放不下收成一列","計畫 3.2"),
     ("E4","H＝5 時資訊區只有 1 列：分類標題與卡片標題併進上框線，唯一資訊列放問題，建議改用選項上的 ✧","計畫 3.2（g2 8311a9ef）"),
     ("E5","資訊區固定順序：分類標題 → 等你動手 → ↑ → 背景 → 建議 → ‣ 標題問題，空間不夠從背景開始截","計畫 3.2"),
     ("E6","band 與 Pane 共用同一個選取游標（band 提醒＝pane 目前那張卡）","計畫 3.6；R4"),
     ("E7","live pane 新增「帳號」頁：每帳號週與 5h、預估、目標、主機、資料年齡","計畫 U0 對照與 U3c；PM 01M88NXPCR A"),
     ("E8","花費頁補每個模型的 token 與請求數；金額一律拿掉（維持 spend.ts 的 No money）","PM 01M88NXPCR A"),
     ("E9","寵物頁補名字、等級、XP、屬性、策略、說的話、村落（先核對 pet.tiers，畫不出來列缺口）","PM 01M88NXPCR A；U0 對照第 3 項"),
    ]
    body.append('<h2>六、偏離清單（相對 v7.4 基準）</h2><p>共 %d 條。D 開頭是計畫第 6 節原有的五條；E 開頭是計畫其他節寫明、畫面上看得到差別的改動。每條都附計畫節號或 PM 裁定號。</p><table><tr><th>編號</th><th>和 v7.4 不同的地方</th><th>出處</th></tr>%s</table>' % (len(dev), "".join(f"<tr><td>{a}</td><td>{html.escape(b)}</td><td>{html.escape(c)}</td></tr>" for a, b, c in dev)))

    unk = [
     "帳號頁在下框線的頁籤鍵：計畫沒寫，圖上暫用 _ 帳號 占位。",
     "帳號頁的資料來源檔名：圖上寫 fleet-quota.json（forge-stats 現在讀的檔）；live 要不要同一份計畫沒寫。",
     "帳號頁的帳號與主機遮罩規則：計畫只寫「照 live 現有的遮罩規則」，細節未查，圖上用 pool-a、host-1 之類占位字。",
     "帳號頁「目標」：forge-stats 的目標是全域一個值（目標用到 70%），圖上放在標題列與浪費警告列，沒做成每帳號一欄。",
     "H＝5（80x24 boxed）時「等你動手」放哪裡：計畫 3.2 沒寫，這格不畫。",
     "「等你動手」最多 3 列 vs 完整步驟：真實程式（decide.ts）是標題列加每個步驟一列，計畫寫最多 3 列；圖上把「等你動手 N 張」與卡片標題合成第 1 列，再放 2 個步驟，剛好 3 列。這個合併寫法是本稿為了同時滿足兩邊而做的，不是計畫寫的。",
     "docked pane 的寬度：圖上用 76，計畫沒寫會不會隨終端寬度變。",
     "band 展開後除了 ▴收合 之外有沒有別的內容：計畫沒寫，只畫鍵的變化。",
     "寵物頁的 sprite（像素圖）和 pet.tiers 的欄位是否齊全：U3c 要先核對，本稿只畫 forge-stats 現有的欄位。",
     "游標符號：計畫第 4 節 mock 與現行基準用 ▸，autopilot develop 的 decide.ts 實際是 ‣（U+2023）；本稿照程式碼畫 ‣。",
     "/forge-stats 的參數：計畫 D4 寫 pet|usage|fleet 對應 /live-pane pet|spend|fleet，這次交辦用 /forge-stats spend；流程圖照交辦畫。",
     "底線的鍵位提示文字（PgUp/PgDn 捲背景 · p/n 換張 …）取自計畫第 4 節 mock 加現行 locale，v7.4 的「v 單卡版」「w 收合」是否保留計畫沒寫。",
    ]
    body.append('<h2>七、計畫沒寫、本稿標「（計畫未定）」的地方</h2><ul>' + "".join(f"<li>（計畫未定）{html.escape(x)}</li>" for x in unk) + '</ul>')

    body.append(f'<h2>八、檢查結果</h2><p>以下是 gen.py 對「畫出來的這份網格」跑出的結果，同一份也存成 checks.txt。整體：<b>{"全部通過" if ok else "有失敗"}</b>。</p>'
                f'<details open><summary>展開／收合 checks.txt 全文</summary><pre class="chk">{html.escape(chk_text)}</pre></details>')

    css = """
:root{color-scheme:light dark;--pg:#f4f4f2;--fg:#1c1c1c;--mut:#555;--line:#c9c9c4;--card:#fff}
@media (prefers-color-scheme:dark){:root{--pg:#17171a;--fg:#e8e8e8;--mut:#a8a8a8;--line:#3a3a40;--card:#202024}}
html{background:var(--pg)}body{margin:0;padding:16px;font:15px/1.65 system-ui,"Noto Sans CJK TC","PingFang TC",sans-serif;color:var(--fg);background:var(--pg)}
main{max-width:1400px;margin:0 auto}h1{font-size:24px;margin:.2em 0}h2{font-size:20px;margin:2em 0 .4em;border-top:1px solid var(--line);padding-top:1em}h3{font-size:16px;margin:1.6em 0 .2em}
small{font-weight:400;color:var(--mut)}p,li{max-width:70em}code{background:var(--card);padding:0 .3em;border-radius:3px}
.ctl{position:absolute;opacity:0;pointer-events:none}.bar{display:flex;flex-wrap:wrap;gap:8px 16px;align-items:center;margin:12px 0;padding:8px 12px;background:var(--card);border:1px solid var(--line);border-radius:6px}
.bar label{cursor:pointer;padding:2px 10px;border:1px solid var(--line);border-radius:14px}
#th-both:checked~main .bar label[for=th-both],#th-dark:checked~main .bar label[for=th-dark],#th-light:checked~main .bar label[for=th-light],#hl:checked~main .bar label[for=hl]{background:var(--fg);color:var(--pg)}
#th-dark:checked~main .theme-light{display:none}#th-light:checked~main .theme-dark{display:none}
#hl:checked~main [data-press]{box-shadow:inset 0 -2px 0 #ff2bd6;background:rgba(255,43,214,.16)}
.facts{margin:.2em 0 .4em;padding-left:1.2em;color:var(--mut);font-size:14px}.tl{margin:.6em 0 .1em;font-size:13px;color:var(--mut)}
pre.grid{margin:0 0 8px;padding:6px 8px;overflow-x:auto;font:12px/1.3 ui-monospace,"Noto Sans Mono CJK TC",monospace;font-variant-ligatures:none;letter-spacing:0;border:1px solid var(--line);border-radius:4px;width:max-content;max-width:100%}
pre.chk{font:12px/1.4 ui-monospace,"Noto Sans Mono CJK TC",monospace;background:var(--card);padding:10px;overflow:auto;max-height:70vh;border:1px solid var(--line)}
.b{font-weight:700}table{border-collapse:collapse;width:100%;margin:.5em 0;font-size:14px}th,td{border:1px solid var(--line);padding:6px 8px;text-align:left;vertical-align:top}th{background:var(--card)}
"""
    css += css_vars().replace(".dark{", "pre.dark{").replace(".light{", "pre.light{")
    for role in sorted(set(ROLE_KEY_NEW) | {"dim"}):
        css += f"pre.grid .c-{role}{{color:var(--c-{role})}}"
    css += "pre.grid .c-dim{opacity:1}"
    head = f"""<!doctype html><html lang="zh-Hant"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>R1 rev4 靜態示意</title><style>{css}</style></head><body>
<input class="ctl" type="radio" name="th" id="th-both" checked><input class="ctl" type="radio" name="th" id="th-dark"><input class="ctl" type="radio" name="th" id="th-light"><input class="ctl" type="checkbox" id="hl" checked>
<main>
<h1>R1 rev4：pane 改完會長什麼樣（靜態示意）</h1>
<p>這份是凍結計畫 rev 4（<code>doc/plans/2026-11-25-r1-bottom-controls.md</code>）的示意，不是新設計：框線、字樣、配色都沿用 v7.4 與 autopilot develop 的真實文字，只改「操作區固定在下半部」。<b>你說好，才動 pane。</b>每張圖都有深色和淺色兩份，顏色是 Claude Code 真實主題的 RGB。</p>
<p>圖都是一格一格的等寬字元格（中文占 2 格）。用系統等寬字型顯示，如果你的字型把某些符號畫得比 1 格寬，列尾可能看起來歪一點；寬度的正確性以第八節腳本檢查為準，不是以眼睛看。</p>
<div class="bar"><span>主題：</span><label for="th-both">深色＋淺色</label><label for="th-dark">只看深色</label><label for="th-light">只看淺色</label><span>｜</span><label for="hl">標出可點元素（洋紅底線）</label></div>
<p>目錄：<a href="#dec-b200">一 待決 boxed</a> · <a href="#dec-d80">二 待決 docked</a> · <a href="#int-120c">三 band＋pane</a> · <a href="#pet-80">四 forge-stats 頁籤</a> · <a href="#flow-1">流程</a> · 五 R1–R6 · 六 偏離清單 · 七 計畫未定 · 八 檢查</p>
"""
    return head + "".join(body) + "</main></body></html>\n"

def main():
    out, summary, ok = run_checks()
    chk = "\n".join(out) + "\n"
    open(os.path.join(HERE, "checks.txt"), "w", encoding="utf-8").write(chk)
    open(os.path.join(HERE, "r1-rev4-mockup.html"), "w", encoding="utf-8").write(build_html(chk, ok))
    print("\n".join(out[-8:]))
    return 0 if ok else 1

if __name__ == "__main__":
    sys.exit(main())
