#!/usr/bin/env python3
"""Mockups for plan 2026-11-18-pane-tabs-r2: band + 8 pane pages at 160x40 and 80x24, state variants, one 120x30.

Every frame is composed from the fixed layout of §3 and checked: exactly H rows, every row exactly W cells,
header = row 1, body <= H-2-A, actions occupy only the A rows directly above the strip (bottom-anchored),
strip = last row. A page that passes more than A actions fails here. Body overflow: the last body row is
the hint "↓ 還有 N 列（↓）". Run: python3 doc/plans/2026-11-18-pane-tabs-r2.mockups.py > out.md
Rows are printed right-trimmed to keep the file small; the asserts check the padded width.
Role glyphs (colour = CC theme key of plan 1fba336 §5): ✖ broken/error  ◈ overdue/error  ▲ warning
● running  ⋯ waiting_on  ✓ ok  ○ idle  ★ recommended  ▸ focused row.
Quota arithmetic base: now = 2026-11-18 (週三) 01:00 Taipei; projection = 7d% + rate x hours to reset.
"""
import sys
import unicodedata

SIZES = {"160x40": (160, 40), "120x30": (120, 30), "80x24": (80, 24)}
TABS = ["現況", "待決", "艦隊", "額度", "派工", "審查", "花費", "殘留"]
NONE = "沒有要你處理的事"
EXTRA = [("9", "寵物"), ("0", "forge"), ("i", "說明")]


def cells(s):
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 1 for c in s)


def fit(s, w):
    out, n = "", 0
    for c in s:
        k = cells(c)
        if n + k > w:
            break
        out, n = out + c, n + k
    return out + " " * (w - n)


def pad(s, w):
    if w == 0:  # last column: free width
        return s
    assert cells(s) <= w, (s, w)
    return s + " " * (w - cells(s))


def cols(items, widths, trim=True):
    s = "".join(pad(i, w) for i, w in zip(items, widths))
    return s.rstrip() if trim else s


def lr(left, right, w):
    gap = w - cells(left) - cells(right)
    assert gap >= 2, (left, right)
    return left + " " * gap + right


def cap(acts, a):
    if len(acts) <= a:
        return acts
    return acts[: a - 1] + [f"… 還有 {len(acts) - a + 1} 項（j/k 選列）"]


assert cap(list("abcde"), 4)[-1] == "… 還有 2 項（j/k 選列）"


def reserved(h):
    return 6 if h >= 32 else 4


def strip(active, w, badges, pane=True):
    parts = []
    for i, t in enumerate(TABS, 1):
        lab = f"{i} {t}{badges.get(t, '')}" if w >= 120 else f"{i}{t}{badges.get(t, '')}"
        parts.append(f"[{lab}]" if t == active else f" {lab} ")
    for k, t in EXTRA:
        lab = f"{k} {t}" if w >= 120 else k
        parts.append(f"[{lab}]" if t == active else f" {lab} ")
    esc = "  Esc 收起" if w >= 120 else " Esc"
    s = "".join(parts) + (esc if pane else "")
    assert cells(s) <= w, s
    return s


def frame(size, active, header, body, actions, badges):
    w, h = SIZES[size]
    a = reserved(h)
    nb = h - 2 - a
    assert len(actions) <= a, f"{active} {size}: {len(actions)} actions > A={a}"
    for r in body + actions:
        assert cells(r) <= w, (active, size, r)
    if len(body) > nb:
        body = body[: nb - 1] + [f"↓ 還有 {len(body) - (nb - 1)} 列（↓）"]
    acts = actions or [NONE]
    rows = [fit(header, w)] + [fit(r, w) for r in body] + [" " * w] * (nb - len(body))
    rows += [" " * w] * (a - len(acts)) + [fit(r, w) for r in acts]
    rows.append(fit(strip(active, w, badges), w))
    assert len(rows) == h and all(cells(r) == w for r in rows), (active, size)
    assert cells(header) <= w and rows[0].rstrip() == header.rstrip(), (active, size, "header truncated")
    srow = fit(strip(active, w, badges), w)
    assert rows[-1] == srow and srow not in rows[:-1], (active, size, "strip not last")
    assert "[" in srow and "Esc" in srow, (active, size)
    assert rows[h - 1 - a:h - 1][-len(acts):] == [fit(r, w) for r in acts]  # actions bottom-anchored above strip
    assert not any(r.strip() for r in rows[h - 1 - a:h - 1 - len(acts)])
    if not actions:
        assert rows[h - 2].strip() == NONE
    if any(r.startswith("↓ 還有") for r in rows):
        assert rows[nb].startswith("↓ 還有")  # hint is the last body row
    return [r.rstrip() for r in rows]


def band(size, left, right, badges):
    w = SIZES[size][0]
    row1 = lr(left, right, w)
    return [fit(row1, w).rstrip(), fit(strip(None, w, badges, pane=False), w).rstrip()]


BADGES = {"待決": "³", "艦隊": "✖", "額度": "✖", "殘留": "²"}
HDR = {
    "160x40": "autopilot · pm 席 · fleet 資料 12 秒前 · 額度 40 秒前 · 本席 codeforge--claude plan/shared-tui@926af0e",
    "120x30": "autopilot · pm 席 · fleet 資料 12 秒前 · 額度 40 秒前 · 本席 codeforge--claude",
    "80x24": "pm 席 · fleet 12 秒前 · 額度 40 秒前",
}
# most dangerous pool (A7 order: limit_hit first) = personal; pre-r5 data: stranded/switched null
BAND_R = {"160x40": "額度 ✖ personal 已用完，換池資料無來源",
          "120x30": "額度 ✖ personal 已用完，換池資料無來源",
          "80x24": "✖ personal 已用完"}
