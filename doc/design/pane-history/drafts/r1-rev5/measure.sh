#!/usr/bin/env bash
# 用真 Chromium 量全部步驟；失敗就回傳非零，不填造量測資料。
set -euo pipefail
R1_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export R1_DIR
export R1_CHROME="${R1_CHROME:-/home/team/.cache/ms-playwright/chromium_headless_shell-1248/chrome-headless-shell-linux64/chrome-headless-shell}"
python3 - <<'PY'
import html
import json
import os
from pathlib import Path
import re
import subprocess
import sys

root = Path(os.environ['R1_DIR'])
chrome = Path(os.environ['R1_CHROME'])
if not chrome.is_file():
    sys.exit('Chromium 不存在；停止，不產生量測或假截圖。')
page = (root / 'r1-rev5-interactive.html').as_uri()
steps_dir = root / 'steps'
steps_dir.mkdir(exist_ok=True)
base = [str(chrome), '--no-sandbox', '--disable-gpu', '--hide-scrollbars',
        '--run-all-compositor-stages-before-draw', '--virtual-time-budget=1500']
errors = []
figures = 0
screenshots = 0
receipts = 0
out = root / 'pixel-widths.txt'


def run(args):
    p = subprocess.run(base + args, text=True, capture_output=True, timeout=45)
    if p.returncode:
        raise RuntimeError(f'Chromium 退出碼 {p.returncode}：{p.stderr[-2000:]}')
    return p.stdout


def extract(dom, element):
    m = re.search(r'<pre id="' + element + r'">(.*?)</pre>', dom, re.S)
    if not m:
        raise RuntimeError(f'找不到 {element}，停止；不假造資料。')
    return json.loads(html.unescape(m[1]))


with out.open('w') as log:
    def emit(text):
        log.write(text + '\n')
        log.flush()

    emit('R1 rev5 真 Chromium 像素量測；每個值來自 getBoundingClientRect().width。')
    emit('尺寸 × 8 步 × 雙主題；每組同時量 new 與 live；每列寬度單位 px。')
    emit('命令：chrome-headless-shell --no-sandbox --disable-gpu --dump-dom file://…?measure=1&step=…&size=…&theme=…&view=both')
    emit('截圖命令：同一瀏覽器 --screenshot=steps/<步驟>-<尺寸>-<主題>.png --window-size=<寬>,<高>')
    try:
        for size in ('120x40', '200x50', '80x24'):
            cols, rows = map(int, size.split('x'))
            for theme in ('dark', 'light'):
                for step in range(1, 9):
                    url = page + f'?measure=1&step={step}&size={size}&theme={theme}&view=both'
                    if step == 1:
                        url += '&selftest=1'
                    dom = run(['--dump-dom', url])
                    receipt = extract(dom, 'measure-out')
                    if receipt['step'] != step or receipt['size'] != size or receipt['theme'] != theme:
                        raise RuntimeError('量測參數與回覆不一致')
                    if receipt['r1']:
                        errors.append(f'{size}/{theme}/{step} 上半部有熱區')
                    if {f['view'] for f in receipt['figures']} != {'new', 'live'}:
                        raise RuntimeError('沒有同時量到 new／live')
                    for f in receipt['figures']:
                        figures += 1
                        emit(json.dumps(dict(step=step, size=size, theme=theme, **f), ensure_ascii=False))
                        if f['rows'] != rows or f['spread'] > .01 or not f['presetEqual']:
                            errors.append(f'{size}/{theme}/{step}/{f["view"]} 寬度失敗')
                    if step == 1:
                        tests = extract(dom, 'browser-tests')
                        receipts += len(tests)
                        emit('瀏覽器互動：' + json.dumps(dict(size=size, theme=theme, tests=tests), ensure_ascii=False))
                        if not tests or any(not t['pass'] for t in tests):
                            errors.append(f'{size}/{theme} 互動失敗')
                    dest = steps_dir / f'{step}-{size}-{theme}.png'
                    # 截圖保留新／現行並排、說明、pane、band 與 prompt 全高。
                    window = f'{max(1500, int(cols * 7.21875 * 2 + 100))},{int(rows * 14.4 + 650)}'
                    run([f'--screenshot={dest}', f'--window-size={window}', url])
                    if not dest.is_file() or dest.read_bytes()[:8] != b'\x89PNG\r\n\x1a\n':
                        raise RuntimeError(f'截圖無效：{dest.name}')
                    screenshots += 1
                    emit(f'截圖 steps/{dest.name}；視窗 {window}；Chromium 退出碼 0')
                    print(f'{size} {theme} 步驟 {step}：已量測、已截圖', flush=True)
                # 反向探針仍取同一份實拍、同一字型；只移除格子包裝。
                url = page + f'?measure=1&nogrid=1&step=6&size={size}&theme={theme}&view=both'
                receipt = extract(run(['--dump-dom', url]), 'measure-out')
                emit('nogrid 反向探針：' + json.dumps(receipt, ensure_ascii=False))
                if not receipt['nogrid'] or any(f['spread'] <= 0 for f in receipt['figures']):
                    errors.append(f'{size}/{theme} nogrid 反向探針未報錯')
        emit(f'總結：{figures} 張圖、{screenshots} 張截圖、{receipts} 項瀏覽器互動檢查、6 組 nogrid 反向探針。')
        emit('退出碼：' + ('1' if errors else '0'))
        for error in errors:
            emit('失敗：' + error)
    except (RuntimeError, subprocess.TimeoutExpired) as error:
        emit('中止：' + str(error))
        emit('退出碼：1；保留已完成的真實結果，其餘未量。')
        raise
sys.exit(1 if errors else 0)
PY
