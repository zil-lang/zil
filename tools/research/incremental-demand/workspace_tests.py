#!/usr/bin/env python3
"""Run the complete Cargo workspace tests in disk-bounded groups.

Same test profile/features/assertions as cargo test --workspace. Driver integration
executables dominate disk space, so build/run them in groups and remove only
completed test executables reported by Cargo. Raw stdout/stderr/exits are kept.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def frozen_source(repo, expected=None):
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=repo).decode()
    source = git('rev-parse', 'HEAD').strip()
    if expected is not None and source != expected:
        raise ValueError('source HEAD changed during the test window')
    changed = git('diff', '--name-only', 'HEAD', '-z').split('\0')
    changed += git('ls-files', '--others', '--exclude-standard', '-z').split('\0')
    compiler = ('crates/', 'src/', 'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/',
                'rust-toolchain', 'policy/', 'validation/', 'tests/', 'spec/', 'tools/llvm/')
    if any(path.startswith(compiler) for path in changed if path):
        raise ValueError('compiler or validation inputs differ from the frozen source')
    return source


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--features', default='llvm-api')
    parser.add_argument('--batch-size', type=int, default=12)
    args = parser.parse_args()
    if args.batch_size < 1:
        parser.error('batch-size must be positive')
    repo = args.repo.resolve(strict=True)
    target = repo / 'target'
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = {**os.environ, 'CARGO_TARGET_DIR': str(target)}
    source = frozen_source(repo)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=repo, env=env))
    driver = next(p for p in metadata['packages'] if p['name'] == 'zil-driver')
    tests = sorted(t['name'] for t in driver['targets'] if 'test' in t['kind'])
    groups = [('other-workspace', ['--workspace', '--exclude', 'zil-driver']),
              ('driver-library', ['-p', 'zil-driver', '--lib']),
              ('driver-examples', ['-p', 'zil-driver', '--examples', '--no-run'])]
    for start in range(0, len(tests), args.batch_size):
        selected = tests[start:start + args.batch_size]
        groups.append((f'driver-integrations-{start:03d}', ['-p', 'zil-driver'] + [x for name in selected for x in ('--test', name)]))
    groups.append(('driver-doc', ['-p', 'zil-driver', '--doc']))
    receipt = {'source': source,
               'features': args.features, 'profile': 'test', 'driver_integration_targets': tests,
               'driver_targets': driver['targets'],
               'group_count': len(groups), 'groups': [], 'policy': 'No test filters; all workspace packages and every driver integration target plus driver library/doc; examples compiled. Only test executables from successful completed groups removed; logs retained.'}
    (out/'plan.json').write_text(json.dumps(receipt, indent=2)+'\n')
    for index, (name, selection) in enumerate(groups):
        frozen_source(repo, receipt['source'])
        directory = out / name
        directory.mkdir()
        command = ['cargo', 'test', '--locked', *selection, '--features', args.features, '--no-fail-fast', '--message-format=json']
        record = {'name': name, 'argv': command, 'removed_executables': []}
        (directory/'command.json').write_text(json.dumps(command, indent=2)+'\n')
        with (directory/'stdout.log').open('wb') as stdout, (directory/'stderr.log').open('wb') as stderr:
            result = subprocess.run(command, cwd=repo, env=env, stdout=stdout, stderr=stderr)
        record['exit_code'] = result.returncode
        (directory/'exit').write_text(str(result.returncode)+'\n')
        frozen_source(repo, receipt['source'])
        removed = set()
        canonical_deps = (target/'debug/deps').resolve()
        for line in (directory/'stdout.log').read_text(errors='replace').splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if not isinstance(item, dict) or item.get('reason') != 'compiler-artifact' or not item.get('profile', {}).get('test') or not item.get('executable'):
                continue
            # A compilation failure can leave linked but unexecuted targets.
            # Retain all of them for that failed group, along with its logs.
            if result.returncode or name == 'driver-examples':
                continue
            path = Path(item['executable'])
            if str(path) in removed:
                continue
            if not path.is_relative_to(target/'debug/deps') or not path.resolve().is_relative_to(canonical_deps) or not path.is_file() or path.is_symlink():
                raise ValueError('unexpected test executable location: '+str(path))
            with path.open('rb') as stream:
                if stream.read(4) != b'\x7fELF':
                    raise ValueError('unexpected test executable format')
            record['removed_executables'].append({'path': str(path), 'bytes': path.stat().st_size})
            path.unlink()
            removed.add(str(path))
        receipt['groups'].append(record)
        (out/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(json.dumps({'group': index+1, 'total': len(groups), 'name': name, 'exit_code': result.returncode}), flush=True)
    return int(any(r['exit_code'] for r in receipt['groups']))


if __name__ == '__main__':
    sys.exit(main())
