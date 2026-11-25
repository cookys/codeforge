#!/usr/bin/env python3
"""Console 九個分頁的設計稿（單 01M89Q2ZBX5F7W2XAMJK9KC3J8）。

骨架照 v2：寬螢幕左下 index、右下回覆區、右上來龍去脈；80 欄時 index 縮成回覆框第一列。
每一步是一筆資料（step(...)），畫法共用 v2/gen.py 的框線、寬度、上色函式。
鍵列放不下、band 出現 背景／卡住／來源／sha、index 摘要放不下，都直接 assert，不截斷。

用法：python3 gen.py out
輸出 out/<W>x<H>-s<N>/{screen.txt, roles.json, dark.ans, light.ans[, urls.json]}、
out/flow.json、out/coverage.json。
"""
import importlib.util
import json
import os
import shutil
import sys

_spec = importlib.util.spec_from_file_location('v2', os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'v2', 'gen.py'))
v = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(v)
T, G = v.T, v.G
TABS = v.TABS
URL = 'https://staging.shiye.test/pl02?v=241115'  # r2 codex 1：完整網址，80 欄放得下
CARDS = v.CARDS  # 合併 plan、外掛更新時機、預覽先拍幾欄、▲ 時冶 PL02 定案
CARDS = [{**c, 'steps': [x.replace(v.URL, URL) for x in c['steps']], 'detail': [(k, t.replace(v.URL, URL)) for k, t in c['detail']]} for c in CARDS]  # 網址換成完整版
SIZES = ((209, 49), (120, 40), (80, 24))
LW = 37  # 寬螢幕 index 欄寬（含左框線與中間直線）
BAND_BANNED = ('背景', '卡住', '來源', 'sha')  # check3 R7


# ---------- 小工具 ----------
def H(k, d):
    return (k, d)


def opt(letter, label, rec=False, why='', picked=False):
    return ('opt', letter, label, rec, why, picked)


def warn(s):
    return (s, 'warning')


def err(s):
    return (s, 'error')


def dim(s):
    return (s, 'inactive')


def ok(s):
    return (s, 'success')


def seg(item):
    """一列內容 → (文字, 角色)。"""
    return (item, 'text') if isinstance(item, str) else item


# ---------- 待決卡 ----------
def card_opts(c, picked=None):
    return [opt(l, lab, rec, why, picked == l) for l, lab, rec, why in c['opts']]


def card_detail(c):
    return [f'{k}   {s}' if k else f'       {s}' for k, s in c['detail']]


def decide_ix(focus, marks=None, sent=()):
    """待決 index：每張卡一列。marks {i: 'a'} 已選；sent 已送出；▲ 要先動手。"""
    marks = marks or {}
    rows = [dim(f'待決 {len(CARDS) - len(sent)} 張 · 已選 {len(marks)} 張' if marks else '待決 3 張 · 等你動手 1 張')]
    for i, c in enumerate(CARDS):
        m = '▸' if i == focus else ' '
        if i in sent:
            rows.append(ok(f'{m} ✓{i+1} {c["name"]}：{marks[i]} 已送出'))
        elif i in marks:
            rows.append(f'{m} ✓{i+1} {c["name"]}：{marks[i]}')
        else:
            rows.append(f'{m} {"▲" if c["steps"] else "·"}{i+1} {c["name"]}')
    return rows


def decide_compact(focus, marks=None, sent=()):
    marks = marks or {}
    for short in (False, True):
        parts = []
        for i, c in enumerate(CARDS):
            name = c['name'].split(' ')[0] if short else c['name']
            mk = '⇡' if i in sent else '✓' if i in marks else '▲' if c['steps'] else '·'
            parts.append(('▸' if i == focus else '') + mk + f'{i+1} {name}' + (f'：{marks[i]}' if i in marks else ''))
        s = '  '.join(parts)
        if v.dw(s) <= 76:
            return s
    return s


def card_reply(i, picked=None, note=None):
    c = CARDS[i]
    out = [('▸ ' + c['q'], 'text')]
    for n, s in enumerate(c['steps'], 1):
        out.append(f'  {n}. {s}')
    out += card_opts(c, picked)
    if note:
        out.append(dim('  m 附註：' + note))
    return out


# ---------- 每一步 ----------
# 欄位：
#   id tab state title  步號、分頁 1–9、狀態、標題
#   ix        寬螢幕 index 列；▸ 開頭的是焦點
#   compact   80 欄 index 那一列
#   reply     回覆區列；opt(...) 是選項
#   rscroll   回覆區捲到底（2-L3、8-L2）
#   pin       來龍去脈最上面固定的警告（不收起）；detail 其餘內容
#   hints / nhints  寬螢幕 / 80 欄鍵列（None = 該頁預設）；內容超出框時自動補 ↑↓、PgUp/PgDn
#   status    底部狀態列：dec act（待決／動手數，'—' 讀不到）、fleet（執行, 失敗, 卡住）或 '—'、speed、stale 集合
#   quota     最底 statusline 的 5h/7d：'ok' | 'missing' | 'stale'
#   key next  按什麼到下一步
S = []


def step(**k):
    for name, d in (('pin', []), ('rscroll', 0), ('hints', None), ('nhints', None), ('status', {}), ('quota', 'ok'), ('compact', '')):
        k.setdefault(name, d)
    S.append(k)


NEXT = '設計稿：換狀態'

# ===== 1 現況 =====
step(id='1-N', tab=1, state='N', title='正常',
     ix=[dim('要你處理'), '▸ 待決 3 張 · 動手 1 張', '  殘留 2 項', dim('艦隊'), '  產線 12：執行 8 · 失敗 2 · 等 2', dim('本席與審查'), '  本席 ctx 42% · QC 欠審 1'],
     compact='▸待決 3·動手 1  ·殘留 2  ·產線 12（失敗 2）  ·QC 欠審 1',
     reply=['待決 3 張要你選，另有 1 張要先動手做完再選。', '最舊的一張開了 3 小時：要不要把 tui 共用計畫合進 develop。', '按 2 到待決頁作答；按 o 展開這一段的清單。'],
     detail=['pm status 09:41 更新（1 分鐘前）。', '艦隊 12 條產線：執行 8、失敗 2、卡住 2。', '失敗：hangar 換池（3 輪）、docs 建置（1 輪）。', '本席 ctx 42%，本 session 240k tokens。', 'QC 欠審 1 條；代理決策 4 條（今天）。'],
     key=NEXT, next='1-E')
step(id='1-E', tab=1, state='E', title='有資料、全部正常',
     ix=[dim('要你處理'), '▸ 沒有待決 · 沒有殘留', dim('艦隊'), '  產線 3：全部執行中', dim('本席與審查'), '  本席 ctx 18% · QC 全過'],
     compact='▸沒有待決  ·殘留 0  ·產線 3 全部執行中  ·QC 全過',
     reply=[ok('✓ 目前沒有要你處理的事。'), '3 條產線都在跑；最近一次失敗是昨天 22:10，已重跑通過。', '有新卡時底列會亮黃色「待決 N」。'],
     detail=['pm status 09:41 更新（1 分鐘前）。', '艦隊 3 條產線全部執行中，沒有失敗或卡住。', '本席 ctx 18%；QC 全過；代理決策 0 條。'],
     status=dict(dec='0', act='0', fleet=(3, 0, 0)), key=NEXT, next='1-M')
step(id='1-M', tab=1, state='M', title='很多事',
     ix=[dim('要你處理'), '▸ 待決 12 張 · 動手 2 張', '  殘留 9 項', dim('艦隊'), '  產線 31：執行 19 · 失敗 7 · 等 5', dim('本席與審查'), '  本席 ctx 81% · QC 欠審 6'],
     compact='▸待決 12·動手 2  ·殘留 9  ·產線 31（失敗 7）  ·QC 欠審 6',
     reply=['待決 12 張、動手 2 張，最舊的開了 2 天。', '失敗 7 條產線裡有 4 條是同一個原因：hangar 換池逾時。', '建議先到 2 待決處理最舊的 3 張，再到 3 派工看失敗的產線。'],
     detail=['失敗的產線（7）：', '  hangar 換池 · 3 輪逾時', '  docs 建置 · mdbook 版本不符', '  codeforge check-all · clippy 2 條', '  autopilot mods/live · 10 紅（基準同）',
             '  lab gate · private-leaks 1 條', '  alfred 部署 · 等 PM go', '卡住的產線（5）：', '  pinball 美術 · 等操作員選版', '  308 地圖 · 等素材', '  revival 物理 · 等 GPU slot',
             '  twgame 音效 · 等授權', '  mple2 回收 · 等驗收'],
     status=dict(dec='12', act='2', fleet=(19, 7, 5)), key=NEXT, next='1-L')