BAND_L = {"160x40": "✖ capacity 卡住 2 時 · 待決 3 張（最舊 40 分）", "120x30": "✖ capacity 卡住 2 時 · 待決 3 張（最舊 40 分）",
          "80x24": "✖ capacity 卡住 2時 · 待決 3"}
SEATS_Q = "seats --quota 40 秒前"

P = {}
# ---------------------------------------------------------------- 現況
P["現況"] = {
    "160x40": (
        ["── 全艦隊 ──────────────────────────────────────────────",
         "  產線 51 ｜ ● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ｜ 主機 17/18 在線 ｜ 額度 最緊 personal 已用完，換池資料無來源",
         lr("▸ ✖ capacity--claude 卡住 2 小時：等待沒寫等誰（wait_untyped）", "2 時前", 120),
         lr("  ◈ hangar--claude 01M7N2ZC 證據逾期 40 分", "40 分前", 120),
         lr("  ▲ 主機 h6 離線 12 分", "12 分前", 120),
         "",
         "── 本席 codeforge--claude ───────────────────────────────",
         "  位置   plan/shared-tui @926af0e · 階段 plan · ctx 41%",
         "  任務   01M7N307 ⋯ 等待：r2 草稿 03:00 · 01M7N2TZ ⋯ 等 autopilot 01M7N2Z5",
         "  背景   ● 盤點子代理 4 分 · ● plan review astra 第 1 代",
         "  派工   ● 1 執行中（sonnet hand feat/console-source 23 分）",
         "  審查   plan 第 1 代 2/2 席回來 · 推送前檢查 ✓",
         f"  花費   本席（池 team）5 小時 17% · 7 天 68% · 今天 $3.10（{SEATS_Q}）",
         "  殘留   2 項可清",
         "",
         "── 最近對話 ─────────────────────────────────────────────",
         lr("  PM 01M7N33J 三條硬規定：分頁列在最下一列、高度固定、過期標紅", "15 分前", 120),
         lr("  autopilot 01M7N30R 已開工 feat/console-source", "18 分前", 120)],
        ["→ 待決 3 張卡等你（最舊 40 分）                                  [2]",
         "→ 殘留 2 項可清                                                  [8]",
         "[o] 看焦點項"],
    ),
    "120x30": (
        ["── 全艦隊 ──────────────────────────────────────────────",
         "  產線 51 ｜ ● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ｜ 主機 17/18 在線",
         "  額度   最緊 personal 已用完，換池資料無來源",
         lr("▸ ✖ capacity--claude 卡住 2 小時：等待沒寫等誰（wait_untyped）", "2 時前", 118),
         lr("  ◈ hangar--claude 01M7N2ZC 證據逾期 40 分", "40 分前", 118),
         lr("  ▲ 主機 h6 離線 12 分", "12 分前", 118),
         "",
         "── 本席 codeforge--claude ───────────────────────────────",
         "  位置   plan/shared-tui @926af0e · 階段 plan · ctx 41%",
         "  任務   01M7N307 ⋯ 等待：r2 草稿 03:00 · 01M7N2TZ ⋯ 等 autopilot 01M7N2Z5",
         "  派工   ● 1 執行中（sonnet hand feat/console-source 23 分）",
         "  審查   plan 第 1 代 2/2 席回來 · 推送前檢查 ✓",
         f"  花費   本席（池 team）5 小時 17% · 7 天 68% · 今天 $3.10（{SEATS_Q}）",
         "  殘留   2 項可清",
         "",
         "── 最近對話 ─────────────────────────────────────────────",
         lr("  PM 01M7N33J 三條硬規定：分頁列在最下一列、高度固定、過期標紅", "15 分前", 118),
         lr("  autopilot 01M7N30R 已開工 feat/console-source", "18 分前", 118)],
        ["→ 待決 3 張卡等你（最舊 40 分）                       [2]",
         "→ 殘留 2 項可清                                       [8]",
         "[o] 看焦點項"],
    ),
    "80x24": (
        ["產線 51 ●6 ⋯40 ✖5 · 主機 17/18 · 額度 ✖ personal 已用完",
         "▸✖ capacity 卡住 2 時：等待沒寫等誰",
         " ◈ hangar 01M7N2ZC 證據逾 40 分",
         " ▲ 主機 h6 離線 12 分",
         "── 本席 codeforge ──",
         "plan/shared-tui@926af0e · plan · ctx 41%",
         "01M7N307 ⋯ r2 草稿 03:00",
         "派工 ●1 · 審查 2/2 · 花費 7天 68%",
         "── 最近 ──",
         "PM 01M7N33J 三條硬規定 15 分前",
         "autopilot 01M7N30R 已開工 18 分前"],
        ["→ 待決 3 張（最舊 40 分）     [2]", "→ 殘留 2 項               [8]", "[o] 看焦點項"],
    ),
}
OVERFLOW80 = [
    "產線 51 ●6 ⋯40 ✖5 · 主機 17/18 · 額度 ✖ personal 已用完",
    "▸✖ capacity 卡住 2 時：等待沒寫等誰",
    " ✖ fleet 卡住 5 時：等 hangar 01M7N1AA",
    " ✖ alfred 卡住 3 時：測試紅未處理",
    " ✖ 308 卡住 1 天：等操作員到場",
    " ✖ lab 卡住 6 時：等 PM 01M7N1B2",
    " ◈ hangar 01M7N2ZC 證據逾 40 分",
    " ◈ autopilot 01M7N2Z5 證據逾 25 分",
    " ▲ 主機 h6 離線 12 分",
    " ▲ 主機 h4 磁碟 91%",
    "── 本席 codeforge ──",
    "plan/shared-tui@926af0e · plan · ctx 41%",
    "01M7N307 ⋯ r2 草稿 03:00",
    "背景 ●盤點 4 分 · ●plan review g1",
    "派工 ●1 · 審查 2/2 · 花費 7天 68%",
    "殘留 2 項可清",
    "── 最近 ──",
    "PM 01M7N33J 三條硬規定 15 分前",
    "autopilot 01M7N30R 已開工 18 分前",
    "hangar 01M7N2ZC 回報盤點 40 分前",
    "PM 01M7N2YJ 退回讀快照 1 時前",
    "alfred 01M7N2QX 測試紅 3 時前",
]
# no-pace 現況: same most-dangerous pool as the band (used up > stale > highest 7d%); spend = same seats --quota value as 額度
NOPACE_NOW80 = list(P["現況"]["80x24"][0])
NOPACE_NOW80[3] = " ▲ 額度 personal 已用完，換池資料無來源"

# ---------------------------------------------------------------- 待決
P["待決"] = {
    "160x40": (
        ["▸ d-89fe394f 1/3  feature/legacy-payment-rewrite 和 feature/social-login 依序 --no-ff 合進 legacy？     執行者 game-lead · 40 分",
         "  背景   兩支分支都過審，合併順序影響 migration 編號",
         "  卡住   game-lead 的 row 不能自己合 main 以外的 legacy 分支",
         "  建議   可以：先 payment 再 social，CI 綠才合第二支",
         "  [a] 可以 ★    [b] 先不要",
         "  補充   （按 n 寫補充，會跟答案一起送）",
         "",
         "── 其他待決 2（z 收合）──────────────────────────────",
         "  d-fb890f48  [測試卡] carddispatch 真 courier 紅綠，請勿在面板作答                       PM · 2 時",
         "  d-fd51ee8d  下次到 308 現場時量 30 格（要你到場，只能回報）                             308 · 1 天",
         "── 別台的卡（唯讀，到那一席答）────────────────────────────",
         "  d-1c0a77e2  h6 席：GPU slot 重排要你點頭 → 到 cookys/h6/game-lead--claude 答",
         "── 要你做或看 ──────────────────────────────────────────",
         "  步驟  01M7N2TZ 上線後看 pane 截圖"],
        ["已選 a 可以（尚未送出）",
         "[a]–[b] 選項（每張卡最多 5 個選項 a–e）   [n] 補充（輸入中數字字母都是文字，Enter/Esc 結束）",
         "[s] 送出焦點卡   [u] 送出後 10 秒內撤回",
         "[h] 上一張  [l] 下一張  [z] 收合其他待決",
         "[f] 回報填入提示列（不送出）"],
    ),
    "80x24": (
        ["▸ d-89fe394f 1/3 · game-lead · 40 分",
         "payment 和 social-login 依序 --no-ff 合進 legacy？",
         "背景 兩支都過審，順序影響 migration",
         "建議 可以：先 payment 再 social",
         "[a] 可以 ★   [b] 先不要",
         "── 其他待決 2（z 收合）──",
         "d-fb890f48 [測試卡] 請勿作答 · 2時",
         "d-fd51ee8d 308 現場量 30 格 · 1天",
         "── 別台（唯讀）──",
         "d-1c0a77e2 h6：GPU 重排 → 到該席答",
         "── 要你做或看 ──",
         "01M7N2TZ 上線後看截圖"],
        ["已選 a（尚未送出） [n] 補充", "[s] 送出  [u] 10 秒內撤回", "[h] 上張 [l] 下張 [f] 填入提示列"],
    ),
}
HOLD80 = (
    ["▸ d-89fe394f 1/3 · game-lead · 40 分",
     "payment 和 social-login 依序 --no-ff 合進 legacy？",
     "已送出：[b] 先不要",
     "── 其他待決 2（z 收合）──",
     "d-fb890f48 [測試卡] 請勿作答 · 2時",
     "d-fd51ee8d 308 現場量 30 格 · 1天",
     "── 別台（唯讀）──",
     "d-1c0a77e2 h6：GPU 重排 → 到該席答"],
    ["已送出 b，8 秒內按 u 撤回", "[h] 上張 [l] 下張（可先看別張）"],
)
EMPTY_CARDS80 = (["○ 目前沒有待決卡", "── 別台的卡（唯讀）──", "○ 沒有"], [])
CARD5_120 = (
    ["▸ d-4c2e19a0 2/3  馬拉松分支 5 支怎麼合進 release？                       執行者 game-lead · 2 時",
     "  背景   五支分支互相有衝突，合併順序決定誰要 rebase",
     "  卡住   game-lead 只能合 main，release 要你點頭",
     "  建議   [a] 一次全合；風險是一支壞全壞",
     "  [a] 全部一次合",
     "  [b] 依序合，每支綠才合下一支 ★",
     "  [c] 拆兩批，每批 2–3 支",
     "  [d] 只合前兩支，其餘下週",
     "  [e] 先不要",
     "  補充   （按 n 寫補充，會跟答案一起送）",
     "",
     "── 其他待決 2（z 收合）───────────────────────────────",
     "  d-fb890f48  [測試卡] carddispatch 真 courier 紅綠，請勿在面板作答               PM · 2 時",
     "  d-fd51ee8d  下次到 308 現場時量 30 格（要你到場，只能回報）                     308 · 1 天"],
    ["已選 c 拆兩批，每批 2–3 支（尚未送出）",
     "[a]–[e] 選項（卡上限 5 個）   [n] 補充（輸入中數字字母都是文字，Enter/Esc 結束）",
     "[s] 送出焦點卡   [u] 送出後 10 秒內撤回   [f] 回報填入提示列（不送出）",
     "[h] 上一張  [l] 下一張  [z] 收合其他待決"],
)
P["待決"]["120x30"] = CARD5_120
# fold pair at 80x24 (same rows): the 其他待決 group
PENDING_CLOSED80 = (
    P["待決"]["80x24"][0][:5] + ["── 其他待決 2（z 展開）──", "── 別台（唯讀）──",
     "d-1c0a77e2 h6：GPU 重排 → 到該席答", "── 要你做或看 ──", "01M7N2TZ 上線後看截圖"],
    ["已選 a（尚未送出） [n] 補充", "[s] 送出  [u] 10 秒內撤回", "[h] 上張 [l] 下張 [z] 展開其他"],
)

