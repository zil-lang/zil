#!/usr/bin/env python3
"""Import 209 into an authenticated clone; --push-main publishes a non-force fast-forward."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ALLOWED_ORIGINS = {
    'https://github.com/zil-lang/zil', 'https://github.com/zil-lang/zil.git',
    'git@github.com:zil-lang/zil.git', 'ssh://git@github.com/zil-lang/zil.git',
}


def git(repo, *args):
    result = subprocess.run(['git', '-C', str(repo), *args], capture_output=True,
                            text=True, timeout=180, env=dict(os.environ, GIT_NO_REPLACE_OBJECTS='1'))
    if result.returncode:
        raise RuntimeError(f"git {args[0]} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def check_origin(repo):
    for args in [('remote', 'get-url', '--all', 'origin'), ('remote', 'get-url', '--push', '--all', 'origin')]:
        urls = git(repo, *args).splitlines()
        if len(urls) != 1 or urls[0] not in ALLOWED_ORIGINS:
            raise RuntimeError('origin is not the expected zil-lang/zil GitHub repository')


def execute(repo, bundle, manifest, push=False):
    expected, candidate = manifest['remote_base'], manifest['candidate_commit']
    if any(len(x) != 40 or any(c not in '0123456789abcdef' for c in x) for x in (expected, candidate)):
        raise RuntimeError('invalid pinned commit identity')
    with bundle.open('rb') as stream:
        actual_hash = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual_hash != manifest['bundle_sha256']:
        raise RuntimeError('bundle SHA256 mismatch')
    check_origin(repo)
    git(repo, 'fetch', 'origin', 'refs/heads/main:refs/remotes/origin/main')
    before = git(repo, 'rev-parse', 'refs/remotes/origin/main')
    if before != expected:
        raise RuntimeError(f'main moved: expected {expected}, found {before}; re-integrate, do not force-push')
    git(repo, 'bundle', 'verify', str(bundle))
    ref = 'refs/heads/sync/209-llvm-api-mainline'
    existing = subprocess.run(['git', '-C', str(repo), 'rev-parse', '--verify', ref],
                              capture_output=True, text=True, timeout=15)
    if existing.returncode == 0 and existing.stdout.strip() != candidate:
        raise RuntimeError('local sync branch already exists at a different commit')
    git(repo, 'fetch', str(bundle), 'refs/heads/integration/209-mainline:' + ref)
    if git(repo, 'rev-parse', ref) != candidate:
        raise RuntimeError('bundle ref does not match candidate identity')
    git(repo, 'merge-base', '--is-ancestor', expected, candidate)
    result = {'status': 'Prepared', 'remote_before': before, 'candidate': candidate,
              'local_ref': ref, 'remote_updated': False, 'worktree_unchanged': True}
    if push:
        # A race or branch protection failure must be rejected by Git; never force.
        git(repo, 'push', 'origin', ref + ':refs/heads/main')
        lines = git(repo, 'ls-remote', 'origin', 'refs/heads/main').splitlines()
        if len(lines) != 1 or lines[0].split()[0] != candidate:
            raise RuntimeError('push returned but main could not be confirmed; inspect before retrying')
        result.update(status='Published', remote_after=candidate, remote_updated=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True, help='existing authenticated Git clone')
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('PUBLISH-209.json'))
    parser.add_argument('--push-main', action='store_true')
    args = parser.parse_args()
    try:
        manifest = json.loads(args.manifest.read_text())
        name = manifest['bundle_file']
        if Path(name).name != name or name in ('.', '..'):
            raise RuntimeError('bundle path must be a sibling filename')
        result = execute(args.repo.resolve(), args.manifest.resolve().parent / name, manifest, args.push_main)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print(json.dumps({'status': 'Fail', 'error': str(error), 'publication_not_confirmed': True}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps(result, indent=2))
    return 0

if __name__ == '__main__':
    sys.exit(main())
