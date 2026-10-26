#!/usr/bin/env python3
"""Bounded independent model and Rust/ZIL native behavior comparison (not timing)."""
import argparse
import datetime
import json
import os
from pathlib import Path
import random
import sys
import uuid

from borrowed_closures import ROOT, call, provenance, save, sha


def model(case):
    captured = case['left'] + case['snapshot']
    calls = [captured + case['a'], captured + case['b']]
    left = case['left'] + case['inc_left']
    right = case['right'] + case['inc_right']
    return sum(calls) + case['right'] + left + right + case['fresh'], {
        'capture': {'address_source': case['left'], 'snapshot': case['snapshot']},
        'calls': calls, 'after_last_use': {'left': left, 'right': right},
    }


def zil_source(c, mutation=False):
    wrong = ' + 1' if mutation else ''
    return f'''fn answer() -> i64 {{
    var left: i64 = {c['left']}
    var right: i64 = {c['right']}
    var address = &left
    var snapshot: i64 = {c['snapshot']}
    let f = move (delta) => *address + snapshot + delta{wrong}
    let g = f
    address = &right
    snapshot = {c['fresh']}
    let calls = f({c['a']}) + g({c['b']}) + *address
    right = *address + {c['inc_right']}
    left = left + {c['inc_left']}
    return calls + left + right + snapshot
}}
fn main() -> i32 => 0
'''


def rust_source(c):
    return f'''fn main() {{
    let mut left: i64 = {c['left']};
    let mut right: i64 = {c['right']};
    let mut address = &left;
    let mut snapshot: i64 = {c['snapshot']};
    let f = move |delta: i64| *address + snapshot + delta;
    let g = f;
    address = &right;
    snapshot = {c['fresh']};
    let calls = f({c['a']}) + g({c['b']}) + *address;
    right = *address + {c['inc_right']};
    left = left + {c['inc_left']};
    println!("{{}}", calls + left + right + snapshot);
}}
'''


def execute(case, folder, compiler, clang, rustc, mutation=False):
    folder.mkdir()
    zil, rust = folder / 'program.zil', folder / 'program.rs'
    zil.write_text(zil_source(case, mutation))
    rust.write_text(rust_source(case))
    expected, trace = model(case)
    save(folder / 'model.json', {'input': case, 'expected': expected, 'trace': trace})
    record = {'id': folder.name, 'status': 'Fail', 'evidence_level': 'PairedNativeBehavior',
              'performance_evidence': False, 'expected': expected, 'mutation_control': mutation,
              'source_sha256': sha(zil), 'rust_source_sha256': sha(rust),
              'assertions': 0, 'zil_native_executions': 0, 'rust_native_executions': 0, 'observed': []}
    try:
        binary = folder / 'zil-O0'
        built = call([compiler, 'build', zil, '--entry', 'answer', '--json', '--output', binary,
                      '--emit-dir', folder / 'stages'], folder / 'build-zil')
        report = json.loads(built.stdout)
        assert built.returncode == 0 and report.get('status') == 'Success' and binary.is_file(), built.stdout
        record['assertions'] += 1
        typed = Path(report['artifacts']['typed']).read_text()
        assert all(op in typed for op in ('BorrowLocal', 'ReadAddress', 'ReadCapture', 'MakeClosure', 'CallClosure'))
        record['assertions'] += 1
        llvm = Path(report['artifacts']['llvm'])
        text = llvm.read_text()
        assert 'type { ptr, i64 }' in text and 'getelementptr i8, ptr %local.' in text
        record['assertions'] += 1
        optimized, rust_binary = folder / 'zil-O2', folder / 'rust'
        linked = call([clang, '-O2', llvm, '-o', optimized], folder / 'link-O2')
        assert linked.returncode == 0, linked.stderr
        record['assertions'] += 1
        compiled = call([rustc, '--edition=2024', '-C', 'opt-level=0', '-C', 'overflow-checks=yes',
                         rust, '-o', rust_binary], folder / 'build-rust')
        assert compiled.returncode == 0, compiled.stderr
        record['assertions'] += 1
        for name, executable in [('ZIL-O0', binary), ('ZIL-O2', optimized), ('Rust', rust_binary)]:
            ran = call([executable], folder / ('run-' + name))
            record['rust_native_executions' if name == 'Rust' else 'zil_native_executions'] += 1
            assert ran.returncode == 0 and ran.stderr == '', str(ran)
            value = int(ran.stdout.strip())
            record['observed'].append({'name': name, 'value': value, 'machine_sha256': sha(executable)})
            assert (value != expected) if mutation and name != 'Rust' else (value == expected), (name, value, expected)
            record['assertions'] += 2
        assert record['observed'][0]['value'] == record['observed'][1]['value']
        record['assertions'] += 1
        for executable in (binary, optimized, rust_binary):
            executable.unlink()
        record['removed_native_artifacts'] = 3
        record.update(status='Pass', llvm_sha256=sha(llvm))
    except Exception as error:
        record['reason'] = f'{type(error).__name__}: {error}'
    save(folder / 'result.json', record)
    return record


