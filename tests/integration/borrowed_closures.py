#!/usr/bin/env python3
"""Bounded S0/C2a source/native oracle, independent of compiler safety analysis.

The Python state model records each iteration's scalar values. Native O0/O2
execution checks results; actual IR/LLVM checks distinguish address captures
from observationally equivalent snapshots. Mutations must execute and disagree.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[2]


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def provenance(compiler):
    return {
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()),
        'compiler_path': str(compiler), 'compiler_sha256': sha(compiler),
        'harness_sha256': sha(Path(__file__)),
    }


def model(case):
    total, slot, trace = 0, case['seed'], []
    for index in range(case['iterations']):
        slot += case['delta']
        first = slot + (index + case['a']) * case['factor']
        second = slot + case['b'] * case['factor']
        shadow = 1000 + index
        total += first + second + shadow
        borrowed_slot = slot
        slot += 1  # all copies have had their last use
        total += slot
        trace.append({'index': index, 'borrowed_slot': borrowed_slot,
                      'shadow_slot': shadow, 'calls': [first, second],
                      'after_last_use': slot, 'total': total})
    return total, trace


def source(case, mutation=False):
    extra = ' + 1' if mutation else ''
    return f'''fn probe(seed: i64) -> i64 {{
    var total: i64 = 0
    var slot = seed
    let factor: i64 = {case['factor']}
    let limit: i64 = {case['iterations']}
    for index in 0..limit {{
        slot = slot + {case['delta']}
        let f = (x) => slot + x * factor{extra}
        var chosen = f
        let saved = chosen
        chosen = if index % 2 == 0 {{ f }} else {{ saved }}
        total = total + chosen(index + {case['a']})
        if true {{
            var slot: i64 = 999 + index
            slot = slot + 1
            total = total + saved({case['b']}) + slot
        }}
        slot = slot + 1
        total = total + slot
    }}
    return total
}}
fn answer() -> i64 => probe({case['seed']})
fn main() -> i32 => 0
'''


def call(command, folder):
    folder.mkdir()
    command = [str(part) for part in command]
    try:
        process = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as error:
        save(folder / 'command.json', {'command': command, 'infrastructure_error': str(error)})
        raise
    (folder / 'stdout.txt').write_text(process.stdout)
    (folder / 'stderr.txt').write_text(process.stderr)
    save(folder / 'command.json', {'command': command, 'exit_code': process.returncode})
    return process


def execute(case, folder, compiler, clang, mutation=False):
    folder.mkdir()
    program = folder / 'program.zil'
    program.write_text(source(case, mutation))
    expected, trace = model(case)
    save(folder / 'model.json', {'input': case, 'expected': expected, 'trace': trace})
    record = {'id': folder.name, 'status': 'Fail', 'evidence_level': 'NativeSource',
              'input': case, 'expected': expected, 'source_sha256': sha(program),
              'model_steps': len(trace), 'assertions': 0, 'native_executions': 0,
              'mutation_control': mutation, 'observed': []}
    try:
        binary = folder / 'program-O0'
        compiled = call([compiler, 'build', program, '--entry', 'answer', '--json',
                         '--output', binary, '--emit-dir', folder / 'stages'], folder / 'compile')
        report = json.loads(compiled.stdout)
        if compiled.returncode != 0 or report.get('status') != 'Success' or not binary.is_file():
            raise AssertionError('source compile failed: ' + compiled.stdout)
        record['assertions'] += 2
        typed = Path(report['artifacts']['typed']).read_text()
        if not all(op in typed for op in ('BorrowLocal', 'ReadAddress', 'ReadCapture', 'MakeClosure', 'CallClosure')):
            raise AssertionError('actual typed artifact lacks borrowed closure operations')
        record['assertions'] += 1
        llvm = Path(report['artifacts']['llvm'])
        text = llvm.read_text()
        if 'type { ptr' not in text or 'getelementptr i8, ptr %local.' not in text:
            raise AssertionError('actual LLVM does not carry real stack addresses')
        record['assertions'] += 1
        optimized = folder / 'program-O2'
        linked = call([clang, '-O2', llvm, '-o', optimized], folder / 'optimize')
        if linked.returncode != 0:
            raise AssertionError('O2 link failed: ' + linked.stderr)
        record['assertions'] += 1
        for label, executable in [('O0', binary), ('O2', optimized)]:
            ran = call([executable], folder / ('run-' + label))
            record['native_executions'] += 1
            if ran.returncode != 0:
                raise AssertionError('native exit: ' + str(ran.returncode))
            value = int(ran.stdout.strip())
            record['observed'].append({'optimization': label, 'value': value, 'machine_sha256': sha(executable)})
            if (value == expected) == mutation:
                raise AssertionError('mutation missed' if mutation else f'oracle mismatch: {value} != {expected}')
            record['assertions'] += 2
        if record['observed'][0]['value'] != record['observed'][1]['value']:
            raise AssertionError('O0 and O2 disagree')
        record['assertions'] += 1
        record['llvm_sha256'] = sha(llvm)
        record['status'] = 'Pass'
    except (OSError, ValueError, KeyError, AssertionError, subprocess.TimeoutExpired) as error:
        record['reason'] = str(error)
    save(folder / 'result.json', record)
    return record


REJECTIONS = [
    'var x = 1; let f = () => x; x = 2; return f();',
    'var x = 1; let f = () => x; let g = f; f(); x = 2; return g();',
    'var x = 1; let f = () => x; for i in 0..2 { f(); x = x + 1; } return 0;',
    'var x = 1; let f = () => x; if true { x = 2; } return f();',
]


def rejected(index, body, output, compiler):
    folder = output / f'reject-{index:03}'
    folder.mkdir()
    program, binary = folder / 'program.zil', folder / 'program'
    program.write_text('fn answer() -> i32 { ' + body + ' }\n')
    record = {'id': folder.name, 'status': 'Fail', 'evidence_level': 'SourceRejection',
              'source_sha256': sha(program), 'assertions': 0, 'native_executions': 0}
    try:
        ran = call([compiler, 'build', program, '--entry', 'answer', '--json', '--output', binary,
                    '--emit-dir', folder / 'stages'], folder / 'compile')
        report = json.loads(ran.stdout)
        if ran.returncode == 0 or report.get('status') != 'SafeReject':
            raise AssertionError('conflict did not SafeReject: ' + ran.stdout)
        if not any(d['code'] == 'E-BORROW-CONFLICT' and d['phase'] == 'verify' for d in report['diagnostics']):
            raise AssertionError('wrong rejection boundary: ' + ran.stdout)
        if binary.exists() or 'llvm' in report.get('artifacts', {}):
            raise AssertionError('rejected source produced native artifacts')
        record.update(status='Pass', assertions=4)
    except (OSError, ValueError, KeyError, AssertionError, subprocess.TimeoutExpired) as error:
        record['reason'] = str(error)
    save(folder / 'result.json', record)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out-root', default='out/runs/borrowed-closures')
    parser.add_argument('--seed', type=int, default=23042)
    parser.add_argument('--cases', type=int, default=24)
    parser.add_argument('--compiler', default=os.environ.get('ZIL_BIN', ROOT / 'target/debug/zil'))
    args = parser.parse_args()
    compiler = Path(args.compiler).resolve()
    output = (ROOT / args.out_root).resolve()
    if not 2 <= args.cases <= 256 or not compiler.is_file() or not output.is_relative_to(ROOT / 'out/runs'):
        parser.error('require 2..256 cases, built compiler, and output beneath out/runs')
    before = provenance(compiler)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    output = output / stamp
    output.mkdir(parents=True, exist_ok=False)
    rng = random.Random(args.seed)
    inputs = [{'seed': rng.randint(-1000, 1000), 'factor': rng.randint(-7, 7),
               'iterations': rng.randint(1, 9), 'delta': rng.randint(1, 100),
               'a': rng.randint(-20, 20), 'b': rng.randint(-20, 20)} for _ in range(args.cases)]
    inputs[0].update(factor=0, iterations=3)
    inputs[1].update(factor=-3, iterations=4)
    clang = os.environ.get('ZIL_CLANG', 'clang')
    records = [execute(case, output / f'case-{i:03}', compiler, clang) for i, case in enumerate(inputs)]
    controls = [execute(case, output / f'control-{i:03}', compiler, clang, True) for i, case in enumerate(inputs[:2])]
    rejections = [rejected(i, body, output, compiler) for i, body in enumerate(REJECTIONS)]
    after = provenance(compiler)
    all_cases = records + controls + rejections
    report = {'schema_version': 1, 'scope': 'S0/C2a local shared scalar captures; bounded source/native oracle, not full borrow acceptance',
              'run_id': stamp, **before, 'seed': args.seed, 'provenance_start': before,
              'provenance_end': after, 'provenance_stable': before == after,
              'selected': len(all_cases), 'executed': len(all_cases), 'normal_cases': len(records),
              'mutation_controls': len(controls), 'rejection_cases': len(rejections),
              'passed': sum(r['status'] == 'Pass' for r in all_cases),
              'model_steps': sum(r['model_steps'] for r in records),
              'native_executions': sum(r['native_executions'] for r in all_cases),
              'assertions': sum(r['assertions'] for r in all_cases),
              'status': 'Pass' if before == after and all(r['status'] == 'Pass' for r in all_cases) else 'Fail',
              'cases': all_cases, 'result_path': str((output / 'result.json').relative_to(ROOT))}
    save(output / 'result.json', report)
    print(json.dumps({k: v for k, v in report.items() if k != 'cases'}, indent=2))
    return report['status'] != 'Pass'


if __name__ == '__main__':
    sys.exit(main())