step(id='1-L', tab=1, state='L', title='失敗原因很長',
     ix=[dim('要你處理'), '  待決 3 張 · 動手 1 張', '  殘留 2 項', dim('艦隊'), '▸ 產線 12：失敗 2', dim('本席與審查'), '  本席 ctx 42%'],
     compact='·待決 3·動手 1  ·殘留 2  ▸產線 12（失敗 2）  ·QC 欠審 1',
     reply=['焦點：失敗的 hangar 換池。原因很長，全文在來龍去脈。', '已連續 3 輪逾時，每輪都停在同一步：等 NAS 回應。', '按 3 到派工頁看這條產線的每一輪。'],
     detail=['hangar 換池 · 失敗原因（第 3 輪）', '09:02 開始換池：team 帳號 → pool-b。', '09:03 備份設定檔，sha16 記錄完成。', '09:04 停 courier；等 4 個 session 到安全點。',
             '09:10 4 個 session 都回「可以」。', '09:11 改用同一台主機上既有的 token。', '09:12 重啟 courier，等 NAS 掛載點回應。', '09:22 逾時 600 秒：NAS 沒有回應 stat。',
             '09:22 自動回滾到 pool-a，回滾成功。', '09:23 第 3 輪結束，標失敗；下一輪排在 10:00。', '前兩輪同樣停在 09:12 那一步。', '判斷：NAS 掛載點的問題，跟換池腳本無關。',
             '建議：先查 NAS，再排第 4 輪。'],
     key=NEXT, next='1-X')
step(id='1-X', tab=1, state='X', title='缺源（pm 讀不到）',
     ix=[dim('要你處理'), '▸ 待決 3 張 · 動手 1 張', '  殘留 2 項', dim('艦隊（改用 console）'), '  產線 12：執行 8 · 失敗 2', dim('本席與審查'), err('  本席、QC 讀不到')],
     compact='▸待決 3·動手 1  ·殘留 2  ·產線 12（console）  ·本席讀不到',
     reply=['pm status 讀不到，艦隊數字改用 console（2 分鐘前）。', '本席與 QC 的即時資料讀不到，這兩格不顯示數字。', '待決與殘留照常，可以照常作答。'],
     pin=[err('⊘ pm status 讀不到（連線逾時）→ 改用 console'), err('⊘ live 讀不到：本席、QC、代理決策')],
     detail=['console 09:39 更新：產線 12，執行 8、失敗 2、卡住 2。'], key=NEXT, next='1-S')
step(id='1-S', tab=1, state='S', title='過時',
     ix=[dim('要你處理'), '▸ 待決 3 張 · 動手 1 張', '  殘留 2 項', dim('艦隊（12 分鐘前）'), warn('  產線 12：執行 8 · 失敗 2'), dim('本席與審查'), warn('  本席 ctx 42%（12 分鐘前）')],
     compact='▸待決 3·動手 1  ·殘留 2  ·產線 12（過時）  ·本席（過時）',
     reply=['艦隊和本席的數字是 12 分鐘前的，可能已經變了。', '待決卡是即時的，可以照常作答。'],
     pin=[warn('⚠ pm status 12 分鐘前（超過 5 分鐘算過時）'), warn('⚠ console 12 分鐘前'), warn('⚠ live 本席 12 分鐘前')],
     detail=['艦隊 12 條產線：執行 8、失敗 2、卡住 2（09:29）。', '本席 ctx 42%（09:29）。'],
     status=dict(stale={'fleet'}), key=NEXT, next='1-X2')
step(id='1-X2', tab=1, state='X2', title='缺源（pm 和 console 都讀不到）',
     ix=[dim('要你處理'), '▸ 待決 3 張 · 動手 1 張', '  殘留 2 項', dim('艦隊（只剩 fleetSummary）'), '  lane 9：忙 5 · 閒 4', dim('本席與審查'), '  本席 ctx 42% · QC 欠審 1'],
     compact='▸待決 3·動手 1  ·殘留 2  ·lane 9（fleetSummary）',
     reply=['pm 和 console 都讀不到，艦隊只剩 fleetSummary 的 lane 忙閒。', '產線的失敗和停住的看不到；要看每個 agent 請到 7 艦隊。'],
     pin=[err('⊘ pm status 讀不到'), err('⊘ console 讀不到 → 改用 fleetSummary')],
     detail=['fleetSummary 09:40：lane 9，忙 5、閒 4。'], status=dict(fleet='—'), key='2', next='2-N')

# ===== 2 待決 =====
D_HINTS = [H('a/b/c', '選'), H('p/n', '換張'), H('f', '填建議'), H('m', '附註'), H('s', '送出'), H('Esc', '回輸入框')]
ESC = [H('Esc', '回輸入框')]
D_NSCROLL = [H('a/b/c', '選'), H('p/n', '換張'), H('m', '附註'), H('s', '送出'), H('↑↓', '捲'), H('PgUp/PgDn', '捲上方')]  # 80 欄、兩區都要捲時
step(id='2-N', tab=2, state='N', title='第 1 張', ix=decide_ix(0), compact=decide_compact(0),
     reply=card_reply(0) + [dim('s 送出 · 4 張都還沒選 · 送出後不能改')], detail=card_detail(CARDS[0]), key=NEXT, next='2-E')
step(id='2-E', tab=2, state='E', title='沒有卡', ix=[dim('待決 0 張'), '  沒有要你處理的卡'], compact='沒有要你處理的卡',
     reply=[ok('✓ 沒有待決的卡。'), '新卡進來時底列會亮黃色「待決 N」，按 2 回到這裡。'], detail=['最近一張在 09:30 送出（合併 plan：a）。'],
     hints=ESC, status=dict(dec='0', act='0'), key=NEXT, next='2-M')
step(id='2-M', tab=2, state='M', title='14 張的第 5 張',
     ix=[dim('待決 12 張 · 等你動手 2 張'), dim('⋯ 上面還有 3 張'), '  ✓4 時冶 PL02：a', '▸ ·5 hippo 安裝', '  ·6 mnemos 匯入', '  ▲7 alfred 上線', dim('⋯ 下面還有 7 張')],
     compact='⋯上3  ✓4 時冶：a  ▸·5 hippo  ·6 mnemos  ▲7 alfred  ⋯下7',
     reply=[('▸ hippo P1.5 要先裝 canary 還是全部一起裝？', 'text'), opt('a', '先裝 canary', True, '一台出事只影響一台；全部晚半天'),
            opt('b', '全部一起裝', False, '今天就全上；出事要一起回滾'), opt('c', '待辦', False, '離開待決，到期日前不再出現'), dim('s 送出 · 已選 4 張 · 還有 10 張沒選')],
     detail=['來源   PM · 開卡 5 小時前', '背景   hippo P1.5 要裝到 11 台主機。', '卡住   canary 主機要等 hangar 排機。'],
     status=dict(dec='12', act='2'), key=NEXT, next='2-L')
step(id='2-L', tab=2, state='L', title='第 4 張 · 來龍去脈很長', ix=decide_ix(3), compact=decide_compact(3),
     reply=card_reply(3), detail=card_detail(CARDS[3]) + ['補充   staging 是 11-24 22:00 的版本，之後沒有再部署。', '       8 個值：衰減、半徑、上限、下限、權重 ×4。',
                                                        '       第 5、7 兩個值在 11-24 改過，其餘沿用第一版。', '       定案後時冶會開 PL03 的派工。'],
     nhints=D_NSCROLL, key=NEXT, next='2-X')
step(id='2-X', tab=2, state='X', title='缺源', ix=[err('待決讀不到')], compact='待決讀不到',
     reply=[err('⊘ decision list 讀不到（fleet decision list 結束碼 1）'), '卡片內容看不到，不能作答。已選的答案留在本機，恢復後會回來。', '30 秒後自動重試。'],
     detail=['09:41 讀取失敗：courier 沒有回應。'], hints=ESC, status=dict(dec='—', act='—'), key=NEXT, next='2-S')
step(id='2-S', tab=2, state='S', title='預覽中卡片變了', ix=decide_ix(1, {0: 'a', 1: 'b'}), compact=decide_compact(1, {0: 'a', 1: 'b'}),
     reply=[warn('⚠ 預覽之後，第 2 張被 PM 改了（選項 b 的說明）。'), '確認已被拒絕，你的選擇都還在；看過再按 s 重新預覽。'] + card_opts(CARDS[1], 'b'),
     pin=[warn('⚠ 第 2 張 09:40 被修改：b 的說明改了')], detail=card_detail(CARDS[1]),
     hints=[H('a/b/c', '選'), H('p/n', '換張'), H('s', '重新預覽'), H('Esc', '回輸入框')], key='3', next='3-N')

# ===== 3 派工 =====
RUN_IX = [dim('執行中 2 輪'), '▸ 甲線 codeforge · codex · 12 分', '  乙線 autopilot · claude · 8 分', dim('失敗 1 輪'), '  丙線 hangar · codex · 31 分', dim('完成 1 輪'), '  丁線 docs · claude · 40 分']
R_HINTS = [H('j/k', '換筆'), H('o', '展開'), H('Esc', '回輸入框')]
step(id='3-N', tab=3, state='N', title='正常', ix=RUN_IX, compact='▸甲 codeforge 12分  ·乙 autopilot 8分  ·丙 hangar 失敗  ·丁 docs 完成',
     reply=['甲線 codex 正在改 console/gen.py，第 3 次 commit。', '預計 10 分鐘內交件，交件後本席審。'],
     detail=['run hetero-1795623455 · codex gpt-6.1 · effort high', '09:30 開始；分支 design/console-gen2', '已 commit 3 次；最後一次 09:41', 'Fleet-Task 01M89Q2Z'], key=NEXT, next='3-E')
