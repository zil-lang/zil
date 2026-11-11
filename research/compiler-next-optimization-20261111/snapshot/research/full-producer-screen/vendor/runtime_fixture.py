#!/usr/bin/env python3
"""Same pure functions, fixed 100000-call main, independent 251-period oracle."""
import hashlib
import json
from pathlib import Path
from fixture import generate, sha

ITERATIONS = 100_000
SEED_PERIOD = 251
ACC_MODULUS = 1_000_003


def independent_value(n, functions, selected, seed):
    # This evaluator consumes integer case parameters, never source text, LLVM,
    # an executable, or compiler output. It intentionally does not call the
    # source generator's model/body_parameters helpers.
    total = 3 if 0 in selected else 0
    for file_index in range(1, n):
        for function_index in range(functions):
            y = seed
            edit = (1 if function_index == 0 else 2 if function_index == functions - 1 else 0) if file_index in selected else 0
            for step in range(4):
                coefficient = 2 + (3 * file_index + function_index + 2 * step) % 11
                constant = 1 + (7 * file_index + 3 * function_index + step) % 37
                y = (coefficient * y + constant + (edit if step == 0 else 0)) % 251
            total += y
    return total


def periodic_oracle(n, functions, selected):
    values = [independent_value(n, functions, selected, seed) for seed in range(SEED_PERIOD)]
    cycles, remainder = divmod(ITERATIONS, SEED_PERIOD)
    cycle_sum = sum(values)
    vectors = []
    for byte in [0, 65, 255]:
        tail = sum(values[(byte + i) % SEED_PERIOD] for i in range(remainder))
        answer = (cycles * cycle_sum + tail) % ACC_MODULUS
        vectors.append({'stdin_hex': bytes([byte]).hex(), 'expected_acc': answer,
                        'stdout_hex': answer.to_bytes(4, 'little').hex()})
    return {'period': SEED_PERIOD, 'iterations': ITERATIONS, 'cycles': cycles,
            'remainder': remainder, 'cycle_sum': cycle_sum, 'value_by_seed': values,
            'vectors': vectors,
            'proof': 'seed(i)=(input+i) mod251; sum of value(seed) has 251 period; repeated accumulator reduction equals total sum mod1000003; all intermediates fit i32'}


def generate_runtime(root, n, k, reference_fixture):
    root = Path(root)
    manifest = generate(root, n, k, functions=8, flavor='dynamic-body')
    reference_fixture = Path(reference_fixture)
    pure_hashes = {}
    for ext, language in [('zil', 'zil'), ('rs', 'rust')]:
        for i in range(1, n):
            name = f'm{i}.{ext}'
            if (root / name).read_bytes() != (reference_fixture / name).read_bytes():
                raise ValueError(f'pure leaf differs from original corpus: {name}')
            pure_hashes[name] = sha(root / name)
        main = root / f'main.{ext}'
        original = main.read_text()
        marker = 'extern "C" fn putchar' if ext == 'zil' else 'unsafe extern "C"'
        prefix = original[:original.index(marker)]
        reference = (reference_fixture / main.name).read_text()
        if prefix != reference[:reference.index(marker)]:
            raise ValueError('root pure functions differ from original corpus')
        pure_hashes[main.name + ':pure-prefix'] = hashlib.sha256(prefix.encode()).hexdigest()
        if language == 'zil':
            body = '''extern "C" fn getchar()->i32;
extern "C" fn putchar(v:i32)->i32;
fn main()->i32{
var input:i32=0;unsafe{input=getchar();}if input<0{return 2;}
var i:i32=0;var acc:i32=0;
while i<100000{let seed=(input+i)%251;acc=(acc+value(seed))%1000003;i+=1;}
unsafe{putchar((acc>>0)&255);putchar((acc>>8)&255);putchar((acc>>16)&255);putchar((acc>>24)&255);}
return 0;}
'''
        else:
            body = '''unsafe extern "C"{fn getchar()->i32;fn putchar(v:i32)->i32;}
fn main(){let input=unsafe{getchar()};if input<0{std::process::exit(2);}
let mut i:i32=0;let mut acc:i32=0;
while i<100000{let seed=(input+i)%251;acc=(acc+value(seed))%1000003;i+=1;}
unsafe{putchar((acc>>0)&255);putchar((acc>>8)&255);putchar((acc>>16)&255);putchar((acc>>24)&255);}}
'''
        main.write_text(prefix + body)
    oracle = periodic_oracle(n, 8, set(manifest['selected_indices']))
    max_value = (n - 1) * 8 * 250 + 3
    manifest.update(source_hashes={p.name: sha(p) for p in sorted(root.iterdir()) if p.suffix in ['.zil', '.rs']},
                    pure_function_hashes=pure_hashes, runtime_oracle=oracle,
                    native_vectors=oracle['vectors'],
                    runtime_bounds={'max_value': max_value, 'max_acc_plus_value': ACC_MODULUS - 1 + max_value,
                                    'max_input_plus_i': 255 + ITERATIONS - 1, 'i32_max': 2_147_483_647})
    for key in ['expected_u32', 'expected_stdout_hex']:
        manifest.pop(key)
    (root / 'fixture.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest
