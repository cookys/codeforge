"""console 大螢幕重設計：單步驟示意（PM 01M8A18Z，操作員 11-26 退回全稿）。

主要尺寸 209x49 是實測（hangar 01M8A1QA，19:08Z：操作員單一 tmux client 209x51，狀態列 2 列，pane 209x49；
先前 01M8A1A0 讀到 209x50 時多掛了一個 209x52 的 client）。
宿主上限（CC 2.1.331 binary 查證，子代理報告；2.1.289 實測對得上）：
- 右側停靠 pane：寬 = min(floor(cols*0.45), cols-60)，209 欄時最寬 94（含 1 欄拖拉條）；mod 以 $.ui.open({columns}) 請求。
- 底部槽：floor(rows/2) 含 prompt 區，49 列時 24 列；prompt 區約 7 列，band 可用 17 列。
- 兩者同時展開：pane 在對話與 band 的右邊，prompt 區在兩者下方，不搶高度；band 寬 = cols - pane 寬。
這一步對應舊稿第 56 步（2-L2 待決 · 等你動手 · 步驟很長）。

用法：python3 gen_wide.py <輸出目錄>  → <輸出目錄>/209x49-s56/{screen.txt,roles.json}
畫面裡不放任何設計稿註記（PM 01M8A18Z：不准有標註框）。
"""
import json
import os
import sys
import unicodedata

W, H = 209, 49
DOCK = 94                 # 右側 pane 寬，含左緣拖拉條（宿主上限）
LEFT = W - DOCK           # 115：對話與 band 的寬
BAND_ROWS = 17            # 底部槽 floor(49/2)=24 列扣 prompt 區 7 列
PROMPT_TOP = H - 7        # 42：prompt 區從這列開始，全寬
BAND_TOP = PROMPT_TOP - BAND_ROWS  # 25（42-17）


def cw(ch):
    return 2 if unicodedata.east_asian_width(ch) in 'WF' else 1


def dw(s):
    return sum(cw(c) for c in s)


grid = [[[' ', 'text'] for _ in range(W)] for _ in range(H)]


def put(y, x, text, role='text', limit=None):
    """從第 x 欄寫起；寬字佔兩格，第二格標 None。limit 是不可超過的欄（不含）。"""
    end = limit if limit is not None else W
    for ch in text:
        w = cw(ch)
        if x + w > end:
            raise ValueError(f'第 {y} 列超出 {end} 欄：{text!r}')
        grid[y][x] = [ch, role]
        if w == 2:
            grid[y][x + 1] = [None, role]
        x += w
    return x


def seq(y, x, parts, limit=None):
    for t, r in parts:
        x = put(y, x, t, r, limit)
    return x


def box(top, left, bottom, right, title_parts, bottom_parts=(), role='inactive'):
    """right 是右框線所在欄（含）。"""
    put(top, left, '╭─', role)
    x = seq(top, left + 2, title_parts, right)
    put(top, x, '─' * (right - x), role)
    put(top, right, '╮', role)
    for y in range(top + 1, bottom):
        put(y, left, '│', role)
        put(y, right, '│', role)
    put(bottom, left, '╰─', role)
    x = seq(bottom, left + 2, bottom_parts, right)
    put(bottom, x, '─' * (right - x), role)
    put(bottom, right, '╯', role)


T, I, K, OK, WARN, ERR = 'text', 'inactive', 'merged', 'success', 'warning', 'error'

# ── 對話區（左上，唯讀）────────────────────────────────────────────
talk = [
    [('  > ', I), ('把 tux-sync 的測試補上，跑完 check-all 再合併', T)],
    [],
    [('  ● ', OK), ('8 個反例全過；拿掉 symlink 拒絕那段，測試會紅', T)],
    [('    ⎿ ', I), ('反向探針有效', I)],
    [],
    [('  ● ', OK), ('check-all 結束碼 0，已 --no-ff 合進 develop', T)],
    [],
    [('  > ', I), ('好，下一張', T)],
    [],
    [('  ● ', OK), ('第 2 張：check-cjk-safe 的測試少了執行權限', T)],
    [('    ⎿ ', I), ('補上執行權限，重跑通過', I)],
]
for i, parts in enumerate(talk):
    seq(1 + i, 0, parts, LEFT)