step(id='3-E', tab=3, state='E', title='沒有派工', ix=[dim('執行中 0 輪'), '  目前沒有派工'], compact='目前沒有派工',
     reply=['目前沒有在跑的派工。', '最近一輪：丁線 docs，09:10 完成。'], detail=['今天派工 6 輪：完成 5、失敗 1。'], key=NEXT, next='3-M')
step(id='3-M', tab=3, state='M', title='很多輪',
     ix=[dim('執行中 9 輪'), '  甲線 codeforge · codex · 12 分', '▸ 乙線 autopilot · claude · 8 分', '  丙線 hangar · codex · 3 分', '  丁線 fleet · grok · 21 分', '  戊線 hippo · agy · 6 分',
         dim('⋯ 下面還有 18 輪（失敗 3）')],
     compact='·甲 codeforge  ▸乙 autopilot  ·丙 hangar  ·丁 fleet  ·戊 hippo  ⋯下 18',
     reply=['乙線 claude 在修 mods/live 的 frameWrap 迴歸。', '已跑 8 分鐘，測試 10 紅與基準相同。'], detail=['run hetero-1795620011 · claude sonnet', '09:34 開始；分支 fix/framewrap'], key=NEXT, next='3-L')
step(id='3-L', tab=3, state='L', title='失敗原因很長',
     ix=[dim('執行中 2 輪'), '  甲線 codeforge · codex · 12 分', '  乙線 autopilot · claude · 8 分', dim('失敗 1 輪'), '▸ 丙線 hangar · codex · 31 分', dim('完成 1 輪'), '  丁線 docs · claude · 40 分'],
     compact='·甲 codeforge  ·乙 autopilot  ▸丙 hangar 失敗  ·丁 docs',
     reply=[err('✗ 丙線第 2 輪失敗：測試逾時後被 wrapper 收掉。'), '原因全文在來龍去脈；最後是判斷。', '這一輪沒有 commit，worktree 保留著。'],
     detail=['丙線 hangar · run hetero-1795619800 · codex', '第 1 輪 08:40–09:02：通過 12/14，2 條 shellcheck。', '第 2 輪 09:05 開始，改 bin/pool-switch.sh。', '09:12 跑 tests/pool-switch.test.sh。',
             '09:14 測試停在「等 NAS stat」。', '09:24 測試逾時 600 秒。', '09:24 wrapper 送 SIGTERM，子程序 3 個沒有結束。', '09:25 wrapper 送 SIGKILL：rsync、stat、sleep。',
             '09:25 worktree 保留，沒有 commit。', '09:26 標失敗：timeout（不是測試紅）。', '判斷：NAS 掛載點沒有回應，跟程式碼無關。', '下一步：等 NAS 恢復後重派，不用改 brief。'],
     key=NEXT, next='3-X')
step(id='3-X', tab=3, state='X', title='缺源', ix=[dim('派工'), err('  派工清單讀不到'), dim('摘要（fleetSummary）'), '  lane 9：忙 5 · 閒 4'], compact='派工清單讀不到  ·lane 9 忙 5（fleetSummary）',
     reply=['live.rows 讀不到，看不到每一輪。', '摘要改用 fleetSummary：lane 忙 5、閒 4。'],
     pin=[err('⊘ live.rows 讀不到'), err('⊘ pm／console 讀不到 → 摘要改用 fleetSummary')], detail=['fleetSummary 09:40：lane 9，忙 5、閒 4。'], status=dict(fleet='—'), key=NEXT, next='3-S')
step(id='3-S', tab=3, state='S', title='過時', ix=[dim('執行中 2 輪（15 分鐘前）')] + RUN_IX[1:], compact='▸甲 codeforge（過時）  ·乙 autopilot  ·丙 hangar 失敗',
     reply=['派工清單是 15 分鐘前的；甲線可能已經交件。'], pin=[warn('⚠ live.rows 15 分鐘前（過時）'), warn('⚠ pm 摘要 15 分鐘前')], detail=['run hetero-1795623455 · codex', '09:26 的狀態：已 commit 2 次'],
     status=dict(stale={'fleet'}), key='4', next='4-N')

# ===== 4 審查 =====
RV_IX = [dim('程式碼審查'), '▸ codeforge 7d1c · codex PASS', '  autopilot e931 · agy PASS', dim('QC'), warn('  欠審 1：hangar 5cab'), dim('我替你決定的事 4'), '  合併 develop ×3 · 換池 ×1']
step(id='4-N', tab=4, state='N', title='正常', ix=RV_IX, compact='▸codeforge PASS  ·autopilot PASS  ·QC 欠審 1  ·代理決策 4',
     reply=['codeforge 7d1c：codex 第 2 輪 PASS，沒有 blocking。', '已 --no-ff 合進 develop；受保護檔案沒有被改。'],
     detail=['審查範圍 develop..7d1c（12 檔）。', '第 1 輪 3 blocking，全部修完。', '第 2 輪 0 blocking、2 note。'], key=NEXT, next='4-E')
step(id='4-E', tab=4, state='E', title='沒有審查', ix=[dim('程式碼審查'), '  今天還沒有審查', dim('QC'), '  沒有欠審', dim('我替你決定的事 0')], compact='今天還沒有審查  ·QC 沒有欠審  ·代理決策 0',
     reply=['今天還沒有審查紀錄，QC 也沒有欠審。'], detail=['昨天審查 5 次，全部 PASS。'], key=NEXT, next='4-M')
step(id='4-M', tab=4, state='M', title='很多條',
     ix=[dim('我替你決定的事 20'), '  09:41 合併 codeforge develop', '▸ 09:30 換池 team → pool-b', '  09:12 延期 01M89GKE 到明天', '  08:55 合併 autopilot develop', '  08:40 退件 codex 第 1 版',
         dim('⋯ 下面還有 15 條')],
     compact='·09:41 合併  ▸09:30 換池  ·09:12 延期  ·08:55 合併  ⋯下 16',
     reply=['09:30 本席決定換池 team → pool-b（同一台主機既有的 token，不算新登入）。', '依據：standing go 2026-11-12；hangar 換池腳本執行。'],
     detail=['決策 id dl-0930；可回滾：pool-switch.sh --to pool-a。'], key=NEXT, next='4-L')
step(id='4-L', tab=4, state='L', title='審查意見很長', ix=RV_IX, compact='▸codeforge PASS  ·autopilot PASS  ·QC 欠審 1',
     reply=['codex 第 2 輪 PASS；note 2 條和第 1 輪的 blocking 全文在來龍去脈。'],
     detail=['note 1｜src/cli/statusline.rs:212｜窄寬度時配額列會先截掉重置時間，', '        操作員看不到幾點重置；建議改成先截掉 fleet 那一段。', 'note 2｜scripts/check-all.sh:88｜flake guard 重跑一次就算過，',
             '        同一條連續兩天都要重跑，應該登進 flaky 清單。', '第 1 輪 blocking 1｜CJK 專案名時切到半個字。', '第 1 輪 blocking 2｜quota 檔不存在時印出空白而不是「—」。', '第 1 輪 blocking 3｜80 欄時分隔線多一格。',
             '三條都修完，第 2 輪重看確認。', '審查席：codex gpt-6.1 · effort high'],
     key=NEXT, next='4-X')
step(id='4-X', tab=4, state='X', title='缺源', ix=[dim('程式碼審查'), err('  審查讀不到'), dim('QC'), err('  QC 讀不到'), dim('我替你決定的事'), err('  決策讀不到')],
     compact='審查讀不到 · QC 讀不到 · 決策讀不到', reply=['審查、QC、代理決策都讀不到，無法判斷有沒有欠審。'], pin=[err('⊘ live.review、qc、decisions 讀不到')], detail=['上次讀到是 09:20。'],
     key=NEXT, next='4-S')
step(id='4-S', tab=4, state='S', title='過時', ix=RV_IX[:4] + [warn('  欠審 1（20 分鐘前）')] + RV_IX[5:], compact='▸codeforge PASS  ·QC 欠審 1（過時）',
     reply=['QC 是 20 分鐘前的；欠審那一條可能已經審完。'], pin=[warn('⚠ qc 20 分鐘前（過時）')], detail=['hangar 5cab 在 09:21 進 QC 佇列。'], key='5', next='5-N')

# ===== 5 花費 =====
SP_IX = [dim('額度'), '▸ 5h 42% · 重置 2h10m 後', '  照速度 3h03m 用完（晚於重置）', '  7d 18% · 4d 後重置', dim('額度池與今日'), '  A 池 42% · B 池 9%', '  今日 380k tokens · 輸出 41 tok/s']
SP_DETAIL = ['5h 已用 42%，每小時約用 19%。', '撞頂：剩下 58% 照這個速度約 3h03m 用完。', '重置：5h 視窗 2h10m 後歸零。', '重置先到，所以這個視窗不會撞頂。', '7d 已用 18%，4d 後重置。',
             '今日 380k tokens；24h 趨勢比昨天高 12%。']
