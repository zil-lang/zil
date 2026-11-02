#!/usr/bin/env python3
"""Frozen/observed preparation source operations; never read compiler/native output."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3] / 'tests/foreign-scoped-callback/local-preparation'


def row(null, fail_at, observed):
    # Mark(3) is created, then exactly one reserve request is attempted. An
    # unsuccessful reserve exits before Event/body/registration exist.
    failed = fail_at == 1
    output = []
    if not failed:
        output.append('R;S1>=0000000000000002;')  # f(1) = 2 at registration
        if null:
            output.append('N;')  # roots cleared, body uncalled
            if observed:
                output.append('D3;')
        else:
            if observed:
                output.extend(['B4;', 'D3;'])  # consume Event(3, one byte)
            # Event's ByteBuffer is freed before Closing/close on success.
            output.append('C;X11>=0000000100000000;Z;')
    elif observed:
        output.append('D3;')  # only Mark was successfully initialized
    if observed:
        output.append('A5;')  # caller invokes the restored f(4) after any return
    return dict(input_hex=('31' if null else '30'), fail_at=fail_at, exit=0,
                signal=None, stdout=''.join(output), alloc_calls=1,
                allocated=int(not failed), frees=int(not failed), failures=int(failed),
                live=0, peak_live=int(not failed))


def build(observed):
    return dict(level='independent preparation source-operation model; no compilation/native claim',
                cases=[dict(name=name, normal=row(null, 0, observed),
                            allocator=[row(null, 1, observed), row(null, 2, observed)])
                       for null, name in [(False, 'success'), (True, 'register-null')]])


if __name__ == '__main__':
    for observed, name in [(False, 'FROZEN_ORACLE.json'), (True, 'OBSERVED_ORACLE.json')]:
        (ROOT / name).write_text(json.dumps(build(observed), indent=2) + '\n')
