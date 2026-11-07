#!/usr/bin/env python3
"""Deterministic invalid lock inputs; never edits a source or frozen valid lock."""
import hashlib
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
text = (ROOT / 'tests/comptime-generated-items/locks/providers-five.lock').read_text()
name = sys.argv[1]
lines = text.splitlines()
index = next(i for i, line in enumerate(lines)
             if line.startswith('provider providers.expressions default '))
if name == 'missing_default':
    del lines[index]
elif name == 'duplicate_entry':
    lines.append(lines[index])
elif name == 'wrong_rule':
    lines[1] = 'rule STALE-LANGUAGE-RULE'
elif name in ['stale_source', 'wrong_grammar']:
    words = lines[index].split()
    if name == 'stale_source':
        words[6] = '00' * 32
        words[7] = hashlib.sha256(b'zil/provider-grammar/v1\0ZIL-LANG-1.4\0' + bytes(32)).hexdigest()
    else:
        words[7] = '00' * 32
    lines[index] = ' '.join(words)
else:
    raise AssertionError(name)
print('\n'.join(lines))