REJECTIONS = [
    ('var x = 1; let p = &x; let f = move () => *p; x = 2; return f();',
     'let mut x = 1; let p = &x; let f = move || *p; x = 2; f()', 'E0506'),
    ('var x = 1; let p = &x; let f = move () => *p; let g = f; f(); x = 2; return g();',
     'let mut x = 1; let p = &x; let f = move || *p; let g = f; f(); x = 2; g()', 'E0506'),
    ('let x = 1; var p = &x; if true { let inner = 2; p = &inner; } let f = move () => *p; return f();',
     'let x = 1; let mut p = &x; if true { let inner = 2; p = &inner; } let f = move || *p; f()', 'E0597'),
]


def rejected(index, item, output, compiler, rustc):
    folder = output / f'reject-{index:03}'
    folder.mkdir()
    zil_body, rust_body, code = item
    zil, rust = folder / 'program.zil', folder / 'program.rs'
    zil.write_text('fn answer() -> i32 { ' + zil_body + ' }\n')
    rust.write_text('fn answer() -> i32 { ' + rust_body + ' } fn main() { println!("{}", answer()); }\n')
    binary, rust_binary = folder / 'zil', folder / 'rust'
    record = {'id': folder.name, 'status': 'Fail', 'evidence_level': 'PairedSourceRejection',
              'assertions': 0, 'zil_native_executions': 0, 'rust_native_executions': 0}
    try:
        built = call([compiler, 'build', zil, '--entry', 'answer', '--json', '--output', binary,
                      '--emit-dir', folder / 'stages'], folder / 'build-zil')
        report = json.loads(built.stdout)
        assert built.returncode != 0 and report.get('status') == 'SafeReject', built.stdout
        assert any(d['code'] == 'E-BORROW-CONFLICT' and d['phase'] == 'verify' for d in report['diagnostics']), built.stdout
        assert not binary.exists() and 'llvm' not in report.get('artifacts', {})
        record['assertions'] += 3
        compiled = call([rustc, '--edition=2024', '--error-format=json', rust, '-o', rust_binary], folder / 'build-rust')
        errors = [json.loads(line) for line in compiled.stderr.splitlines() if line.strip()]
        assert compiled.returncode != 0 and any((x.get('code') or {}).get('code') == code for x in errors)
        assert not rust_binary.exists()
        record.update(status='Pass', assertions=5, rust_error=code, zil_error='E-BORROW-CONFLICT')
    except Exception as error:
        record['reason'] = f'{type(error).__name__}: {error}'
    save(folder / 'result.json', record)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    if not __debug__:
        parser.error('Python optimized mode removes assertions; this oracle requires normal execution')
    parser.add_argument('--out-root', default='out/runs/mixed-captures')
    parser.add_argument('--seed', type=int, default=23043)
    parser.add_argument('--compiler', default=os.environ.get('ZIL_BIN', ROOT / 'target/debug/zil'))
    args = parser.parse_args()
    compiler = Path(args.compiler).resolve()
    output = (ROOT / args.out_root).resolve()
    if not compiler.is_file() or not output.is_relative_to(ROOT / 'out/runs'):
        parser.error('require a built compiler and output beneath out/runs')
    before = provenance(compiler)
    before['harness_sha256'] = sha(Path(__file__))
    before['helper_sha256'] = sha(Path(__file__).with_name('borrowed_closures.py'))
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    output = output / stamp
    output.mkdir(parents=True)
    rng = random.Random(args.seed)
    inputs = [{key: rng.randint(-1000, 1000) for key in
               ('left', 'right', 'snapshot', 'fresh', 'a', 'b', 'inc_left', 'inc_right')} for _ in range(8)]
    inputs[0].update(left=2147483648, right=-2147483649)
    clang, rustc = os.environ.get('ZIL_CLANG', 'clang'), os.environ.get('RUSTC', 'rustc')
    tool_versions = {name: call([command, '--version'], output / (name + '-version')).stdout
                     for name, command in [('clang', clang), ('rustc', rustc)]}
    normal = [execute(c, output / f'case-{i:03}', compiler, clang, rustc) for i, c in enumerate(inputs)]
    controls = [execute(c, output / f'control-{i:03}', compiler, clang, rustc, True) for i, c in enumerate(inputs[:2])]
    rejections = [rejected(i, c, output, compiler, rustc) for i, c in enumerate(REJECTIONS)]
    after = provenance(compiler)
    after['harness_sha256'], after['helper_sha256'] = sha(Path(__file__)), sha(Path(__file__).with_name('borrowed_closures.py'))
    records = normal + controls + rejections
    report = {'schema_version': 1, 'scope': 'S1a bounded mixed capture behavior, not product or performance acceptance',
              'run_id': stamp, **before, 'seed': args.seed, 'tool_versions': tool_versions,
              'provenance_start': before, 'provenance_end': after, 'provenance_stable': before == after,
              'selected': len(records), 'executed': len(records), 'normal_cases': len(normal),
              'mutation_controls': len(controls), 'rejection_cases': len(rejections),
              'passed': sum(x['status'] == 'Pass' for x in records),
              'assertions': sum(x['assertions'] for x in records),
              'zil_native_executions': sum(x['zil_native_executions'] for x in records),
              'rust_native_executions': sum(x['rust_native_executions'] for x in records),
              'status': 'Pass' if before == after and all(x['status'] == 'Pass' for x in records) else 'Fail',
              'cases': records, 'result_path': str((output / 'result.json').relative_to(ROOT))}
    save(output / 'result.json', report)
    print(json.dumps({key: value for key, value in report.items() if key != 'cases'}, indent=2))
    return report['status'] != 'Pass'


if __name__ == '__main__':
    sys.exit(main())
