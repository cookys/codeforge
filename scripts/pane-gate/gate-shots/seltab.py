#!/usr/bin/env python3
"""seltab.py < capture-pane -e output: print the selected tab of the shared tab strip ("3:艦"), or "?" when none stands out.

The strip is the last line that carries both "1: " and "2: " (at 120 columns it ends at 6). Each tab is "<digit>: <label>"; the selected one is the
label whose foreground colour differs from the majority of labels, so the check does not depend on the theme.
keyprobe uses it instead of the pane's first row, which a scrolled page fills with "上面還有 N 行" (gate b2 astra M03).
"""
import re
import sys
from collections import Counter

SGR = re.compile(r"\x1b\[([0-9;]*)m")
TAB = re.compile(r"([0-9]): (\S)")


def styled(line):
    """[(char, sgr-state)] for a line; the state is the tuple of SGR params in force since the last reset."""
    out, state, i = [], [], 0
    while i < len(line):
        m = SGR.match(line, i)
        if m:
            p = m.group(1)
            if p in ("", "0"):
                state = []
            elif p in ("39", "49", "27"):
                state = [s for s in state if not (p == "39" and s.startswith("38")) and not (p == "49" and s.startswith("48")) and not (p == "27" and s == "7")]
            else:
                state = [s for s in state if not (p.startswith("38") and s.startswith("38")) and not (p.startswith("48") and s.startswith("48"))] + [p]
            i = m.end()
            continue
        out.append((line[i], tuple(sorted(state))))
        i += 1
    return out


def main():
    plain_strip = None
    for line in sys.stdin.read().splitlines():
        plain = SGR.sub("", line)
        if "1: " in plain and "2: " in plain:
            plain_strip = line
    if plain_strip is None:
        print("?")
        return
    cells = styled(plain_strip)
    text = "".join(c for c, _ in cells)
    # only the label's foreground counts: inverse (7) marks the hovered/focused Button and moves with the mouse, the background is the strip's
    tabs = [(m.group(1), m.group(2), tuple(s for s in cells[m.start(2)][1] if s.startswith("38"))) for m in TAB.finditer(text)]
    common = Counter(s for _, _, s in tabs).most_common(1)[0][0] if tabs else None
    odd = [f"{d}:{lab}" for d, lab, s in tabs if s != common]
    print(odd[0] if len(odd) == 1 else "?" if not odd else "?" + ",".join(odd))


if __name__ == "__main__":
    main()