step(id='5-N', tab=5, state='N', title='正常', ix=SP_IX, compact='▸5h 42%（重置 2h10m）  ·7d 18%  ·A 池 42%  ·今日 380k',
     reply=['5h 已用 42%：2h10m 後重置，照目前速度要 3h03m 才用完，重置前不會撞頂。', '不用調整；長任務可以照排。'], detail=SP_DETAIL, key=NEXT, next='5-E')
step(id='5-E', tab=5, state='E', title='還沒有用量', ix=[dim('額度'), '▸ 5h 0% · 視窗還沒開始', '  7d 3% · 6d 後重置', dim('今日'), '  還沒有請求'], compact='▸5h 0%  ·7d 3%  ·今日還沒有請求',
     reply=['今天還沒有用量；5h 視窗從第一次請求開始算。'], detail=['7d 已用 3%，6d 後重置。'], status=dict(speed='—'), key=NEXT, next='5-M')
step(id='5-M', tab=5, state='M', title='10 個額度池',
     ix=[dim('額度池 10'), '  team · claude max · 42%', '▸ cookys · claude max · 77%', '  ops · claude pro · 12%', '  codex 席 · 64%', '  grok 席 · 9%', dim('⋯ 下面還有 5 個池')],
     compact='·team 42%  ▸cookys 77%  ·ops 12%  ·codex 64%  ·grok 9%  ⋯下 5',
     reply=[warn('cookys 池 77%：照目前速度約 1h03m 後撞頂，重置在 2h40m 後。'), '撞頂比重置早，建議把 cookys 上的長任務移到 ops 池（12%）。'],
     detail=['cookys 池每小時約用 22%。', '撞頂：剩 23% ÷ 22%/h ≈ 1h03m。', '重置：2h40m 後。'], key=NEXT, next='5-L')
step(id='5-L', tab=5, state='L', title='說明很長', ix=SP_IX, compact='▸5h 42%（重置 2h10m）  ·7d 18%  ·A 池 42%',
     reply=['焦點：5h 額度的估算方式，全文在來龍去脈。'],
     detail=SP_DETAIL + ['估算用最近 30 分鐘的燃燒率，不是整個 5h 的平均。', '燃燒率每 5 分鐘更新一次。', '「撞頂」指剩餘用完；「重置」指視窗結束歸零。', '兩個時間比較，先到的那個決定要不要調整。',
                         '額度池的數字由 capacity 提供，不是本機算的。', '今日用量只算本機轉錄檔裡的數字。'],
     key=NEXT, next='5-X')
step(id='5-X', tab=5, state='X', title='缺源（quota 和速度）',
     ix=[dim('額度'), err('▸ 5h 讀不到'), err('  7d 讀不到'), dim('額度池'), '  A 池 42% · B 池 9%', dim('今日'), '  380k tokens · 速度 —'],
     compact='▸5h 讀不到  ·7d 讀不到  ·A 池 42%  ·今日 380k  ·速度 —',
     reply=['seats --quota 讀不到，5h 和 7d 都沒有數字，無法估撞頂時間。', '額度池和今日用量另有資料，照常顯示。', '輸出速度：本 session 還沒有請求。'],
     pin=[err('⊘ seats --quota 結束碼 1，也沒有快取'), err('⊘ 輸出速度：本 session 還沒有請求')], detail=['額度池 09:40 更新；今日用量 09:41 更新。'],
     status=dict(speed='—'), quota='missing', key=NEXT, next='5-S')
step(id='5-S', tab=5, state='S', title='過時（用快取）',
     ix=[dim('額度（快取）'), warn('▸ 5h 42% ⏶ 18 分鐘前'), warn('  7d 18% ⏶ 18 分鐘前'), dim('額度池（14 分鐘前）'), warn('  A 池 42% · B 池 9%'), dim('今日'), warn('  380k · 41 tok/s（35 分鐘前）')],
     compact='▸5h 42% ⏶  ·7d 18% ⏶  ·A 池（過時）  ·速度（過時）',
     reply=['quota 指令失敗，顯示的是 18 分鐘前的快取（黃色 ⏶）。', '最後一次請求在 09:06（35 分鐘前），41 tok/s 是那時的速度。', '撞頂估算暫停，等新數字回來。'],
     pin=[warn('⏶ seats --quota 失敗，用 18 分鐘前的快取'), warn('⚠ 額度池 14 分鐘前'), warn('⚠ 最後一次請求 09:06（35 分鐘前）')], detail=['今日 380k tokens（09:06）。'],
     status=dict(stale={'speed'}), quota='stale', key=NEXT, next='5-X2')
step(id='5-X2', tab=5, state='X2', title='缺源（額度池和今日用量）',
     ix=[dim('額度'), '▸ 5h 42% · 重置 2h10m 後', '  7d 18% · 4d 後重置', dim('額度池'), err('  額度池讀不到'), dim('今日'), err('  今日用量讀不到')],
     compact='▸5h 42%  ·7d 18%  ·額度池讀不到  ·今日讀不到',
     reply=['額度池和今日用量讀不到；5h、7d 照常。', '不能比較各池，換池建議暫停。'], pin=[err('⊘ capacity 額度池讀不到'), err('⊘ 今日用量、24h 趨勢讀不到')], detail=SP_DETAIL[:5],
     key='6', next='6-N')

# ===== 6 殘留（唯讀，只給人工處理的指引） =====
RS_IX = [dim('要你處理 2'), '▸ worktree tri-merge（4 天沒動）', '  分支 fix/verify-old（沒合併）', dim('自動清理 1'), '  過期暫存：3 個 handoff', dim('外掛載入 1'), '  forge-stats 從主 clone 載入']
step(id='6-N', tab=6, state='N', title='正常', ix=RS_IX, compact='▸舊 worktree tri-merge  ·分支 fix/verify-old  ·自動清理 1  ·外掛 1',
     reply=['tri-merge 這個 worktree 4 天沒動，裡面有 2 個沒合併的 commit。', '人工處理：在 codeforge 主 clone 跑', dim('  git -C .claude/worktrees/tri-merge log develop..'), '看完確定不要，再自己移除（這頁只看不刪）。'],
     detail=['路徑 .claude/worktrees/tri-merge · 分支 design/tri-merge', '最後 commit 11-21 · 沒有對應的活單', '規則：超過 3 天、沒有活單 → 開單接著做或刪掉'], key=NEXT, next='6-E')
step(id='6-E', tab=6, state='E', title='沒有殘留', ix=[dim('要你處理 0'), '  沒有殘留', dim('自動清理'), '  今天清了 4 個暫存檔'], compact='沒有殘留  ·自動清理今天 4 個',
     reply=[ok('✓ 沒有要你處理的殘留。'), '自動清理今天清了 4 個暫存檔。'], detail=['上次掃描 09:30。'], key=NEXT, next='6-M')
step(id='6-M', tab=6, state='M', title='很多項',
     ix=[dim('要你處理 9'), '  worktree tri-merge · 4 天', '▸ worktree tri-fable · 5 天', '  worktree tri-brief · 5 天', '  分支 fix/verify-old', '  stash WIP pane', dim('⋯ 下面還有 11 項')],
     compact='·tri-merge  ▸tri-fable  ·tri-brief  ·fix/verify-old  ⋯下 12',
     reply=['tri-fable 5 天沒動，已經合進 develop，沒有沒合併的 commit。', '人工處理：git worktree remove .claude/worktrees/tri-fable'], detail=['分支 design/tri-fable 已合進 develop（d05a850）。'], key=NEXT, next='6-L')
step(id='6-L', tab=6, state='L', title='說明很長', ix=RS_IX, compact='▸舊 worktree tri-merge  ·分支 fix/verify-old  ·自動清理 1',
     reply=['tri-merge 有 2 個沒合併的 commit，全文在來龍去脈。', '人工處理前請先看完。'],
     detail=['路徑 .claude/worktrees/tri-merge · 分支 design/tri-merge', 'commit 1  29a1b1c  合併稿三家版本（11-21）', '          改 doc/design/tri/merge/ 18 檔', 'commit 2  4c0e2d1  合併稿 NOTES（11-21）',
             '          只改 NOTES.md', '這兩個 commit 的內容已被 v2 取代（74f24a9）。', '刪除前規則：沒合併的工作先打 archive tag，', '再 git bundle create 到備份目錄並 verify。', '做完才算可回復（tier 1）。',
             '之後才跑 git worktree remove 和 git branch -D。', '這一頁不會替你做任何一步。'],
     key=NEXT, next='6-X')
step(id='6-X', tab=6, state='X', title='缺源', ix=[dim('要你處理'), err('  殘留讀不到'), dim('外掛載入 1'), '  forge-stats 從主 clone 載入'], compact='殘留讀不到  ·外掛 1',
     reply=['殘留掃描讀不到，不知道有沒有要你處理的。', '外掛載入另有資料，照常。'], pin=[err('⊘ residue 掃描失敗（結束碼 2）')], detail=['上次成功掃描 08:30。'], key=NEXT, next='6-S')
step(id='6-S', tab=6, state='S', title='過時', ix=[dim('要你處理 2（2 小時前）')] + RS_IX[1:], compact='▸舊 worktree tri-merge（過時）  ·外掛 1',
     reply=['殘留清單是 2 小時前掃的；tri-merge 可能已經處理掉。', '人工處理前先重新掃描：codeforge hygiene --scan'], pin=[warn('⚠ residue 2 小時前（過時）')], detail=['路徑 .claude/worktrees/tri-merge'],
     key=NEXT, next='6-X2')
step(id='6-X2', tab=6, state='X2', title='缺源（外掛載入）', ix=RS_IX[:6] + [err('  外掛載入讀不到')], compact='▸舊 worktree tri-merge  ·外掛載入讀不到',
     reply=['load_source 讀不到，不知道 forge-stats 是從哪裡載入的。', '殘留清單照常。'], pin=[err('⊘ load_source 讀不到')], detail=['上次讀到：主 clone（develop）。'], key='7', next='7-N')

# ===== 7 艦隊（agentsColumn：每個 agent 的 tok/s） =====
FL_IX = [dim('執行中 3'), '▸ codeforge--claude · 41 tok/s', '  autopilot--claude · 37 tok/s', '  hangar--claude · 63 tok/s', dim('失敗 1'), '  codex 派工 · 測試逾時', dim('待決卡：本機 3 · 他機 1')]
step(id='7-N', tab=7, state='N', title='正常', ix=FL_IX, compact='▸codeforge 41  ·autopilot 37  ·hangar 63  ·codex 失敗  ·卡 本機3 他機1',
     reply=['codeforge--claude 在審 console 設計稿，第 1 輪。'], detail=['模型 opus · 41 tok/s ▆▆▄▃▆（近 5 分鐘）', '最近 run hetero-1795623455（codex，已交件）', '產線：design/console-full · 審查中'],
     key=NEXT, next='7-E')
step(id='7-E', tab=7, state='E', title='沒有 agent', ix=[dim('執行中 0'), '  沒有在跑的 agent', dim('待決卡：本機 0 · 他機 0')], compact='沒有在跑的 agent  ·卡 本機0 他機0',
     reply=['目前沒有在跑的 agent。'], detail=['最近一個在 09:10 結束（docs--claude）。'], status=dict(speed='—', dec='0', act='0', fleet=(0, 0, 0)), key=NEXT, next='7-M')
step(id='7-M', tab=7, state='M', title='很多 agent',
     ix=[dim('執行中 17'), '  codeforge--claude · 41 tok/s', '  autopilot--claude · 37 tok/s', '▸ hangar--claude · 63 tok/s', '  fleet--claude · 22 tok/s', '  gpu-lead--claude · 48 tok/s', dim('⋯ 下面還有 12 個、失敗 4、完成 9')],
     compact='·codeforge  ·autopilot  ▸hangar  ·fleet  ·gpu-lead  ⋯下 25', reply=['hangar--claude 在換池，第 3 輪。'], detail=['模型 sonnet · 63 tok/s ▆▆▆▄▆', '最近失敗：換池逾時（09:22）'],
     status=dict(fleet=(17, 4, 0)), key=NEXT, next='7-L')
step(id='7-L', tab=7, state='L', title='失敗原因很長', ix=FL_IX[:5] + ['▸ codex 派工 · 測試逾時'] + FL_IX[6:], compact='·codeforge  ·autopilot  ·hangar  ▸codex 失敗',
     reply=[err('✗ codex 派工：測試逾時，全文在來龍去脈。')],
     detail=['run hetero-1795619800 · codex gpt-6.1', '09:12 跑 tests/pool-switch.test.sh', '09:14 停在「等 NAS stat」', '09:24 逾時 600 秒，wrapper 送 SIGTERM', '09:25 子程序沒有結束，送 SIGKILL',
             '09:25 worktree 保留，沒有 commit', '09:26 標失敗：timeout', '判斷：NAS 掛載點沒有回應，跟程式碼無關', '下一步：NAS 恢復後重派', '同一原因今天第 3 次（1 現況、3 派工也看得到）',
             '這一筆的 tok/s 不顯示：已結束'], key=NEXT, next='7-X')
step(id='7-X', tab=7, state='X', title='缺源（本機卡、他機卡、速度）',
     ix=[dim('執行中 3'), '▸ codeforge--claude · opus · —', '  autopilot--claude · opus · —', '  hangar--claude · sonnet · —', dim('待決卡'), err('  本機卡讀不到'), dim('  他機卡：還沒接線')],
     compact='▸codeforge  ·autopilot  ·hangar  ·本機卡讀不到  ·他機卡沒接線',
     reply=['本機待決卡讀不到；他機卡還沒接線（ctx.others）。', '每個 agent 的速度（agentsColumn）沒有資料，寫「—」。'],
     pin=[err('⊘ decide 讀不到'), err('⊘ agentsColumn 沒有 steps'), dim('他機卡 others 還沒接線')], detail=['runs 照常（09:41）。'],
     status=dict(dec='—', act='—', speed='—'), key=NEXT, next='7-S')
step(id='7-S', tab=7, state='S', title='過時', ix=FL_IX[:6] + [warn('待決卡 本機 3 · 他機 1（過時）')], compact='▸codeforge 41  ·runs（過時）  ·他機卡（過時）',
     reply=['runs 和他機卡都是 11 分鐘前的。', '他機卡這格畫的是接線後的樣子（fixture）。'],
     pin=[warn('⚠ runs 11 分鐘前'), warn('⚠ pm／console 11 分鐘前'), warn('⚠ 他機卡 11 分鐘前（接線後）')], detail=['模型 opus · 41 tok/s'], status=dict(stale={'fleet'}), key=NEXT, next='7-X2')
step(id='7-X2', tab=7, state='X2', title='缺源（runs 和摘要）', ix=[dim('執行中'), err('  runs 讀不到'), dim('待決卡：本機 3 · 他機 1')], compact='runs 讀不到  ·卡 本機3 他機1',
     reply=['runs 和產線摘要讀不到，看不到 agent 在做什麼。', '待決卡照常。'], pin=[err('⊘ runs 讀不到'), err('⊘ pm／console／fleetFeed 讀不到')], detail=['上次讀到 09:20：執行中 3、失敗 1。'],
     status=dict(fleet='—', speed='—'), key='8', next='8-N')

# ===== 8 待辦（問題和選項放回覆區；沒有 m） =====
TD_IX = [dim('待辦 3 張'), '▸ 11-30 合併計畫', '  12-02 外掛更新', '  12-05 核對報表']
TD_HINTS = [H('a/b/c', '選'), H('j/k', '換筆'), H('Esc', '關閉')]
TD_Q = [('▸ 這次合併要不要等 hippo 裝完？', 'text'), opt('a', '先合併', True, '今天合併；hippo 之後再接'), opt('b', '等 hippo', False, '晚兩天；少一次回滾'), opt('c', '再放一週', False, '到期日改到 12-07')]
step(id='8-N', tab=8, state='N', title='正常', ix=TD_IX, compact='▸11-30 合併計畫  ·12-02 外掛更新  ·12-05 核對報表',
     reply=TD_Q + [dim('附註（移來時寫的）：等 hippo 的時程再說')], detail=['到期 11-30（還有 4 天）· 從待決移來 11-24', '到期前不會再出現在待決'], hints=TD_HINTS, key=NEXT, next='8-E')
step(id='8-E', tab=8, state='E', title='沒有待辦', ix=[dim('待辦 0 張'), '  沒有待辦'], compact='沒有待辦', reply=['沒有待辦。待決卡選 c 就會移到這裡。'], detail=['上一張在 11-24 回到待決。'],
     hints=[H('Esc', '關閉')], key=NEXT, next='8-M')
step(id='8-M', tab=8, state='M', title='很多張',
     ix=[dim('待辦 11 張'), '  11-26 NAS 換池', '▸ 11-28 憑證輪替提醒', '  11-30 合併計畫', '  12-02 外掛更新', '  12-05 核對報表', dim('⋯ 下面還有 6 張')],
     compact='·11-26 NAS  ▸11-28 憑證  ·11-30 合併  ·12-02 外掛  ⋯下 7',
     reply=[('▸ 憑證輪替要排在週末還是平日？', 'text'), opt('a', '週末', True, '影響少；要有人值班'), opt('b', '平日', False, '有人在線上；會中斷 10 分鐘'), opt('c', '再放一週', False, '到期日改到 12-05')],
     detail=['到期 11-28（還有 2 天）'], hints=TD_HINTS, key=NEXT, next='8-L')
LONG_Q = ('▸ 這次合併牽涉 codeforge、autopilot、hangar 三條產線，審查和部署時程都還沒對齊；要現在合併、等明天的額外審查，還是延到下週？', 'text')
LONG_OPTS = [opt('a', '現在合併', True, '值班的人看著；出事回滾到 7d1c'), opt('b', '等明天', False, '多一輪 agy 審查，晚一天；明早 9 點前審完就合併，審出 blocking 就改到下週，到期日跟著延'), opt('c', '延到下週', False, '保留目前版本，到期日改 12-07')]
LONG_DETAIL = ['附註（移來時寫的）：先確認審查結果', '到期 11-30 · 從待決移來 11-24', '退回原因：（新增資料，目前沒有資料來源）', '11-24 PM：hippo 裝完前不要合併', '11-25 PM：hippo 延到 11-29']
step(id='8-L', tab=8, state='L', title='長問題加長選項', ix=TD_IX, compact='▸11-30 合併計畫  ·12-02 外掛更新  ·12-05 核對報表',
     reply=[LONG_Q] + LONG_OPTS, detail=LONG_DETAIL, hints=TD_HINTS, key='↓', next='8-L2')
