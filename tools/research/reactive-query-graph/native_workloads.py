#!/usr/bin/env python3
"""Real Source workload snapshots and independent native-output oracles.

No compiler is invoked here. Generated arithmetic is bounded and consumes stdin;
all functions contribute to output. The mixed case uses actual generic/trait,
recursive, CT and owned-closure syntax copied from existing language consumers.
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
REQUESTS = ['cold', 'unchanged', 'private', 'multi', 'broad', 'signature', 'layout',
            'trait', 'ct', 'positions', 'error', 'recovery', 'restore']
INPUTS = [b'', b'\x00', b'1', b'A', b'\x80', b'\xff']
EXISTING = ['resource_closures', 'where-traits', 'trait-modules', 'ct-selection', 'generics']


def sha_bytes(value):
    return hashlib.sha256(value).hexdigest()


def delta(label, module, function, modules, functions):
    if label == 'private':
        return int(module == 0 and function == 0)
    if label == 'multi':
        return 1 if module in {0, modules // 2, modules - 1} and function == 0 else 0
    return int(label == 'broad')


def generated(modules=8, functions=8, steps=16, label='cold', mixed=True):
    """Return complete current-source snapshot; labels are independent of history."""
    if min(modules, functions, steps) < 1:
        raise ValueError('modules/functions/steps must be positive')
    files = {}
    files['io.zil'] = (ROOT / 'examples/mainline/resource_closures/io.zil').read_text()
    if mixed:
        files['token.zil'] = (ROOT / 'examples/mainline/resource_closures/token.zil').read_text()
        files['config.zil'] = 'pub fn compact() -> bool { return ' + ('false' if label == 'ct' else 'true') + ' }\n'
        layout = label == 'layout'
        files['common.zil'] = '''import config
pub trait Read { fn get(self: &Self) -> i32; }
pub struct Counter { n: i32''' + ('; pad: i32' if layout else '') + ''' }
impl Read for Counter { fn get() -> i32 => .n''' + (' + 1' if label == 'trait' else '') + ''' }
fn read<T>(value: &T) -> i32 where T: Read => value.get()
fn recur(x: i32, depth: i32) -> i32 {
    if depth == 0 { return x }
    return recur(x, depth - 1) + 1
}
#if (config.compact()) {
    fn bias() -> i32 { return 3 }
} else {
    fn bias() -> i32 { return 5 }
}
pub fn seed(x: i32''' + (', offset: i32' if label == 'signature' else '') + ''') -> i32 {
    let item = Counter { n: x''' + (', pad: 0' if layout else '') + ''' }
    return read(&item) + recur(x, 3) + bias()''' + (' + offset' if label == 'signature' else '') + '''
}
'''
    for i in range(modules):
        code = ['import common\n'] if mixed else []
        for j in range(functions):
            d = delta(label, i, j, modules, functions)
            code.append(f'fn h{j}(x: i32) -> i32 {{\n    var y: i32 = x\n')
            for k in range(steps):
                # Both branches depend on live state and differ in arithmetic.
                a, b = (i + j + k) % 13 + 1 + d, (j * 3 + k) % 11 + 2
                code.append(f'    if y % 3 == 0 {{ y = (y * 2 + {a}) % 251 }} else {{ y = (y + {b}) % 251 }}\n')
            code.append('    return absent_name\n}\n' if label == 'error' and i == j == 0 else '    return y\n}\n')
        seed = 'common.seed(x' + (', 0' if label == 'signature' else '') + ')' if mixed else 'x'
        code.append(f'pub fn value(x: i32) -> i32 {{\n    var total: i32 = {seed}\n')
        code.extend(f'    total = (total + h{j}(x)) % 65521\n' for j in range(functions))
        code.append('    return total\n}\n')
        files[f'm{i}.zil'] = ''.join(code)
    main = ['import io\n'] + (['import token\n'] if mixed else [])
    main.extend(f'import m{i}\n' for i in range(modules))
    main.append('fn main() -> i32 {\n    var x: i32 = io.read()\n    if x < 0 { x = 0 }\n    x = x % 251\n')
    if mixed:
        main.append('    let owned = token.make(65)\n    let read = move () => token.read(owned)\n    var total: i32 = read()\n')
    else:
        main.append('    var total: i32 = 0\n')
    main.extend(f'    total = (total + m{i}.value(x)) % 65521\n' for i in range(modules))
    main.extend(f'    io.emit((total >> {shift}) & 255)\n' for shift in [0, 8, 16, 24])
    main.append('    return 0\n}\n')
    files['main.zil'] = ''.join(main)
    if label == 'positions':
        files = {name: '// Source position edit; semantic body is unchanged.\n\n' + value for name, value in files.items()}
    return files


def expected_generated(data, modules=8, functions=8, steps=16, label='cold', mixed=True):
    """Independent integer interpreter: no source parse or compiler output reuse."""
    x = (data[0] if data else 0) % 251
    total = 65 if mixed else 0
    for i in range(modules):
        value = 2 * x + 3 + (5 if label == 'ct' else 3) + int(label == 'trait') if mixed else x
        for j in range(functions):
            y = x
            change = delta(label, i, j, modules, functions)
            for k in range(steps):
                a, b = (i + j + k) % 13 + 1 + change, (j * 3 + k) % 11 + 2
                y = (y * 2 + a) % 251 if y % 3 == 0 else (y + b) % 251
            value = (value + y) % 65521
        total = (total + value) % 65521
    return {'exit': 0, 'stdout_hex': (total.to_bytes(4, 'little') + (b'A' if mixed else b'')).hex(), 'stderr_hex': ''}


def existing(case, label='cold'):
    source = ROOT / ('examples/generics' if case == 'generics' else f'examples/mainline/{case}')
    files = {str(p.relative_to(source)): p.read_text() for p in sorted(source.rglob('*.zil'))}
    if label == 'positions':
        files = {name: '// Source position edit.\n\n' + value for name, value in files.items()}
    elif label == 'error':
        files['main.zil'] += '\nfn injected_error() -> i32 { return absent_name }\n'
    elif label not in ['cold', 'unchanged', 'recovery', 'restore']:
        raise ValueError(f'{case} does not define request {label}')
    return files


def inputs(case):
    if case == 'resource_closures':
        return [b'', b'!', *[bytes([c]) for c in b'DSUOMETL0123']]
    if case == 'ct-selection':
        return [b'', b'!', b'0', b'1', b'2', b'3']
    return INPUTS


def expected_existing(case, data):
    """Frozen branch-by-branch oracle, derived from checked-in source semantics."""
    c = data[0] if data else -1
    if case in ['where-traits', 'trait-modules', 'generics']:
        status, output = 0, b''
    elif case == 'resource_closures':
        status, output = {
            ord('D'): (0, b'C'), ord('S'): (0, b'A'), ord('U'): (0, b'BA'),
            ord('O'): (0, b'CB'), ord('M'): (0, b'C'), ord('E'): (7, b'A'),
            ord('T'): (0, b'A'), ord('L'): (0, b'AB'),
            ord('0'): (1, b'A'), ord('1'): (2, b'BA'),
            ord('2'): (3, b'CBA'), ord('3'): (0, b'CBA'),
        }.get(c, (8, b''))
    elif case == 'ct-selection':
        if c not in b'0123':
            status, output = 9, b''
        else:
            status = 2 if c == ord('2') else 0
            output = b'A' + bytes([c]) + (b'' if c in b'01' else b'G') + (b'E' if c == ord('3') else b'')
            output += (b'' if c == ord('0') else b'y') + b'aZR'
    else:
        raise ValueError(case)
    return {'exit': status, 'stdout_hex': output.hex(), 'stderr_hex': ''}


def snapshot(case, label, modules, functions, steps):
    return generated(modules, functions, steps, label, case == 'mixed') if case in ['mixed', 'scalar'] else existing(case, label)


def oracle(case, data, label, modules, functions, steps):
    return expected_generated(data, modules, functions, steps, label, case == 'mixed') if case in ['mixed', 'scalar'] else expected_existing(case, data)


def write_snapshot(directory, files):
    """Write only byte changes, leaving unrelated source mtimes untouched."""
    directory.mkdir(parents=True, exist_ok=True)
    changed = []
    for name, content in sorted(files.items()):
        path = directory / name
        data = content.encode()
        if not path.exists() or path.read_bytes() != data:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            changed.append(name)
    for path in directory.rglob('*.zil'):
        if str(path.relative_to(directory)) not in files:
            changed.append(str(path.relative_to(directory)))
            path.unlink()
    hashes = {name: sha_bytes(content.encode()) for name, content in sorted(files.items())}
    return {'changed_files': changed, 'file_hashes': hashes,
            'identity_sha256': sha_bytes(json.dumps(hashes, sort_keys=True).encode()),
            'files': len(files), 'bytes': sum(len(c.encode()) for c in files.values()),
            'lines': sum(len(c.splitlines()) for c in files.values())}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--case', choices=['mixed', 'scalar', *EXISTING], default='mixed')
    p.add_argument('--modules', type=int, default=8)
    p.add_argument('--functions', type=int, default=8)
    p.add_argument('--steps', type=int, default=16)
    p.add_argument('--request', choices=REQUESTS, default='cold')
    a = p.parse_args()
    meta = write_snapshot(a.out, snapshot(a.case, a.request, a.modules, a.functions, a.steps))
    meta['expected'] = [{'stdin_hex': d.hex(), **oracle(a.case, d, a.request, a.modules, a.functions, a.steps)} for d in inputs(a.case)]
    print(json.dumps(meta, indent=2))


if __name__ == '__main__':
    main()
