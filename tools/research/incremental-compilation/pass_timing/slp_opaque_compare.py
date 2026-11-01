#!/usr/bin/env python3
"""Functional actual-API SLP on/off comparison; timings are not exclusive performance evidence."""
import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

from build_probe import identity

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from stage6_runtime import generate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--zil', type=Path, required=True)
    parser.add_argument('--probe-manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    rounds = 1000
    runner = Path(__file__).with_name('run_probe.py')
    (args.out/'manifest.json').write_text(json.dumps({
        'scope': __doc__, 'compiler': identity(args.zil), 'tool': identity(Path(__file__)),
        'runner': identity(runner), 'generator': identity(Path(__file__).resolve().parent.parent/'stage6_runtime.py'),
        'probe_manifest': json.loads(args.probe_manifest.read_text()),
        'cases': [[n, shape] for n in [512, 2048, 8192] for shape in ['arithmetic', 'branch']],
        'rounds': rounds, 'merge_policy': 'after', 'seeds': [0, 1, 7, 42, 255],
    }, indent=2)+'\n')
    rows = []
    for n in [512, 2048, 8192]:
        for shape in ['arithmetic', 'branch']:
            source = args.out/f'{n}-{shape}'
            source.mkdir()
            generate(source, n, shape, rounds)
            expected = {}
            for seed in [0, 1, 7, 42, 255]:
                total, x = 0, seed
                for _ in range(rounds):
                    counts = [(n//97 + int(i < n % 97)) for i in range(97)]
                    if shape == 'branch' and x % 3 != 0:
                        value = sum((n//31 + int(i < n % 31)) * ((x+i) % 1009) for i in range(31))
                    else:
                        value = sum(counts[i] * ((x*3+i) % 1009) for i in range(97))
                    total = (total + value) % 1000003
                    x = (x+17) % 1009
                expected[seed] = total.to_bytes(4, 'little').hex()
            (source/'input.bin').write_bytes(b'\x07')
            for closed in [False, True]:
                for slp in ['on', 'off']:
                    dest = source/f'closed{int(closed)}-slp-{slp}'
                    command = [sys.executable, str(runner), '--zil', str(args.zil.resolve()),
                               '--probe-manifest', str(args.probe_manifest.resolve()), '--source', str(source/'main.zil'),
                               '--out', str(dest), '--merge', 'after', '--slp', slp,
                               '--native-input', str(source/'input.bin'), '--expected-hex', expected[7], '--timeout', '180']
                    if closed:
                        command += ['--closed']
                    run = subprocess.run(command, capture_output=True, timeout=200)
                    (source/f'{dest.name}.stdout').write_bytes(run.stdout)
                    (source/f'{dest.name}.stderr').write_bytes(run.stderr)
                    row = {'helpers': n, 'shape': shape, 'closed': closed, 'slp': slp,
                           'command': command, 'runner_exit': run.returncode, 'valid': False, 'native_controls': []}
                    result_file = dest/'result.json'
                    if result_file.is_file():
                        result = json.loads(result_file.read_text())
                        row['result'] = result
                        row['valid'] = run.returncode == 0 and result['valid']
                        if row['valid']:
                            tables = json.loads((dest/'pass-tables.json').read_text())
                            row['pass_cpu_seconds'] = {name: sum(t['cpu_seconds'] or 0 for t in tables if t['name'] == name)
                                                       for name in ['SLPVectorizerPass', 'JumpThreadingPass', 'InstCombinePass', 'IPSCCPPass']}
                            for seed, want in expected.items():
                                start = time.perf_counter_ns()
                                native = subprocess.run([str(dest/'program')], input=bytes([seed]), capture_output=True, timeout=60)
                                item = {'seed': seed, 'exit': native.returncode, 'stdout_hex': native.stdout.hex(),
                                        'stderr_hex': native.stderr.hex(), 'expected_hex': want,
                                        'diagnostic_wall_ns': time.perf_counter_ns()-start}
                                item['valid'] = native.returncode == 0 and not native.stderr and native.stdout.hex() == want
                                row['native_controls'].append(item)
                                row['valid'] &= item['valid']
                    rows.append(row)
                    (source/f'{dest.name}.row.json').write_text(json.dumps(row, indent=2)+'\n')
                    (args.out/'summary.json').write_text(json.dumps({'functional_only': True, 'rows': rows,
                        'valid': all(r['valid'] for r in rows), 'planned': 24, 'completed': len(rows)}, indent=2)+'\n')
                    print(n, shape, closed, slp, row['valid'], flush=True)
    if not all(row['valid'] for row in rows):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