# ---------------------------------------------------------------- 艦隊 (A5: fold = z, o = 看; A11: shown + folded = 51)
W_L = [22, 8, 14, 44, 8]
W_L2 = [20, 7, 12, 40, 6]
STUCK = ["lab · h3", "pinball · h4", "revival · h5"]
EXEC = ["hippo · h2", "mnemos · h3", "proj-c · h3"]


def folded_rows(n_wait, widths, narrow=False):
    rows = [cols(["  ✖ " + n, "卡住", "PM", "等 PM 回覆", f"{k} 時"], widths) for n, k in zip(STUCK, (6, 5, 3))]
    rows += [cols(["  ● " + n, "執行", "PM", "進行中", "2 分"], widths) for n in EXEC]
    rows += [cols([f"  ⋯ lane-{i:02d} · h{i % 5 + 1}", "等待", "PM", "waiting_on task", f"{i % 9 + 1} 時"], widths)
             for i in range(n_wait)]
    return rows


LANES160 = [
    cols(["  產線", "狀態", "負責", "任務 / 證據", "上次"], W_L),
    cols(["▸ ✖ capacity · h1", "卡住", "PM", "等待沒寫等誰（wait_untyped）", "2 時"], W_L),
    cols(["  ✖ hangar · h2", "卡住", "PM", "01M7N2ZC 證據逾期 40 分", "40 分"], W_L),
    cols(["  ● autopilot · h1", "執行", "PM", "01M7N2Z5 feat/console-source", "1 分"], W_L),
    cols(["  ● codeforge · h1", "執行", "PM", "01M7N307 r2 草稿", "0 分"], W_L),
    cols(["  ● fleet · h2", "執行", "PM", "console.json 快照 04:00", "3 分"], W_L),
    cols(["  ⋯ 308 · h1", "等待", "game-lead", "等操作員到現場", "1 天"], W_L),
    cols(["  ⋯ alfred · h1", "等待", "PM", "waiting_on task:01M7…", "4 時"], W_L),
    "  ○ 另 44 條已折疊（顯示 7 + 折疊 44 = 51）· z 展開",
]
LANES120 = [cols([a for a in (r[0], r[1], r[2], r[3], r[4])], W_L2) for r in (
    ["  產線", "狀態", "負責", "任務 / 證據", "上次"],
    ["▸ ✖ capacity · h1", "卡住", "PM", "等待沒寫等誰（wait_untyped）", "2 時"],
    ["  ✖ hangar · h2", "卡住", "PM", "01M7N2ZC 證據逾期 40 分", "40 分"],
    ["  ● autopilot · h1", "執行", "PM", "01M7N2Z5 feat/console-source", "1 分"],
    ["  ● codeforge · h1", "執行", "PM", "01M7N307 r2 草稿", "0 分"],
    ["  ● fleet · h2", "執行", "PM", "console.json 快照 04:00", "3 分"],
    ["  ⋯ 308 · h1", "等待", "game-lead", "等操作員到現場", "1 天"],
    ["  ⋯ alfred · h1", "等待", "PM", "waiting_on task:01M7…", "4 時"],
)] + ["  ○ 另 44 條已折疊（顯示 7 + 折疊 44 = 51）· z 展開"]
HOSTS = ["", "── 主機 17/18 在線 ─────────────────────────────────────",
         "  ● h1 6 席   ● h2 4 席   ● h3 GPU 0/2   ● h4 2 席   ● h5 1 席   ✖ h6 離線 12 分   … 另 12 台 ●"]
FLEET_ACTS = ["▸ 焦點 capacity · h1：等待沒寫等誰（wait_untyped）", "[o] 看 capacity",
              "[z] 展開／收合其他 44 條", "[f] 催辦填入提示列（不送出）"]
P["艦隊"] = {
    "160x40": (["── 產線 51：● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ─────────────────"] + LANES160 + HOSTS, FLEET_ACTS),
    "120x30": (["── 產線 51：● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ───────────"] + LANES120 + HOSTS[:2]
               + ["  ● h1 6 席  ● h2 4 席  ● h3 GPU 0/2  ● h4 2 席  ● h5 1 席  ✖ h6 離線 12 分  … 另 12 台 ●"], FLEET_ACTS),
    "80x24": (
        ["產線 51 ●6 ⋯40 ✖5",
         "▸✖ capacity 卡住 等待沒寫等誰   2時",
         " ✖ hangar   卡住 證據逾期      40分",
         " ● autopilot 執行 console-source 1分",
         " ● codeforge 執行 r2 草稿        0分",
         " ● fleet     執行 console.json  3分",
         " ⋯ 308       等待 操作員到場     1天",
         " ○ 另 45 條折疊（6+45=51）z 展開",
         "── 主機 17/18 ──",
         "●h1 ●h2 ●h3 ●h4",
         "✖ h6 離線 12 分 · 另 12 台 ●"],
        ["[o] 看 capacity", "[z] 展開／收合其他 45 條", "[f] 催辦填入提示列（不送出）"],
    ),
}
FLEET_OPEN80 = (P["艦隊"]["80x24"][0][:7]
                + [" ○ 已展開 45 條（6+45=51）z 收合"]
                + [r.rstrip() for r in [
                    f" {m} {n}  {st}" for m, n, st in
                    [("✖", "lab", "卡住 6時"), ("✖", "pinball", "卡住 5時"), ("✖", "revival", "卡住 3時"),
                     ("●", "hippo", "執行 2分"), ("●", "mnemos", "執行 2分"), ("●", "proj-c", "執行 2分")]
                    + [("⋯", f"lane-{i:02d}", f"等待 {i % 9 + 1}時") for i in range(39)]]])
