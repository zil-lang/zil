#!/usr/bin/env python3
"""Two baseline functional builds only; no performance samples or comparisons."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess

ROOT = Path(__file__).resolve().parent
BASELINE = Path('/workspace/scratch/abf690ce880c/targets/baseline/release/zil')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def main():
    os.sched_setaffinity(0, {0, 1})
    plans = []
    for n in [64, 256]:
        source = ROOT / f'n{n}' / 'base'
        out = ROOT / f'build-{n}'
        out.mkdir(exist_ok=False)
        before = json.loads((ROOT.parent / 'final-inc-after-v1' / f'n{n}' / 'base' / 'fixture.json').read_text())
        fixture = json.loads((source / 'fixture.json').read_text())
        changed = [name for name, digest in fixture['source_hashes'].items()
                   if before['source_hashes'][name] != digest]
        assert changed == ['main.zil'], changed
        assert fixture['native_vectors'] == before['native_vectors']
        command = [str(BASELINE), 'build', str(source / 'main.zil'), '--json', '--backend', 'llvm-api',
                   '--opt', 'O2', '--codegen-units', '8', '--codegen-jobs', '2', '--incremental-o2',
                   '--function-merging', 'after', '-o', str(out / 'program')]
        plans.append({'n': n, 'source': str(source), 'out': str(out), 'command': command,
                      'changed_since_v1': changed, 'source_hashes': fixture['source_hashes'],
                      'native_vectors': fixture['native_vectors']})
    dump(ROOT / 'protocol.json', {'status': 'functional-only', 'performance_samples': 0,
                                 'baseline': {'path': str(BASELINE), 'sha256': sha(BASELINE)},
                                 'cpu_affinity': [0, 1], 'builds': plans})
    records = []
    for plan in plans:
        out = Path(plan['out'])
        process = subprocess.Popen(plan['command'], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=180)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        (out / 'report.json').write_bytes(stdout)
        (out / 'stderr').write_bytes(stderr)
        report = json.loads(stdout)
        record = {'n': plan['n'], 'exit': process.returncode, 'timeout': timed_out,
                  'performance_sample': False, 'status': report.get('status'),
                  'verification_runs': report.get('verification_runs'),
                  'source_files': len(report.get('source_files', {})),
                  'native_codegen': report.get('native_codegen'), 'native': []}
        records.append(record)
        dump(ROOT / 'result.json', {'status': 'running', 'performance_samples': 0, 'builds': records})
        assert process.returncode == 0 and not timed_out and report['status'] == 'Success', report
        assert report['verification_runs'] == 1 and record['source_files'] == plan['n']
        codegen = report['native_codegen']
        assert codegen['backend'] == 'llvm-api' and codegen['actual_units'] == len(codegen['object_reports'])
        assert codegen['actual_units'] > 0
        for receipt in codegen['object_reports']:
            assert receipt['llvm_version'] == '23.1.4', receipt
            assert receipt['verify_before_after'] is True and receipt['preflight_ns'] > 0, receipt
            assert receipt['pipeline'].startswith('default<O2>'), receipt
        for vector in plan['native_vectors']:
            native = subprocess.run([str(out / 'program')], input=bytes.fromhex(vector['stdin_hex']),
                                    capture_output=True, timeout=10)
            record['native'].append({'stdin_hex': vector['stdin_hex'], 'exit': native.returncode,
                                     'stdout_hex': native.stdout.hex(), 'stderr_hex': native.stderr.hex(),
                                     'expected_stdout_hex': vector['stdout_hex']})
            assert native.returncode == 0 and not native.stderr and native.stdout.hex() == vector['stdout_hex']
        record['valid'] = True
        dump(ROOT / 'result.json', {'status': 'running', 'performance_samples': 0, 'builds': records})
        print(json.dumps({'n': plan['n'], 'status': 'functional-pass',
                          'pipelines': sorted({r['pipeline'] for r in codegen['object_reports']}),
                          'native_checks': len(record['native'])}), flush=True)
    dump(ROOT / 'result.json', {'status': 'functional-pass', 'performance_samples': 0, 'builds': records})


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        dump(ROOT / 'failure.json', {'status': 'failed', 'performance_samples': 0,
                                    'type': type(error).__name__, 'message': str(error)})
        raise
