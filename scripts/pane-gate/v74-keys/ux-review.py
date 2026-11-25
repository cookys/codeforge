#!/usr/bin/env python3
"""ux-review.py <run.py-out-dir> <autopilot-sha> [--packet PATH] [--allow-red]  |  ux-review.py --verdict <reviewer.out>...

Builds the strict UX-director packet for the v7.4 pane (PM 01M7XARR, 01M7XBE2): the real key captures from a
run.py run, the per-key sequence, and the ux-v7.4 design (doc/design/pane-history/drafts/ux-v74/) to compare against.
Two heterogeneous engines score it (ux-review.sh); each must give overall >= 8/10 with no Critical.
Refuses a RED run (the order is: gate green -> UX review -> PM press -> operator use) unless --allow-red (dry run only).
Runs the same private-name check as build-b2.py before writing; a hit writes nothing and exits 1.
"""
import argparse, hashlib, json, os, re, sys

HERE = os.path.dirname(os.path.realpath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..', '..'))
DESIGN = os.path.join(ROOT, 'doc', 'design', 'pane-history', 'drafts', 'ux-v74')
CAP = 165000  # agy prompt cap is ~170 KB
PAGES = [('1', '現況', 'S2'), ('2', '待決', 'S3'), ('3', '派工', 'S4'), ('4', '審查', 'S5'), ('5', '花費', 'S6'),
         ('6', '殘留', 'S7'), ('7', '艦隊', 'S8'), ('8', '待辦', 'S9'), ('9', '說明', 'S10')]

HEAD = """# v7.4 autopilot-live pane — 嚴格 UX 總監審查

你是嚴格的 UX 總監。預設這個畫面有問題，直到截圖證明沒有。每一條發現都要引用「步驟名稱 + 列號」或「設計稿頁 + 列號」，
沒有引用的意見不算。不要評程式碼，只評操作員在終端機裡看到和按到的東西。

受測版本：autopilot @ {sha}。以下截圖全部來自 `run.py` 的實機互動驗收：私有 tmux、209x49、`claude --plugin-dir <checkout>`，
打 `/live-pane` 後一次按一個鍵，每按一鍵擷取一次畫面。資料全是假的（測試卡 d-keys0004 是驗收用的假卡）。驗收結果：{result}。

## 評分（每項 0–10，整數；總分不是平均，是你的整體判斷）

1. **好不好用**：每個動作要按幾個鍵、會不會按錯、按了有沒有回饋。動作清單與目前按鍵數：
   - 拿鍵盤（從輸入框交給 pane）：ctrl+x、Tab，2 鍵。
   - 換頁：1–9，1 鍵（輸入框空著時；輸入框有字時要先拿鍵盤，這是平台規定）。
   - 換張卡：↑ 或 ↓，1 鍵。
   - 選選項：a、b、c，1 鍵。
   - 附註：m，打字，Enter。
   - 送出全部：s，1 鍵。
   - 還給輸入框：Esc，1 鍵。
   判斷這些數字合不合理、操作員記不記得住、畫面上有沒有提示。
2. **資訊密度**：一屏看得到的有用資訊夠不夠；有沒有浪費的列或重複的資訊；重要的東西是否最先看到。
3. **與設計稿一致**：逐頁對照下面的設計稿（S2–S10 對應 1–9 頁，S1 是收起時的一行 band）。列出每一處差異（位置、文字、
   有無、順序）。資料不同（假資料筆數、時間）不算差異；版面、元件、文字標籤、按鍵提示不同才算。
4. **閱讀舒適度**（PM 01M7XBE2）：長文要換行且用懸掛縮排（標籤固定在左欄，換行後對齊內文起點）；內文欄約 70–90 欄，
   不能拉滿 209 欄；選項之間空一行；建議的選項（★）要用粗體或顏色突出，不能只靠星號；放得下就不能截斷，
   真放不下要在尾端標「↓ 還有 N 列」。

## 版面規則（PM 01M7XB0B，視為規格）

- 分頁列放在框的下框線右側，目前這頁反白；下框線左側是按鍵提示；框下面只剩一行 band。
- 寬度不夠時：提示先縮成最短（例如「↑↓ · a/b/c · s · Esc」），分頁再縮成數字、只有目前這頁顯示名字；永遠不換行、不和提示重疊。
- 收起時沒有框，只有 band 一行。

## 不計分

- band 下面、輸入框附近由 Claude Code 自己畫的通知列（Advisor、effort、tmux focus-events 的提示等）屬於平台，不是 band，不計分。
- 輸入框和最下面的 statusline 不是這次的範圍。

## 嚴重度

- Critical：操作員無法完成動作，或會按錯而送出錯的答案，或資訊錯誤會誤導決定。
- Major：能完成但明顯多按、看不懂、和設計稿或版面規則不符。
- Minor：細節、用字、對齊。

## 輸出格式

先寫逐項評論（中文），最後一定要附一個 ```json 區塊，格式如下，不能省略：

```json
{{"overall": 0, "axes": {{"usability": 0, "density": 0, "design_match": 0, "reading": 0}},
 "findings": [{{"sev": "Critical|Major|Minor", "where": "步驟名稱或設計稿頁 + 列號", "what": "看到什麼", "fix": "要怎麼改"}}],
 "verdict": "PASS|FAIL"}}
```

PASS 的條件：overall ≥ 8，而且沒有 Critical。

## 如何讀截圖

- 每張截圖是 `capture-pane -p` 的純文字，每行一列，只保留 pane 的框（╭ 起）到畫面底部；框上面的對話紀錄已省略。
- 為了控制長度，連續 8 個以上的空白寫成 ⟨N⟩（N 是空白數），欄位對齊照原樣計算。
- 顏色另附 SGR 摘要：每張截圖列出下框線上目前分頁的樣式，以及待決頁每個選項標籤的樣式（bold、rev、fg、bg）。
"""


def squeeze(line):
    return re.sub(r' {8,}', lambda m: '⟨%d⟩' % len(m.group(0)), line.rstrip())


def frame_part(text):
    lines = text.rstrip('\n').split('\n')
    top = next((i for i, l in enumerate(lines) if '╭─' in l), None)
    keep = lines[top:] if top is not None else lines[-8:]
    return '\n'.join(squeeze(l) for l in keep)


def load_run(out):
    rows = [json.loads(l) for l in open(os.path.join(out, 'results.jsonl'), encoding='utf-8') if l.strip()]
    return rows


def style_notes(r, ans_path, title):
    """Style of the current page in the strip and of each option label, read with run.py's own SGR parser."""
    if not os.path.exists(ans_path):
        return ''
    sc = r.Screen.from_ansi(open(ans_path, encoding='utf-8', errors='replace').read())
    notes = []
    bot = next((l for l in sc.ansi if '╰─' in r.ESC.sub('', l)), None)
    k = next((k for k, t, _ in PAGES if t == title), None)
    if bot is not None and k is not None:
        notes.append('分頁 %s %s：%s' % (k, title, r.sgr_at(bot, '%s %s' % (k, title)) or '無樣式'))
    for l in sc.ansi:
        p = r.ESC.sub('', l)
        m = re.search(r'(?:^|\s)([abc]): (\S)', p)
        if m and '│' in p:
            notes.append('選項 %s%s：%s' % (m.group(1), '（★）' if '★' in p else '', r.sgr_at(l, '%s: %s' % (m.group(1), m.group(2))) or '無樣式'))
    return '；'.join(notes)


def private_hits(text):
    S = os.path.join(HERE, '..')
    h = lambda t: hashlib.sha256(t.encode()).hexdigest()
    dl = [l.strip() for l in open(os.path.join(S, '..', 'private-denylist.sha256'), encoding='utf-8') if l.strip() and not l.startswith('#')]
    anyset = {e for e in dl if ':' not in e}
    exact = {e[6:] for e in dl if e.startswith('exact:')}
    home = {e[5:] for e in dl if e.startswith('home:')}
    mail = re.compile(r'[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})')
    n = 0
    for tok in re.split(r'[^A-Za-z0-9.-]+', text):
        t = tok.strip('.-').lower()
        if t and (h(t) in exact or any(h(p) in anyset for p in {t, *re.split(r'[-.]+', t)} if p)):
            n += 1
    n += sum(1 for m in re.findall(r'(?:/home/|-home-)([A-Za-z0-9_]+)', text) if h(m.lower()) in home)
    n += sum(1 for m in mail.finditer(text) if not re.search(r'(^|\.)(example\.(com|org|net)|invalid|test|example)(\.|$)', m.group(1).lower()))
    return n


def verdict(paths):
    """Read each reviewer's last json block; 0 = all PASS (overall >= 8, no Critical), 1 = a fail, 2 = no verdict."""
    rc = 0
    for p in paths:
        txt = open(p, encoding='utf-8', errors='replace').read() if os.path.exists(p) else ''
        blocks = re.findall(r'```json\s*(\{.*?\})\s*```', txt, re.S)
        if not blocks:  # dispatch-author's stdout is its receipt; the reviewer's text is in the receipt's raw_log
            try:
                raw = json.loads(txt.strip().splitlines()[-1]).get('raw_log')
            except (ValueError, IndexError, AttributeError):
                raw = None
            if isinstance(raw, str) and os.path.isfile(raw):
                blocks = re.findall(r'```json\s*(\{.*?\})\s*```', open(raw, encoding='utf-8', errors='replace').read(), re.S)
        try:
            v = json.loads(blocks[-1])
            overall = int(v['overall'])
            crit = [f for f in v.get('findings', []) if str(f.get('sev', '')).lower() == 'critical']
        except (IndexError, ValueError, KeyError, TypeError):
            print('%s: NO VERDICT (no parseable json block)' % os.path.basename(p))
            rc = max(rc, 2)
            continue
        ok = overall >= 8 and not crit
        print('%s: %s overall=%d axes=%s critical=%d findings=%d' % (os.path.basename(p), 'PASS' if ok else 'FAIL', overall,
              json.dumps(v.get('axes', {}), ensure_ascii=False), len(crit), len(v.get('findings', []))))
        rc = max(rc, 0 if ok else 1)
    print('UX REVIEW %s' % ('PASS' if rc == 0 else 'FAIL'))
    return rc


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--verdict':
        return verdict(sys.argv[2:])
    ap = argparse.ArgumentParser()
    ap.add_argument('out')
    ap.add_argument('sha')
    ap.add_argument('--packet', default=None)
    ap.add_argument('--allow-red', action='store_true')
    a = ap.parse_args()
    import importlib.util
    spec = importlib.util.spec_from_file_location('run', os.path.join(HERE, 'run.py'))
    r = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(r)

    rows = load_run(a.out)
    red = [x['step'] for x in rows if not x['ok']]
    if red and not a.allow_red:
        print('ux-review: the gate is RED (%d step(s): %s); UX review comes after a green run' % (len(red), ', '.join(red)), file=sys.stderr)
        return 1
    result = 'GREEN，%d/%d 步通過' % (len(rows) - len(red), len(rows)) if not red else 'RED（演練用），%d/%d 步通過' % (len(rows) - len(red), len(rows))
    parts = [HEAD.format(sha=a.sha, result=result)]

    seq = ['## 逐鍵序列（步驟、按的鍵、斷言結果）', '']
    for x in rows:
        seq.append('- %s：%s' % (x['step'], '；'.join(('✓ ' if c['ok'] else '✗ ') + c['check'] for c in x['checks'])))
    parts.append('\n'.join(seq))

    shots = ['## 實機截圖', '']
    pick = {x['step'] for x in rows}
    seen_frames = {}
    for x in rows:
        step = x['step']
        txt = os.path.join(a.out, step + '.txt')
        if not os.path.exists(txt):
            continue
        title = (x.get('seen') or {}).get('title') or ''
        st = style_notes(r, os.path.join(a.out, step + '.ans'), title)
        fr = frame_part(open(txt, encoding='utf-8').read())
        if fr in seen_frames:  # identical capture: point at the first one (keeps the packet under the agy cap)
            shots += ['### ' + step, '畫面與 %s 完全相同。' % seen_frames[fr], '']
            continue
        seen_frames[fr] = step
        shots += ['### ' + step, '樣式：' + (st or '—'), '```', fr, '```', '']
    parts.append('\n'.join(shots))

    des = ['## 設計稿 ux-v7.4（209x49；S1 收起，S2–S10 對應 1–9 頁）', '']
    for name in ['S1'] + [s for _, _, s in PAGES]:
        des += ['### 設計稿 %s' % name, '```', frame_part(open(os.path.join(DESIGN, 'design-%s.txt' % name), encoding='utf-8').read()), '```', '']
    des += ['### 設計稿：寬度不夠時的下框線（欄數 + 內容）', '```', open(os.path.join(DESIGN, 'design-narrow.txt'), encoding='utf-8').read().rstrip(), '```']
    parts.append('\n'.join(des))

    packet = '\n\n'.join(parts) + '\n'
    if private_hits(packet):
        print('ux-review: private name or email in the packet; NOT written', file=sys.stderr)
        return 1
    path = a.packet or os.path.join(a.out, 'ux-review.md')
    open(path, 'w', encoding='utf-8').write(packet)
    size = len(packet.encode())
    print('ux-review: packet %s bytes=%d steps=%d red=%d' % (path, size, len(pick), len(red)))
    if size > CAP:
        print('ux-review: WARN over the agy prompt cap (~170 KB)', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