# ── 底部 band（左下，操作熱區；可按的鍵只在這裡和 pane 下半部）──────────
bl, br = 0, LEFT - 1          # band 左右框線
box(BAND_TOP, bl, PROMPT_TOP - 1, br,
    [(' 2 待決 · 4 張 · 已選 2 張 ', T)],
    [(' a/b/c 選 · 1-4 換卡 · m 附註 · s 送出 · g 複製網址 · e 展開來龍去脈 · Tab 換頁 ', K)])
put(BAND_TOP, br - 5, '[-]', K)
SPLIT = 38                    # 清單欄與這張卡的分隔
DIV = PROMPT_TOP - 4          # 清單與卡片兩欄收在這條橫線；下面兩列是全寬的狀態和頁籤
for y in range(BAND_TOP + 1, DIV):
    put(y, SPLIT, '│', I)
put(BAND_TOP, SPLIT, '┬', I)
put(DIV, bl, '├' + '─' * (SPLIT - bl - 1) + '┴' + '─' * (br - SPLIT - 1) + '┤', I)

# 左欄：全部的卡和選擇狀態（舊稿要捲或切頁才看全）
cards = [
    [('  ✓1 ', OK), ('合併 plan', T), ('  → a 合併', I)],
    [('  ✓2 ', OK), ('外掛更新時機', T), ('  → b 明早', I)],
    [('  ·3 ', I), ('預覽先拍幾欄', T), ('  未選', I)],
    [('▸ ▲4 ', WARN), ('時冶 PL02 定案', T), ('  未選', I)],
]
y0 = BAND_TOP + 1
seq(y0, 2, [('待決清單', I)], SPLIT)
for i, parts in enumerate(cards):
    seq(y0 + 1 + i, 2, parts, SPLIT)
seq(y0 + 6, 2, [('送出時一起送：', I)], SPLIT)
seq(y0 + 7, 2, [('1 → a · 2 → b', T)], SPLIT)
seq(y0 + 8, 2, [('3、4 未選，留在清單', I)], SPLIT)

# 右欄：這張卡的問題、建議、全部選項和理由（R7：第一眼只看問題、選項、建議）
cx = SPLIT + 2
seq(y0, cx, [('▸ 時冶 PL02 模型的 8 個推定值照建議定案？', T)], br)
seq(y0 + 1, cx, [('建議 a：8 個值都和 staging 看到的一致', I)], br)
opts = [
    ('a', '照建議定案', '★ 建議', '8 個值都和 staging 看到的一致'),
    ('b', '不照建議', '', '按 m 寫哪一個值要改'),
    ('c', '待辦', '', '還沒空做，到期日前不再出現'),
]
for i, (k, lab, rec, why) in enumerate(opts):
    y = y0 + 3 + i
    x = seq(y, cx, [(f'{k}: ', K), (lab, T)], br)
    x = seq(y, cx + 16, [(rec, WARN)] if rec else [], br)
    seq(y, cx + 26, [(why, I)], br)
seq(y0 + 7, cx, [('m 附註：', K), ('（空）', I)], br)
seq(y0 + 8, cx, [('g 複製網址', K), ('  https://staging.shiye.test/pl02?v=241115', T)], br)
seq(y0 + 10, cx, [('s 送出 2 張', K), ('  · 送出前會先預覽，按 y 確定', I)], br)

# band 最下面兩列：頁籤與艦隊摘要（舊稿在框外，這裡併進 band，Tab 或數字鍵直接跳）
tabs = '1 現況  2 待決  3 派工  4 審查  5 花費  6 殘留  7 艦隊  8 待辦  9 說明'
seq(PROMPT_TOP - 3, 2, [('● 進行中', OK), (' │ 艦隊 ● 8 ✗ 2 ⋯ 2 │ 動手 1 │ ϟ 41 tok/s', I)], SPLIT + 40)
seq(PROMPT_TOP - 2, 2, [(tabs, K)], br)

