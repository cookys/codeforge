#!/usr/bin/env python3
"""build-b2.py <sha> <shots-label> [<nojump-label>] — assemble gate-b2.md (batch 2 full gate, carbon + astra) from the r3 packet head + gate-b2-insert.md + fresh shots in gate-shots/out/<label>."""
import collections, os, re, sys
S = os.path.dirname(os.path.abspath(__file__)); G = f"{S}/gate-shots"
SHA, LABEL = sys.argv[1], sys.argv[2]; NJ = sys.argv[3] if len(sys.argv) > 3 else None
from importlib.machinery import SourceFileLoader
nj = SourceFileLoader("nj", f"{S}/nojump-check.py").load_module()
r3 = open(f"{S}/gate-b1-r3.md", encoding="utf-8").read()
head = r3.split("## 第 2 輪結果與處置", 1)[0]
head = head.replace("第 1 批 — UX 閘門第 3 輪（最後一輪）", "第 2 批 — UX 閘門第 1 輪").replace("第 3 輪起適用", "第 1 批第 3 輪起適用")
head = re.sub(r"受測版本是 autopilot feat/pane-batch1 @ \w+，base 是 \w+。", f"受測版本是 autopilot @ {SHA}，base 是第 1 批上線版 083cf2e91。", head)
head = head.replace("這一批交 1–4 頁。", "第 1 批（1–4 頁）已經上線；這一批交第 1 批評審發現的修正，範圍還是 1–4 頁和 band。")
assert SHA in head, "sha line not replaced"
insert = open(f"{S}/gate-b2-insert.md", encoding="utf-8").read()
O = f"{G}/out/{LABEL}"
order = [f"S1-full-p{p}-{sz}" for sz in ("160x40", "120x30", "80x24") for p in (1, 2, 3, 4)] + \
        [f"S1-light-p{p}-160x40" for p in (1, 4)] + [f"S4-small-p{p}-160x40" for p in (1, 4)] + \
        [f"S5-stale-10min-p{p}-160x40" for p in (1, 4)] + [f"S5-console-exit1-nofile-p{p}-160x40" for p in (1, 4)]
order = [k for k in order if os.path.exists(f"{O}/{k}.txt")]
missing = [k for k in ("S1-full-p1-80x24", "S1-full-p3-80x24", "S1-full-p4-80x24", "S1-full-p2-80x24") if k not in order]
assert not missing, f"missing must-have 80x24 shots: {missing}"
parts = [head, insert]
if NJ:
    ND = f"{G}/out/{NJ}"; rows = []
    for n in sorted(x for x in os.listdir(ND) if x.endswith(".txt")):
        lines = open(f"{ND}/{n}", encoding="utf-8", errors="replace").read().split("\n")
        strip, prompt, _ = nj.scan(lines)
        rows.append(f"{n[:-4]}: 提示列={None if prompt is None else prompt + 1} 分頁列={None if strip is None else strip + 1}")
    parts.append("## 畫面不跳動：連拍結果（列號從 1 起算）\n" + "\n".join(rows))
else:
    parts.append("## 畫面不跳動\n這一批沒有重拍 30 格連拍；請只根據下面的截圖判斷有沒有殘字、重複的分頁列或中間的空洞。")
colors = ["## 顏色（ANSI SGR 統計，前 12 種）"]
for k in ("S1-full-p4-160x40", "S1-light-p4-160x40"):
    if os.path.exists(f"{O}/{k}.ans"):
        a = open(f"{O}/{k}.ans", encoding="utf-8", errors="replace").read()
        c = collections.Counter(m for seq in re.findall(r"\x1b\[([0-9;]*)m", a) for m in re.findall(r"(?:38;5;\d+|48;5;\d+|\d+)", seq))
        colors.append(f"- {k}: " + ", ".join(f"{s}×{n}" for s, n in c.most_common(12)))
parts.append("\n".join(colors))
shots = ["## 截圖（capture-pane -p 純文字；每行就是一列）"]
for k in order:
    shots += ["", f"### {k}", "```", open(f"{O}/{k}.txt", encoding="utf-8").read().rstrip("\n"), "```"]
parts.append("\n".join(shots))
kp = f"{S}/keyprobe-{sys.argv[2]}.log"
assert os.path.exists(kp), f"missing B56 keyprobe log: {kp}"
parts.append("## keyprobe（B56 真機按鍵驗收）\n\n160x40 開 pane，縮到 80x24，依序按下列鍵；每鍵之後記頁首和提示列內容。pass = 每一鍵頁首換成對應分頁、提示列是空的。\n\n```\n" + open(kp, encoding="utf-8").read().strip() + "\n```")
out = "\n\n".join(parts) + "\n"
bad = re.findall(r"(?i)cookys|host-console|stranity|singular|host-infra|gpu-host-1|host-recovery", out)
open(f"{S}/gate-b2.md", "w", encoding="utf-8").write(out)
print(f"bytes={len(out.encode())} shots={len(order)} realnames={len(bad)}")
if len(out.encode()) > 165000: print("WARN: over agy prompt cap (~170KB); drop S1-light / S4 shots")
