#!/usr/bin/env python3
"""Validate actual legal native products against the frozen source oracle.

This reads only existing products; it never compiles or executes a program.
Negative source results and whole-task acceptance are separate.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[3]
COUNTERS = ['alloc_calls', 'allocated', 'frees', 'failures', 'live', 'peak_live']


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()


def git(*arguments):
    return subprocess.check_output(['git', *arguments], cwd=ROOT)


def collect(args):
    commit = git('rev-parse', args.source_commit).decode().strip()
    pinned_oracle = git('show', commit + ':tests/impl262/ORACLE.json')
    oracle = json.loads(pinned_oracle)
    cases = {case['name']: case for case in oracle['positive']}
    assert len(cases) == 9
    cells = Counter()
    compiles = []
    executions = []
    for group in ['native', 'allocator']:
        allocator = group == 'allocator'
        for path in sorted((args.run / group).glob('*/report.json')):
            directory = path.parent
            report = json.loads(path.read_text())
            request = json.loads((directory / 'project-input.json').read_text())
            case = cases[request['case']]
            variant = request['variant']
            assert variant in ['generated', 'handwritten']
            required = {module['logical']: module for module in case[variant + '_modules']}
            assert set(required) == {module['logical'] for module in request['modules']}
            assert report['source_files'] == {str(module['module_id']): module['path']
                                              for module in request['modules']}
            for module in request['modules']:
                expected = required[module['logical']]
                pinned = git('show', commit + ':tests/impl262/' + expected['source'])
                assert hashlib.sha256(pinned).hexdigest() == expected['sha256'] == module['source_sha256']
                assert Path(module['path']).read_bytes() == pinned
            lock = case['syntax_lock'] if variant == 'generated' else None
            assert bool(request['syntax_lock']) == bool(lock)
            if lock:
                pinned = git('show', commit + ':tests/impl262/locks/' + lock)
                assert Path(request['syntax_lock']).read_bytes() == pinned
                assert hashlib.sha256(pinned).hexdigest() == request['syntax_lock_sha256']
            assert report['status'] == 'Success' and report['phase'] == 'link'
            assert report['verification_runs'] == 1
            codegen = report['native_codegen']
            assert codegen['backend'] == 'llvm-api' and codegen['fallback'] is None
            assert codegen['actual_units'] > 0 and len(codegen['object_reports']) == codegen['actual_units']
            opts = {arg for command in report['commands'] for arg in command if arg in ['-O0', '-O2']}
            assert len(opts) == 1
            optimization = next(iter(opts))[1:]
            assert all(obj['input_kind'] == 'builder' and obj['verify_before_after'] is True
                       and obj['pipeline'] == 'default<' + optimization + '>'
                       for obj in codegen['object_reports'])
            key = (case['name'], variant, optimization, group)
            cells[key] += 1
            artifacts = {}
            for name, original in report['artifacts'].items():
                artifact = Path(original)
                assert artifact.resolve().is_relative_to(directory.resolve())
                assert artifact.is_file()
                artifacts[name] = dict(path=str(artifact.relative_to(directory)),
                    sha256=digest(artifact), bytes=artifact.stat().st_size,
                    mode=stat.S_IMODE(artifact.stat().st_mode))
            assert artifacts['executable']['mode'] & 0o111
            assert (directory / 'program.ll').is_file()
            if allocator:
                assert codegen['actual_units'] == 1
                for name in ['api.o', 'probe', 'capture-object.sh', 'link.stdout', 'link.stderr']:
                    assert (directory / name).is_file()
                assert (directory / 'probe').stat().st_mode & 0o111
                rows = [('input--fail-' + str(row['fail_at']), row) for row in case['allocator']]
            else:
                rows = [('case-0', case['normal'])]
            assert {str(p.relative_to(directory)) for p in directory.rglob('expected.json')} == {
                name + '/expected.json' for name, _ in rows}
            for name, expected in rows:
                run = directory / name
                assert json.loads((run / 'expected.json').read_text()) == expected
                assert (run / 'stdin').read_bytes() == bytes.fromhex(case['input_hex'])
                match = re.fullmatch(r'Some\((-?\d+)\)\s*', (run / 'exit.txt').read_text())
                assert match is not None
                actual = dict(exit=int(match[1]), stdout=(run / 'stdout').read_text())
                assert actual['exit'] == expected['exit'] and actual['stdout'] == expected['stdout']
                stderr = (run / 'stderr').read_text()
                if allocator:
                    actual.update(json.loads(stderr))
                    assert all(type(actual[k]) is int and actual[k] == expected[k] for k in COUNTERS)
                    assert actual['live'] == 0 and actual['allocated'] == actual['frees']
                else:
                    assert not stderr
                executions.append(dict(compile=directory.name, case=name, source_case=case['name'],
                    variant=variant, optimization=optimization, allocator=allocator,
                    stdin_hex=case['input_hex'], expected=expected, actual=actual,
                    native_harness_assertions=9 if allocator else 3))
            compiles.append(dict(id=directory.name, group=group, case=case['name'],
                variant=variant, optimization=optimization, source_request=request,
                report_sha256=digest(path), verification_runs=report['verification_runs'],
                native_codegen=codegen, artifacts=artifacts))
    expected_cells = Counter({(name, variant, opt, 'native'): 1
        for name in cases for variant in ['generated', 'handwritten'] for opt in ['O0', 'O2']})
    expected_cells.update({(name, variant, opt, 'allocator'): 1
        for name in cases if cases[name]['allocator']
        for variant in ['generated', 'handwritten'] for opt in ['O0', 'O2']})
    assert cells == expected_cells
    assert len(compiles) == 44 and len(executions) == 56
    allocation_runs = [row for row in executions if row['allocator']]
    assert len(allocation_runs) == 20 and sum(row['actual']['failures'] for row in allocation_runs) == 4
    return dict(verdict='Pass legal native group only; source owner rejection and final candidate separate',
        compiler_commit=commit, collector_commit=git('rev-parse', 'HEAD').decode().strip(),
        oracle_sha256=hashlib.sha256(pinned_oracle).hexdigest(), native_compiles=len(compiles),
        independent_runtime_verifications=sum(row['verification_runs'] for row in compiles),
        native_runs=len(executions), normal_runs=36, allocator_runs=len(allocation_runs),
        actual_allocation_failures=4, all_allocator_live_zero=True,
        native_harness_assertions=sum(row['native_harness_assertions'] for row in executions),
        generated_and_handwritten_grouping={variant: dict(
            normal=sum(not row['allocator'] and row['variant'] == variant for row in executions),
            allocator=sum(row['allocator'] and row['variant'] == variant for row in executions))
            for variant in ['generated', 'handwritten']},
        historical_or_other_candidate_runs_added=False, additional_compiler_or_native_execution=False,
        compiles=compiles, executions=executions)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not args.out.exists()
    receipt = collect(args)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({key: receipt[key] for key in ['verdict', 'compiler_commit', 'native_compiles',
        'native_runs', 'allocator_runs', 'actual_allocation_failures', 'native_harness_assertions']}))