# ── 右側 pane（停靠，最寬 94）：上半部唯讀詳情與各頁摘要，下半部跳頁鍵 ─────
dl, dr = LEFT, W - 1
box(0, dl, PROMPT_TOP - 1, dr, [(' 2 待決 · 詳情 ', T)], [(' 3-9 跳到那一頁 · e 展開來龍去脈 ', K)])
px = dl + 2
rows_up = [
    [('時冶 PL02 定案 · 等你動手', T)],
    [('1. 打開 staging 頁', T), ('（按 g 複製網址）', I)],
    [('   https://staging.shiye.test/pl02?v=241115', T)],
    [('2. 走一圈，核對 8 個推定值：', T)],
    [('   衰減、半徑、上限、下限、速度、密度、冷卻、門檻', T)],
    [('3. 和 PL02 表上的建議值逐一對照', T)],
    [('4. 一致選 a；有值要改選 b，用 m 寫哪一個', T)],
    [],
    [('來龍去脈 4 筆 · 收起', I)],
    [],
    [('艦隊 · 8 個 agent', T)],
    [('  ● codeforge    ϟ 41 tok/s  ▆▆▄▃▆   tux-sync 測試', I)],
    [('  ● autopilot    ϟ 37 tok/s  ▄▅▆▆▅   band 對比修正', I)],
    [('  ● hangar       ϟ 12 tok/s  ▂▂▃▂▁   盤點 client 尺寸', I)],
    [('  ✗ lab          失敗 · 測試逾時，等重派', I)],
    [('  ⋯ mnemos       等 PM 回覆', I)],
    [],
    [('派工 · 3 輪進行中 · 1 輪等審', T)],
    [('  派工 tux-sync 測試       codex    12 分前開始', I)],
    [('  派工 band 對比           sonnet    5 分前開始', I)],
    [('  審查 console 重設計      等 PM', I)],
]
for i, parts in enumerate(rows_up):
    if parts:
        seq(1 + i, px, parts, dr)

# 下半部（band 頂那列起）：每頁一列摘要，前面是跳頁鍵
jumps = [
    ('1', '現況', '進行中 3 · 失敗 2 · 等待 2'),
    ('3', '派工', '3 輪進行中 · 1 輪等審'),
    ('4', '審查', '2 條待看 · 最新：console 重設計'),
    ('5', '花費', '5h 42% · 7d 18% · 今日 1.2M tok'),
    ('6', '殘留', '0 項'),
    ('7', '艦隊', '8 個 agent · ✗ 2 · 平均 ϟ 30 tok/s'),
    ('8', '待辦', '3 張 · 最近到期 11-28'),
    ('9', '說明', '狀態圖示與按鍵表'),
]
seq(BAND_TOP, px, [('其他頁', I)], dr)
for i, (k, name, summ) in enumerate(jumps):
    seq(BAND_TOP + 1 + i, px, [(f'{k} {name}', K), ('  ' + summ, I)], dr)
seq(BAND_TOP + 10, px, [('e 展開來龍去脈', K), ('  4 筆：11-22 交第一版 → 11-25 審過數字', I)], dr)

# ── prompt 區（全寬）──────────────────────────────────────────────
put(PROMPT_TOP, 0, '─' * W, I)
put(PROMPT_TOP + 1, 0, '>', T)
put(PROMPT_TOP + 2, 0, '─' * W, I)
seq(PROMPT_TOP + 3, 1, [('codeforge │ develop │ 5h ▰▰▰▰▱▱▱▱▱▱ 42% 重置 2h10m │ 7d ▰▰▱▱▱▱▱▱▱▱ 18% │ fleet 8 lane 執行中 │ 記憶 L1 214 條', I)])
seq(PROMPT_TOP + 4, 2, [('⏵⏵ auto mode on (shift+tab to cycle)', I)])


def need(ch):
    return 'text' if ch.isalnum() or cw(ch) == 2 else 'glyph'


out = os.path.join(sys.argv[1], '209x49-s56')
os.makedirs(out, exist_ok=True)
lines, roles = [], []
for row in grid:
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
open(os.path.join(out, 'screen.txt'), 'w').write('\n'.join(lines) + '\n')
json.dump(roles, open(os.path.join(out, 'roles.json'), 'w'), ensure_ascii=False)
json.dump(['https://staging.shiye.test/pl02?v=241115'], open(os.path.join(out, 'urls.json'), 'w'))
print('wrote', out)
