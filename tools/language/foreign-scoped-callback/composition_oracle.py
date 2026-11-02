#!/usr/bin/env python3
"""Source-only Event/Reply, propagation and remaining-capture protocol oracle."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3] / 'tests/foreign-scoped-callback/callable-composition'


def row(null, fail_at, observed):
    # Exactly one reserve: failure cleans Mark(3) in prepare, returns Fault
    # through run/main and never constructs Event, tail(7), body or C roots.
    failed = fail_at == 1
    events = []
    if failed:
        if observed:
            events.append('D3;')
    else:
        events.append('R;S1>=0000000000000002;')
        if null:
            events.append('N;')
            if observed:
                # Source first uses Event, then tail: environment declaration order.
                events.extend(['D3;', 'D7;'])
        else:
            if observed:
                # Event -> Reply -> render gives 3 + one byte = 4; render
                # cleans the consumed value, then With cleans remaining tail.
                events.extend(['B4;', 'D3;', 'D7;'])
            events.append('C;X11>=0000000100000000;Z;')
    if observed:
        events.append('A5;')  # f(4) == 5 before outcome? on every return
    return dict(input_hex='31' if null else '30', fail_at=fail_at,
                exit=int(null or failed), signal=None, stdout=''.join(events),
                alloc_calls=1, allocated=int(not failed), frees=int(not failed),
                failures=int(failed), live=0, peak_live=int(not failed))


def build(observed):
    return dict(level='independent source-operation model; no compilation/native claim',
                cases=[dict(name=name, normal=row(null, 0, observed),
                            allocator=[row(null, 1, observed), row(null, 2, observed)])
                       for null, name in [(False, 'success'), (True, 'register-null')]])


if __name__ == '__main__':
    for observed, name in [(False, 'FROZEN_ORACLE.json'), (True, 'OBSERVED_ORACLE.json')]:
        (ROOT / name).write_text(json.dumps(build(observed), indent=2) + '\n')
