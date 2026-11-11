#!/usr/bin/env python3
"""Build one exact full-producer candidate using a copy of prior Cargo artifacts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

WORKSPACE = Path('/workspace/scratch/abf690ce880c')
sys.path.insert(0, str(WORKSPACE / 'research/thinlto-private-linear'))
from processes import measured


def identity(path):
    path = Path(os.path.abspath(path))
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--checkout', type=Path, required=True)
    q.add_argument('--commit', required=True)
    q.add_argument('--tree', required=True)
    q.add_argument('--out', type=Path, required=True)
    q.add_argument('--target', type=Path, required=True)
    q.add_argument('--seed-target', type=Path, required=True)
    a = q.parse_args()
    checkout = a.checkout.resolve(strict=True)
    def git(*args):
        return subprocess.check_output(['git', '-C', str(checkout), *args], text=True).strip()
    def source():
        return (git('rev-parse', 'HEAD'), git('rev-parse', 'HEAD^{tree}'), git('status', '--porcelain=v1'))
    expected = (a.commit, a.tree, '')
    if source() != expected:
        raise ValueError('build requires exact clean frozen checkout')
    out, target = a.out.resolve(), a.target.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if target.exists():
        raise ValueError('new source requires its own unused target directory')
    seed = a.seed_target.resolve(strict=True)
    seed_binary = identity(seed / 'release/zil')
    # A distinct directory/inodes (reflinks when supported), never hardlink or
    # overwrite the immutable earlier compiler used by a completed protocol.
    subprocess.run(['cp', '-a', '--reflink=auto', str(seed), str(target)], check=True)
    os.environ['CARGO_TARGET_DIR'] = str(target)
    os.sched_setaffinity(0, [0, 1])
    os.chdir(checkout)
    commands = [
        ('api-tests', ['cargo', 'test', '--locked', '-j', '2', '-p', 'zil-llvm-api', '--features', 'native']),
        ('proof-tests', ['cargo', 'test', '--locked', '-j', '2', '-p', 'zil-backend-llvm', '--features', 'llvm-api', 'complete_visibility']),
        ('no-native', ['cargo', 'check', '--locked', '-j', '2', '-p', 'zil-backend-llvm', '--no-default-features']),
        ('release', ['cargo', 'build', '--locked', '--release', '-j', '2', '-p', 'zil-cli']),
    ]
    record = {'status': 'Running', 'source_commit': a.commit, 'source_tree': a.tree,
              'checkout': str(checkout), 'target': str(target), 'seed_target': str(seed), 'seed_binary': seed_binary,
              'reuse_policy': 'copied prior Cargo artifacts; real cargo fingerprint/rebuild and all controls still execute', 'recipe': identity(__file__),
              'cpus': [0, 1], 'jobs': 2, 'attempts': []}
    def save():
        (out / 'binding.json').write_text(json.dumps(record, indent=2) + '\n')
    save()
    for name, command in commands:
        result = measured(command, out / name, 900)
        ok = result['exit'] == 0 and not result['timeout'] and not result['exception'] and not result['residual_descendants']
        record['attempts'].append({'phase': name, 'command': command, 'passed': ok,
            'process_attempt': identity(out / name / 'process-attempt.json'),
            'stdout': identity(out / name / 'stdout'), 'stderr': identity(out / name / 'stderr')})
        if source() != expected:
            record['status'] = 'Failed-source-changed'
            save()
            raise ValueError('source changed during build')
        if not ok:
            record['status'] = 'Failed'
            save()
            print(json.dumps(record, indent=2))
            raise SystemExit(1)
        save()
    record['binary'] = identity(target / 'release/zil')
    if identity(seed / 'release/zil') != seed_binary:
        record['status'] = 'Failed-seed-artifact-changed'
        save()
        raise ValueError('immutable earlier compiler changed')
    record['status'] = 'Pass'
    save()
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()
