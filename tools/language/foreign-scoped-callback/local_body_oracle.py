#!/usr/bin/env python3
"""Local-body source operations and Direct C protocol; no compiler/output input."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3] / 'tests/foreign-scoped-callback'


def build():
    output, count = ['R0;'], 0
    # register invokes f(1), body invokes f(2), f(3), close rejects f(11).
    for phase, value in [('S', 1), ('P', 2), ('P', 3)]:
        if phase == 'P' and value == 2:
            output.append('B1;')
        count += 1
        output.append(f'{phase}0:{value}>FL;={value + count:016x};')
    output.append('C0;X0:11>=0000000100000000;Z0;')
    count += 1
    assert 4 + count == 8  # restored caller access after close
    output.append('FL;')
    row = dict(input_hex='30', fail_at=0, exit=0, signal=None,
               stdout=''.join(output), alloc_calls=0, allocated=0, frees=0,
               failures=0, live=0, peak_live=0)
    return dict(level='independent local-body source-operation model; no compilation/native claim',
                cases=[dict(name='f-only-wrapper-local-concrete-body', normal=row, allocator=[])])


if __name__ == '__main__':
    (ROOT / 'LOCAL_BODY_ORACLE.json').write_text(json.dumps(build(), indent=2) + '\n')