step(id='8-L2', tab=8, state='L2', title='捲到底', ix=TD_IX, compact='▸11-30 合併計畫  ·12-02 外掛更新  ·12-05 核對報表',
     reply=[LONG_Q] + LONG_OPTS, rscroll=1, detail=LONG_DETAIL, hints=TD_HINTS, key=NEXT, next='8-X')
step(id='8-X', tab=8, state='X', title='缺源（還沒接線）', ix=[dim('待辦'), err('  待辦還沒接線')], compact='待辦還沒接線',
     reply=['待辦的資料還沒接上，這頁目前一定是空的。', '接線後讀不到的樣子同 2-X。'], pin=[err('⊘ fleet todo 還沒接線')], detail=['runtime 現況：沒有資料。'],
     hints=[H('Esc', '關閉')], key=NEXT, next='8-S')
step(id='8-S', tab=8, state='S', title='過時', ix=[dim('待辦 3 張（30 分鐘前）')] + TD_IX[1:], compact='▸11-30 合併計畫（過時）  ·12-02  ·12-05',
     reply=TD_Q, pin=[warn('⚠ 待辦清單 30 分鐘前（過時）')], detail=['到期 11-30'], hints=TD_HINTS, key='9', next='9-N')

# ===== 9 說明 =====
HP_HINTS = [H('j/k', '換段'), H('Esc', '關閉')]
step(id='9-N', tab=9, state='N', title='狀態圖示', ix=[dim('說明'), '▸ 狀態圖示', '  按鍵', '  符號'], compact='▸狀態圖示  ·按鍵  ·符號',
     reply=['● 進行中  ✓ 完成／已選  ✗ 失敗  ⊘ 讀不到  ⚠ 過時  ⏶ 用快取', '▸ 目前這一筆  ★ 建議  ▲ 要先動手  ⇡ 已送出', '底列左邊是狀態，右邊是分頁；黃色數字是待決。'],
     detail=['顏色：黃 要注意 · 紅 讀不到或失敗 · 綠 完成 · 灰 說明'], hints=HP_HINTS, key=NEXT, next='9-L')
step(id='9-L', tab=9, state='L', title='按鍵表', ix=[dim('說明'), '  狀態圖示', '▸ 按鍵', '  符號'], compact='·狀態圖示  ▸按鍵  ·符號',
     reply=['焦點「按鍵」：全部按鍵在來龍去脈，新增的標 ＊。', '1–9 切頁 · j/k 換筆 · o 展開 · Esc 回輸入框或關閉'],
     detail=['1–9        切到第 N 頁', 'j/k        換一筆（待決頁用 p/n 換張）', 'o          展開焦點列（待決頁沒有）', 'a/b/c      選選項（待決、待辦）', 'f          沒選的卡填成建議（▲ 卡不填）',
             'm          寫附註（只有待決）', 's          送出前先預覽', 'y / x      預覽中確認 / 取消', 'g ＊       複製等你動手卡的網址', '↑↓ ＊      捲回覆區（待決以外的頁，原本是換筆）',
             'PgUp/PgDn ＊ 捲來龍去脈（原本只有待決有）', 'Esc        待決回輸入框；其他頁關閉', 'Enter      附註輸入中：存附註'],
     hints=HP_HINTS, key='2', next='2-L2')

# ===== 2 待決：互動流程 =====
ACT = CARDS[3]
NOTE4 = 'staging 走過一圈，8 個都對；第 5、7 兩個值和 11-24 的對照一致'
MK = {0: 'a', 1: 'b', 3: 'a'}
MK0 = {0: 'a', 1: 'b'}  # 第 4 張（▲）還沒選：先走完 staging 才選 a（r2 codex 3）
URL_PIN = [('網址   ' + URL, 'text')]
ACT_STEPS = [('▸ ' + ACT['q'], 'text'), '  1. 打開 staging 頁（按 g 複製網址）', '     ' + URL, '  2. 走一圈，核對 8 個推定值：', '     衰減、半徑、上限、下限', '     權重 1、權重 2、權重 3、權重 4',
             '  3. 有不一致的值，按 m 寫是哪一個']
L2_HINTS = [H('a/b/c', '選'), H('↑↓', '捲'), H('PgUp/PgDn', '捲來龍去脈'), H('g', '複製網址'), H('m', '附註'), H('s', '送出')]
L2_N = [H('a/b/c', '選'), H('↑↓', '捲'), H('PgUp/PgDn', '捲上方'), H('g', '複製網址'), H('m', '附註')]
step(id='2-L2', tab=2, state='L2', title='等你動手 · 步驟很長', ix=decide_ix(3, MK0), compact=decide_compact(3, MK0),
     reply=ACT_STEPS + card_opts(ACT), pin=URL_PIN, detail=card_detail(ACT), hints=L2_HINTS, nhints=L2_N, key='↓', next='2-L3')
step(id='2-L3', tab=2, state='L3', title='等你動手 · 捲到底', ix=decide_ix(3, MK0), compact=decide_compact(3, MK0),
     reply=ACT_STEPS + card_opts(ACT), rscroll=1, pin=URL_PIN, detail=card_detail(ACT), hints=L2_HINTS, nhints=L2_N, key='a', next='2-A')
step(id='2-A', tab=2, state='A', title='走完 staging · 選 a', ix=decide_ix(3, MK), compact=decide_compact(3, MK),
     reply=ACT_STEPS + card_opts(ACT, 'a'), rscroll=1, pin=URL_PIN, detail=card_detail(ACT), hints=L2_HINTS, nhints=L2_N, key='m', next='2-I')
step(id='2-I', tab=2, state='I', title='寫附註', ix=decide_ix(3, MK), compact=decide_compact(3, MK),
     reply=[('▸ ' + ACT['q'], 'text'), dim('附註（Enter 存，Esc 取消）：'), '  ' + NOTE4 + '▏', dim('輸入中：a/b/c、s 這些鍵都當成文字')],
     detail=card_detail(ACT), hints=[H('Enter', '存附註'), H('Esc', '取消附註')], key='Enter 存附註，再按 s', next='2-P')
def _pv(n, name, ans):
    return f'{n} {name}' + ' ' * (20 - v.dw(name)) + ans


PREVIEW = [warn('送出預覽 · 3 張 · 確認後不能改'), _pv(1, '合併 plan', 'a 合併'), _pv(2, '外掛更新時機', 'b 明早'), _pv(3, '預覽先拍幾欄', '（沒選，不送）'),
           _pv(4, '時冶 PL02 定案', 'a 照建議定案'), dim('  附註：' + NOTE4)]
step(id='2-P', tab=2, state='P', title='送出預覽', ix=decide_ix(3, MK), compact=decide_compact(3, MK), reply=PREVIEW,
     detail=['預覽列出要送的每一張：選的選項和附註全文。', '沒選的卡不會送，留在待決。'], hints=[H('y', '確認送出'), H('x', '取消'), H('Esc', '取消')], key='x', next='2-Pc')
step(id='2-Pc', tab=2, state='Pc', title='取消預覽', ix=decide_ix(3, MK), compact=decide_compact(3, MK),
     reply=[dim('已取消預覽；選擇和附註都留著，按 s 再預覽。')] + card_reply(3, 'a', NOTE4), detail=card_detail(ACT),
     nhints=D_NSCROLL, key='s 再按 y', next='2-R')
SENT = (0, 1, 3)
step(id='2-R', tab=2, state='R', title='送出成功', ix=decide_ix(2, MK, SENT), compact=decide_compact(2, MK, SENT),
     reply=[ok('✓ 已送出 3 張，PM 09:42 收到（回執 01M89R…）。'), '送出的卡不能再改，a/b/c 只對沒送的這張有效：'] + card_reply(2),
     detail=card_detail(CARDS[2]), hints=[H('a/b/c', '選第 3 張'), H('m', '附註'), H('s', '送出'), H('Esc', '回輸入框')], status=dict(dec='1', act='0'), key=NEXT, next='2-F')
step(id='2-F', tab=2, state='F', title='送出失敗', ix=decide_ix(3, MK), compact=decide_compact(3, MK),
     reply=[err('✗ 送出失敗：courier 逾時（30 秒），3 張都沒送出。'), '選擇和附註都留著；按 s 再預覽一次就能重送。'] + card_reply(3, 'a', NOTE4),
     pin=[err('⊘ 送出失敗 09:42 · courier 逾時')], detail=card_detail(ACT), nhints=D_NSCROLL, key=NEXT, next='2-Fl0')
step(id='2-Fl0', tab=2, state='Fl0', title='按 f 之前', ix=decide_ix(0, {1: 'a'}), compact=decide_compact(0, {1: 'a'}),
     reply=['已選 1 張：第 2 張你選了 a（今晚），不是建議的 b。', '按 f：沒選的卡填成建議；已選的和 ▲ 要動手的不動。'] + card_reply(0), detail=card_detail(CARDS[0]), key='f', next='2-Fl')
