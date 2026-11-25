#!/usr/bin/env python3
"""從 ANSI 實拍疊改 R1 示意；只用 Python 標準庫。執行：python3 gen.py。"""
from pathlib import Path
from dataclasses import dataclass, replace
import copy
import html
import json
import re
import subprocess
import sys
import unicodedata

HERE = Path(__file__).resolve().parent
THEMES = json.loads((HERE / 'themes-cc.json').read_text())
SIZES = ('120x40', '200x50', '80x24')
ANSI = re.compile(r'\x1b\[([0-9;]*)m')


def cw(ch):
    """AP mods/tux/width.ts：模糊寬度及框線為 1，不用 wcwidth 套件。"""
    c = ord(ch)
    if c == 0 or any(a <= c <= b for a, b in
                     [(0x300, 0x36f), (0x200b, 0x200f), (0xfe00, 0xfe0f), (0x20d0, 0x20ff)]):
        return 0
    ranges = [(0x1100, 0x115f), (0x2e80, 0x303e), (0x3041, 0x33ff),
              (0x3400, 0x4dbf), (0x4e00, 0x9fff), (0xa000, 0xa4cf),
              (0xac00, 0xd7a3), (0xf900, 0xfaff), (0xfe30, 0xfe4f),
              (0xff00, 0xff60), (0xffe0, 0xffe6), (0x1f300, 0x1f64f),
              (0x1f900, 0x1f9ff), (0x20000, 0x3fffd)]
    return 2 if any(a <= c <= b for a, b in ranges) else 1


def dw(s):
    return sum(map(cw, s))


def cut(s, width):
    if dw(s) <= width:
        return s
    out = ''
    for ch in s:
        if dw(out + ch) > width - 1:
            break
        out += ch
    return out + '…'


