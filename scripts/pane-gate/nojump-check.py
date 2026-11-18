#!/usr/bin/env python3
"""nojump-check.py <frames-dir> — r2 g2 folds A12 「畫面不跳動」.

Input: tmux captures (`tmux capture-pane -p`), one file per frame, sorted by name (1 fps × 30).
For each frame find:
  strip  = last row matching the tab strip (digits 1..4 in order, e.g. `1 現況 … 4 額度`)
  prompt = first row starting with '>' or '❯' (Claude Code prompt) — ignored if absent
  header = row above the body when the pane is expanded (a row containing the page title marker '│' or '──' right
           after the band) — detected as: the row index of the first line matching HEADER_RE; frames without one are
           band-collapsed and skipped for the header check.
Pass = strip index identical in all frames, prompt index identical in all frames where present,
       header index identical in all expanded frames. Exit 0 pass, 1 fail, 2 usage/no frames.
"""
import os
import re
import sys

STRIP_RE = re.compile(r"1\D{0,12}2\D{0,12}3\D{0,12}4")
HEADER_RE = re.compile(os.environ.get("NOJUMP_HEADER_RE", r"(^|│)\s*(現況|待決|艦隊|額度|派工|審查|花費|殘留|寵物|forge|說明)\s*($|[·｜|])"))


def scan(lines):
    strip = max((i for i, l in enumerate(lines) if STRIP_RE.search(l)), default=None)
    prompt = next((i for i, l in enumerate(lines) if l.lstrip().startswith((">", "❯"))), None)
    header = next((i for i, l in enumerate(lines) if HEADER_RE.search(l)), None)
    return strip, prompt, header


def main():
    if len(sys.argv) != 2 or not os.path.isdir(sys.argv[1]):
        print(__doc__)
        return 2
    d = sys.argv[1]
    names = sorted(n for n in os.listdir(d) if not n.startswith("."))
    if not names:
        print("no frames")
        return 2
    rows = []
    for n in names:
        with open(os.path.join(d, n), encoding="utf-8", errors="replace") as f:
            rows.append((n,) + scan(f.read().split("\n")))
    bad = []
    for label, col in (("strip", 1), ("prompt", 2), ("header", 3)):
        seen = {r[col] for r in rows if r[col] is not None}
        if label == "strip" and any(r[1] is None for r in rows):
            bad.append("strip missing in: " + ", ".join(r[0] for r in rows if r[1] is None))
        if len(seen) > 1:
            bad.append(f"{label} row moves: " + ", ".join(f"{r[0]}={r[col]}" for r in rows if r[col] is not None))
    print(f"frames={len(rows)} strip={sorted({r[1] for r in rows if r[1] is not None})} "
          f"prompt={sorted({r[2] for r in rows if r[2] is not None})} header={sorted({r[3] for r in rows if r[3] is not None})}")
    for b in bad:
        print("FAIL", b)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