STALE80 = (
    ["產線 ✖51 ●6✖ ⋯40✖ ✖5✖（6 分前舊數字）",
     "▸✖ capacity 卡住 等待沒寫等誰  ✖2時",
     " ✖ hangar   卡住 證據逾期    ✖40分",
     " ● autopilot 執行 console-source ✖1分",
     " ● codeforge 執行 r2 草稿        ✖0分",
     " ● fleet     執行 console.json  ✖3分",
     " ⋯ 308       等待 操作員到場     ✖1天",
     " ○ 另 45 條折疊 ✖（6+45=51）z 展開",
     "── 主機 ✖17/18（6 分前）──",
     "●h1✖ ●h2✖ ●h3✖ ●h4✖",
     "✖ h6 離線 ✖12 分 · 另 12 台 ●✖"],
    ["[o] 看 capacity（舊資料）", "[z] 展開／收合其他 45 條", "[f] 催辦填入提示列（不送出）"],
)
# A10: at 80x24 the healthy fields shrink first, the whole 「門檻；原因」 stays
STALE_HDR = "✖ fleet 6 分未更新（>5 分；console 逾時）· 額度 40 秒"

# ---------------------------------------------------------------- 額度 (A7: pre-r5 data, burn_window_h 6, stranded/switched null)
W_Q = [22, 7, 8, 17, 10, 7, 13, 6]
HDRQ = cols(["  池", "7 天", "5 小時", "重置（台北）", "近 6 時", "預測", "用完時間", "席數"], W_Q, False) + "判定"


def qrow(*c):
    return cols(list(c[:8]), W_Q, False) + c[8]


STALE_TRIN = "✖ 180 分未更新（>10 分；statusline 沒在跑）"
STALE_SEAT = "✖ 席 twgame-2 過期 14 分，舊值 55% 較高"
Q160 = [
    "── 額度池（console quota.pools；預測 = fleet 算（projection_source），pane 不重算；近 6 時 = burn_window_h 6）──── 資料 40 秒前",
    HDRQ,
    qrow("▸ ✖ personal", "100%", "100%", "11-19 週四 08:00", "+1.1/h", "100%", "已用完", "5 席", "已用完，換池資料無來源"),
    qrow("  ✖ team", "68%", "17%", "11-21 週六 12:00", "+1.9/h", ">100%", "11-18 18:00", "3 席", "約 17 時後用完，換池資料無來源"),
    qrow("  ✖ trinity", "31% ✖", "0% ✖", "11-20 週五 06:00", "+0.3/h ✖", "47% ✖", "—", "1 席", STALE_TRIN),
    qrow("  ✖ twgame", "49%", "0%", "11-19 週四 09:00", "+1.4/h", "94%", "—", "2 席", STALE_SEAT),
    qrow("  ▲ grok", "10%", "0%", "11-24 週二 00:00", "+0.0/h", "10%", "—", "1 席", "約剩 90% 會浪費"),
    qrow("  ▲ lab", "22%", "4%", "11-22 週日 10:00", "+0.2/h", "43%", "—", "2 席", "約剩 57% 會浪費"),
    qrow("  ✓ codex", "80%", "12%", "11-19 週四 21:00", "+0.4/h", "98%", "—", "2 席", "會用到 98%"),
    "",
    "── 焦點池 personal（o 展開各席）──────────────",
    "  席      capacity--claude · hangar--claude · … 共 5 席（seats --quota 40 秒前，都在 10 分鐘內）",
    "  換池    stranded_seats、switched 都是 null（capacity r5 之前沒有來源）",
    "  判定    紅：limit_hit 為真，已用完，換池資料無來源",
    "  排序    紅：已用完 → 最早用完 → 過期（最舊在前）；黃：浪費比例多的在前；最後才是綠",
]
W_Q120 = [21, 6, 7, 16, 9, 7]
HDRQ120 = cols(["  池", "7 天", "5 小時", "重置（台北）", "近 6 時", "預測"], W_Q120, False) + "判定"


def qrow120(*c):
    return cols(list(c[:6]), W_Q120, False) + c[6]


Q120 = [
    "── 額度池（預測 = fleet 算，pane 不重算；近 6 時 = burn_window_h 6）──── 資料 40 秒前",
    HDRQ120,
    qrow120("▸ ✖ personal", "100%", "100%", "週四 08:00", "+1.1/h", "100%", "已用完，換池資料無來源"),
    qrow120("  ✖ team", "68%", "17%", "週六 12:00", "+1.9/h", ">100%", "約 17 時後用完，換池資料無來源"),
    qrow120("  ✖ trinity", "31% ✖", "0% ✖", "週五 06:00", "+0.3/h ✖", "47% ✖", STALE_TRIN),
    qrow120("  ✖ twgame", "49%", "0%", "週四 09:00", "+1.4/h", "94%", STALE_SEAT),
    qrow120("  ▲ grok", "10%", "0%", "週二 00:00", "+0.0/h", "10%", "約剩 90% 會浪費"),
    qrow120("  ▲ lab", "22%", "4%", "週日 10:00", "+0.2/h", "43%", "約剩 57% 會浪費"),
    qrow120("  ✓ codex", "80%", "12%", "週四 21:00", "+0.4/h", "98%", "會用到 98%"),
    "",
    "── 焦點池 personal（o 展開各席）──────────────────────",
    "  席    capacity--claude · hangar--claude · … 共 5 席（seats --quota 40 秒前）",
    "  換池  stranded_seats、switched 都是 null（capacity r5 之前沒有來源）",
    "  判定  紅：limit_hit 為真，已用完，換池資料無來源",
]
WQ80 = [12, 5, 5, 7, 8, 6, 0]
Q80 = [
    "額度池 · 預測 fleet 算 · 近 6 時 = burn_window_h 6",
    cols(["池", "7d", "5h", "重置", "近 6 時", "預測", "判定"], WQ80),
    cols(["▸✖ personal", "100%", "100%", "週四08", "+1.1", "100%", "紅 已用完，換池資料無來源"], WQ80),
    cols([" ✖ team", "68%", "17%", "週六12", "+1.9", ">100%", "紅 約17時後用完，換池資料無來源"], WQ80),
    cols([" ✖ trinity", "31%✖", "0%✖", "週五06", "+0.3✖", "47%✖", "紅 ✖ 見下"], WQ80),
    cols([" ✖ twgame", "49%", "0%", "週四09", "+1.4", "94%", "紅 ✖ 見下"], WQ80),
    cols([" ▲ grok", "10%", "0%", "週二00", "+0.0", "10%", "黃 約剩90%會浪費"], WQ80),
    cols([" ▲ lab", "22%", "4%", "週日10", "+0.2", "43%", "黃 約剩57%會浪費"], WQ80),
    cols([" ✓ codex", "80%", "12%", "週四21", "+0.4", "98%", "綠 會用到98%"], WQ80),
    "✖ trinity 180 分未更新（>10 分；statusline 沒在跑）",
    "✖ twgame 席 twgame-2 過期 14 分，舊值 55% 較高",
    "── 焦點 personal（o 展開各席）──",
    "席 capacity · hangar · … 共 5 席",
    "判定 紅：已用完，換池資料無來源",
]
P["額度"] = {
    "160x40": (Q160, ["[o] 展開焦點池各席（j/k 換池）", "[f] 把焦點池的換池建議填入提示列（不送出）"]),
    "120x30": (Q120, ["[o] 展開焦點池各席（j/k 換池）", "[f] 換池建議填入提示列（不送出）"]),
    "80x24": (Q80, ["[o] 展開焦點池", "[f] 換池建議填入提示列（不送出）"]),
}
NP = "✖ 無來源：capacity pace 未接"
W_N = [12, 5, 5, 8]
NOPACE80 = (
    ["額度池 · ✖ 無來源：capacity pace 未接",
     "（seats --quota 逐席列，依 pool_key 分池；池取席內最大值）",
     cols(["池", "7d", "5h", "重置", "速率 · 預測 · 判定"], W_N + [0]),
     cols(["▸✖ personal", "100%", "100%", "週四08", "紅 已用完，換池資料無來源"], W_N + [0]),
     cols([" ✖ trinity", "31%✖", "0%✖", "週五06", STALE_TRIN], W_N + [0]),
     cols([" ✖ codex", "80%", "12%", "週四21", NP], W_N + [0]),
     cols([" ✖ team", "68%", "17%", "週六12", NP], W_N + [0]),
     cols([" ✖ twgame", "49%", "0%", "週四09", NP], W_N + [0]),
     cols([" ✖ lab", "22%", "4%", "週日10", NP], W_N + [0]),
     cols([" ✖ grok", "10%", "0%", "週二00", NP], W_N + [0]),
     "危險排序：1 已用完 personal → 2 過期 trinity → 3 7d 最高 codex",
     "── 焦點 personal（逐席）──",
     "capacity 100%/100% · hangar 100%/100% · … 共 5 席",
     "limit_hit 為真，所以判定仍是紅：已用完"],
    ["[o] 展開焦點池逐席", "（沒有 pace，就沒有換池建議）"],
)
NOPACE_HDR = "pm 席 · fleet 12 秒前 · 額度 40 秒前（seats --quota 逐席）"