FL = {0: 'a', 1: 'a', 2: 'b'}
step(id='2-Fl', tab=2, state='Fl', title='按 f 之後', ix=decide_ix(0, FL), compact=decide_compact(0, FL),
     reply=[ok('✓ 填入建議 2 張：第 1 張 a、第 3 張 b。'), '第 2 張保留你選的 a（今晚）；第 4 張要先動手，不自動填。', '送出前會先預覽，可以再改。'] + card_opts(CARDS[0], 'a'),
     detail=card_detail(CARDS[0]), key=NEXT, next='C1')

# ===== 跨頁 =====
C_MARKS = {0: 'a', 1: 'b'}
C_NOTE = '明早 9 點有人在'
step(id='C1', tab=2, state='C1', title='開著卡（第 2 張選了 b、寫了附註）', ix=decide_ix(1, C_MARKS), compact=decide_compact(1, C_MARKS),
     reply=card_reply(1, 'b', C_NOTE), detail=card_detail(CARDS[1]), key='7', next='C2')
step(id='C2', tab=7, state='C2', title='開著卡時看艦隊', ix=FL_IX, compact='▸codeforge 41  ·autopilot 37  ·hangar 63  ·codex 失敗',
     reply=['看完艦隊按 2 回待決：第 2 張、選的 b 和附註都還在。'], detail=['模型 opus · 41 tok/s ▆▆▄▃▆', '最近 run hetero-1795623455'], key='2', next='C3')
step(id='C3', tab=2, state='C3', title='回到待決（全部恢復）', ix=decide_ix(1, C_MARKS), compact=decide_compact(1, C_MARKS),
     reply=[dim('回到第 2 張；選擇、附註、來龍去脈都和離開時一樣。')] + card_reply(1, 'b', C_NOTE), detail=card_detail(CARDS[1]),
     key='1', next='1-N')

IDS = [s['id'] for s in S]
assert len(IDS) == len(set(IDS)) == 68, len(IDS)
NUM = {s['id']: n for n, s in enumerate(S, 1)}
assert all(s['next'] in NUM for s in S)

DEFAULT_HINTS = {1: [H('j/k', '換段'), H('o', '展開'), H('2', '去待決'), H('Esc', '關閉')], 2: D_HINTS, 3: R_HINTS, 4: R_HINTS, 5: R_HINTS,
                 6: [H('j/k', '換筆'), H('o', '展開'), H('Esc', '關閉')], 7: R_HINTS, 8: TD_HINTS, 9: HP_HINTS}
for t in (3, 4, 5, 7):
    DEFAULT_HINTS[t] = [H('j/k', '換筆'), H('o', '展開'), H('Esc', '關閉')]


def hint_row(items):
    out = []
    for k, d in items:
        if out:
            out.append(G(' · '))
        out += [T(k, 'merged'), T(' ' + d, 'inactive')]
    return out


# ---------- 內容排版 ----------
def render_items(items, w):
    """回覆區、來龍去脈的內容 → (已斷行的列, 第一個選項列的位置或 None)。"""
    rows, first_opt = [], None
    optw = max([v.dw(x[2]) for x in items if isinstance(x, tuple) and x[0] == 'opt'] or [0])
    for it in items:
        if isinstance(it, tuple) and it[0] == 'opt':
            _, l, lab, rec, why, picked = it
            r = [T('    '), T(l + ':', 'merged'), T(' ' + lab + ' ' * (optw - v.dw(lab)), 'success' if picked else 'text')]
            r += [T('  '), G('★', 'warning'), T(' 建議', 'warning')] if rec else [T(' ' * 8)]
            pre = v.rowlen(r) + 2
            tag = [T('  ✓ 已選', 'success')] if picked else []
            lines = v.wrap(why, w - pre - v.rowlen(tag)) if why else ['']
            if first_opt is None:
                first_opt = len(rows)
            rows.append(v.fit(r + [T('  ' + lines[0], 'inactive')] + (tag if len(lines) == 1 else []), w))
            for i, ln in enumerate(lines[1:], 1):  # 選項說明太長：換行接在說明欄下面，選項整段固定看得到
                rows.append(v.fit([T(' ' * pre + ln, 'inactive')] + (tag if i == len(lines) - 1 else []), w))
            continue
        s, role = seg(it)
        lead = len(s) - len(s.lstrip(' '))
        for line in v.wrap(s.lstrip(' '), w - lead):
            rows.append([T(' ' * lead + line, role)])
    return rows, first_opt


def window(rendered, n, at_end, w):
    """回覆區取 n 列；放不下時收起前面的題目和步驟，選項以下固定看得到（PM 01M89BCR）。"""
    rows, first_opt = rendered
    if len(rows) <= n:
        return rows, False
    tail = rows[first_opt:] if first_opt is not None else []
    body = rows[:first_opt] if first_opt is not None else rows
    room = n - len(tail)
    assert room >= 1, ('選項放不下', n, len(tail))
    if room == 1:  # 只剩一列：捲到哪就顯示哪一列，旁邊寫還收著幾列
        k = len(body) - 1
        if at_end:
            mark = f'⋯ 上 {k} 列 '
            text = ''.join(x for x, _, _ in body[-1]).strip()
            row = [T(mark, 'inactive'), T(v.cut(text, w - v.dw(mark)), body[-1][0][1])]
        else:
            mark = f' ⋯ 下 {k} 列'
            text = ''.join(x for x, _, _ in body[0])
            row = [T(v.cut(text, w - v.dw(mark)), body[0][0][1]), T(mark, 'inactive')]
        return [row] + tail, True
    if at_end:
        return [[T(f'⋯ 上面還有 {len(body) - (room - 1)} 列 · ↑ 捲回', 'inactive')]] + body[-(room - 1):] + tail, True
    return body[:room - 1] + [[T(f'⋯ 下面還有 {len(body) - (room - 1)} 列 · ↓ 捲', 'inactive')]] + tail, True


def detail_window(pin, rows, n):
    """來龍去脈：pin 固定在最上面；其餘放不下時最後一列寫收起提示。"""
    room = n - len(pin)
    assert room >= 1, (len(pin), n)
    if len(rows) <= room:
        return pin + rows, False
    return pin + rows[:room - 1] + [[T(f'⋯ 下面還有 {len(rows) - room + 1} 列', 'inactive')]], True


# ---------- 狀態列 ----------
def status_line(W, tab, st):
    dec, act, speed, stale = st.get('dec', '3'), st.get('act', '1'), st.get('speed', '41 tok/s'), st.get('stale', set())
    fl = st.get('fleet', (8, 2, 2))
    role = 'inactive' if dec == '0' else 'error' if dec == '—' else 'warning'
    counts = [T(f'待決 {dec} · 動手 {act}', role)]
    counts_s = [T(f'待決 {dec} ', role), G('▲', role), T(f' {act}', role)]
    if fl == '—':
        fleet = [T('艦隊 —', 'inactive')]
    else:
        frole = 'warning' if 'fleet' in stale else 'text'
        fleet = [T('艦隊 ', frole), G('●', 'suggestion'), T(f' {fl[0]} ', frole), G('✗', 'error'), T(f' {fl[1]} ', frole), T(f'卡住 {fl[2]}', frole)]
        if 'fleet' in stale:
            fleet.append(T(' 過時', 'warning'))
    if speed == '—':
        sp = sp_s = [T('ϟ —', 'inactive')]
    elif 'speed' in stale:
        sp = sp_s = [T(f'ϟ {speed} 過時', 'warning')]
    else:
        sp, sp_s = [T(f'ϟ {speed} '), G('▆▆▄▃▆', 'suggestion')], [T(f'ϟ {speed}')]
    sep = [G(' │ ')]
    head = [G(' ● ', 'suggestion'), T('進行中')]
    toggle = [T(' '), G('▾', 'merged'), T(' ')]
    for left, tm in ((head + sep + fleet + sep + counts + sep + sp, 'full'), (head + sep + counts + sep + sp, 'full'), (head + sep + counts_s + sep + sp, 'full'),
                     (head + sep + fleet + sep + counts_s + sep + sp_s, 'cur'), (head + sep + counts_s + sep + sp_s, 'cur')):
        right = v.tabs(tab, tm) + toggle
        if v.rowlen(left) + 2 + v.rowlen(right) <= W:
            return v.pad(left, W - v.rowlen(right)) + right
    raise AssertionError(W)


def prompt(W, quota, st0=None):
    line = [G('─' * W)]
    if quota == 'missing':
        q = [T('5h —', 'inactive'), G(' │ '), T('7d —', 'inactive')]
    elif quota == 'stale':
        q = [T('5h 42% ', 'warning'), G('⏶', 'warning'), T(' 18 分鐘前', 'warning'), G(' │ '), T('7d 18% ', 'warning'), G('⏶', 'warning')]
    else:
        q = [T('5h '), G('▰▰▰▰▱▱▱▱▱▱', 'success'), T(' 42% 重置 2h10m'), G(' │ '), T('7d '), G('▰▰▱▱▱▱▱▱▱▱', 'success'), T(' 18%')]
    st = [T(' codeforge'), G(' │ '), T('develop'), G(' │ ')] + q + [G(' │ ')] + fleet_seg(st0 or {}) + [G(' │ '), T('記憶 L1 214 條')]
    return [line, v.pad([T('> ')], W), line, v.pad(v.fit(st, W), W)]


