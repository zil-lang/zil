#!/usr/bin/env python3
"""Author C API differential for the composed four-TU LLVM candidate."""
import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def identity(path):
    return {'path': str(path.resolve()), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--review-root', type=Path, required=True)
    parser.add_argument('--control-root', type=Path, required=True)
    parser.add_argument('--llvm-prefix', type=Path, required=True)
    parser.add_argument('--original', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    child = args.review_root / 'c_api_child.py'
    plan = json.loads((args.review_root / 'c-api/plan.json').read_text())
    # O0 controls establish that the O2-only insertion leaves O0 unchanged.
    plan += [dict(case, name=case['name'].removesuffix('-o2')+'-o0', pipeline='default<O0>')
             for case in plan if case['name'].endswith('-o2')]
    plan_path = args.out / 'plan.json'
    plan_path.write_text(json.dumps(plan, indent=2) + '\n')
    result = {'status': 'Running', 'scope': __doc__, 'child': identity(child),
              'author_reuse_boundary': 'Reuses the frozen independent C API child and input fixtures as author tests; this is not a new independent review.',
              'producer': identity(Path(__file__)), 'inputs': {case['name']: identity(Path(case['source'])) for case in plan},
              'arms': [], 'native': [], 'comparisons': []}

    def save():
        (args.out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')

    def run(label, command, environment=None):
        record = {'label': label, 'command': [str(x) for x in command]}
        result.setdefault('commands', []).append(record)
        save()
        try:
            execution = subprocess.run(record['command'], env=environment, stdin=subprocess.DEVNULL,
                                       capture_output=True, timeout=90)
            (args.out / (label + '.stdout')).write_bytes(execution.stdout)
            (args.out / (label + '.stderr')).write_bytes(execution.stderr)
            record['exit'] = execution.returncode
            assert execution.returncode == 0, label
            return execution
        except Exception as error:
            record['error'] = repr(error)
            raise
        finally:
            save()

    save()
    try:
        for arm, library in [('installed', args.llvm_prefix/'lib/libLLVM.so.23.1'),
                             ('original', args.original), ('candidate', args.candidate)]:
            environment = os.environ.copy()
            for name in ['LD_PRELOAD', 'LD_AUDIT', 'LD_DEBUG', 'LD_DEBUG_OUTPUT']:
                environment.pop(name, None)
            environment['LD_LIBRARY_PATH'] = str(library.parent)
            destination = args.out / arm
            run(arm+'-api', [sys.executable, child, library, plan_path, destination], environment)
            receipt = json.loads((destination/'result.json').read_text())
            assert receipt['status'] == 'Pass' and receipt['library_sha256'] == identity(library)['sha256']
            result['arms'].append({'arm': arm, 'library': identity(library),
                                    'receipt': identity(destination/'result.json')})
            for mode in ['jt', 'o2', 'o0']:
                for name in ['generated', 'nonreturn_assume']:
                    harness = (args.control_root/'differential/main.c' if name == 'generated'
                               else args.control_root/'boundaries/main.c')
                    executable = destination/(name+'-'+mode)
                    run(arm+'-'+name+'-'+mode+'-link', [args.llvm_prefix/'bin/clang', '-O0', harness,
                        destination/(name+'-'+mode+'.o'), '-o', executable])
                    for value in ([None] if name == 'generated' else [0, 1, 63, 127, 128, 129, 254, 255]):
                        label = arm+'-'+name+'-'+mode+'-'+str(value)
                        execution = run(label, [executable] + ([] if value is None else [str(value)]))
                        expected = ((args.control_root/'differential/expected.bin').read_bytes() if value is None
                                    else (11).to_bytes(4,'little')+(111).to_bytes(4,'little') if value < 128
                                    else (22).to_bytes(4,'little'))
                        assert execution.stdout == expected and not execution.stderr, label
                        result['native'].append({'case': label, 'valid': True,
                            'expected_sha256': hashlib.sha256(expected).hexdigest(),
                            'actual_sha256': hashlib.sha256(execution.stdout).hexdigest()})
        for case in plan:
            name = case['name']+'.ll'
            installed = (args.out/'installed'/name).read_bytes()
            original = (args.out/'original'/name).read_bytes()
            candidate = (args.out/'candidate'/name).read_bytes()
            assert installed == original, ('same-flags-original-changed', name)
            if case['pipeline'] == 'default<O0>':
                assert candidate == original, ('non-O2-changed', name)
            result['comparisons'].append({'case': case['name'], 'installed_original_equal': True,
                                          'candidate_original_equal': candidate == original})
        result['status'] = 'Pass'
        result['ir_requests'] = len(plan) * 3
    except Exception as error:
        result['status'] = 'Fail'
        result['error'] = repr(error)
        raise
    finally:
        save()
        print(json.dumps({'status': result['status'], 'ir_requests': result.get('ir_requests'),
                          'native': len(result['native']), 'error': result.get('error')}))


if __name__ == '__main__':
    main()
