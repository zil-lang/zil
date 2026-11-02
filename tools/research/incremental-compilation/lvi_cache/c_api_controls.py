#!/usr/bin/env python3
"""Exercise real LLVM C API pipelines in fresh original/indexed DSO children."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def identity(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'bytes': path.stat().st_size}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--original-receipt', type=Path, required=True)
    parser.add_argument('--indexed-receipt', type=Path, required=True)
    parser.add_argument('--review-inputs', type=Path, required=True)
    parser.add_argument('--llvm-bin', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    child = Path(__file__).with_name('c_api_child.py')
    copied = args.out / 'inputs'
    copied.mkdir()
    names = ['nonreturn_assume', 'guard', 'eh', 'convergence',
             'address_indirect', 'profile_switch']
    for name in names:
        shutil.copyfile(args.review_inputs / 'boundaries' / (name + '.ll'),
                        copied / (name + '.ll'))
    for name in ['control.ll', 'main.c', 'expected.bin']:
        shutil.copyfile(args.review_inputs / 'differential' / name,
                        copied / ('generated-' + name))
    shutil.copyfile(args.review_inputs / 'boundaries' / 'main.c',
                    copied / 'nonreturn-main.c')
    plan = []
    for name in ['generated'] + names:
        source = copied / ('generated-control.ll' if name == 'generated' else name + '.ll')
        for mode, pipeline in [
            ('jt', 'function(jump-threading,verify<domtree>,jump-threading,verify<domtree>,verify<loops>)'),
            ('o2', 'default<O2>'),
        ]:
            plan.append({'name': name + '-' + mode, 'source': str(source),
                         'pipeline': pipeline, 'emit': name in ['generated', 'nonreturn_assume']})
    planpath = args.out / 'plan.json'
    planpath.write_text(json.dumps(plan, indent=2) + '\n')
    receipts = {'original': args.original_receipt, 'indexed': args.indexed_receipt}
    manifest = {'scope': __doc__, 'runner': identity(Path(__file__)),
                'child': identity(child), 'inputs': [identity(p) for p in sorted(copied.iterdir())],
                'clang': identity(args.llvm_bin / 'clang'), 'recipes': {}}
    for arm, receipt in receipts.items():
        data = json.loads(receipt.read_text())
        assert data['status'] == 'Pass'
        library = Path(data['library']['path'])
        assert identity(library)['sha256'] == data['library']['sha256']
        manifest['recipes'][arm] = {'receipt': identity(receipt), 'library': data['library']}
    (args.out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    rows = []
    for arm in receipts:
        library = Path(manifest['recipes'][arm]['library']['path'])
        for per_pred in [False, True]:
            label = arm + '-perpred-' + str(per_pred).lower()
            dest = args.out / label
            command = [sys.executable, str(child), str(library), str(planpath),
                       str(dest), str(per_pred).lower()]
            env = os.environ.copy()
            env.pop('LD_PRELOAD', None)
            env.pop('LD_AUDIT', None)
            env['LD_LIBRARY_PATH'] = str(library.parent)
            run = subprocess.run(command, env=env, capture_output=True, timeout=90)
            (args.out / (label + '.stdout')).write_bytes(run.stdout)
            (args.out / (label + '.stderr')).write_bytes(run.stderr)
            assert run.returncode == 0, (label, run.stderr.decode(errors='replace'))
            result = json.loads((dest / 'result.json').read_text())
            assert result['library_sha256'] == manifest['recipes'][arm]['library']['sha256']
            row = {'arm': arm, 'per_pred': per_pred, 'command': command,
                   'api_requests': len(result['rows']), 'native': []}
            rows.append(row)
            for name, harness in [('generated', copied / 'generated-main.c'),
                                  ('nonreturn_assume', copied / 'nonreturn-main.c')]:
                for mode in ['jt', 'o2']:
                    case = name + '-' + mode
                    exe = dest / case
                    command = [str(args.llvm_bin / 'clang'), '-O0', str(harness),
                               str(dest / (case + '.o')), '-o', str(exe)]
                    link = subprocess.run(command, capture_output=True, timeout=20)
                    (dest / (case + '.link.json')).write_text(json.dumps(command) + '\n')
                    (dest / (case + '.link.stderr')).write_bytes(link.stderr)
                    assert link.returncode == 0, (case, link.stderr)
                    seeds = [None] if name == 'generated' else [0, 1, 63, 127, 128, 129, 254, 255]
                    for seed in seeds:
                        command = [str(exe)] + ([] if seed is None else [str(seed)])
                        native = subprocess.run(command, capture_output=True, timeout=5)
                        if seed is None:
                            expected = (copied / 'generated-expected.bin').read_bytes()
                        elif seed < 128:
                            expected = (11).to_bytes(4, 'little') + (111).to_bytes(4, 'little')
                        else:
                            expected = (22).to_bytes(4, 'little')
                        log = case + '.native.' + str(seed)
                        (dest / (log + '.stdout')).write_bytes(native.stdout)
                        (dest / (log + '.stderr')).write_bytes(native.stderr)
                        valid = native.returncode == 0 and not native.stderr and native.stdout == expected
                        row['native'].append({'case': case, 'seed': seed, 'command': command,
                                              'exit': native.returncode, 'valid': valid,
                                              'stdout_sha256': hashlib.sha256(native.stdout).hexdigest(),
                                              'expected_sha256': hashlib.sha256(expected).hexdigest()})
                        assert valid, (label, case, seed)
    comparisons = []
    for per_pred in [False, True]:
        for case in plan:
            name = case['name'] + '.ll'
            paths = [args.out / (arm + '-perpred-' + str(per_pred).lower()) / name
                     for arm in ['original', 'indexed']]
            equal = paths[0].read_bytes() == paths[1].read_bytes()
            comparisons.append({'case': case['name'], 'per_pred': per_pred,
                                'original_indexed_ir_equal': equal})
            assert equal, (case['name'], per_pred, 'cache bookkeeping changed IR')
    result = {'status': 'Pass', 'ir_api_requests': sum(r['api_requests'] for r in rows),
              'native_invocations': sum(len(r['native']) for r in rows),
              'generated_full_u32_results': 65 * 256 * 2 * len(rows),
              'rows': rows, 'comparisons': comparisons}
    (args.out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ['rows', 'comparisons']}))


if __name__ == '__main__':
    main()
