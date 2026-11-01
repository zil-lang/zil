#!/usr/bin/env python3
"""Scoped full-CLI O0 A/B screen, not a product gate or a Rust comparison.

Run after both release CLI builds and all concurrent compiler/test jobs finish.
All five requests include current Verify, LLVM/cache, link and publication.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    compilers = {name: getattr(args, name).resolve(strict=True) for name in ('baseline', 'candidate')}
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    manifest = {
        'scope': 'complete CLI O0 declaration-dependency screen, not product acceptance',
        'baseline_source': '21058d0e331de764e3a1b9123c3e1edf5e55a1aa',
        'candidate_source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'compilers': {name: {'path': str(path), 'sha256': digest(path)} for name, path in compilers.items()},
        'cpus': cpus, 'blocks': 5, 'modules': [4, 32],
        'requests': ['cold', 'unchanged', 'unused', 'private', 'restore'],
        'flags': ['--entry', 'value', '--opt', 'O0', '--codegen-units', '8', '--codegen-jobs', '2'],
        'timing': 'subprocess complete wall; native execution checked outside timer; OS cache uncontrolled',
    }
    (args.out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    rows = []
    for modules in manifest['modules']:
        source = args.out / f'source-{modules}'
        source.mkdir()
        leaves = modules - 1
        names = [f'm{i:03}' for i in range(leaves)]
        root = ''.join(f'import {name}\n' for name in names)
        root += 'fn value()->i32{return ' + '+'.join(f'{name}.value()' for name in names) + '}\n'
        (source / 'main.zil').write_text(root)
        texts = []
        for i, name in enumerate(names):
            body = f'fn h0()->i32{{return {i + 1}}}\n'
            body += ''.join(f'fn h{j}()->i32{{return h{j - 1}()}}\n' for j in range(1, 7))
            body += 'pub fn value()->i32{return h6()}\n'
            texts.append(body)
            (source / f'{name}.zil').write_text(body)
        last = source / f'{names[-1]}.zil'
        base = texts[-1]
        unused = base + 'extern "C" fn unused_foreign(x:i32)->i32;\n'
        private = unused.replace(f'return {leaves}', f'return {leaves + 1}', 1)
        snapshots = [base, base, unused, private, base]
        expected = sum(range(1, leaves + 1))
        for block in range(5):
            order = ['baseline', 'candidate', 'candidate', 'baseline']
            if block % 2:
                order = ['candidate', 'baseline', 'baseline', 'candidate']
            for position, arm in enumerate(order):
                run = args.out / f'm{modules}-b{block}-p{position}-{arm}'
                run.mkdir()
                for request, snapshot in zip(manifest['requests'], snapshots):
                    last.write_text(snapshot)
                    command = [str(compilers[arm]), 'build', str(source / 'main.zil'),
                               *manifest['flags'], '--object-cache', str(run / 'cache'),
                               '-o', str(run / 'program'), '--json']
                    started = time.perf_counter_ns()
                    result = subprocess.run(command, capture_output=True, timeout=60)
                    elapsed = time.perf_counter_ns() - started
                    (run / f'{request}.json').write_bytes(result.stdout)
                    (run / f'{request}.stderr').write_bytes(result.stderr)
                    assert result.returncode == 0, (command, result.stderr.decode())
                    report = json.loads(result.stdout)
                    assert report['status'] == 'Success' and report['verification_runs'] == 1, report
                    native = subprocess.run([str(run / 'program')], capture_output=True, timeout=10)
                    wanted = expected + int(request == 'private')
                    assert native.returncode == 0 and native.stdout == f'{wanted}\n'.encode(), native
                    (run / f'{request}.native').write_bytes(native.stdout)
                    codegen = report['native_codegen']
                    cache = codegen['object_cache']
                    if request == 'unused':
                        assert cache['hits'] == (codegen['actual_units'] if arm == 'candidate' else 0), report
                    row = {'modules': modules, 'block': block, 'position': position, 'arm': arm,
                           'request': request, 'wall_ns': elapsed, 'command': command,
                           'source_sha256': digest(last), 'elf_sha256': digest(run / 'program'),
                           'hits': cache['hits'], 'misses': cache['misses'],
                           'units': codegen['actual_units'], 'verification_runs': report['verification_runs']}
                    rows.append(row)
                    with (args.out / 'rows.jsonl').open('a') as stream:
                        stream.write(json.dumps(row) + '\n')
    summary = []
    for modules in manifest['modules']:
        for request in manifest['requests']:
            cells = {}
            for arm in compilers:
                means = [statistics.mean(r['wall_ns'] for r in rows if r['modules'] == modules
                                         and r['request'] == request and r['arm'] == arm and r['block'] == b)
                         for b in range(5)]
                cells[arm] = statistics.median(means) / 1e6
            summary.append({'modules': modules, 'request': request, 'median_block_mean_ms': cells,
                            'candidate_over_baseline': cells['candidate'] / cells['baseline']})
    (args.out / 'summary.json').write_text(json.dumps({'builds': len(rows), 'native_checks': len(rows),
                                                    'cells': summary}, indent=2) + '\n')
    print(json.dumps({'builds': len(rows), 'cells': summary}, indent=2))


if __name__ == '__main__':
    main()
