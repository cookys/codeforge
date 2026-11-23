#!/usr/bin/env python3
"""Keep forge-stats' shared glyphs and rendered theme colors in the vendored tables."""
import json
from pathlib import Path
import re
import sys

repo = Path(__file__).resolve().parent.parent
glyphs = json.loads((repo / 'plugins/forge-stats/vendor/tux/golden/glyphs.json').read_text())

def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)

# ASCII punctuation and ordinary prose punctuation have uses outside the glyph vocabulary.
shared = {g for key, group in glyphs.items() if key != 'verified'
          for g in strings(group) if len(g) == 1 and ord(g) > 127}
tokens = re.compile(r'''(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)|(?P<string>'(?:\\.|[^'\\])*'|"(?:\\.|[^"\\])*"|`(?:\\.|[^`\\])*`)''')
colors = r'(?:warning|error|suggestion|inactive|success|subtle|claude|merged|remember)'
direct_color = re.compile(r'''(?:\b(?:color|tone)\s*[:=]\s*['"]''' + colors + r'''['"]|\bseg\([^\n]*,\s*['"]''' + colors + r'''['"]\))''')

def violations(source):
    clean = tokens.sub(lambda m: '' if m.group('comment') else m[0], source)
    bad = ['direct theme color'] if direct_color.search(clean) else []
    for match in tokens.finditer(source):
        if match.group('string'):
            found = sorted(shared.intersection(match[0]))
            if found:
                bad.append('literal glyph: ' + ' '.join(found))
    return bad

if sys.argv[1:] == ['--test']:
    assert violations("const mark = '◉'")
    assert violations('<Text color="warning" />')
    assert violations("seg('alert', undefined, 'warning')")
    assert not violations('// ◉ example\nconst mark = GLYPHS.state.running')
    assert not violations('<Text color={PALETTE.attention} />')
    print('tux-lint: ok (5 controls)')
else:
    errors = [f'{path.relative_to(repo)}: {error}'
              for path in sorted((repo / 'plugins/forge-stats/hooks').glob('*.ts*'))
              for error in violations(path.read_text())]
    if errors:
        sys.exit('\n'.join(errors))
    print('tux-lint: ok')
