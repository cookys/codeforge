#!/usr/bin/env python3
"""Mockups for plan 2026-11-18-pane-tabs-r2: 7 pane pages + band, at 160x40 and 80x24.

Every frame is composed from the fixed-height layout of §3 and checked: exactly H rows, every row
exactly W cells. A frame that grows or shrinks by one row fails here, the same rule the golden
of §5 enforces in autopilot. Run: python3 doc/plans/2026-11-18-pane-tabs-r2.mockups.py > out.md
Each row ends with │ at column W+1 so the width is visible.
Role glyphs (colour is the CC theme key of plan 1fba336 §5): ✖ broken/error  ◈ overdue/error
▲ warning  ● running/suggestion  ⋯ waiting_on/subtle  ✓ ok/success  ○ idle/inactive  ★ recommended.
"""
import sys
import unicodedata

SIZES = {"160x40": (160, 40, 6), "80x24": (80, 24, 4)}  # W, H, reserved action rows
TABS = ["現況", "待決", "艦隊", "派工", "審查", "花費", "殘留"]


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


def strip(active, w, badges, pane=True):
    parts = []
    for i, t in enumerate(TABS, 1):
        b = badges.get(t, "")
        lab = f"{i} {t}{b}" if w >= 160 else f"{i}{t}{b}"
        parts.append(f"[{lab}]" if t == active else f" {lab} ")
    tail = ("  ? 說明  Esc 收起" if w >= 160 else " ? Esc") if pane else ""
    return "".join(parts) + tail


def frame(size, active, header, body, actions, badges):
    w, h, a = SIZES[size]
    body_rows = h - 2 - a  # header + strip + reserved action rows
    rows = [header]
    if len(body) > body_rows:
        rest = len(body) - body_rows + 1
        body = body[: body_rows - 1] + [f"  ↓ 還有 {rest} 列（PgDn）"]
    rows += body + [""] * (body_rows - len(body))
    if len(actions) > a:
        raise SystemExit(f"{active} {size}: {len(actions)} action rows > reserved {a}")
    rows += [""] * (a - len(actions)) + actions  # bottom-anchored, next to the strip
    rows.append(strip(active, w, badges))
    out = [fit(r, w) for r in rows]
    assert len(out) == h and all(cells(r) == w for r in out), (active, size)
    return out


def band(size, active, line2, badges):
    w = SIZES[size][0]
    out = [fit(strip(None, w, badges, pane=False), w), fit(line2, w)]
    assert len(out) == 2
    return out


BADGES = {"待決": "³", "艦隊": "✖", "殘留": "²"}
HDR = {
    "160x40": "autopilot · pm 席 · fleet 資料 12 秒前 · 額度 40 秒前 · 本席 codeforge--claude plan/shared-tui@926af0e",
    "80x24": "pm 席 · fleet 12 秒前 · 額度 40 秒前",
}
HDR_STALE = {
    "160x40": "autopilot · pm 席 · ✖ fleet 資料 6 分鐘未更新（超過 5 分鐘算過期；fleet console --json 逾時 5 秒） · ✖ 額度 無來源：seats --quota 失敗",
    "80x24": "✖ fleet 6 分未更新（逾時）· ✖ 額度無來源",
}
NONE = "  ○ 沒有要你處理的事"

