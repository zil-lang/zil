#!/usr/bin/env python3
"""Exclusive paired original-clone/entry-preserving full-service screen; separate linkage contracts."""
import argparse
import hashlib
import json
import os
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stage6_bench import REQUESTS, generate, measured, shapes
from run_request import identity


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['zil', 'clang', 'opt', 'plugin', 'lld', 'pipeline-manifest', 'snapshot-receipt', 'out']:
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--blocks', type=int, default=2)
    parser.add_argument('--case', action='append')
    parser.add_argument('--closed', action='store_true')
    parser.add_argument('--incremental-o2', action='store_true')
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--functional-smoke', action='store_true')
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    names = args.case or ['large-128', 'large-512', 'large-2048', 'large-4096', 'large-8192',
                         'large-even-4x2048', 'large-skew-8', 'body-16x16', 'branch-64x16']
    cases = [case for case in shapes() if case['name'] in names]
    assert set(names) == {case['name'] for case in cases}
    if args.functional_smoke:
        assert len(cases) == 1 and args.blocks == 1
    for path in [Path(__file__), HERE/'run_request.py', HERE/'clang_wrapper.py', HERE/'cache_bundle.py', HERE.parent/'stage6_bench.py']:
        shutil.copy2(path, args.out/path.name)
    common = [sys.executable, str(HERE/'run_request.py')]
    for name in ['zil', 'clang', 'opt', 'plugin', 'lld', 'pipeline_manifest', 'snapshot_receipt']:
        common += ['--'+name.replace('_', '-'), str(getattr(args, name).absolute())]
    if args.closed:
        common += ['--closed']
    if args.incremental_o2:
        common += ['--incremental-o2']
    # No -time-passes/-zil-jt-audit or full -verify-each instrumentation in
    # this formal screen. Current source Verify, machine preflight and opt's
    # normal input/final verifier remain active in both arms.
    manifest = {'scope': __doc__, 'cases': cases, 'blocks': args.blocks, 'cpus': cpus,
                'functional_smoke_only': args.functional_smoke,
                'requests': REQUESTS, 'closed': args.closed, 'incremental_o2': args.incremental_o2,
                'manual_imports': False, 'common_command': common, 'diagnostics': False,
                'tools': {name: identity(getattr(args, name)) for name in ['zil', 'clang', 'opt', 'plugin', 'lld']},
                'snapshot_sha256': sha(args.snapshot_receipt), 'pipeline_manifest_sha256': sha(args.pipeline_manifest),
                'scripts': {name: sha(args.out/name) for name in ['screen.py', 'run_request.py', 'clang_wrapper.py', 'cache_bundle.py', 'stage6_bench.py']},
                'primary_time': 'service_elapsed_ns includes tool hashing, current-source compiler/Verify/preflight, serialization, opt, O2 machine emission, link, audit, transactional object-cache publication. Excludes native. Outer wall includes process launch and native. No API-vs-opt gain attribution.'}
    (args.out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    rows = []
    for case in cases:
        source = args.out/('source-'+case['name'])
        for block in range(args.blocks):
            arms = ['original', 'entry'] if block % 2 == 0 else ['entry', 'original']
            for position, arm in enumerate(arms):
                sequence = args.out/f'{case["name"]}-{block}-{position}-{arm}'
                sequence.mkdir()
                cache = sequence/'cache'
                for request, edit in REQUESTS:
                    expected = generate(case, source, edit).to_bytes(4, 'little').hex()
                    source_identity = {p.name: sha(p) for p in sorted(source.glob('*.zil'))}
                    (sequence/(request+'.source.json')).write_text(json.dumps(source_identity, indent=2)+'\n')
                    destination = sequence/request
                    command = common+['--source', str(source/'main.zil'), '--out', str(destination),
                                      '--cache', str(cache), '--arm', arm, '--expected-hex', expected,
                                      '--timeout', str(args.timeout-10)]
                    outer = measured(command, sequence, request, args.timeout)
                    row = {'case': case['name'], 'block': block, 'position': position, 'arm': arm,
                           'request': request, 'expected_hex': expected, 'command': command, 'outer': outer,
                           'source_identity_sha256': hashlib.sha256(json.dumps(source_identity, sort_keys=True).encode()).hexdigest(),
                           'valid': False}
                    try:
                        result = json.loads((destination/'result.json').read_text())
                        row.update(result=result, service_elapsed_ns=result['service_elapsed_ns'],
                                   driver_elapsed_ns=result['driver_elapsed_ns'], actual_units=result['current_preflight_units'],
                                   cache_hits=result['cache_hits'], cache_misses=result['cache_misses'])
                        row['valid'] = outer['exit'] == 0 and not outer['timeout'] and result['valid'] and result['verification_runs'] == 1
                        row['actual_keys'] = [json.loads(p.read_text())['action_key'] for p in sorted(destination.glob('unit-*.ll.input.json'))]
                        if row['valid']:
                            executable = destination/'program'
                            row['executable_bytes'] = executable.stat().st_size
                            size = subprocess.check_output(['size', '--format=berkeley', str(executable)], text=True)
                            (destination/'size.txt').write_text(size)
                            row['text_bytes'], row['data_bytes'], row['bss_bytes'] = map(int, size.splitlines()[-1].split()[:3])
                            symbols = subprocess.check_output(['nm', '-D', '--defined-only', str(executable)], text=True)
                            (destination/'dynamic-symbols.txt').write_text(symbols)
                            exports = set((destination/'units/exports.txt').read_text().splitlines())
                            names_present = {line.split()[-1] for line in symbols.splitlines() if line.split()}
                            row['export_roots_preserved'] = exports <= names_present
                            row['valid'] &= row['export_roots_preserved']
                    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
                        row['validation_error'] = str(error)
                    rows.append(row)
                    with (args.out/'rows.jsonl').open('a') as stream:
                        stream.write(json.dumps(row)+'\n')
                    if not row['valid']:
                        print('FAIL', case['name'], block, arm, request, flush=True)
                print(case['name'], block+1, '/', args.blocks, arm, flush=True)
    cells = []
    for case in cases:
        for request, _ in REQUESTS:
            arms = {}
            for arm in ['original', 'entry']:
                selected = [r for r in rows if r['case'] == case['name'] and r['request'] == request and r['arm'] == arm]
                valid = [r for r in selected if r['valid']]
                arms[arm] = {'failures': len(selected)-len(valid),
                    'service_ms': [r['service_elapsed_ns']/1e6 for r in valid],
                    'driver_ms': [r['driver_elapsed_ns']/1e6 for r in valid],
                    'outer_wall_ms': [r['outer']['elapsed_ns']/1e6 for r in valid],
                    'text_bytes': [r['text_bytes'] for r in valid], 'executable_bytes': [r['executable_bytes'] for r in valid],
                    'actual_units': [r['actual_units'] for r in valid], 'cache_hits': [r['cache_hits'] for r in valid]}
            ratio = statistics.median(arms['entry']['service_ms'])/statistics.median(arms['original']['service_ms']) if all(not x['failures'] and x['service_ms'] for x in arms.values()) else None
            cells.append({'case': case['name'], 'request': request, 'arms': arms, 'entry_over_original_service': ratio})
    (args.out/'summary.json').write_text(json.dumps({'rows': len(rows), 'failures': sum(not r['valid'] for r in rows), 'cells': cells}, indent=2)+'\n')
    print('complete', len(rows), flush=True)


if __name__ == '__main__':
    main()
