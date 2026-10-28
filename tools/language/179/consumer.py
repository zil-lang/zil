#!/usr/bin/env python3
"""Actual sibling-field windows, runtime input, and each allocation failure."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--repo', type=Path, required=True)
p.add_argument('--cli', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('--source-commit')
p.add_argument('--compiler-commit')
a = p.parse_args()
repo, cli, out = a.repo.resolve(), a.cli.resolve(), a.out.resolve()
out.mkdir(parents=True, exist_ok=False)
# Reuse only mechanical command capture and hashing, not 177's scenario oracle.
helpers = runpy.run_path(str(repo/'tools/language/177/consumer.py'))
command, digest, save = [helpers[k] for k in ('command', 'digest', 'save')]
commit = a.source_commit or subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
sources = repo/'examples/mainline/field_views179'
(out/'sources').mkdir()
for source in sources.glob('*.zil'):
    (out/'sources'/source.name).write_bytes(source.read_bytes())
report = {'status': 'Running', 'source_commit': commit, 'compiler_source_commit': a.compiler_commit or commit,
          'cli_sha256': digest(cli), 'sources': {f.name: digest(f) for f in sorted(sources.glob('*.zil'))},
          'probe_sources': {str(f.relative_to(repo)): digest(f) for f in [repo/'tools/language/173/allocator_probe.c', repo/'tools/research/142/allocator_probe.c']},
          'builds': [], 'observations': [], 'full_language': 'NotSatisfied', 'product': 'NotRun'}
save(out/'summary.json', report)

def observe(executable, directory, data, expected, exit_code, opt, mode, allocated=None, fail_at=0):
    result = command([executable], directory, data, {'ZIL_TEST_FAIL_AT': str(fail_at)} if allocated is not None else None)
    if result.returncode != exit_code or result.stdout != expected:
        raise RuntimeError(f'{opt}/{mode}: exit={result.returncode}, stdout={result.stdout!r}, stderr={result.stderr!r}')
    row = {'optimization': opt, 'mode': mode, 'stdin_hex': data.hex(), 'exit_code': result.returncode, 'stdout': result.stdout.decode(), 'fail_at': fail_at}
    if allocated is not None:
        allocation = json.loads(result.stderr)
        if allocation['live'] != 0 or allocation['allocated'] != allocated or allocation['frees'] != allocated or allocation['failures'] != int(fail_at > 0):
            raise RuntimeError(f'{opt}/{mode}: responsibility mismatch {allocation}')
        row['allocation'] = allocation
    elif result.stderr:
        raise RuntimeError(f'unexpected stderr: {result.stderr!r}')
    report['observations'].append(row)
    save(out/'summary.json', report)

try:
    for opt in ('O0', 'O2'):
        current = out/opt
        executable, probe, phases = current/'program', current/'allocation-probe', current/'phases'
        result = command([cli, 'build', sources/'main.zil', '--opt', opt, '-o', executable, '--emit-dir', phases, '--json'], current/'build')
        built = json.loads(result.stdout)
        if result.returncode or built['status'] != 'Success' or built['verification_runs'] != 1 or len(built['source_files']) != 4:
            raise RuntimeError(f'{opt}: {built}')
        report['builds'].append({'optimization': opt, 'source_files': built['source_files'], 'verification_runs': 1, 'llvm_sha256': digest(phases/'llvm.ll'), 'native_sha256': digest(executable)})
        linked = command([os.environ.get('ZIL_CLANG', 'clang'), '-'+opt, phases/'llvm.ll', repo/'tools/language/173/allocator_probe.c', '-Wl,--wrap=realloc', '-Wl,--wrap=free', '-o', probe], current/'probe-link')
        if linked.returncode:
            raise RuntimeError('allocation probe failed to link')
        report['builds'][-1]['probe_sha256'] = digest(probe)
        for count in (1, 2):
            data, expected = str(count).encode(), bytes(range(66, 66+count))+b'A'
            case = current/str(count)
            observe(executable, case/'ordinary', data, expected, 0, opt, 'ordinary')
            observe(probe, case/'allocation', data, expected, 0, opt, 'allocation', 2)
            observe(probe, case/'failure-1', data, b'A', 21, opt, 'allocation_failure', 0, 1)
            observe(probe, case/'failure-2', data, expected, 22, opt, 'allocation_failure', 1, 2)
        observe(executable, current/'invalid', b'9', b'', 9, opt, 'ordinary')
    report.update(status='Pass', native_executions=len(report['observations']), allocation_failure_executions=sum(x['mode']=='allocation_failure' for x in report['observations']))
except Exception as error:
    report.update(status='Fail', error_type=type(error).__name__, error=str(error))
    save(out/'summary.json', report)
    raise
save(out/'summary.json', report)
print(json.dumps(report, indent=2))