def xterm(n):
    if n < 16:
        return [(0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0),
                (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
                (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0),
                (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255)][n]
    if n >= 232:
        return (8 + (n - 232) * 10,) * 3
    v = (0, 95, 135, 175, 215, 255)
    n -= 16
    return (v[n // 36], v[(n // 6) % 6], v[n % 6])


@dataclass(frozen=True)
class Cell:
    ch: str = ' '
    fg: tuple | None = None
    bg: tuple | None = None
    bold: bool = False
    dim: bool = False
    inverse: bool = False
    role: str = ''
    press: str = ''


def source(path, width, height):
    """每列為顯示欄陣列；寬字尾格用 None。只補尾空格，不挪任何字。"""
    text = path.read_text()
    style = Cell()
    rows = []
    for line in text.splitlines():
        row = []
        pos = 0
        for match in list(ANSI.finditer(line)) + [None]:
            end = match.start() if match else len(line)
            for ch in line[pos:end]:
                w = cw(ch)
                if not w:
                    raise ValueError(f'實拍有未處理的零寬字元：{path.name}')
                row.append(replace(style, ch=ch))
                row.extend([None] * (w - 1))
            if match is None:
                break
            codes = [int(x or 0) for x in match[1].split(';')]
            i = 0
            while i < len(codes):
                n = codes[i]
                if n == 0:
                    style = Cell()
                elif n == 1:
                    style = replace(style, bold=True)
                elif n == 2:
                    style = replace(style, dim=True)
                elif n == 22:
                    style = replace(style, bold=False, dim=False)
                elif n in (7, 27):
                    style = replace(style, inverse=n == 7)
                elif n in (39, 49):
                    style = replace(style, **{'fg' if n == 39 else 'bg': None})
                elif n in (38, 48):
                    if codes[i + 1] == 5:
                        rgb = xterm(codes[i + 2]); i += 2
                    elif codes[i + 1] == 2:
                        rgb = tuple(codes[i + 2:i + 5]); i += 4
                    else:
                        raise ValueError('未知 SGR 色彩')
                    style = replace(style, **{'fg' if n == 38 else 'bg': rgb})
                elif 30 <= n <= 37 or 90 <= n <= 97:
                    style = replace(style, fg=xterm(n - 30 if n < 90 else n - 90 + 8))
                elif 40 <= n <= 47 or 100 <= n <= 107:
                    style = replace(style, bg=xterm(n - 40 if n < 100 else n - 100 + 8))
                else:
                    raise ValueError(f'未知 SGR {n}')
                i += 1
            pos = match.end()
        if len(row) > width:
            raise ValueError(f'{path.name} 超寬：{len(row)} > {width}')
        rows.append(row + [Cell()] * (width - len(row)))
    if len(rows) > height:
        raise ValueError(f'{path.name} 超高')
    return rows + [[Cell()] * width for _ in range(height - len(rows))]


def plain(row):
    return ''.join(c.ch for c in row if c is not None)


def read_path(size, p):
    w, h = map(int, size.split('x'))
    path = next((HERE / 'live-path' / size).glob(f'P{p}-*.ans'))
    return source(path, w, h), path.relative_to(HERE).as_posix()


def check_sources():
    count = 0
    for size in SIZES:
        w, h = map(int, size.split('x'))
        for p in range(10):
            rows, name = read_path(size, p)
            txt = (HERE / name).with_suffix('.txt').read_text().splitlines()
            assert [plain(r).rstrip() for r in rows] == [r.rstrip() for r in txt]
            assert all(dw(plain(r)) == w for r in rows)
            count += 1
        for mode in ('fs', 'normal'):
            source(HERE / 'live-shots' / mode / f'S-{size}-p2.ans', w, h)
            count += 1
    # 驗證 ANSI 的前景、背景、粗體、dim 均非空殼解析。
    sample, _ = read_path('120x40', 0)
    assert any(c and c.bg for r in sample for c in r)
    assert any(c and c.fg for r in sample for c in r)
    assert any(c and c.bold for r in sample for c in r)
    print(f'實拍轉換：{count} 張，逐字逐格與 .txt 一致，寬度全數通過')


CARDS = [
    ('合併 plan', '要不要把 shared-tui plan 合進 develop？', ['合併', '再審一輪'], 0, ['沒有補充']),
    ('外掛更新時機', 'forge-stats 外掛今晚還是明早更新？', ['今晚', '明早'], 1, ['沒有補充']),
    ('預覽先拍幾欄', '預覽先拍 80 欄還是 120 欄？', ['80 欄', '120 欄'], 1,
     ['操作員平常用寬螢幕，', '外出用筆電只有 80 欄。']),
    ('驗收長卡', '這張卡的背景有八十行，用來驗證 PgDn 的步長', ['收到', '再看'], 0,
     [f'第 {i:02d} 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。' for i in range(1, 81)]),
    ('驗收測試卡', '這是驗收腳本的測試卡', ['收到', '再看'], 0, ['沒有補充']),
] + [(f'捲動測試卡 {i:02d}', f'捲動測試 {i:02d}：這張卡用來湊滿 21 張，選哪個？',
      ['合併', '再審一輪'], 0, ['沒有補充']) for i in range(1, 17)]
# 核心路徑以外只有清單實拍，不能把未拍明細當成真資料。
CARDS = [card if i < 3 else (card[0], card[1], [], -1, ['缺少此卡實拍明細'])
         for i, card in enumerate(CARDS)]
STEPS = ['band 出現提醒', '展開', 'pane 停在同一張卡', '換卡，兩邊一起變',
         '選 a／b', '寫附註', '預覽並送出', '收合']
PATH_STATES = [(0, '', 'closed', 0), (0, '', 'open', 0), (0, '', 'open', 0),
               (1, '', 'open', 1), (1, 'a', 'open', 2), (1, 'a', 'edit', 4),
               (2, '', 'sent', 7), (2, '', 'closed-sent', 8)]


def glyphs(text, role='text', press=''):
    out = []
    for ch in text:
        out.append(Cell(ch, role=role, press=press, bold=role in ('warning', 'claude')))
        out.extend([None] * (cw(ch) - 1))
    return out


def pane_bounds(rows):
    top = next(i for i, row in enumerate(rows) if plain(row).startswith('╭'))
    bottom = next(i for i, row in enumerate(rows) if plain(row).startswith('╰'))
    edge = max(i for i, c in enumerate(rows[top + 1]) if c and c.ch == '│')
    return top, bottom, edge


def layout(size, rows):
    top, bottom, edge = pane_bounds(rows)
    height = bottom - top + 1
    cap = (height + 1) // 2 - 1
    dual = edge - 3 >= 83 and height - 2 - cap >= 6
    costs = [5 if dual else 9, 5 if dual else 7, 4, 3, 2, 1]
    level = next(i for i, cost in enumerate(costs) if cost <= cap)
    return dict(top=top, bottom=bottom, edge=edge, height=height, cap=cap,
                dual=dual, level=level, controls=costs[level])


def paint(rows, y, left, right, parts):
    """右界不包含外框：所有疊改都用 edge−1，避免多／少一格。"""
    result = []
    for text, role, press in parts:
        result.extend(glyphs(text, role, press))
    width = right - left
    if len(result) > width:
        # 先依欄切，絕不留下寬字的半格。
        result = result[:width - 1]
        if result and result[-1] is not None and cw(result[-1].ch) == 2:
            result.pop()
        result += glyphs('…')
    result += [Cell()] * (width - len(result))
    rows[y][left:right] = result


def textpart(text, role='text', press=''):
    return (text, role, press)


def controls(card, index, count, choice, mode, note, level, dual, width):
    title, question, opts, rec, bg = card
    nav = [textpart('‹ p ', 'remember', 'p'), textpart(f'{index + 1}/{count}'),
           textpart(' n ›', 'remember', 'n')]
    options = [[textpart(f'{chr(97+i)} {label}' + (' ✧' if i == rec else '') +
                         (' → 已選' if choice == chr(97+i) else ''),
                         'warning' if choice == chr(97+i) else 'remember', chr(97+i))]
               for i, label in enumerate(opts)]
    m = [textpart('m 附註：' + (note or '（空）'), 'remember', 'm')]
    s = [textpart(f'[s] 送出已選 {int(bool(choice))} 張 · 另 {count-int(bool(choice))} 張未選', 'remember', 's')]
    if level in (2, 3):
        m = [textpart(cut('m 附註：' + (note or '（空）'), width - dw(s[0][0]) - 3), 'remember', 'm')]
    elif level >= 4:
        s = [textpart('[s] 送出', 'remember', 's')]
        m = [textpart(cut('m 附註：' + (note or '（空）'), width - sum(dw(p[0]) for p in nav+s) - 6), 'remember', 'm')]
    if level <= 1:
        if dual:
            navs = [nav]
        else:
            half = 2 if level == 0 else 1
            navs = [[textpart(f'  {j + 1}. 換到這張卡', 'remember', 'n' if j > index else 'p')]
                    for j in range(index-half, index+half+1)]
            navs[half] = [textpart('▸ ' + title, 'claude', f'card:{index}')]
        out = navs + options + [m, s]
    elif level == 2:
        out = [nav] + options + [m + [textpart(' · ')] + s]
    elif level == 3:
        out = [nav, options[0] + [textpart(' · ')] + options[1], m + [textpart(' · ')] + s]
    else:
        out = [options[0] + [textpart(' · ')] + options[1], nav + [textpart(' · ')] + m + [textpart(' · ')] + s]
    if mode == 'edit':
        entry = [textpart('m 附註：' + note + '▏', 'remember', 'm')]
        confirm = [textpart('Enter 確定', 'remember', 'enter'), textpart(' · '), textpart('Esc 取消', 'remember', 'esc')]
        if level <= 1:
            out[-2:] = [entry, confirm]
        elif level in (2, 3):
            out[0], out[-1] = confirm, entry
        else:
            out = [entry, confirm]
    elif mode == 'preview':
        out = [[textpart('預覽：' + title + ' → ' + choice + ' · 附註：' + note)]]
        out += [[] for _ in range((5 if dual else [9, 7, 4, 3, 2][level]) - 2)]
        out += [[textpart('[y] 確定送出', 'remember', 'y'), textpart(' · '), textpart('Esc 回去', 'remember', 'esc')]]
    return out


def overlay(base, spec, cur=0, choice='', mode='open', note='', removed=()):
    rows = copy.deepcopy(base)
    top, bottom, edge = (spec[k] for k in ('top', 'bottom', 'edge'))
    remain = [i for i in range(len(CARDS)) if i not in removed]
    index = remain.index(cur)
    card = CARDS[cur]
    title, question, opts, rec, bg = card
    count = len(remain)
    if mode.startswith('closed') and plain(rows[top]).startswith('╭'):
        for y in range(top, bottom + 1):
            rows[y][:edge + 5] = [Cell()] * min(len(rows[y]), edge + 5)
    elif not mode.startswith('closed'):
        # 留住上框線、宿主 [-]、左右框線與額度來源訊息。
        for y in range(top + 2, bottom):
            paint(rows, y, 2, edge - 1, [])
        ctrl = controls(card, index, count, choice, mode, note, spec['level'], spec['dual'], edge-3)
        start = bottom - len(ctrl)
        info = list(range(top + 2, start))
        if spec['level'] >= 4:
            paint(rows, top, 3, edge - 2, [textpart(cut(f'待決 {count} · 第 {index+1} 張 · {title}', edge-5))])
            paint(rows, top + 1, 2, edge - 1, [textpart(cut(question, edge-3))])
        else:
            info_rows = [[textpart(f'待決 {count} 張 · 第 {index+1} 張')],
                         [textpart('等你動手：實拍無此資料', 'inactive')]]
            for y, parts in zip(info, info_rows):
                paint(rows, y, 2, edge - 1, parts)
            remaining_info = info[2:]
            if spec['dual'] and remaining_info:
                sep = edge // 2
                for j, y in enumerate(remaining_info):
                    if cur < 2 or (removed == (1,) and cur == 2):
                        rows[y][2:sep] = base[y][2:sep]
                        if choice and title in plain(rows[y][2:sep]):
                            arrow = next((x for x, c in enumerate(rows[y][:sep]) if c and c.ch == '→'), None)
                            if arrow is not None and rows[y][arrow+1]:
                                rows[y][arrow+1] = replace(rows[y][arrow+1], ch=choice)
                    elif j < len(remain):
                        cid = remain[j]
                        paint(rows, y, 2, sep-1, [textpart(('▸ ' if cid == cur else '  ') + CARDS[cid][0])])
                    rows[y][sep] = Cell('│', role='inactive')
                right = sep+2
            else:
                right = 2
            if remaining_info:
                # 背景先裁、建議次之，卡片問題最後截。
                for j, y in enumerate(remaining_info[:-2]):
                    paint(rows, y, right, edge-1, [textpart('背景 ' + bg[j])] if j < len(bg) else [])
                if len(remaining_info) >= 2:
                    paint(rows, remaining_info[-2], right, edge-1, [textpart('建議 ' + opts[rec], 'inactive')])
                paint(rows, remaining_info[-1], right, edge-1, [textpart('▸ ' + title + '：' + question, 'claude')])
            else:
                paint(rows, info[-1], 2, edge-1, [textpart('▸ ' + title + '：' + question)])
        for y, parts in enumerate(ctrl, start):
            paint(rows, y, 2, edge-1, parts)
        # 下框線僅把現行鍵位標成可點；維持原字及原格。
        end = next((x for x, c in enumerate(rows[bottom]) if x > 15 and c and c.ch == '─'), 0)
        paint(rows, bottom, 4, end-2 if end > 4 else min(edge-2, 46),
              [textpart('p/n · m · s · w · Esc' if spec['level'] >= 4 else
                        'PgUp/PgDn · p/n · a/b · m · s · w · Esc', 'remember')])
        for x, c in enumerate(rows[bottom]):
            if c and c.ch in ('w', 's', 'm', 'p', 'n'):
                rows[bottom][x] = replace(c, press=c.ch)
    band = next(i for i, row in enumerate(rows) if '要你決定：' in plain(row))
    div = next(i for i, c in enumerate(rows[band]) if c and c.ch == '│')
    paint(rows, band, 0, div-1, [textpart(cut('‣ 要你決定：' + question, div-1), 'warning', 'open')])
    # 固定右端原有展開鍵所在格；不重排其他 band 區段。
    r = plain(rows[band])
    old = '▴收合' if '▴收合' in r else '▾展開'
    at = next((i for i, c in enumerate(rows[band]) if c and c.ch == old[0]), None)
    if at is not None:
        paint(rows, band, at, at+5, [textpart('▾展開' if mode.startswith('closed') else '▴收合', 'remember', 'open' if mode.startswith('closed') else 'w')])
    return rows


def lum(rgb):
    v = [x / 255 for x in rgb]
    v = [x / 12.92 if x <= .04045 else ((x + .055) / 1.055) ** 2.4 for x in v]
    return sum(a*b for a, b in zip(v, (.2126, .7152, .0722)))


def contrast(a, b):
    a, b = lum(a), lum(b)
    return (max(a, b)+.05) / (min(a, b)+.05)


def upper_clicks(rows, spec):
    return [(y, x) for y in range(spec['top'], spec['top'] + spec['height']//2)
            for x, c in enumerate(rows[y]) if c and c.press]


def width_errors(rows, width):
    return [(y, len(row), dw(plain(row))) for y, row in enumerate(rows)
            if len(row) != width or dw(plain(row)) != width]


def contrast_errors(theme, role, threshold):
    ratio = contrast(THEMES[theme][role], THEMES['background'][theme])
    return [] if ratio >= threshold else [(theme, role, ratio, threshold)]


def run_checks():
    out = ['R1 rev5 檢查（Python 標準庫）', '實拍只補足行尾空格；所有疊改右界不包含框線。']
    n = 0
    unique = set()
    for size in SIZES:
        w, h = map(int, size.split('x'))
        base, _ = read_path(size, 0)
        spec = layout(size, base)
        for cur in range(3):
            for choice in ('', 'a', 'b'):
                for mode in ('open', 'edit', 'preview', 'closed'):
                    rows = overlay(base, spec, cur, choice, mode, '測試附註' * 40)
                    assert len(rows) == h
                    assert not width_errors(rows, w), (size, cur, choice, mode)
                    for y, row in enumerate(rows):
                        assert len(row) == w and dw(plain(row)) == w, (size, cur, mode, y, len(row))
                        unique.add(plain(row))
                    assert not upper_clicks(rows, spec), (size, mode)
                    keys = {c.press for row in rows[spec['top']:spec['bottom']] for c in row if c and c.press}
                    expected = {'a', 'b', 'm', 's', 'p', 'n'} if mode == 'open' else {'m', 'enter', 'esc'} if mode == 'edit' else {'y', 'esc'} if mode == 'preview' else set()
                    assert expected <= keys, (size, mode, expected - keys)
                    out.append(f'狀態 {size}/卡{cur+1}/{choice or "未選"}/{mode}：各列欄寬 ' +
                               ','.join(str(dw(plain(r))) for r in rows) +
                               f'；上半部熱區 0；下半部必要鍵 {sorted(expected)} 全在。')
                    # pane 與 band 以外，包含 prompt 與宿主抬頭，逐格不變。
                    for y in range(h):
                        if not spec['top'] <= y <= spec['bottom'] and '要你決定：' not in plain(base[y]):
                            assert rows[y] == base[y]
                    if mode != 'closed':
                        for y in range(spec['top']+1, spec['bottom']):
                            assert rows[y][spec['edge']] == base[y][spec['edge']], (size, y, '外框')
                    n += 1
        out.append(f'{size}：H={spec["height"]}、ctrlCap={spec["cap"]}、L{spec["level"]}、雙欄={spec["dual"]}，逐列寬度與 R1 通過')
    out.append(f'共 {n} 狀態、{len(unique)} 去重列；逐格寬度、R1、下半部必要熱鍵、原格保留通過。')
    for size in SIZES:
        initial, _ = read_path(size, 0)
        spec = layout(size, initial)
        for cur, choice, mode, p in PATH_STATES:
            base, _ = read_path(size, p)
            removed = (1,) if p >= 7 else ()
            rows = overlay(base, spec, cur, choice, mode,
                           '先跑 canary' if cur == 1 and p >= 4 else '', removed)
            assert not upper_clicks(rows, spec)
            assert all(len(row) == len(base[y]) for y, row in enumerate(rows))
            if p == 8:
                band = next(y for y, row in enumerate(base) if '要你決定：' in plain(row))
                assert all(rows[y] == base[y] for y in range(len(base)) if y != band)
    out.append('八步核心路徑 × 三尺寸：原格保留通過；收合實拍的對話內容不清除。')
    for theme in ('dark', 'light'):
        for role in ('text', 'inactive', 'remember', 'warning', 'claude'):
            ratio = contrast(THEMES[theme][role], THEMES['background'][theme])
            threshold = 3 if role in ('inactive', 'claude') else 4.5
            assert not contrast_errors(theme, role, threshold)
            out.append(f'{theme} {role}：{ratio:.3f} ≥ {threshold}，通過' + ('；粗體' if role == 'claude' else ''))
    out.append('對比範圍：新增角色；實拍原有 SGR 原樣保留，不以改色掩飾現行對比。')
    bad = copy.deepcopy(base); bad[0] += glyphs('中')
    failures = width_errors(bad, w)
    assert failures
    out.append(f'寬度反向探針：多加「中」→ 同一檢查報錯 {failures}。')
    bad = copy.deepcopy(base); bad[spec['top']+1][2] = Cell('a', press='a')
    assert upper_clicks(bad, spec)
    out.append('R1 反向探針：上半部種入 key:a → 成功抓到。')
    assert contrast_errors('dark', 'subtle', 3)
    out.append('對比反向探針：structure 改回 subtle → 深色對比 <3，成功抓到。')
    for kind in ('width', 'r1', 'contrast'):
        result = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--probe', kind],
                                capture_output=True, text=True)
        assert result.returncode == 1, (kind, result.returncode)
        out.append(f'反向探針子程序 --probe {kind}：退出碼 {result.returncode}（預期 1）；{result.stdout.strip()}')
    (HERE / 'checks.txt').write_text('\n'.join(out) + '\n')
    print(out[-4:])


def write_diff_list():
    lines = ['# R1 rev5 逐步差異清單', '',
             '由 gen.py 從同尺寸 ANSI 實拍逐格比較產生。列、格從 0 起算；只補足實拍行尾空格。', '',
             '## 實拍與配置', '',
             '- 主畫面優先使用 live-path；一般畫面附錄使用 live-shots/normal 的 .ans。',
             '- 80×24：保留 H＝5 外框，L4；120×40：保留 H＝13 外框，L2；200×50：實拍 H＝18，依公式為雙欄 L0。',
             '- 計畫表列 H＝17，但實拍為 H＝18；保留實際框高，以 §3.1 的 ctrlCap 公式計算，沒有重畫或移動整個 pane。',
             '- 額度缺來源、宿主抬頭、prompt、狀態列與 pane 外的對話逐格保留；閉合時只清除原 pane 範圍。',
             '- 等你動手區塊的實際資料未拍到，顯示「實拍無此資料」；資料內容（計畫未定），不捏造待處理數。', '',
             '## 八步對照', '']
    for size in SIZES:
        initial, _ = read_path(size, 0)
        spec = layout(size, initial)
        lines += [f'### {size}', '']
        for k, (cur, choice, mode, p) in enumerate(PATH_STATES, 1):
            base, path = read_path(size, p)
            rows = overlay(base, spec, cur, choice, mode, '先跑 canary' if cur == 1 and p >= 4 else '', (1,) if p >= 7 else ())
            approximate = k in (1, 3) or size == '80x24' and k in (2, 4, 5, 6)
            lines += [f'第 {k} 步：{STEPS[k-1]}；現行 `{path}`。' +
                      ('現行無此狀態，配最接近的。' if approximate else ''), '']
            for y, (old, new) in enumerate(zip(base, rows)):
                changed = [x for x, (a, b) in enumerate(zip(old, new)) if a != b]
                if not changed:
                    continue
                if '要你決定：' in plain(old):
                    explanation = 'band 跟隨同一張卡，保持既有左右分區與展開鍵原格（§3.6、D3）'
                elif mode.startswith('closed'):
                    explanation = '刻意的空白：新設計這一步 pane 尚未展開，只留 band 提醒；此列是現行 pane 的位置（§3.6）'
                elif y == spec['bottom']:
                    explanation = '下框線顯示背景捲動及控制鍵，頁籤位置不變（§3.2–3.3）'
                elif size == '80x24' and y <= spec['top']+1:
                    explanation = 'H＝5 時分類及卡名合入上框線，問題留唯一資訊列（§3.2、§3.3 L4）'
                else:
                    explanation = '資訊不可點，等你動手及卡片問題在上、換卡選項附註送出在下（§3.1–3.4、D1、D2、D5）'
                lines.append(f'- 第 {y} 列，第 {min(changed)}–{max(changed)} 格：{explanation}。')
            lines.append('')
    lines += ['## 偏離與範圍', '',
              '- D1：選項、附註及送出沉到下半部，照 §3.1–3.3；長附註截斷，不擠掉送出鍵。',
              '- D2：雙欄左清單不可點，換卡入口位於下半部；照 §3.4。',
              '- D3：band 仍為底部一列；沿用原有其他區段及右端展開格。',
              '- D4：本次只做待決流程示意；未實作外掛退役或 /forge-stats 轉填命令。§6 的 PM 裁決優先於 §3.6 的舊 view.page 草案。',
              '- D5：依資訊區剩餘列數判斷雙欄；120×40 實拍原為雙欄，新設計依 §3.4 改為單欄。',
              '- 非核心路徑的第 4–21 張卡有清單實拍，未提供全部明細；未拍到的選項不作真資料宣稱（計畫未定）。',
              '- 實拍原 SGR RGB、粗體、dim、背景及反白保留。新增角色由 themes-cc.json 取值；未改 AP 色票。',
              '- 未變更 AP 或 forge-stats 程式碼，未跑 AP 全套執行期測試、未換 live pin、未推送或合併。', '',
              '## 開啟與驗證', '',
              '- 開啟同目錄 `r1-rev5-interactive.html`；不需伺服器或外部資源。',
              '- `python3 doc/design/r1-rev5/gen.py`：重產 HTML、checks.txt、本差異清單。',
              '- `bash doc/design/r1-rev5/measure.sh`：48 組截圖、96 張圖逐列量測、六組反向探針及瀏覽器互動檢查。',
              '- 量測網址參數：`?measure=1&step=6&size=120x40&theme=dark&view=both`；可選 `view=new|live`。',
              '- `nogrid=1` 移除格子包裝，應報出 max−min＞0；`selftest=1` 跑頁內鍵盤與點擊檢查。', '']
    (HERE / 'diff-list.md').write_text('\n'.join(lines))


def reference_frames():
    result = {}
    for size in SIZES:
        first, _ = read_path(size, 0)
        spec = layout(size, first)
        frames = []
        for cur, choice, mode, p in PATH_STATES:
            base, _ = read_path(size, p)
            rows = overlay(base, spec, cur, choice, mode,
                           '先跑 canary' if cur == 1 and p >= 4 else '', (1,) if p >= 7 else ())
            frames.append([plain(row) for row in rows])
        result[size] = frames
    return result


def encode_cell(c):
    if c is None:
        return None
    return [c.ch, c.fg, c.bg, int(c.bold), int(c.dim), int(c.inverse), c.role, c.press]


def export_sources():
    data = {}
    bank = []
    lookup = {}
    def intern(rows):
        ids = []
        for row in rows:
            runs = []
            for c in row:
                if c is None:
                    continue
                encoded = encode_cell(c)
                if runs and runs[-1][1:] == encoded[1:]:
                    runs[-1][0] += c.ch
                else:
                    runs.append(encoded)
            key = json.dumps(runs, ensure_ascii=False)
            if key not in lookup:
                lookup[key] = len(bank)
                bank.append(runs)
            ids.append(lookup[key])
        return ids
    for size in SIZES:
        states = {}
        for p in range(10):
            rows, path = read_path(size, p)
            states[str(p)] = dict(rows=intern(rows), path=path)
        base, _ = read_path(size, 0)
        appendix = []
        w, h = map(int, size.split('x'))
        path = HERE / 'live-shots' / 'normal' / f'S-{size}-p2.ans'
        appendix = intern(source(path, w, h))
        data[size] = dict(states=states, spec=layout(size, base), appendix=appendix)
    return dict(sizes=data, bank=bank)


CSS = r'''
:root{color-scheme:dark light;--page:#f4f4f4;--fg:#171717;--panel:#fff;--line:#888;--focus:#905000}
:root[data-theme=dark]{--page:#191919;--fg:#eee;--panel:#262626;--line:#888;--focus:#ffc107}
*{box-sizing:border-box}body{margin:16px;font:14px system-ui,sans-serif;background:var(--page);color:var(--fg)}
h1{font-size:21px;margin:8px 0}header{display:flex;gap:18px;align-items:center;flex-wrap:wrap}
button,select,input{font:inherit;background:var(--panel);color:var(--fg);border:1px solid var(--line);padding:5px 9px;border-radius:4px}
button,[data-press]{cursor:pointer}button[aria-current=true]{outline:2px solid var(--focus)}
nav{display:flex;gap:6px;flex-wrap:wrap;margin:12px 0}#steps{margin-bottom:10px}#steps button{font-size:12px}
.comparison{display:flex;gap:18px;align-items:flex-start;width:max-content;min-width:100%}.shot{margin:0}
.shot figcaption{margin:7px 0;max-width:70ch}.terminal{background:var(--terminal-bg);color:var(--terminal-fg);padding:9px;border:1px solid var(--line);width:max-content}
pre.grid{font-family:Menlo,Consolas,"DejaVu Sans Mono","Liberation Mono",monospace;font-size:12px;line-height:1.2;margin:0;display:block;width:max-content;font-variant-ligatures:none;font-kerning:none}
pre.grid .ln{display:block;white-space:pre;width:max-content;min-height:1.2em;position:relative}
pre.grid i{font-style:normal;display:inline-block;text-align:center;vertical-align:top;line-height:1.2}
pre.grid i.n{width:var(--cw)}pre.grid i.w{width:calc(var(--cw) * 2);font-family:"PingFang TC","Microsoft JhengHei","Noto Sans CJK TC","WenQuanYi Zen Hei",sans-serif}
.ln.changed{outline:2px solid var(--focus);outline-offset:-1px}
[data-press]:hover{outline:2px solid var(--focus);z-index:2;position:relative}
.explain{max-width:100ch;line-height:1.6}#note-wrap{display:none;margin:8px 0}#note{width:35em;max-width:80vw}
#measure-out{white-space:pre-wrap;max-width:100vw;overflow-wrap:anywhere;font-size:10px}details{margin:18px 0}
'''

JS = r'''
const q=new URLSearchParams(location.search), $=id=>document.getElementById(id);
let size=q.get('size')||'120x40', step=Number(q.get('step')||1), view=q.get('view')||'both';
if(!DATA.sizes[size])size='120x40';step=Math.min(8,Math.max(1,step));
let theme=q.get('theme')|| (matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light');
let cur=0,selected={},notes={},removed=[],mode='closed',editing=false,note='',scroll=0,liveP=0;
const blank=()=>[' ',null,null,0,0,0,'',''];
const width=c=>WIDE.has(c)?2:1;
function cells(text,role='text',press=''){
 const out=[];for(const ch of text){out.push([ch,null,null,+['warning','claude'].includes(role),0,0,role,press]);if(width(ch)===2)out.push(null)}return out;
}
function decode(ids){return ids.map(id=>DATA.bank[id].flatMap(run=>{const out=[];for(const ch of run[0]){out.push([ch,...run.slice(1)]);if(width(ch)===2)out.push(null)}return out}))}
function plain(row){return row.filter(Boolean).map(c=>c[0]).join('')}
function cut(text,w){let out='',used=0;if([...text].reduce((s,c)=>s+width(c),0)<=w)return text;for(const c of text){if(used+width(c)>w-1)break;out+=c;used+=width(c)}return out+'…'}
function part(t,r='text',k=''){return [t,r,k]}
function paint(rows,y,left,right,parts){
 let a=parts.flatMap(p=>cells(...p)),w=right-left;
 if(a.length>w){a=a.slice(0,w-1);if(a.length&&a.at(-1)&&width(a.at(-1)[0])===2)a.pop();a.push(...cells('…'))}
 while(a.length<w)a.push(blank());rows[y].splice(left,w,...a);
}
function active(){return CARDS.map((_,i)=>i).filter(i=>!removed.includes(i))}
function controls(spec){
 const ids=active(),index=ids.indexOf(cur),count=ids.length,card=CARDS[cur],choice=selected[cur]||'',level=spec.level;
 const nav=[part('‹ p ','remember','p'),part(`${index+1}/${count}`),part(' n ›','remember','n')];
 const opts=card[2].length?card[2].map((label,i)=>[part(`${String.fromCharCode(97+i)} ${label}${i===card[3]?' ✧':''}${choice===String.fromCharCode(97+i)?' → 已選':''}`,choice===String.fromCharCode(97+i)?'warning':'remember',String.fromCharCode(97+i))]):[[part('選項：缺少此卡實拍明細','inactive')],[]];
 let m=[part('m 附註：'+(notes[cur]||'（空）'),'remember','m')];
 const countSelected=Object.keys(selected).length;
 let s=[part(`[s] 送出已選 ${countSelected} 張 · 另 ${count-countSelected} 張未選`,'remember','s')];
 const dw=t=>[...t].reduce((s,c)=>s+width(c),0),iw=spec.edge-3;
 if(level===2||level===3)m=[part(cut('m 附註：'+(notes[cur]||'（空）'),iw-dw(s[0][0])-3),'remember','m')];
 if(level>=4){s=[part('[s] 送出','remember','s')];m=[part(cut('m 附註：'+(notes[cur]||'（空）'),iw-nav.reduce((a,p)=>a+dw(p[0]),0)-dw(s[0][0])-6),'remember','m')];}
 let out;
 if(level<=1){let navs;if(spec.dual)navs=[nav];else {const half=level===0?2:1;navs=[];for(let j=index-half;j<=index+half;j++){const id=ids[j];navs.push(id===undefined?[]:[part((id===cur?'▸ ':'  ')+CARDS[id][0],'remember','card:'+id)])}}
 out=[...navs,...opts,m,s];}
 else if(level===2)out=[nav,...opts,[...m,part(' · '),...s]];
 else if(level===3)out=[nav,[...opts[0],part(' · '),...opts[1]],[...m,part(' · '),...s]];
 else out=[[...opts[0],part(' · '),...opts[1]],[...nav,part(' · '),...m,part(' · '),...s]];
 if(editing){const entry=[part('m 附註：'+note+'▏','remember','m')],confirm=[part('Enter 確定','remember','enter'),part(' · '),part('Esc 取消','remember','esc')];
 if(level<=1)out.splice(-2,2,entry,confirm);else if(level<=3){out[0]=confirm;out[out.length-1]=entry}else out=[entry,confirm];}
 if(mode==='preview'){
  const entries=Object.entries(selected).map(([id,ch])=>[part(`${CARDS[id][0]} → ${ch} ${CARDS[id][2][ch.charCodeAt(0)-97]} · 附註：${notes[id]||'（空）'}`)]);
  const n=out.length;out=entries.slice(0,n-1);if(entries.length>n-1)out[n-2]=[part(`…另 ${entries.length-n+2} 張`)];while(out.length<n-1)out.push([]);
  out.push([part('[y] 確定送出','remember','y'),part(' · '),part('Esc 回去','remember','esc')]);
 }
 return out;
}
function newRows(base,spec){
 const rows=structuredClone(base),{top,bottom,edge}=spec,ids=active(),count=ids.length,index=ids.indexOf(cur),card=CARDS[cur];
 if(mode==='closed'&&plain(rows[top]).startsWith('╭')){for(let y=top;y<=bottom;y++)paint(rows,y,0,Math.min(rows[y].length,edge+5),[])}
 else if(mode!=='closed'){
  for(let y=top+2;y<bottom;y++)paint(rows,y,2,edge-1,[]);
  const ctrl=controls(spec),start=bottom-ctrl.length,info=[];for(let y=top+2;y<start;y++)info.push(y);
  if(spec.level>=4){paint(rows,top,3,edge-2,[part(cut(`待決 ${count} · 第 ${index+1} 張 · ${card[0]}`,edge-5))]);paint(rows,top+1,2,edge-1,[part(cut(card[1],edge-3))]);}
  else {
   paint(rows,info[0],2,edge-1,[part(`待決 ${count} 張 · 第 ${index+1} 張`)]);
   paint(rows,info[1],2,edge-1,[part('等你動手：實拍無此資料','inactive')]);
   const rest=info.slice(2);let right=2;
   if(spec.dual){const sep=Math.floor(edge/2);right=sep+2;const sourceCur=liveP===0?0:liveP>=7?2:1;
    const sameList=cur===sourceCur&&count===(liveP>=7?20:21)&&(!removed.length||(removed.length===1&&removed[0]===1));
    rest.forEach((y,j)=>{const id=ids[j];if(sameList){rows[y].splice(2,sep-2,...structuredClone(base[y].slice(2,sep)));
     if(selected[cur]&&plain(rows[y].slice(2,sep)).includes(card[0])){const arrow=rows[y].findIndex((c,x)=>x<sep&&c&&c[0]==='→');if(arrow>=0&&rows[y][arrow+1])rows[y][arrow+1][0]=selected[cur];}
    }else if(id!==undefined)paint(rows,y,2,sep-1,[part((id===cur?'▸ ':'  ')+String(j+1).padStart(2,'0')+' '+CARDS[id][0]+(selected[id]?' →'+selected[id]:''))]);rows[y][sep]=structuredClone(base[y][sep]);});}
   if(rest.length){const bg=card[4],space=Math.max(0,rest.length-2),offset=Math.min(scroll,Math.max(0,bg.length-space));
    rest.slice(0,-2).forEach((y,j)=>paint(rows,y,right,edge-1,offset+j<bg.length?[part((j===0&&offset?`↑ 上面還有 ${offset} 列 · `:'背景 ')+bg[offset+j])]:[]));
    if(rest.length>=2)paint(rows,rest.at(-2),right,edge-1,[part(card[3]>=0?'建議 '+card[2][card[3]]:'建議：缺少此卡實拍明細','inactive')]);
    paint(rows,rest.at(-1),right,edge-1,[part('▸ '+card[0]+'：'+card[1],'claude')]);
   }else paint(rows,info.at(-1),2,edge-1,[part('▸ '+card[0]+'：'+card[1])]);
  }
  ctrl.forEach((parts,j)=>paint(rows,start+j,2,edge-1,parts));
  // 下框線只疊改鍵位提示；保留頁籤與邊線原格。
  const end=rows[bottom].findIndex((c,x)=>x>15&&c&&c[0]==='─');
  paint(rows,bottom,4,end>4?end-2:Math.min(edge-2,46),[part(spec.level>=4?'p/n · m · s · w · Esc':'PgUp/PgDn · p/n · a/b · m · s · w · Esc','remember')]);
  rows[bottom].forEach(c=>{if(c&&['w','s','m','p','n'].includes(c[0]))c[7]=c[0]});
 }
 const band=rows.findIndex(r=>plain(r).includes('要你決定：')),div=rows[band].findIndex(c=>c&&c[0]==='│');
 paint(rows,band,0,div-1,[part(cut('‣ 要你決定：'+card[1],div-1),'warning','open')]);
 // 只換待決數字的原格，不挪動其他 band 區段。
 const str=plain(rows[band]),match=/待決\s+(\d+)/.exec(str);
 if(match){const charAt=str.indexOf(match[1],match.index),col=[...str.slice(0,charAt)].reduce((s,c)=>s+width(c),0);paint(rows,band,col,col+match[1].length,[part(String(count),'text','open')]);}
 const at=rows[band].findIndex(c=>c&&['▴','▾'].includes(c[0]));
 if(at>=0)paint(rows,band,at,at+5,[part(mode==='closed'?'▾展開':'▴收合','remember',mode==='closed'?'open':'w')]);
 return rows;
}
function rgb(v){return `rgb(${v.join(',')})`}
function cellStyle(c){let fg=c[6]?THEMES[theme][c[6]]:(c[1]||THEMES[theme].text),bg=c[2]||THEMES.background[theme];
 if(c[5])[fg,bg]=[bg,fg];if(c[4])fg=fg.map((x,i)=>Math.round((x+bg[i])/2));
 return `color:${rgb(fg)};background-color:${rgb(bg)};font-weight:${c[3]?'700':'400'}`;}
function grid(rows,label,diffs){
 const pre=document.createElement('pre');pre.className='grid';pre.dataset.label=label;pre.dataset.cols=size.split('x')[0];pre.setAttribute('aria-label',label);
 // 量出 monospace 的一格後固定成 px；避免 CJK 字型自己的 ch 單位改變寬度。
 pre.style.setProperty('--cw',CELL_WIDTH+'px');
 rows.forEach((row,y)=>{const ln=document.createElement('span');ln.className='ln';ln.dataset.row=y;
  if(diffs&&diffs[y].some(Boolean))ln.classList.add('changed');
  row.forEach((c,x)=>{if(!c)return;const el=document.createElement(q.has('nogrid')?'span':'i');
   if(!q.has('nogrid'))el.className=width(c[0])===2?'w':'n';
   if(diffs&&diffs[y][x])el.classList.add('cell-diff');el.style.cssText=cellStyle(c);el.textContent=c[0];
   if(c[7]){el.dataset.press=c[7];el.title='按 '+c[7];}ln.append(el);
  });pre.append(ln);
 });return pre;
}
const ruler=document.createElement('span');ruler.style.cssText='position:absolute;visibility:hidden;font:12px "DejaVu Sans Mono",monospace';ruler.textContent='0000000000';document.body.append(ruler);const CELL_WIDTH=Math.floor(ruler.getBoundingClientRect().width/10*64)/64;ruler.remove();
function render(){
 document.documentElement.dataset.theme=theme;document.documentElement.style.setProperty('--terminal-bg',rgb(THEMES.background[theme]));document.documentElement.style.setProperty('--terminal-fg',rgb(THEMES[theme].text));
 const state=DATA.sizes[size].states[liveP],base=decode(state.rows),spec=DATA.sizes[size].spec,newer=newRows(base,spec);
 const diffs=base.map((row,y)=>row.map((c,x)=>JSON.stringify(c)!==JSON.stringify(newer[y][x])));
 $('screens').replaceChildren();
 for(const kind of (view==='both'?['new','live']:[view])){
  const fig=document.createElement('figure');fig.className='shot';const cap=document.createElement('figcaption');
  cap.textContent=(kind==='new'?'新設計':'現行實拍')+` · ${size} · `+(kind==='live'?state.path:`L${spec.level}，框高 ${spec.height}`);
  const box=document.createElement('div');box.className='terminal';box.append(grid(kind==='new'?newer:base,kind,diffs));fig.append(cap,box);$('screens').append(fig);
 }
 const liveCur=liveP===0?0:liveP>=7?2:1;
 const approx=step===1||step===3||cur!==liveCur||(selected[cur]==='b')||(editing&&note!=='先跑 canary')||(size==='80x24'&&[2,4,5,6].includes(step));
 $('description').textContent=`第 ${step} 步：${STEPS[step-1]}。`+(approx?'現行無此狀態，配最接近的。':'')+`目前「${CARDS[cur][0]}」，待決 ${active().length} 張。`+(mode==='preview'?'預覽中；按 y 確定，Esc 回去。':'')+(mode==='sent'?'已送出，卡片已從 pane 與 band 移除。':'');
 $('difference').textContent='外框標出有差異的列。控制區沉到下半部（§3.1–3.3、D1）；左欄不可點，改以下方換卡（§3.4、D2、D5）；等你動手明示缺資料（§3.2；實拍缺此資料，內容計畫未定）；band 共用游標及計數（§3.6）。'+(mode==='closed'?'收合後只留原有 band（§3.6）。':'')+(mode==='closed'&&liveP===0?'【刻意設計】新設計這一步 pane 還沒展開，只有最下面的 band 一列在提醒；上面成排的空白橘框，是現行實拍裡 pane 所在的位置，新設計在這一步把它收起來，所以是空的。按「下一步」或在 band 上點「▾展開」就會出現 pane。':'');
 $('note-wrap').style.display=editing?'block':'none';if(editing&&document.activeElement!==$('note')){$('note').value=note;}
 $('size').value=size;$('theme').value=theme;document.querySelectorAll('#steps button').forEach((b,i)=>b.setAttribute('aria-current',String(i===step-1)));
 const rowsInfo=newer.map((r,y)=>({y,width:r.length,upper: y>=spec.top&&y<spec.top+Math.floor(spec.height/2),press:r.filter(c=>c&&c[7]).map(c=>c[7])}));
 window.R1={step,size,theme,cur,selected:{...selected},notes:{...notes},removed:[...removed],mode,liveP,rows:rowsInfo,terminal:newer.map(plain),spec};
 if($('appendix').open)renderNormal();
 if(q.has('measure'))measure();
}
function preset(n){step=n;cur=n>=7?2:n>=4?1:0;selected=n===5||n===6?{1:'a'}:{};removed=n>=7?[1]:[];notes=n>=6?{1:'先跑 canary'}:{};mode=n===1||n===8?'closed':n===7?'sent':'open';editing=n===6;note=editing?'先跑 canary':'';scroll=0;liveP=[0,0,0,1,2,4,7,8][n-1];render();}
function press(key){
 if(editing){if(key==='enter'){notes[cur]=note;editing=false;liveP=5;render();}else if(key==='esc'){editing=false;note=notes[cur]||'';liveP=2;render();}else if(key==='m')$('note').focus();return;}
 if(key==='esc'){if(mode==='preview'){mode='open';liveP=5;}else mode='closed';render();return;}
 if(mode==='preview'){if(key==='y'){const ids=active(),index=ids.indexOf(cur),gone=Object.keys(selected).map(Number);removed.push(...gone);selected={};const after=active();cur=after[Math.min(index,after.length-1)]??0;mode='sent';step=7;liveP=7;render();}return;}
 if(key==='open'||key==='enter'){mode='open';if(liveP>=8)liveP=removed.length?7:0;step=step===1?2:step;render();return;}
 if(key==='w'){mode='closed';step=8;liveP=removed.length?8:0;render();return;}
 if(key==='n'||key==='p'||key==='j'||key==='k'||key.startsWith('card:')){const ids=active(),at=ids.indexOf(cur);cur=key.startsWith('card:')?Number(key.slice(5)):ids[(at+(key==='n'||key==='j'?1:-1)+ids.length)%ids.length];scroll=0;mode='open';step=4;liveP=cur===0?0:cur===1?1:7;render();return;}
 if(['a','b'].includes(key)){if(!CARDS[cur][2].length){$('feedback').textContent='此卡只有清單實拍，缺少選項明細；核心路徑請用前三張卡。';return;}selected[cur]=key;step=5;mode='open';liveP=2;render();return;}
 if(key==='m'){editing=true;note=notes[cur]||'';mode='open';step=6;liveP=3;render();$('note').value=note;$('note').focus();return;}
 if(key==='s'){if(!Object.keys(selected).length){$('feedback').textContent='請先按 a 或 b 選一個選項。';return;}mode='preview';step=7;liveP=6;render();return;}
 if(key==='pgdn'||key==='pgup'){scroll=Math.max(0,scroll+(key==='pgdn'?4:-4));render();}
}
function measure(){
 const figures=[...document.querySelectorAll('pre.grid')].filter(p=>p.getBoundingClientRect().height>0);
 const result=figures.map(p=>{const widths=[...p.querySelectorAll('.ln')].map(l=>l.getBoundingClientRect().width),max=Math.max(...widths),min=Math.min(...widths),expected=CELL_WIDTH*Number(p.dataset.cols);
  return {view:p.dataset.label,rows:widths.length,widths,max,min,spread:max-min,expected,presetEqual:widths.every(w=>Math.abs(w-expected)<.1)};});
 $('measure-out').textContent=JSON.stringify({step,size,theme,nogrid:q.has('nogrid'),figures:result,r1:window.R1.rows.filter(r=>r.upper&&r.press.length),status:'已量測'},null,2);
}
document.addEventListener('click',e=>{const el=e.target.closest('[data-press]');if(el){e.preventDefault();press(el.dataset.press)}});
document.addEventListener('keydown',e=>{
 if(e.target===$('note')){if(e.key==='Enter'||e.key==='Escape'){e.preventDefault();press(e.key==='Enter'?'enter':'esc')}return;}
 if(['SELECT','INPUT'].includes(e.target.tagName)&&!['Escape'].includes(e.key))return;
 if(e.target.tagName==='BUTTON'&&e.key==='Enter')return;
 const keys={ArrowDown:'n',ArrowUp:'p',PageDown:'pgdn',PageUp:'pgup',Enter:'enter',Escape:'esc'};const key=keys[e.key]||e.key;
 if(['n','p','j','k','a','b','m','s','y','w','enter','esc','pgdn','pgup'].includes(key)){e.preventDefault();press(key);}
});
$('note').addEventListener('input',e=>{note=e.target.value;liveP=note?4:3;render()});
$('size').onchange=e=>{size=e.target.value;render()};$('theme').onchange=e=>{theme=e.target.value;render()};
$('previous').onclick=()=>preset(Math.max(1,step-1));$('next').onclick=()=>preset(Math.min(8,step+1));
STEPS.forEach((text,i)=>{const b=document.createElement('button');b.textContent=`${i+1}. ${text}`;b.onclick=()=>preset(i+1);$('steps').append(b)});
function renderNormal(){$('normal').replaceChildren();const box=document.createElement('div');box.className='terminal';box.append(grid(decode(DATA.sizes[size].appendix),'normal'));$('normal').append(box)}
$('appendix').addEventListener('toggle',()=>{if($('appendix').open)renderNormal()});
preset(step);window.r1Press=press;window.r1Preset=preset;
if(q.has('selftest')){
 const results=[],assert=(name,condition)=>{results.push({name,pass:!!condition});if(!condition)throw Error(name)};
 try {
  const key=k=>document.dispatchEvent(new KeyboardEvent('keydown',{key:k,bubbles:true,cancelable:true}));
  const click=k=>{const el=document.querySelector(`#screens [data-press="${k}"]`);assert('可點 '+k,!!el);el.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}));};
  preset(1);click('open');assert('展開保留同一張卡',cur===0&&mode==='open');
  key('n');assert('n 換卡且 band 同步',cur===1&&window.R1.terminal.some(r=>r.includes('要你決定：forge-stats')));
  key('p');assert('p 回上一張',cur===0);key('ArrowDown');assert('方向鍵換卡',cur===1);
  click('b');assert('按 b 選 B',selected[1]==='b');assert('選 B 後左清單不殘留 A',!window.R1.terminal.some(r=>r.includes('→a')));key('a');assert('按 a 選 A',selected[1]==='a');
  click('m');assert('m 進附註',editing);$('note').value='先跑 canary';$('note').dispatchEvent(new Event('input',{bubbles:true}));key('n');assert('附註輸入暫停換卡',cur===1);click('enter');assert('附註存好',notes[1]==='先跑 canary'&&!editing);
  click('s');assert('s 只預覽',mode==='preview'&&removed.length===0);click('esc');assert('Esc 退回預覽一層',mode==='open'&&removed.length===0);
  click('s');click('y');assert('送出後兩邊消失且計數減一',removed.includes(1)&&active().length===20&&cur===2&&!window.R1.terminal.some(r=>r.includes('外掛更新時機')));
  key('w');assert('w 收合',mode==='closed');click('open');assert('重新展開仍在同一張卡',cur===2);
  // 原格、框線、上半部熱區及長附註，以瀏覽器實際渲染的資料檢查。
  for(let n=1;n<=8;n++){preset(n);const info=window.R1,w=Number(size.split('x')[0]);assert(`步驟 ${n} 每列欄數`,info.rows.every(r=>r.width===w));assert(`步驟 ${n} 上半部無熱區`,info.rows.every(r=>!r.upper||!r.press.length));assert(`步驟 ${n} 與 Python 檢查的逐格文字一致`,info.terminal.every((r,y)=>r===EXPECTED[size][n-1][y]));}
  preset(6);$('note').value='長附註'.repeat(100);$('note').dispatchEvent(new Event('input',{bubbles:true}));assert('長附註不超寬',window.R1.rows.every(r=>r.width===Number(size.split('x')[0])));
  click('esc');assert('Esc 取消附註',!editing);preset(Number(q.get('step')||1));
 }catch(error){results.push({name:error.message,pass:false})}
 window.R1_TESTS=results;
 const receipt=document.createElement('pre');receipt.id='browser-tests';receipt.textContent=JSON.stringify(results);document.body.append(receipt);
}
if(q.has('measure')){document.fonts.ready.then(()=>requestAnimationFrame(()=>{render();measure()}));}
'''


def write_html():
    data = export_sources()
    wide = sorted({ch for row in data['bank'] for run in row for ch in run[0]
                   if unicodedata.east_asian_width(ch) in 'WF'} |
                  {ch for card in CARDS for ch in str(card) if unicodedata.east_asian_width(ch) in 'WF'})
    # 新提示也可能用到底稿沒有的中文字，直接收全 BMP 寬字範圍。
    wide = ''.join(chr(c) for c in range(0x20, 0x10000)
                   if cw(chr(c)) == 2)
    head = '''<!doctype html><html lang="zh-Hant"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>R1 rev5 互動實拍對照</title><style>'''
    body = '''</style><header><h1>R1 rev5 互動實拍對照</h1>
<label>尺寸 <select id="size"><option>120x40</option><option>200x50</option><option>80x24</option></select></label>
<label>主題 <select id="theme"><option value="dark">深色</option><option value="light">淺色</option></select></label></header>
<p class="explain">以 AP develop 008e79826 實拍為底，完整保留 pane、band、輸入框與宿主狀態。這是離線互動示意，送出只改本頁狀態。</p>
<nav><button id="previous">上一步</button><button id="next">下一步</button></nav><nav id="steps" aria-label="核心路徑八步"></nav>
<p id="description" class="explain" aria-live="polite"></p><p id="difference" class="explain"></p>
<p class="explain">鍵位：n／p 或 ↑↓ 換卡；a／b 選；m 附註；Enter 存附註；s 預覽、y 確定；w 收合；Esc 退一層。PgUp／PgDn 捲背景。</p>
<div id="note-wrap"><label>附註 <input id="note" aria-label="附註輸入" autocomplete="off"></label> <button data-press="enter">存好附註</button> <button data-press="esc">取消</button></div>
<p id="feedback" aria-live="polite"></p><main id="screens" class="comparison"></main>
<details id="appendix"><summary>附錄：normal 一般畫面的現行實拍（同尺寸；pane 在對話內）</summary><p>來源：live-shots/normal/S-&lt;尺寸&gt;-p2.ans。保留原有 SGR；此附錄不套新設計。</p><div id="normal"></div></details>
<pre id="measure-out"></pre><script>'''
    constants = 'const DATA=' + json.dumps(data, ensure_ascii=False, separators=(',', ':')) + ';\n'
    constants += 'const THEMES=' + json.dumps(THEMES, ensure_ascii=False) + ';\n'
    constants += 'const CARDS=' + json.dumps(CARDS, ensure_ascii=False) + ';\n'
    constants += 'const STEPS=' + json.dumps(STEPS, ensure_ascii=False) + ';\n'
    constants += 'const EXPECTED=' + json.dumps(reference_frames(), ensure_ascii=False) + ';\n'
    constants += 'const WIDE=new Set(' + json.dumps(wide, ensure_ascii=False) + ');\n'
    (HERE / 'r1-rev5-interactive.html').write_text(head + CSS + body + constants.replace('</', '<\\/') + JS + '</script></html>')
    print('已產生 r1-rev5-interactive.html（單檔，無外部資源）')


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--probe':
        probe = sys.argv[2]
        base, _ = read_path('80x24', 0)
        spec = layout('80x24', base)
        if probe == 'width':
            base[0] += glyphs('中')
            errors = width_errors(base, 80)
        elif probe == 'r1':
            base[spec['top']+1][2] = Cell('a', press='a')
            errors = upper_clicks(base, spec)
        elif probe == 'contrast':
            errors = contrast_errors('dark', 'subtle', 3)
        else:
            raise SystemExit('未知探針')
        print(f'故意違規：{probe}；同一檢查報錯 {errors}')
        raise SystemExit(1 if errors else 0)
    check_sources()
    run_checks()
    write_html()
    write_diff_list()
