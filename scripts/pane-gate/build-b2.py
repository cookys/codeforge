#!/usr/bin/env python3
"""build-b2.py <sha> <shots-label> [<nojump-label>] — assemble gate-b2.md (batch 2 full gate, carbon + astra) from the r3 packet head + gate-b2-insert.md + fresh shots in gate-shots/out/<label>."""
import collections, os, re, sys
S = os.path.dirname(os.path.abspath(__file__)); G = f"{S}/gate-shots"
SHA, LABEL = sys.argv[1], sys.argv[2]; NJ = sys.argv[3] if len(sys.argv) > 3 else None
from importlib.machinery import SourceFileLoader
nj = SourceFileLoader("nj", f"{S}/nojump-check.py").load_module()
r3 = open(f"{S}/gate-b1-r3.md", encoding="utf-8").read()
head = r3.split("## 第 2 輪結果與處置", 1)[0]
ROUND = int(os.environ.get("GATE_ROUND", "1"))  # 2nd/3rd round: the packet carries the previous round's dispositions (doc/design/ux-gate/pane-batch2/r<n-1>-dispositions.md)
head = head.replace("第 1 批 — UX 閘門第 3 輪（最後一輪）", f"第 2 批 — UX 閘門第 {ROUND} 輪" + ("（最後一輪）" if ROUND == 3 else "")).replace("第 3 輪起適用", "第 1 批第 3 輪起適用")
head = re.sub(r"受測版本是 autopilot feat/pane-batch1 @ \w+，base 是 \w+。", f"受測版本是 autopilot @ {SHA}，base 是第 1 批上線版 083cf2e91。", head)
head = head.replace("這一批交 1–4 頁。", "第 1 批（1–4 頁）已經上線；這一批交第 1 批評審發現的修正，範圍還是 1–4 頁和 band。")
assert SHA in head, "sha line not replaced"
insert = open(f"{S}/gate-b2-insert.md", encoding="utf-8").read()
O = f"{G}/out/{LABEL}"
order = [f"S1-full-p{p}-{sz}" for sz in ("160x40", "120x30", "80x24", "80x20", "80x16") for p in (1, 2, 3, 4)] + \
        [f"S1-full-p0-{sz}" for sz in ("160x40", "80x24")] + ["S1-closed-160x40"] + [f"S1-light-p{p}-160x40" for p in (1, 4)] + [f"S6-own-p2-{sz}" for sz in ("160x40", "80x24")] + [f"S4-small-p{p}-160x40" for p in (1, 4)] + \
        [f"S5-stale-10min-p{p}-{sz}" for sz in ("160x40", "120x30") for p in (1, 4)] + [f"S5-console-exit1-nofile-p{p}-160x40" for p in (1, 4)]
order = [k for k in order if os.path.exists(f"{O}/{k}.txt")]
missing = [k for k in ("S1-full-p1-80x24", "S1-full-p3-80x24", "S1-full-p4-80x24", "S1-full-p2-80x24", "S1-full-p1-80x16", "S1-light-p1-160x40", "S1-full-p0-160x40", "S1-closed-160x40", "S5-stale-10min-p1-120x30") if k not in order]
assert not missing, f"missing must-have 80x24 shots: {missing}"
parts = [head, insert]
if ROUND > 1:
    prev = f"{S}/../../doc/design/ux-gate/pane-batch2/r{ROUND - 1}-dispositions.md"
    parts.append(f"## 第 {ROUND - 1} 輪的發現與處置（這一輪請逐條判定：修好／部分／未修，未修的照原嚴重度再列一次）\n\n" + open(prev, encoding="utf-8").read().split("\n", 1)[1].replace("\n## ", "\n### "))
    parts.append("## 這一輪新增的截圖\n\n- S1-full-*-80x20、80x16：pane 只剩 2 列、1 列內容時，至少要露出最嚴重的一類（PM 01M7PMAN 說的「80x3」就是這個狀況）。\n- S1-light-*：Claude Code 淺色主題（白底）。" + ("\n- S6-own-p2-*：本席可答的卡（對照 S1-full-p2 的不可答卡）。" if any(k.startswith("S6-") for k in order) else "") + "\n- keyprobe 改看分頁列選中哪一頁（tab=），不再看 pane 第一列。"
                 + ("\n- S1-full-p0-*：鍛造頁整頁（K1）。\n- S1-closed-160x40：pane 關著時的 band，第 2 列應是唯一的共用分頁列（X1 後半、第 1 批 M6）。band 下面那列「tmux focus-events off …」是 Claude Code 對截圖用 tmux 的提示，不是 pane 或 band 畫的，請不要計分。\n- S5-stale-10min-*-120x30：120x30 的資料過期（X4／W5）。" if "S1-closed-160x40" in order else ""))
if os.environ.get("IMPL_NOTE"):  # the implementer's evidence for a disputed finding (r3: autopilot mods/live/docs/w1-host-row.md); reviewers judge it against the shots
    parts.append("## 實作方的說明（autopilot 提供，不是評審結論；請對照截圖自行判斷）\n\n" + re.sub(r"\[([^\]]+)\]\(\.\./[^)]*\)", r"`\1`", open(os.environ["IMPL_NOTE"], encoding="utf-8").read().split("\n", 1)[1].replace("\n## ", "\n### ")))  # its links point into the autopilot repo: keep the name, drop the target
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
parts.append("## keyprobe（B56 真機按鍵驗收）\n\n160x40 開 pane，縮到 80x24，依序按下列鍵；每鍵之後記錄三件事：分頁列選中哪一頁（tab，從分頁列顏色判斷）、pane 第一列（page，頁面捲動時會是「上面還有 N 行」，只供參考）、提示列內容（composer）。每一列最後的 ok 表示提示列是空的，而且分頁列選中的就是按下的那一鍵；BAD 表示其中一項不符。\n\n```\n" + open(kp, encoding="utf-8").read().strip() + "\n```")
out = "\n\n".join(parts) + "\n"
bad = re.findall(r"(?i)cookys|host-console|stranity|singular|host-infra|gpu-host-1|host-recovery", out)
open(f"{S}/gate-b2.md", "w", encoding="utf-8").write(out)
print(f"bytes={len(out.encode())} shots={len(order)} realnames={len(bad)}")
if len(out.encode()) > 165000: print("WARN: over agy prompt cap (~170KB); drop S1-light / S4 shots")
