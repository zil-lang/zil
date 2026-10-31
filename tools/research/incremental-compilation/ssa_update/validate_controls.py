#!/usr/bin/env python3
"""Run the independent review's frozen C API plan and full native oracle on a DSO."""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--library', type=Path, required=True)
    p.add_argument('--child', type=Path, required=True)
    p.add_argument('--plan', type=Path, required=True)
    p.add_argument('--oracle-root', type=Path, required=True)
    p.add_argument('--original-ir', type=Path, required=True)
    p.add_argument('--clang', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(a.child, a.out / 'c_api_child.py')
    shutil.copyfile(a.plan, a.out / 'plan.json')
    env = dict(os.environ)
    for key in ['LD_PRELOAD', 'LD_AUDIT']:
        env.pop(key, None)
    env['LD_LIBRARY_PATH'] = str(a.library.resolve().parent)
    argv = ['python3', str(a.out / 'c_api_child.py'), str(a.library.resolve()),
            str(a.out / 'plan.json'), str(a.out / 'candidate')]
    result = subprocess.run(argv, env=env, capture_output=True, timeout=180)
    (a.out / 'stdout.txt').write_bytes(result.stdout)
    (a.out / 'stderr.txt').write_bytes(result.stderr)
    (a.out / 'execution.json').write_text(json.dumps({
        'command': argv, 'returncode': result.returncode,
        'child_sha256': sha(a.child.read_bytes()), 'plan_sha256': sha(a.plan.read_bytes()),
        'clang_sha256': sha(a.clang.read_bytes())}, indent=2) + '\n')
    assert result.returncode == 0, 'C API controls failed; raw output retained'
    rows = []
    for name in ['generated-jt', 'generated-o2', 'nonreturn_assume-jt', 'nonreturn_assume-o2']:
        generated = name.startswith('generated')
        harness = a.oracle_root / ('differential/main.c' if generated else 'boundaries/main.c')
        exe = a.out / name
        command = [str(a.clang), '-O0', str(harness), str(a.out / 'candidate' / (name + '.o')), '-o', str(exe)]
        linked = subprocess.run(command, capture_output=True, timeout=30)
        (a.out / (name + '.link-stderr')).write_bytes(linked.stderr)
        assert linked.returncode == 0, command
        for x in ([None] if generated else [0, 1, 63, 127, 128, 129, 254, 255]):
            expected = (a.oracle_root / 'differential/expected.bin').read_bytes() if generated else (
                (11).to_bytes(4, 'little') + (111).to_bytes(4, 'little') if x < 128 else (22).to_bytes(4, 'little'))
            native = subprocess.run([str(exe)] + ([] if x is None else [str(x)]), capture_output=True, timeout=10)
            valid = native.returncode == 0 and native.stdout == expected
            rows.append({'case': name, 'input': x, 'exit': native.returncode, 'valid': valid,
                         'stdout_sha256': sha(native.stdout), 'expected_sha256': sha(expected),
                         'harness_sha256': sha(harness.read_bytes())})
            assert valid, rows[-1]
    comparisons = [{'case': ir.stem, 'candidate_original_ir_equal': ir.read_bytes() == (a.original_ir / ir.name).read_bytes()}
                   for ir in sorted((a.out / 'candidate').glob('*.ll'))]
    report = {'status': 'Pass', 'native': rows, 'comparisons': comparisons,
              'full_u32_results': 2 * len((a.oracle_root / 'differential/expected.bin').read_bytes()) // 4,
              'scope': 'Author reruns independent review fixtures; not a new independent review.'}
    (a.out / 'native-result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'status': 'Pass', 'native_invocations': len(rows),
                      'ir_equal': sum(x['candidate_original_ir_equal'] for x in comparisons),
                      'ir_cases': len(comparisons), 'full_u32_results': report['full_u32_results']}))


if __name__ == '__main__':
    main()
