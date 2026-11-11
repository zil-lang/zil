#!/usr/bin/env python3
"""Paired real multi-file source fixtures, adapted from ZIL distribution_bench.py.

N counts every language source file including main. k=N therefore edits main as
well. Every selected file changes two separated function bodies; no intermediate
snapshot is compiled. This is a compiler screen, not a runtime throughput suite.
"""
import hashlib
import json
from pathlib import Path


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def selected_files(n, k):
    if not (n >= 2 and 0 <= k <= n):
        raise ValueError((n, k))
    return sorted({(1 + j * n // k) % n for j in range(k)}) if k else []


def body_parameters(file_index, function_index, step):
    return (2 + (file_index * 3 + function_index + step * 2) % 11,
            1 + (file_index * 7 + function_index * 3 + step) % 37)


def model(n, k, functions, seed, flavor):
    selected = set(selected_files(n, k))
    result = 3 * int(0 in selected)
    for i in range(1, n):
        for j in range(functions):
            delta = (1 if j == 0 else 2 if j == functions - 1 else 0) if i in selected else 0
            if flavor == 'scalar':
                result += j % 5 + j % 7 + 1 + delta
            else:
                y = seed
                for step in range(4):
                    factor, offset = body_parameters(i, j, step)
                    y = (y * factor + offset + (delta if step == 0 else 0)) % 251
                result += y
    return result


def generate(root, n, k, functions=8, flavor='dynamic-body'):
    if functions < 2:
        raise ValueError('at least two function bodies per leaf are edited')
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    changed = set(selected_files(n, k))
    expected = model(n, k, functions, 65, flavor)
    for language, ext in [('zil', 'zil'), ('rust', 'rs')]:
        for i in range(1, n):
            body = []
            for j in range(functions):
                delta = (1 if j == 0 else 2 if j == functions - 1 else 0) if i in changed else 0
                if flavor == 'scalar':
                    body.append(f'fn h{j}(x:i32)->i32{{return x+{j % 7 + 1 + delta};}}\n')
                else:
                    var = 'var' if language == 'zil' else 'let mut'
                    steps = []
                    for step in range(4):
                        factor, offset = body_parameters(i, j, step)
                        steps.append(f'y=(y*{factor}+{offset + (delta if step == 0 else 0)})%251;\n')
                    body.append(f'fn h{j}(x:i32)->i32{{{var} y:i32=x;\n' + ''.join(steps) + 'return y;}\n')
            var = 'var' if language == 'zil' else 'let mut'
            parameter = 'seed:i32' if flavor != 'scalar' else ''
            body.append(f'pub fn value({parameter})->i32{{{var} total:i32=0;\n')
            body.extend(f'total=total+h{j}({"seed" if flavor != "scalar" else j % 5});\n' for j in range(functions))
            body.append('return total;}\n')
            (root / f'm{i}.{ext}').write_text(''.join(body))
        imports = ''.join(f'import m{i}\n' if language == 'zil' else f'mod m{i};\n' for i in range(1, n))
        a, b = (1, 2) if 0 in changed else (0, 0)
        var = 'var' if language == 'zil' else 'let mut'
        sep = '.' if language == 'zil' else '::'
        main = imports + f'fn own_a()->i32{{return {a};}}\nfn own_b()->i32{{return {b};}}\n'
        parameter = 'seed:i32' if flavor != 'scalar' else ''
        argument = 'seed' if flavor != 'scalar' else ''
        main += f'fn value({parameter})->i32{{{var} total:i32=own_a();\n'
        main += ''.join(f'total=total+m{i}{sep}value({argument});\n' for i in range(1, n))
        main += 'return total+own_b();}\n'
        native = ('let seed=unsafe{getchar()};let result=value(seed);' if flavor != 'scalar' else 'let result=value();')
        native += 'unsafe{' + ''.join(f'putchar((result>>{shift})&255);' for shift in [0, 8, 16, 24]) + '}'
        main += ('extern "C" fn putchar(v:i32)->i32;\nextern "C" fn getchar()->i32;\nfn main()->i32{' + native + 'return 0;}\n' if language == 'zil'
                 else 'unsafe extern "C"{fn putchar(v:i32)->i32;fn getchar()->i32;}\nfn main(){' + native + '}\n')
        (root / f'main.{ext}').write_text(main)
    hashes = {p.name: sha(p) for p in sorted(root.iterdir()) if p.is_file()}
    manifest = {'total_files_per_language': n, 'changed_files_per_language': k,
                'selected_indices': sorted(changed), 'edited_function_bodies_per_file': 2,
                'functions_per_leaf': functions + 1, 'flavor': flavor, 'expected_u32': expected,
                'expected_stdout_hex': (expected & 0xffffffff).to_bytes(4, 'little').hex(),
                'native_vectors': [{'stdin_hex': bytes([seed]).hex(),
                                    'stdout_hex': model(n, k, functions, seed, flavor).to_bytes(4, 'little').hex()}
                                   for seed in [0, 65, 255]],
                'source_hashes': hashes}
    (root / 'fixture.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest


def changed_names(n, k, ext):
    return ['main.' + ext if i == 0 else f'm{i}.{ext}' for i in selected_files(n, k)]


def generate_control(root, n, kind, functions=8, flavor='dynamic-body'):
    manifest = generate(root, n, 0, functions, flavor)
    root = Path(root)
    for ext in ['zil', 'rs']:
        path = root / f'm1.{ext}'
        text = path.read_text()
        if kind == 'call-edge':
            j = functions - 1
            value = j % 7 + 1
            # Avoid matching h0 when the last helper has the same literal.
            if flavor == 'scalar':
                original = f'fn h{j}(x:i32)->i32{{return x+{value};}}'
                replacement = f'fn h{j}(x:i32)->i32{{return h0(x)+{value - 1};}}'
                text = text.replace(original, replacement)
            else:
                # A new real call edge with equal semantics, not a changed output oracle.
                text = 'fn relay(x:i32)->i32{return x;}\n' + text.replace('y:i32=x;', 'y:i32=relay(x);', 1)
        elif kind == 'signature':
            text = text.replace('fn h0(x:i32)', 'fn h0(x:i32,pad:i32)')
            if flavor == 'scalar':
                text = text.replace('return x+1;', 'return x+1+pad;', 1).replace('h0(0);', 'h0(0,0);')
            else:
                text = text.replace('y:i32=x;', 'y:i32=x+pad;', 1).replace('h0(seed);', 'h0(seed,0);')
        elif kind == 'cfg':
            text = (text.replace('return x+1;', 'if x<3{return x+1;}else{return x+1;}', 1) if flavor == 'scalar'
                    else text.replace('return y;', 'if x<128{return y;}else{return y;}', 1))
        else:
            raise ValueError(kind)
        assert text != path.read_text(), kind
        path.write_text(text)
    manifest.update(control=kind, changed_files_per_language=1, selected_indices=[1],
                    edited_function_bodies_per_file=None,
                    source_hashes={p.name: sha(p) for p in sorted(root.iterdir()) if p.suffix in ['.zil', '.rs']})
    (root / 'fixture.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest
