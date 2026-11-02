#!/usr/bin/env python3
"""Summarize CI results and safely remove completed phase outputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil

PHASE_PATHS = {
    'workspace': ('native-evidence/workspace', 'target/debug'),
    'reference': ('native-evidence/reference', 'target-reference'),
}
TEST_RESULT = re.compile(
    r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; '
    r'(\d+) ignored; (\d+) measured; (\d+) filtered out;'
)


def cleanup(root, phase):
    root = Path(root)
    if not root.is_absolute() or root == Path('/') or root != root.resolve() or root.is_symlink():
        raise ValueError('Runner temporary root must be absolute, canonical, and not /')
    for relative in PHASE_PATHS[phase]:
        path = root / relative
        if path != path.resolve() or path.is_symlink():
            raise ValueError(f'Cleanup path must not contain a symlink: {path}')
        if path.exists():
            if not path.is_dir():
                raise ValueError(f'Cleanup path is not a directory: {path}')
            shutil.rmtree(path)
        print(f'Released completed phase output: {path}')
    print(f'Free bytes after cleanup: {shutil.disk_usage(root).free}')


def summary(root, commit):
    root = Path(root)
    entries = {}
    for name in ('fmt', 'workspace', 'reference', 'clippy', 'text-reference', 'migration-runner'):
        path = root / 'receipts' / f'{name}.log'
        if not path.is_file():
            continue
        digest = hashlib.sha256()
        totals = [0] * 5
        count = 0
        with path.open('rb') as source:
            for line in source:
                digest.update(line)
                match = TEST_RESULT.search(line.decode('utf-8', errors='replace'))
                if match:
                    count += 1
                    for index, value in enumerate(match.groups()):
                        totals[index] += int(value)
        entries[name] = {
            'sha256': digest.hexdigest(),
            'test_summaries': count,
            'totals': dict(zip(('passed', 'failed', 'ignored', 'measured', 'filtered'), totals)),
        }
    return {'commit': commit, 'steps': {name: os.environ.get(f'OUTCOME_{name.upper()}', '')
            for name in ('fmt', 'workspace', 'reference', 'consumer', 'clippy', 'text', 'ci_test')},
            'logs': entries}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('cleanup', 'summary'))
    parser.add_argument('root', type=Path)
    parser.add_argument('value')
    args = parser.parse_args()
    if args.action == 'cleanup':
        if args.value not in PHASE_PATHS:
            parser.error('unsupported phase')
        cleanup(args.root, args.value)
    else:
        result = summary(args.root, args.value)
        text = json.dumps(result, indent=2) + '\n'
        (args.root / 'receipts' / 'compact-results.json').write_text(text)
        print(text, end='')
        if os.environ.get('GITHUB_STEP_SUMMARY'):
            with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as output:
                output.write('### CI results\n\n```json\n' + text + '```\n')


if __name__ == '__main__':
    main()
