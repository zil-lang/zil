#!/usr/bin/env python3
"""C1 source/native differential checks with an independent Python state model.

This is a bounded reproducible corpus, not a claim of exhaustive fuzzing. Every
case crosses the production compiler and executes at O0 and O2. Two deliberate
source mutations must be detected as value mismatches, never as compiler errors.
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
        'compiler_path': str(compiler),
        'compiler_sha256': sha(compiler),
        'harness_sha256': sha(Path(__file__)),
    }


def model(case):
    """Model stored immutable environments, independently of ZIL AST/IR."""
    total, actions = 0, 0
    for index in range(case['iterations']):
        slot = case['seed'] + index
        environment = (slot, case['factor'], case['seed'] % 2 == 0)
        saved = tuple(environment)
        slot += case['delta']
        chosen = environment if index % 2 == 0 else saved
        for argument, callable_value in [(index + case['a'], chosen), (case['b'], saved)]:
            captured, factor, forward = callable_value
            product = argument * factor
            total += captured + product if forward else captured - product
        actions += 7  # create, copy, mutate slot, select, two calls, accumulate
    return total, actions


def source(case, capture_late=False):
    def number(value):
        return str(value)
    mutation = '        outer = outer + ' + number(case['delta']) + '\n'
    before, after = (mutation, '') if capture_late else ('', mutation)
    return f'''fn probe(seed: i64) -> i64 {{
    var total: i64 = 0
    let limit: i64 = {number(case['iterations'])}
    for index in 0..limit {{
        var outer = seed + index
        let factor: i64 = {number(case['factor'])}
        let forward = seed % 2 == 0
{before}        let f = move (x) => if forward {{ outer + x * factor }} else {{ outer - x * factor }}
        var chosen = f
        let saved = chosen
{after}        chosen = if index % 2 == 0 {{ f }} else {{ saved }}
        total = total + chosen(index + {number(case['a'])})
        total = total + saved({number(case['b'])})
    }}
    return total
}}
fn answer() -> i64 => probe({number(case['seed'])})
fn main() -> i32 => 0
'''


def call(command, folder):
    folder.mkdir()
    command = [str(arg) for arg in command]
    try:
        proc = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as error:
        save(folder / 'command.json', {'command': command, 'infrastructure_error': str(error)})
        raise
    (folder / 'stdout.txt').write_text(proc.stdout)
    (folder / 'stderr.txt').write_text(proc.stderr)
    save(folder / 'command.json', {'command': command, 'exit_code': proc.returncode})
    return proc


def execute(case, folder, compiler, clang, mutation=False):
    folder.mkdir()
    save(folder / 'input.json', case)
    program = folder / 'program.zil'
    program.write_text(source(case, mutation))
    expected, actions = model(case)
    record = {'id': folder.name, 'status': 'Fail', 'evidence_level': 'NativeSource',
              'input': case, 'expected': expected, 'source_sha256': sha(program),
              'state_actions': actions, 'assertions': 0, 'native_executions': 0,
              'mutation_control': mutation, 'observed': []}
    try:
        executable = folder / 'program-O0'
        compiled = call([compiler, 'build', program, '--entry', 'answer', '--json',
                         '--output', executable, '--emit-dir', folder / 'stages'], folder / 'compile')
        report = json.loads(compiled.stdout)
        if compiled.returncode != 0 or report.get('status') != 'Success':
            raise AssertionError('source compile failed: ' + compiled.stdout)
        llvm = Path(report['artifacts']['llvm'])
        typed = Path(report['artifacts']['typed']).read_text()
        if not all(operation in typed for operation in ('MakeClosure', 'ReadCapture', 'CallClosure')):
            raise AssertionError('missing concrete closure instructions in actual typed artifact')
        record['assertions'] += 2
        optimized = folder / 'program-O2'
        linked = call([clang, '-O2', llvm, '-o', optimized], folder / 'optimize')
        if linked.returncode != 0:
            raise AssertionError('O2 LLVM link failed: ' + linked.stderr)
        for label, binary in [('O0', executable), ('O2', optimized)]:
            ran = call([binary], folder / ('run-' + label))
            record['native_executions'] += 1
            if ran.returncode != 0:
                raise AssertionError('native execution failed: ' + str(ran.returncode))
            actual = int(ran.stdout.strip())
            record['observed'].append({'optimization': label, 'value': actual, 'machine_sha256': sha(binary)})
            if (actual == expected) == mutation:
                raise AssertionError('mutation was missed' if mutation else f'oracle mismatch: {actual} != {expected}')
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out-root', default='out/runs/closures')
    parser.add_argument('--seed', type=int, default=23041)
    parser.add_argument('--cases', type=int, default=24)
    parser.add_argument('--compiler', default=os.environ.get('ZIL_BIN', ROOT / 'target/debug/zil'))
    args = parser.parse_args()
    if not 2 <= args.cases <= 256:
        parser.error('--cases must be within 2..256')
    compiler = Path(args.compiler).resolve()
    if not compiler.is_file():
        parser.error('build zil-cli first')
    provenance_start = provenance(compiler)
    output = (ROOT / args.out_root).resolve()
    if not output.is_relative_to(ROOT / 'out/runs'):
        parser.error('--out-root must be beneath out/runs')
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    output = output / stamp
    output.mkdir(parents=True, exist_ok=False)
    generator = random.Random(args.seed)
    inputs = [{'seed': generator.randint(-1000, 1000), 'factor': generator.randint(-7, 7),
               'iterations': generator.randint(1, 9), 'delta': generator.randint(1, 1000),
               'a': generator.randint(-20, 20), 'b': generator.randint(-20, 20)} for _ in range(args.cases)]
    # Fix both parity branches and zero multiplier as independent domain anchors.
    inputs[0].update(seed=20, factor=0, iterations=3)
    inputs[1].update(seed=21, factor=-3, iterations=4)
    clang = os.environ.get('ZIL_CLANG', 'clang')
    records = [execute(case, output / f'case-{index:03}', compiler, clang) for index, case in enumerate(inputs)]
    controls = [execute(case, output / f'control-{index:03}', compiler, clang, True) for index, case in enumerate(inputs[:2])]
    provenance_end = provenance(compiler)
    provenance_stable = provenance_start == provenance_end
    report = {'schema_version': 1, 'scope': 'C1 readonly Copy scalar environments, bounded source/native differential corpus; not borrow/resource/full-language acceptance',
              'run_id': stamp, **provenance_start, 'seed': args.seed,
              'provenance_start': provenance_start, 'provenance_end': provenance_end,
              'provenance_stable': provenance_stable,
              'selected': len(records) + len(controls), 'executed': len(records) + len(controls),
              'normal_cases': len(records), 'mutation_controls': len(controls),
              'passed': sum(item['status'] == 'Pass' for item in records + controls),
              'state_actions': sum(item['state_actions'] for item in records),
              'native_executions': sum(item['native_executions'] for item in records + controls),
              'assertions': sum(item['assertions'] for item in records + controls),
              'status': 'Pass' if provenance_stable and all(item['status'] == 'Pass' for item in records + controls) else 'Fail',
              'cases': records, 'controls': controls, 'result_path': str((output / 'result.json').relative_to(ROOT))}
    save(output / 'result.json', report)
    print(json.dumps({key: value for key, value in report.items() if key not in ('cases', 'controls')}, indent=2))
    return report['status'] != 'Pass'


if __name__ == '__main__':
    sys.exit(main())