P = {}  # page -> size -> (body, actions)
P["現況"] = {
    "160x40": (
        ["── 全艦隊 ──────────────────────────────────────────────",
         "  產線 51 ｜ ● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ｜ 主機 17/18 在線 ｜ 額度 最緊 stranity-personal 7 天 75%（週四 08:00 重置）",
         "  ✖ capacity--claude 卡住 2 小時：等待沒寫等誰（wait_untyped）                                         2 時前",
         "  ◈ hangar--claude 01M7N2ZC 證據逾期 40 分                                                               40 分前",
         "  ▲ 主機 h6 離線 12 分                                                                                  12 分前",
         "",
         "── 本席 codeforge--claude ───────────────────────────────",
         "  位置   plan/shared-tui @926af0e · 階段 plan · ctx 41%",
         "  任務   01M7N307 ⋯ 等待：r2 草稿 03:00 · 01M7N2TZ ⋯ 等 autopilot 01M7N2Z5",
         "  背景   ● 盤點子代理 4 分 · ● plan review astra 第 1 代",
         "  派工   ● 1 執行中（sonnet hand feat/console-source 23 分）",
         "  審查   plan 第 1 代 2/2 席回來 · 推送前檢查 ✓",
         "  花費   本席 5 小時 2% · 7 天 75% · 今天 $3.10",
         "  殘留   2 項可清",
         "",
         "── 最近對話 ─────────────────────────────────────────────",
         "  PM 01M7N33J 三條硬規定：開面板不另起一行、高度固定、過期標紅                                          15 分前",
         "  autopilot 01M7N30R 已開工 feat/console-source                                                       18 分前"],
        ["  → 待決 3 張卡等你（最舊 40 分）                                                                 [2]",
         "  → 殘留 2 項可清                                                                                 [7]"],
    ),
    "80x24": (
        ["產線 51 ●6 ⋯40 ✖5 · 主機 17/18 · 額度 75%",
         "✖ capacity 卡住 2 時：等待沒寫等誰",
         "◈ hangar 01M7N2ZC 證據逾 40 分",
         "▲ 主機 h6 離線 12 分",
         "── 本席 codeforge ──",
         "plan/shared-tui@926af0e · plan · ctx 41%",
         "01M7N307 ⋯ r2 草稿 03:00",
         "派工 ●1 · 審查 2/2 · 花費 7天 75%",
         "── 最近 ──",
         "PM 01M7N33J 三條硬規定 15 分前",
         "autopilot 01M7N30R 已開工 18 分前",
         "（第 3 則以後收在 PgDn）",
         "",
         "",
         ""],
        ["→ 待決 3 張（最舊 40 分）     [2]", "→ 殘留 2 項               [7]"],
    ),
}
P["待決"] = {
    "160x40": (
        ["▸ d-89fe394f 1/3  feature/legacy-payment-rewrite 和 feature/social-login 依序 --no-ff 合進 legacy？     執行者 game-lead · 40 分",
         "  背景   兩支分支都過審，合併順序影響 migration 編號",
         "  卡住   game-lead 的 row 不能自己合 main 以外的 legacy 分支",
         "  建議   可以：先 payment 再 social，CI 綠才合第二支",
         "  補充   （按 n 寫補充，會跟答案一起送）",
         "",
         "── 其他待決 ─────────────────────────────────────────────",
         "  d-fb890f48  [測試卡] carddispatch 真 courier 紅綠，請勿在面板作答                       PM · 2 時",
         "  d-fd51ee8d  下次到 308 現場時量 30 格（要你到場，只能回報）                             308 · 1 天",
         "── 別台的卡（唯讀，到那一席答）────────────────────────────",
         "  d-1c0a77e2  h6 席：GPU slot 重排要你點頭 → 到 cookys/h6/game-lead--claude 答",
         "── 要你做或看 ──────────────────────────────────────────",
         "  步驟  01M7N2TZ 上線後看 pane 截圖"],
        ["  選項  [a] 可以 ★   [b] 先不要   [n] 補充…",
         "  已選  a 可以",
         "  [Enter] 送出（10 秒內可撤回，u 撤回）   []] 下一張   [[] 上一張   [f] 回報填入提示列（不送出）"],
    ),
    "80x24": (
        ["▸ d-89fe394f 1/3 · game-lead · 40 分",
         "payment 和 social-login 依序 --no-ff 合進 legacy？",
         "背景 兩支都過審，順序影響 migration",
         "卡住 game-lead 不能自己合 legacy",
         "建議 可以：先 payment 再 social",
         "── 其他 ──",
         "d-fb890f48 [測試卡] 請勿作答 · 2時",
         "d-fd51ee8d 308 現場量 30 格 · 1天",
         "── 別台（唯讀）──",
         "d-1c0a77e2 h6：GPU 重排 → 到該席答",
         "── 要你做或看 ──",
         "01M7N2TZ 上線後看截圖",
         "",
         "",
         ""],
        ["[a]可以★ [b]先不要 [n]補充", "已選 a", "[Enter]送出(10秒內u撤回) []]下張 [[]上張"],
    ),
}
LANES160 = [
    "  產線                              狀態     負責            任務 / 證據                                     上次",
    "  ✖ capacity · h1          卡住     PM              等待沒寫等誰（wait_untyped）                    2 時",
    "  ✖ hangar · h2                  卡住     PM              01M7N2ZC 證據逾期 40 分                          40 分",
    "  ● autopilot · h1          執行     PM              01M7N2Z5 feat/console-source                    1 分",
    "  ● codeforge · h1          執行     PM              01M7N307 r2 草稿                                0 分",
    "  ● fleet · h2                   執行     PM              console.json 快照 04:00                         3 分",
    "  ⋯ 308 · h1                等待     game-lead       等操作員到現場                                   1 天",
    "  ⋯ alfred · h1             等待     PM              waiting_on task:01M7…                           4 時",
    "  ○ 其他 36 條等待中（Enter 展開）",
]
HOSTS160 = [
    "── 主機 17/18 在線 ─────────────────────────────────────",
    "  ● h1 6 席   ● h2 4 席   ● h3 GPU 0/2   ● h4 2 席   ● h5 1 席   ✖ h6 離線 12 分   … 另 12 台 ●",
]
P["艦隊"] = {
    "160x40": (
        ["── 產線 51：● 執行 6  ⋯ 等待 40  ✖ 卡住 5 ─────────────────"] + LANES160 + [""] + HOSTS160,
        ["  ◈ hangar 01M7N2ZC 證據逾期 40 分     [Enter] 看  [f] 催辦填入提示列（不送出）",
         "  ✖ capacity 等待沒寫等誰               [Enter] 看  [f] 填入提示列"],
    ),
    "80x24": (
        ["產線 51 ●6 ⋯40 ✖5",
         "✖ capacity  卡住 等待沒寫等誰   2時",
         "✖ hangar    卡住 證據逾期      40分",
         "● autopilot 執行 console-source 1分",
         "● codeforge 執行 r2 草稿        0分",
         "● fleet     執行 console.json  3分",
         "⋯ 308       等待 操作員到場     1天",
         "○ 其他 37 條（Enter 展開）",
         "── 主機 17/18 ──",
         "●h1 ●h2 ●h3 ●h4",
         "✖ h6 離線 12 分 · 另 12 台 ●",
         "", "", "", ""],
        ["◈ hangar 逾 40 分 [Enter][f]", "✖ capacity 未寫等誰 [Enter][f]"],
    ),
}
P["派工"] = {
    "160x40": (
        ["── 執行中 1 ──────────────────────────────────────────────",
         "  run                         角色        引擎             經過     上次",
         "  ● console-source            hand        sonnet           23 分    1 分前",
         "── foreman ──",
         "  ○ 無",
         "── 已完成（最近 5）──────────────────────────────────────",
         "  ✓ plan-review astra g1      reviewer    codex/astra      18 分    5 時前",
         "  ✓ plan-review carbon g1     reviewer    agy/carbon       21 分    5 時前",
         "  ✓ inventory                 explore     sonnet           1 分     4 分前"],
        [NONE],
    ),
    "80x24": (
        ["── 執行中 1 ──",
         "● console-source hand sonnet 23分",
         "── 已完成 ──",
         "✓ astra g1 reviewer 18分 5時前",
         "✓ carbon g1 reviewer 21分 5時前",
         "✓ inventory explore 1分 4分前"],
        [NONE],
    ),
}
P["審查"] = {
    "160x40": (
        ["── 程式碼審查 ─────────────────────────────────────────",
         "  第 1 代 f55289ca · google SHIP-AS-IS · xai SHIP-AS-IS                                  ▲ 135 時前，HEAD 已不同",
         "── 推送前檢查 ──",
         "  ✓ check-all 綠 · 627114b",
         "── 替你決定的事（最新在上）───────────────────────────",
         "  01M7N33J 開面板走分頁列，不另起一行                                                         15 分前",
         "  01M7N2YJ 退回讀 console.json 快照，不轉 brief                                               1 時前",
         "── plan review ──",
         "  plan/shared-tui 第 2 代 · astra 8 · carbon 8 · 已凍結 1fba336"],
        [NONE],
    ),
    "80x24": (
        ["── 程式碼 ──",
         "g1 f55289ca google✓ xai✓ ▲HEAD 不同",
         "── 推送前 ── ✓ 627114b",
         "── 替你決定 ──",
         "01M7N33J 開面板走分頁列 15分",
         "01M7N2YJ 退回讀快照 1時",
         "── plan ── 1fba336 凍結 8/8"],
        [NONE],
    ),
}
P["花費"] = {
    "160x40": (
        ["── 本席上限（seats --quota，codeforge--claude 那一列）────",
         "  5 小時  ██░░░░░░░░░░░░░░░░░░   2%                     重置 03:40",
         "  7 天    ███████████████░░░░░  75%                     重置 週四 08:00",
         "  今天    $3.10 · 近 24 小時 ▁▁▂▃▅▂▁▁▁▂▂▃▁▁▁▁▂▅▇▃▂▁▁▁",
         "",
         "── 額度池（原「額度」頁）──────────────────────────────",
         "  池                    席數   5 小時    7 天     重置             資料",
         "  ▲ stranity-personal    5      2%        75%      週四 08:00        6 秒前",
         "  ● stranity-team        3      17%       68%      週六 12:00        6 秒前",
         "  ● twgame               2      —         49%      週四 09:00        6 秒前",
         "  ✖ trinity              1      —         —        —                 ✖ 3 時未更新（statusline 沒在跑）",
         "  ○ 其他 4 池（Enter 展開）"],
        [NONE],
    ),
    "80x24": (
        ["── 本席 ──",
         "5時 ██░░░░░░░░  2% 重置 03:40",
         "7天 ████████░░ 75% 重置 週四",
         "今天 $3.10 ▁▂▃▅▂▁▂▅▇▃",
         "── 額度池 ──",
         "▲ stranity-personal 5席 75%",
         "● stranity-team     3席 68%",
         "● twgame            2席 49%",
         "✖ trinity 3 時未更新",
         "○ 其他 4 池"],
        [NONE],
    ),
}
P["殘留"] = {
    "160x40": (
        ["── 要你處理 ─────────────────────────────────────────────",
         "  ▲ worktree hetero-feat-fs-pane-v2-c1mhR9：分支已併入，可移除          git worktree remove …",
         "── 可清除 2 ──",
         "  ○ /dev/shm/petshot-1001 寵物截圖沙盒（重拍後）                          [Enter] 清這項",
         "  ○ target/flaky-tests.log 7 天前                                       [Enter] 清這項",
         "── 上次自動清理 ── 3 時前 · 4 項",
         "── 外掛來源 ── ✓ autopilot 從主 clone develop 載入"],
        ["  [c] 全部清除 2 項（原本在最上面一列，搬到這裡）"],
    ),
    "80x24": (
        ["── 要你處理 ──",
         "▲ worktree c1mhR9 已併入，可移除",
         "── 可清除 2 ──",
         "○ petshot-1001 沙盒   [Enter]",
         "○ flaky-tests.log    [Enter]",
         "── 上次自動清理 3 時前",
         "── 外掛 ✓ develop"],
        ["[c] 全部清除 2 項"],
    ),
}


def main():
    out = sys.stdout
    for size in SIZES:
        w, h, a = SIZES[size]
        out.write(f"\n## {size}（body {h - 2 - a} 列 + 預留動作 {a} 列 + 頁首 1 + 分頁列 1 = {h}）\n")
        out.write("\n### band（主畫面上方，永遠 2 列；按分頁鍵開 pane，沒有「開面板」那一列）\n\n```\n")
        out.write("\n".join(r + "│" for r in band(size, "現況", "  ✖ capacity 卡住 2 時 · 待決 3 張（最舊 40 分）" if w >= 160 else "✖capacity 卡住 · 待決3", BADGES)) + "\n```\n")
        for page in TABS:
            body, acts = P[page][size]
            out.write(f"\n### {page}\n\n```\n" + "\n".join(r + "│" for r in frame(size, page, HDR[size], body, acts, BADGES)) + "\n```\n")
        out.write("\n### 狀態變化：艦隊，資料過期＋額度無來源（列數不變）\n\n```\n")
        body, acts = P["艦隊"][size]
        out.write("\n".join(r + "│" for r in frame(size, "艦隊", HDR_STALE[size], body, acts, BADGES)) + "\n```\n")
        out.write("\n### 狀態變化：待決，0 張卡（列數不變）\n\n```\n")
        out.write("\n".join(r + "│" for r in frame(size, "待決", HDR[size], ["  ○ 目前沒有待決卡"], [NONE], {})) + "\n```\n")


if __name__ == "__main__":
    main()