# ---------------------------------------------------------------- 派工 / 審查 / 花費 / 殘留
P["派工"] = {
    "160x40": (
        ["── 執行中 1 ──────────────────────────────────────────────",
         "  run                         角色        引擎             經過     上次",
         "▸ ● console-source            hand        sonnet           23 分    1 分前",
         "── foreman ──",
         "  ○ 無",
         "── 已完成 3（z 收合）───────────────────────────────",
         "  ✓ plan-review astra g1      reviewer    codex/astra      18 分    5 時前",
         "  ✓ plan-review carbon g1     reviewer    agy/carbon       21 分    5 時前",
         "  ✓ inventory                 explore     sonnet           1 分     4 分前"],
        [],
    ),
    "80x24": (
        ["── 執行中 1 ──",
         "▸● console-source hand sonnet 23分",
         "── 已完成 3（z 收合）──",
         " ✓ astra g1 reviewer 18分 5時前",
         " ✓ carbon g1 reviewer 21分 5時前",
         " ✓ inventory explore 1分 4分前"],
        ["[z] 收合已完成"],
    ),
}
P["派工"]["120x30"] = P["派工"]["160x40"]
DISPATCH_CLOSED80 = (
    ["── 執行中 1 ──", "▸● console-source hand sonnet 23分", "── 已完成 3（z 展開）──"],
    ["[z] 展開已完成"],
)
P["審查"] = {
    "160x40": (
        ["── 程式碼審查 ─────────────────────────────────────────",
         lr("▸ 第 1 代 f55289ca · google SHIP-AS-IS · xai SHIP-AS-IS", "▲ 135 時前，HEAD 已不同", 120),
         "── 推送前檢查 ──",
         "  ✓ check-all 綠 · 627114b",
         "── 替你決定的事 2（z 收合）─────────────────────────",
         lr("  01M7N33J 分頁列放最下一列，不另起一行", "15 分前", 120),
         lr("  01M7N2YJ 退回讀 console.json 快照，不轉 brief", "1 時前", 120),
         "── plan review ──",
         "  plan/shared-tui 第 2 代 · astra 8 · carbon 8 · 已凍結 1fba336"],
        [],
    ),
    "80x24": (
        ["── 程式碼 ──",
         "▸g1 f55289ca google✓ xai✓ ▲HEAD 不同",
         "── 推送前 ── ✓ 627114b",
         "── 替你決定 2（z 收合）──",
         "01M7N33J 分頁列放最下列 15分",
         "01M7N2YJ 退回讀快照 1時",
         "── plan ── 1fba336 凍結 8/8"],
        ["[z] 收合替你決定"],
    ),
}
P["審查"]["120x30"] = P["審查"]["160x40"]
REVIEW_CLOSED80 = (
    ["── 程式碼 ──", "▸g1 f55289ca google✓ xai✓ ▲HEAD 不同", "── 推送前 ── ✓ 627114b",
     "── 替你決定 2（z 展開）──", "── plan ── 1fba336 凍結 8/8"],
    ["[z] 展開替你決定"],
)
BAR = lambda p, n=20: "█" * round(p * n / 100) + "░" * (n - round(p * n / 100))
# 花費 = seats --quota row of this seat (pool team 68%/17%) = the same value 額度 shows for team
P["花費"] = {
    "160x40": (
        [f"── 本席 codeforge--claude（seats --quota 那一列，40 秒前；池 team）──",
         f"▸ 5 小時  {BAR(17)}  17%    重置 03:40",
         f"  7 天    {BAR(68)}  68%    重置 11-21 週六 12:00",
         "  今天    $3.10 · 近 24 小時 ▁▁▂▃▅▂▁▁▁▂▂▃▁▁▁▁▂▅▇▃▂▁▁▁",
         "",
         "  額度看 [4]"],
        [],
    ),
    "80x24": (
        ["── 本席（池 team，40 秒前）──",
         f"▸5時 {BAR(17, 10)} 17% 重置 03:40",
         f" 7天 {BAR(68, 10)} 68% 重置 週六",
         " 今天 $3.10 ▁▂▃▅▂▁▂▅▇▃",
         " 額度看 [4]"],
        [],
    ),
}
P["花費"]["120x30"] = P["花費"]["160x40"]
SPEND_STALE80 = (
    ["── 本席（池 team）──",
     f"▸5時 {BAR(17, 10)} 17% ✖ 重置 03:40",
     f" 7天 {BAR(68, 10)} 68% ✖ 重置 週六",
     " ✖ 25 分未更新，以上是舊數字",
     " 今天 $3.10 ▁▂▃▅▂▁▂▅▇▃",
     " 額度看 [4]"],
    [],
)
SPEND_STALE_HDR = "✖ 25 分未更新（>10 分；seats --quota 失敗）· pm 席"
P["殘留"] = {
    "160x40": (
        ["── 要你處理 ─────────────────────────────────────────────",
         "  ▲ worktree hetero-feat-fs-pane-v2-c1mhR9：分支已併入，可移除",
         "── 可清除 2 ──",
         "▸ ○ /dev/shm/petshot-1001 寵物截圖沙盒（重拍後）",
         "  ○ target/flaky-tests.log 7 天前",
         "── 自動清理紀錄 4 項（z 收合）──────────────────────",
         "  3 時前 · 清掉 4 項：build 暫存 2、舊 worktree 1、log 1",
         "── 外掛來源 ── ✓ autopilot 從主 clone develop 載入"],
        ["▸ 焦點：/dev/shm/petshot-1001",
         "[o] 清掉焦點這項",
         "[x] 全部清除（可清除 2 項）"],
    ),
    "80x24": (
        ["── 要你處理 ──",
         " ▲ worktree c1mhR9 已併入，可移除",
         "── 可清除 2 ──",
         "▸○ petshot-1001 沙盒",
         " ○ flaky-tests.log",
         "── 自動清理 4 項（z 收合）──",
         " 3 時前 · build 暫存 2、worktree 1、log 1",
         "── 外掛 ✓ develop"],
        ["[o] 清掉焦點這項", "[x] 全部清除 2 項", "[z] 收合自動清理"],
    ),
}
P["殘留"]["120x30"] = P["殘留"]["160x40"]
RESIDUE_CLOSED80 = (
    ["── 要你處理 ──", " ▲ worktree c1mhR9 已併入，可移除", "── 可清除 2 ──", "▸○ petshot-1001 沙盒",
     " ○ flaky-tests.log", "── 自動清理 4 項（z 展開）──", "── 外掛 ✓ develop"],
    ["[o] 清掉焦點這項", "[x] 全部清除 2 項", "[z] 展開自動清理"],
)
EMPTY80 = {
    "派工": (["○ 沒有執行中的派工，也沒有最近完成的"], []),
    "審查": (["○ 還沒有審查紀錄（程式碼審查、推送前檢查、plan review 都是空的）"], []),
    "殘留": (["○ 沒有要清的殘留"], []),
}
# ---------------------------------------------------------------- 9 / 0 / ? : same frame (A6)
P["寵物"] = {"80x24": (["", "  寵物分頁還沒上線", "", "  （P2 之後 pettab 畫在這個 body 區；",
                         "  頁首、動作區、分頁列和其他頁完全一樣）"], [])}
P["forge"] = {"80x24": (["── forge ──", "  forge 欄位沿用現有內容（P6 才改版）", "  目前只有狀態列，沒有可操作項目"], [])}
P["說明"] = {"80x24": (
    ["── 鍵位 ──",
     "1–8 切分頁  9 寵物  0 forge  i 說明",
     "Esc 收起（輸入補充時 Esc 只離開輸入）",
     "j/k 移動焦點列（行首 ▸）↑↓ 捲動",
     "o 對焦點列動作  z 折疊／展開",
     "↑↓ 捲動  f 填入提示列（不送出）",
     "── 各頁的 o ──",
     "1 現況、3 艦隊：看焦點項",
     "4 額度：展開各席   8 殘留：清掉這一項",
     "2 待決：s 送出；Enter 只按 host 焦點按鈕",
     "待決 a–e 選項  n 補充  u 10 秒內撤回",
     "      h/l 上一張／下一張",
     "── 從提示列進來 ──",
     "Ctrl+x Tab 或用滑鼠點分頁；",
     "焦點在提示列時熱鍵不處理"],
    [])}