def fleet_seg(st0):
    """最底 statusline 的 fleet 段跟 pane 底列同一個來源（r2 codex 2）。"""
    if st0.get('fleet') == '—':
        return [T('fleet —', 'inactive')]
    if 'fleet' in st0.get('stale', ()):
        return [T('fleet 3 lane ', 'warning'), G('⏶', 'warning'), T(' 11 分鐘前', 'warning')]
    return [T('fleet 3 lane 執行中')]


# ---------- 組一張 ----------
def with_scroll_keys(hints, r_over, d_over, narrow):
    keys = [k for k, _ in hints]
    extra = []
    if r_over and '↑↓' not in keys:
        extra.append(H('↑↓', '捲'))
    if d_over and 'PgUp/PgDn' not in keys:
        extra.append(H('PgUp/PgDn', '捲上方' if narrow else '捲來龍去脈'))
    if not extra:
        return hints
    cut = len(hints) - 1 if keys and keys[-1] == 'Esc' else len(hints)
    return hints[:cut] + extra + hints[cut:]


def compose(W, Hh, s):
    tab, sid = s['tab'], s['id']
    narrow = W < 110
    base = (s['nhints'] if narrow and s['nhints'] else s['hints']) or DEFAULT_HINTS[tab]
    if narrow:
        rw, n = W - 4, 5
    else:
        rw, n = W - LW - 4, 9 if Hh >= 49 else 7
    vis, r_over = window(render_items(s['reply'], rw), n, s['rscroll'], rw)
    # 上半：來龍去脈的高度（band 高度固定，先算）
    band_h = (2 + n + 1) if narrow else (1 + n + 1)
    top_h = Hh - band_h - 1 - 4
    pw = W if narrow else W - LW
    cap = top_h - 4 - 2 if narrow else min(top_h - 3, 18 if Hh >= 49 else 12)
    pin = [[T(*seg(x))] for x in s['pin']]
    dvis, d_over = detail_window(pin, render_items(s['detail'], pw - 4)[0], cap)
    hrow = hint_row(with_scroll_keys(base, r_over, d_over, narrow))
    tabname = f'{tab} {TABS[tab - 1]}'
    if narrow:
        assert v.dw(s['compact']) <= W - 4, f'{sid}: index 摘要放不下'
        assert v.rowlen(hrow) <= W - 7, f'{sid}: 鍵列放不下（{v.rowlen(hrow)} > {W - 7}）'
        band = [v.frame_top([T(f'{tabname} · {s["title"]}')], W), v.frame_row([T(s['compact'])], W)]
        band += [v.frame_row(r, W) for r in vis] + [v.frame_row([], W)] * (n - len(vis))
        band.append(v.frame_bottom(hrow, W))
    else:
        assert v.rowlen(hrow) <= W - LW - 5, f'{sid}: 鍵列放不下（{v.rowlen(hrow)} > {W - LW - 5}）'
        ix = [[T(*seg(x))] for x in s['ix']]
        assert len(ix) <= n, f'{sid}: index {len(ix)} 列 > {n}'
        tail = [G(' '), T('[-]', 'inactive'), G(' ─╮')]
        band = [v.hline('╭', [T(f'{tabname} · 清單')], LW) + v.hline('┬', [T(s['title'])], W - LW - v.rowlen(tail)) + tail]
        for i in range(n):
            left = ix[i] if i < len(ix) else []
            assert v.rowlen(left) <= LW - 3, f'{sid}: index 列放不下：{left}'
            right = vis[i] if i < len(vis) else []
            band.append([G('│ ')] + v.pad(left, LW - 3) + [G(' │ ')] + v.pad(right, rw) + [G(' │')])
        band.append([G('╰' + '─' * (LW - 1))] + v.hline('┴', hrow, W - LW - 1) + [G('╯')])
    band_text = ''.join(x for r in band for x, _, _ in r)
    bad = [w for w in BAND_BANNED if w in band_text]
    assert not bad, f'{sid}: band 出現 {bad}'
    bottom = band + [status_line(W, tab, s['status'])] + prompt(W, s['quota'], s['status'])
    assert len(bottom) == band_h + 5
    pane = [v.frame_top([T(f'{tabname} · 來龍去脈')], pw, ctrl='')] + [v.frame_row(r, pw) for r in dvis] + [v.frame_bottom([T('只能看 · 跟著下面那一筆換', 'inactive')], pw)]
    conv = [[]] + v.CONV + [[]] * Hh
    if narrow:
        ch = top_h - len(pane)
        assert ch >= 4, (sid, ch)
        rows = [v.pad(v.fit(x, W), W) for x in conv[:ch]] + pane
    else:
        cw = W - pw - 1
        off = top_h - len(pane)
        assert off >= 1, (sid, off)
        rows = [v.pad(v.fit(conv[i], cw), cw) + [T(' ')] + (pane[i - off] if i >= off else [T(' ' * pw)]) for i in range(top_h)]
    act = s['key'] if s['key'].startswith('設計稿') else '按 ' + s['key']
    rows[0] = v.pad(v.fit([T(f'設計稿註記 · {sid} {TABS[tab - 1]} · {s["title"]} · {act} → {s["next"]}', 'inactive')], W), W)
    rows += bottom
    assert len(rows) == Hh and all(v.rowlen(r) == W for r in rows), (W, Hh, sid)
    return rows


# ---------- 覆蓋表：每個資料來源的缺源／過時各在哪一步 ----------
COVERAGE = [
    ('pm status', '缺源', '1-X'), ('pm status', '過時', '1-S'), ('console', '缺源', '1-X2'), ('console', '過時', '1-S'), ('live', '缺源', '1-X'), ('live', '過時', '1-S'),
    ('decision list', '缺源', '2-X'), ('decision list', '過時', '2-S'),
    ('pm／console／fleetSummary', '缺源', '3-X'), ('pm／console／fleetSummary', '過時', '3-S'), ('live.rows', '缺源', '3-X'), ('live.rows', '過時', '3-S'),
    ('live.review、qc、decisions', '缺源', '4-X'), ('live.review、qc、decisions', '過時', '4-S'),
    ('seats --quota', '缺源', '5-X'), ('seats --quota', '過時（快取 ⏶）', '5-S'), ('額度池', '缺源', '5-X2'), ('額度池', '過時', '5-S'),
    ('今日用量、24h 趨勢', '缺源', '5-X2'), ('今日用量、24h 趨勢', '過時', '5-S'), ('輸出速度 steps', '缺源', '5-X'), ('輸出速度 steps', '過時', '5-S'),
    ('residue', '缺源', '6-X'), ('residue', '過時', '6-S'), ('load_source', '缺源', '6-X2'),
    ('本機卡 decide', '缺源', '7-X'), ('本機卡 decide', '過時', '7-S'), ('他機卡 others', '缺源（還沒接線）', '7-X'), ('他機卡 others', '過時（接線後 fixture）', '7-S'),
    ('runs', '缺源', '7-X2'), ('runs', '過時', '7-S'), ('pm／console／fleetFeed', '缺源', '7-X2'), ('pm／console／fleetFeed', '過時', '7-S'),
    ('agentsColumn', '缺源', '7-X'), ('fleet todo', '缺源（還沒接線）', '8-X'), ('fleet todo', '過時', '8-S')]


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else 'out'
    if os.path.basename(os.path.normpath(out)) == 'out' and os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out, exist_ok=True)
    for W, Hh in SIZES:
        for n, s in enumerate(S, 1):
            rows = compose(W, Hh, s)
            d = os.path.join(out, f'{W}x{Hh}-s{n}')
            os.makedirs(d, exist_ok=True)
            text = [''.join(x for x, _, _ in r) for r in rows]
            with open(os.path.join(d, 'screen.txt'), 'w') as o:
                o.write('\n'.join(text) + '\n')
            if any(URL in t for t in text):
                with open(os.path.join(d, 'urls.json'), 'w') as o:
                    json.dump([URL], o, ensure_ascii=False)
            with open(os.path.join(d, 'roles.json'), 'w') as o:
                json.dump(rows, o, ensure_ascii=False)
            for theme in ('dark', 'light'):
                with open(os.path.join(d, theme + '.ans'), 'w') as o:
                    o.write(v.ans(rows, theme))
    flow = [dict(step=n, id=s['id'], tab=TABS[s['tab'] - 1], state=s['state'], title=s['title'], key_to_next=s['key'], next=NUM[s['next']]) for n, s in enumerate(S, 1)]
    cov = [{'來源': a, '狀態': b, '步號': c, 'step': NUM[c]} for a, b, c in COVERAGE]
    with open(os.path.join(out, 'flow.json'), 'w') as o:
        json.dump(flow, o, ensure_ascii=False, indent=2)
    with open(os.path.join(out, 'coverage.json'), 'w') as o:
        json.dump(cov, o, ensure_ascii=False, indent=2)
    print('ok', len(S), 'steps x', len(SIZES), 'sizes')


if __name__ == '__main__':
    main()
