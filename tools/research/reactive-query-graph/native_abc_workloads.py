#!/usr/bin/env python3
"""ABC-only equal-width control; keep the archived historical workloads intact."""
import native_workloads as base


def requests(case):
    labels = base.requests(case)
    return [*labels, 'private_equal_width'] if case == 'scalar' else labels


def snapshot(case, label, modules, functions, steps):
    if label != 'private_equal_width':
        return base.snapshot(case, label, modules, functions, steps)
    if case != 'scalar':
        raise ValueError('private_equal_width is a scalar-only control')
    files = base.snapshot(case, 'cold', modules, functions, steps)
    old = files['m0.zil']
    needle = 'y * 2 + 1)'
    if needle not in old:
        raise ValueError('equal-width control no longer locates its actual first arithmetic statement')
    files['m0.zil'] = old.replace(needle, 'y * 2 + 2)', 1)
    if len(files['m0.zil'].encode()) != len(old.encode()) or old == files['m0.zil']:
        raise ValueError('equal-width private control changed byte length or no bytes')
    return files


def oracle(case, data, label, modules, functions, steps):
    if label != 'private_equal_width':
        return base.oracle(case, data, label, modules, functions, steps)
    if case != 'scalar':
        raise ValueError('private_equal_width is a scalar-only control')
    # Independent arithmetic interpreter; no parsing of the modified source.
    x = (data[0] if data else 0) % 251
    total = 0
    for i in range(modules):
        value = x
        for j in range(functions):
            y = x
            for k in range(steps):
                a = (i + j + k) % 13 + 1 + int(i == j == k == 0)
                b = (j * 3 + k) % 11 + 2
                y = (y * 2 + a) % 251 if y % 3 == 0 else (y + b) % 251
            value = (value + y) % 65521
        total = (total + value) % 65521
    return {'exit': 0, 'stdout_hex': total.to_bytes(4, 'little').hex(), 'stderr_hex': ''}