KEYS = """### 鍵位（一張表，沒有一個鍵兩種意思）

```
1–8 切分頁   9 寵物   0 forge   i 說明   Esc 收起（輸入補充時 Esc 只離開輸入）
j/k 移動焦點列（行首 ▸）   o 對焦點列動作   z 折疊/展開   ↑↓ 捲動   Enter 只按 host 焦點所在的按鈕
待決  a–e 選項（每張卡最多 5 個，autopilot 的上限）  n 補充（輸入中數字字母是文字，Enter/Esc 結束）
      s 送出  u 10 秒內撤回  h/l 上一張/下一張  f 填入提示列（不送出）
艦隊  o 看焦點產線  z 展開/收合  f 催辦填入提示列      額度  o 展開焦點池各席  f 換池建議填入提示列
殘留  o 清焦點這項  x 全部清除             表列本身沒有按鈕，按鈕都在動作區
從提示列進 band/pane：Ctrl+x Tab 或滑鼠點分頁；焦點在提示列時，pane 和 band 的熱鍵一律不處理
```
"""


def block(rows):
    return "```\n" + "\n".join(rows) + "\n```\n"


def main():
    o = sys.stdout.write
    o(KEYS)
    for size in ("160x40", "120x30", "80x24"):
        w, h = SIZES[size]
        a = reserved(h)
        o(f"\n## {size}（頁首 1 + body {h - 2 - a} 列 + 動作區 {a} 列 + 分頁列 1 = {h}）\n")
        o("\n### band（主畫面上方，永遠 2 列：第 1 列摘要，第 2 列分頁列）\n\n")
        o(block(band(size, BAND_L[size], BAND_R[size], BADGES)))
        for page in TABS:
            body, acts = P[page][size]
            note = "（5 個選項的卡）" if (page == "待決" and size == "120x30") else ""
            o(f"\n### {page}{note}\n\n" + block(frame(size, page, HDR[size], body, acts, BADGES)))
    s = "80x24"
    o("\n## 9 / 0 / ?（80x24，和 1–8 同一個框：頁首、body、動作區、分頁列最後）\n")
    for page in ("寵物", "forge", "說明"):
        body, acts = P[page][s]
        o(f"\n### {page}\n\n" + block(frame(s, page, HDR[s], body, acts, BADGES)))

    def var(title, page, header, state, badges=BADGES):
        o(f"\n### {title}\n\n" + block(frame(s, page, header, state[0], state[1], badges)))

    o("\n## 折疊成對（80x24，每對列數都是 24，只有折疊區塊內容不同）\n")
    pairs = [("艦隊", P["艦隊"][s], FLEET_OPEN80, "其他 45 條"),
             ("待決", PENDING_CLOSED80, P["待決"][s], "其他待決"),
             ("派工", DISPATCH_CLOSED80, P["派工"][s], "已完成"),
             ("審查", REVIEW_CLOSED80, P["審查"][s], "替你決定"),
             ("殘留", RESIDUE_CLOSED80, P["殘留"][s], "自動清理")]
    for page, closed, opened, what in pairs:
        if page == "艦隊":
            var(f"{page}：折疊關（顯示 6 + 折疊 45 = 51）", page, HDR[s], closed)
            var(f"{page}：折疊開（{what}全展開，超出的用「↓ 還有 N 列」）", page, HDR[s],
                (opened, ["[o] 看 capacity", "[z] 展開／收合其他 45 條", "[f] 催辦填入提示列（不送出）"]))
        else:
            var(f"{page}：折疊關（{what}）", page, HDR[s], closed)
            var(f"{page}：折疊開（{what}）", page, HDR[s], opened)
    o("\n## 狀態變化（80x24，列數都等於 24）\n")
    var("現況：內容超過一頁（最後一列是溢出提示）", "現況", HDR[s], (OVERFLOW80, P["現況"][s][1]))
    var("現況：非 console 席（console 資料只標快照年齡）", "現況",
        "快照 2 分前（console.json）· seats --quota 40 秒前", P["現況"][s])
    var("待決：送出後 10 秒撤回倒數", "待決", HDR[s], HOLD80)
    var("待決：0 張卡", "待決", HDR[s], EMPTY_CARDS80, {})
    var("艦隊：fleet 資料過期（數字標 ✖；完整「門檻；原因」不截斷）", "艦隊", STALE_HDR, STALE80)
    o("\n### 額度：沒有 pace（Day-1 真實形狀，console quota.pools 為空）與同狀態的 band\n\n")
    o(block(band(s, BAND_L[s], "✖ personal 已用完", BADGES)))
    o("\n" + block(frame(s, "額度", NOPACE_HDR, NOPACE80[0], NOPACE80[1], BADGES)))
    var("現況：沒有 pace（和上面的 band 選同一池 personal）", "現況", NOPACE_HDR, (NOPACE_NOW80, P["現況"][s][1]))
    var("派工：沒有任何派工", "派工", HDR[s], EMPTY80["派工"])
    var("審查：沒有任何紀錄", "審查", HDR[s], EMPTY80["審查"])
    var("花費：本席額度過期", "花費", SPEND_STALE_HDR, SPEND_STALE80)
    var("殘留：沒有殘留", "殘留", HDR[s], EMPTY80["殘留"], {})


if __name__ == "__main__":
    main()
