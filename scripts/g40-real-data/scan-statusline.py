#!/usr/bin/env python3
"""Scan statusline captures for scripts/g40-real-data/check.sh.

Usage: scan-statusline.py <out_dir> <width>...

Each `statusline-W<width>.ansi` must draw only lines no wider than <width> terminal cells, never an
`acct-` alias and never money (`$`). Prints one line per problem; exit 1 if any.
"""
import re
import sys
import unicodedata

ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


def cells(s):
    n = 0
    for ch in s:
        if unicodedata.combining(ch):
            continue
        n += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
    return n


def main():
    out, widths = sys.argv[1], [int(w) for w in sys.argv[2:]]
    problems = []
    for w in widths:
        with open(f"{out}/statusline-W{w}.ansi", encoding="utf-8") as f:
            lines = ANSI.sub("", f.read()).splitlines()
        if not any(l.strip() for l in lines):
            problems.append(f"W{w}: drew nothing")
        for i, line in enumerate(lines, 1):
            if cells(line) > w:
                problems.append(f"W{w} line {i}: {cells(line)} cells > {w}")
            if "acct-" in line:
                problems.append(f"W{w} line {i}: shows an acct- alias")
            if "$" in line:
                problems.append(f"W{w} line {i}: shows money")
    for p in problems:
        print("g40-real-data: statusline " + p)
    if problems:
        sys.exit(1)
    print(f"g40-real-data: statusline ok at {', '.join(map(str, widths))}")


if __name__ == "__main__":
    main()
