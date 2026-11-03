#!/usr/bin/env python3
"""ansi2html.py — turn a terminal capture with ANSI colours into one self-contained HTML page.

For owner review of TUI changes (doc/specs/tui-guideline.md §9): the operator's viewer cannot
scroll horizontally, so captures are <=100 columns and come with a coloured HTML preview.

    tmux capture-pane -p -e -t <target> > capture.ansi
    python3 scripts/ansi2html.py capture.ansi > capture.html          # dark background
    python3 scripts/ansi2html.py --light capture.ansi > capture.html  # light background

Understands SGR: reset, bold (1), dim (2), italic (3), underline (4), inverse (7), their resets
(22/23/24/27), the 16 base colours (30-37, 90-97 / 40-47, 100-107), 256-colour (38;5;n / 48;5;n),
truecolour (38;2;r;g;b / 48;2;r;g;b) and default (39/49). Other escape sequences are dropped.
Standard library only.
"""
import argparse
import html
import re
import sys

BASE16 = [
    '#000000', '#cd3131', '#0dbc79', '#e5e510', '#2472c8', '#bc3fbc', '#11a8cd', '#e5e5e5',
    '#666666', '#f14c4c', '#23d18b', '#f5f543', '#3b8eea', '#d670d6', '#29b8db', '#ffffff',
]


def xterm256(n):
    if n < 16:
        return BASE16[n]
    if n < 232:
        n -= 16
        steps = [0, 95, 135, 175, 215, 255]
        return '#%02x%02x%02x' % (steps[n // 36], steps[(n // 6) % 6], steps[n % 6])
    g = 8 + (n - 232) * 10
    return '#%02x%02x%02x' % (g, g, g)


CSI = re.compile(r'\x1b\[([0-9;:?]*)([A-Za-z])')
OTHER_ESC = re.compile(r'\x1b(?:\][^\x07\x1b]*(?:\x07|\x1b\\)|[()][0-9A-Za-z]|[=>78DEHMNOc])')


class Style:
    def __init__(self):
        self.reset()

    def reset(self):
        self.fg = self.bg = None
        self.bold = self.dim = self.italic = self.underline = self.inverse = False

    def apply(self, params):
        codes = [int(p) if p.isdigit() else 0 for p in re.split('[;:]', params)] if params else [0]
        i = 0
        while i < len(codes):
            c = codes[i]
            if c == 0:
                self.reset()
            elif c == 1:
                self.bold = True
            elif c == 2:
                self.dim = True
            elif c == 3:
                self.italic = True
            elif c == 4:
                self.underline = True
            elif c == 7:
                self.inverse = True
            elif c == 22:
                self.bold = self.dim = False
            elif c == 23:
                self.italic = False
            elif c == 24:
                self.underline = False
            elif c == 27:
                self.inverse = False
            elif 30 <= c <= 37:
                self.fg = BASE16[c - 30]
            elif 90 <= c <= 97:
                self.fg = BASE16[c - 90 + 8]
            elif 40 <= c <= 47:
                self.bg = BASE16[c - 40]
            elif 100 <= c <= 107:
                self.bg = BASE16[c - 100 + 8]
            elif c == 39:
                self.fg = None
            elif c == 49:
                self.bg = None
            elif c in (38, 48) and i + 1 < len(codes):
                if codes[i + 1] == 5 and i + 2 < len(codes):
                    col = xterm256(codes[i + 2])
                    i += 2
                elif codes[i + 1] == 2 and i + 4 < len(codes):
                    col = '#%02x%02x%02x' % tuple(max(0, min(255, v)) for v in codes[i + 2:i + 5])
                    i += 4
                else:
                    col = None
                if c == 38:
                    self.fg = col
                else:
                    self.bg = col
            i += 1

    def css(self, default_fg, default_bg):
        fg, bg = self.fg, self.bg
        if self.inverse:
            fg, bg = (bg or default_bg), (fg or default_fg)
        out = []
        if fg:
            out.append('color:' + fg)
        if bg:
            out.append('background:' + bg)
        if self.bold:
            out.append('font-weight:bold')
        if self.dim:
            out.append('opacity:.6')
        if self.italic:
            out.append('font-style:italic')
        if self.underline:
            out.append('text-decoration:underline')
        return ';'.join(out)


def convert(text, light=False, title='capture'):
    default_fg, default_bg = ('#1e1e1e', '#ffffff') if light else ('#d4d4d4', '#1e1e1e')
    text = OTHER_ESC.sub('', text)
    style = Style()
    parts = []
    pos = 0
    open_css = None

    def emit(chunk):
        nonlocal open_css
        if not chunk:
            return
        css = style.css(default_fg, default_bg)
        if css != open_css:
            if open_css:
                parts.append('</span>')
            if css:
                parts.append('<span style="%s">' % css)
            open_css = css or None
        parts.append(html.escape(chunk))

    for m in CSI.finditer(text):
        emit(text[pos:m.start()])
        if m.group(2) == 'm':
            style.apply(m.group(1))
        pos = m.end()
    emit(text[pos:])
    if open_css:
        parts.append('</span>')
    body = ''.join(parts).rstrip('\n')
    return (
        '<!doctype html>\n<html><head><meta charset="utf-8"><title>%s</title>\n'
        '<style>body{margin:0;background:%s}pre{margin:0;padding:12px;color:%s;background:%s;'
        'font:13px/1.25 "DejaVu Sans Mono","Noto Sans Mono CJK TC",Menlo,Consolas,monospace;'
        'white-space:pre;overflow-x:auto}</style></head>\n<body><pre>%s</pre></body></html>\n'
        % (html.escape(title), default_bg, default_fg, default_bg, body)
    )


def main(argv=None):
    ap = argparse.ArgumentParser(description='ANSI terminal capture -> self-contained HTML')
    ap.add_argument('capture', help='file from `tmux capture-pane -p -e` (or - for stdin)')
    ap.add_argument('--light', action='store_true', help='light background (default: dark)')
    ap.add_argument('--title', default=None)
    a = ap.parse_args(argv)
    raw = sys.stdin.read() if a.capture == '-' else open(a.capture, encoding='utf-8', errors='replace').read()
    sys.stdout.write(convert(raw, a.light, a.title or a.capture))
    return 0


if __name__ == '__main__':
    sys.exit(main())
