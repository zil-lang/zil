#!/usr/bin/env python3
"""Matched actual-CGU plugin correctness/quality diagnostics on retained opaque sources."""
import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent/'pass_timing'))
from run_probe import parse_tables


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage', type=Path, required=True)
    parser.add_argument('--llvm', type=Path, required=True)
    parser.add_argument('--zil', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    runner = Path(__file__).with_name('run_request.py')
    common = [sys.executable, str(runner), '--zil', str(args.zil),
              '--clang', str(args.llvm/'bin/clang'), '--opt', str(args.llvm/'bin/opt'),
              '--plugin', str(args.stage/'jump-threading-plugin-build/research-jump-threading.so'),
              '--lld', str(args.llvm/'bin/ld.lld'),
              '--pipeline-manifest', str(args.stage/'jump-threading-pipelines/manifest.json'),
              '--snapshot-receipt', str(args.stage/'jump-threading-capture-snapshot/RESEARCH.json'),
              '--cache', str(args.out/'cache'), '--verify-analyses', '--diagnostics', '--timeout', '180']
    rows = []
    cases = [(512, 'arithmetic', False), (512, 'arithmetic', True),
             (512, 'branch', False), (512, 'branch', True),
             (2048, 'branch', False), (8192, 'arithmetic', False)]
    (args.out/'plan.json').write_text(json.dumps({'scope': __doc__, 'common_command': common,
        'cases': cases, 'arms': ['original', 'entry'], 'native_seeds': [0, 1, 7, 42, 255],
        'note': 'Diagnostics only; no formal latency or throughput conclusion.'}, indent=2)+'\n')
    for helpers, shape, closed in cases:
        source = args.stage/'slp-effective-opaque'/f'{helpers}-{shape}'
        previous = json.loads((source/'closed0-slp-on.row.json').read_text())
        expected = {x['seed']: x['expected_hex'] for x in previous['native_controls']}
        for arm in ['original', 'entry']:
            name = f'{helpers}-{shape}-closed{int(closed)}-{arm}'
            dest = args.out/name
            command = common+['--source', str(source/'main.zil'), '--native-input', str(source/'input.bin'),
                              '--expected-hex', expected[7], '--arm', arm, '--out', str(dest)]
            if closed:
                command += ['--closed']
            with (args.out/(name+'.log')).open('wb') as log:
                request = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=200)
            result = json.loads((dest/'result.json').read_text())
            row = {'case': [helpers, shape, closed], 'arm': arm, 'command': command,
                   'request_exit': request.returncode, 'result': result, 'native_controls': [],
                   'valid': request.returncode == 0 and result['valid']}
            if row['valid']:
                tables = [entry for p in dest.glob('unit-*.ll.opt.stderr') for entry in parse_tables(p.read_text())]
                row['pass_cpu_seconds'] = {name: sum(x['cpu_seconds'] or 0 for x in tables if x['name'] == name)
                    for name in ['ZilResearchJumpThreadingPass', 'InstCombinePass', 'IPSCCPPass', 'SLPVectorizerPass']}
                row['object_bytes'] = sum(p.stat().st_size for p in dest.glob('unit-*.ll.o'))
                for seed, want in expected.items():
                    start = time.perf_counter_ns()
                    native = subprocess.run([str(dest/'program')], input=bytes([seed]), capture_output=True, timeout=60)
                    control = {'seed': seed, 'exit': native.returncode, 'expected_hex': want,
                               'stdout_hex': native.stdout.hex(), 'stderr_hex': native.stderr.hex(),
                               'diagnostic_wall_ns': time.perf_counter_ns()-start}
                    control['valid'] = native.returncode == 0 and not native.stderr and native.stdout.hex() == want
                    row['native_controls'].append(control)
                    row['valid'] &= control['valid']
                row['native_5seed_median_diagnostic_ns'] = statistics.median(x['diagnostic_wall_ns'] for x in row['native_controls'])
            rows.append(row)
            (dest/'opaque-control.json').write_text(json.dumps(row, indent=2)+'\n')
            (args.out/'summary.json').write_text(json.dumps({'functional_only': True, 'rows': rows,
                'valid': all(x['valid'] for x in rows), 'planned': 12, 'completed': len(rows)}, indent=2)+'\n')
            print(name, row['valid'], flush=True)
            if not row['valid']:
                raise SystemExit(1)


if __name__ == '__main__':
    main()
