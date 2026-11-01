#!/usr/bin/env python3
"""Create a new private compiler snapshot; never mutate an installed compiler."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--toolchain-receipt', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    manifest = json.loads((here / 'complete-clone.json').read_text())
    patch = here / 'complete-clone.patch'
    assert sha(patch) == manifest['patch_sha256']
    subprocess.run([sys.executable, here.parent / 'toolchain_combination/prepare_api.py',
        '--repo', args.repo, '--revision', manifest['revision'], '--toolchain-receipt',
        args.toolchain_receipt, '--out', args.out], check=True)
    for file in manifest['files']:
        path = args.out / file['path']
        if file['original_sha256'] is None:
            assert not path.exists()
        else:
            assert sha(path) == file['original_sha256']
    applied = subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(patch)],
        cwd=args.out, capture_output=True)
    (args.out / 'clone-patch.stdout').write_bytes(applied.stdout)
    (args.out / 'clone-patch.stderr').write_bytes(applied.stderr)
    assert applied.returncode == 0, applied.stderr.decode()
    for file in manifest['files']:
        assert sha(args.out / file['path']) == file['patched_sha256']
    # Preserve the historical patch, including its original test borrow error.
    # New snapshots additionally apply the documented cfg(test)-only correction.
    test_path = args.out / 'crates/zil-backend-llvm/src/machine_cgu_clone_controls.rs'
    assert sha(test_path) == 'a6be1ed9fd900623315318028481b44dc3935f2b70eb4f514821845c93441905'
    test_patch = here / 'test-fixture-fix.patch'
    fixed = subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(test_patch)],
        cwd=args.out, capture_output=True)
    (args.out / 'test-fixture-fix.stdout').write_bytes(fixed.stdout)
    (args.out / 'test-fixture-fix.stderr').write_bytes(fixed.stderr)
    assert fixed.returncode == 0, fixed.stderr.decode()
    assert sha(test_path) == '2378d370d1f11fdcac62f3116d86019da8d3ff8fae327ddfa88ba8f0f5fa5ec6'
    # The reviewed private source was formatted after inserting its long tags.
    # Format only this new snapshot's API file, not its module children.
    formatter = subprocess.run(['rustfmt', '--edition', '2024', '--config', 'skip_children=true',
        str(args.out / 'crates/zil-llvm-api/src/lib.rs')], cwd=args.out, capture_output=True)
    (args.out / 'api-format.stdout').write_bytes(formatter.stdout)
    (args.out / 'api-format.stderr').write_bytes(formatter.stderr)
    assert formatter.returncode == 0, formatter.stderr.decode()
    # Write a local copy of the generator's patch, preserving the repository
    # artifact even if a future caller selects another valid LLVM prefix.
    subprocess.run([sys.executable, here / 'freeze_source.py', '--repo', args.repo,
        '--snapshot', args.out, '--patch-out', args.out / 'complete-clone.patch'], check=True)
    (args.out / 'REPRODUCER.json').write_text(json.dumps(dict(
        schema=1, historical_patch_sha256=manifest['patch_sha256'],
        test_fixture_fix_sha256=sha(test_patch), prepared_patch_sha256=sha(args.out / 'complete-clone.patch'),
        boundary='Fresh private snapshot includes the documented test-only borrow fix. '
                 'It does not replace the historical v1 source receipt, release ELF or failed test log.'), indent=2) + '\n')


if __name__ == '__main__':
    main()
