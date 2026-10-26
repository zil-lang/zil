#!/usr/bin/env python3
"""Check delivered bytes; hashes do not establish language correctness."""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import os
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
EXCLUDE = {'.git', '.work', 'target', 'cache', 'out', '__pycache__',
           '.pytest_cache', 'node_modules', '.venv'}
HEX = re.compile(r'[0-9a-f]{64}\Z')


def safe_name(name):
    """Use the same relative names on POSIX and Windows; never normalize escapes."""
    if not isinstance(name, str) or not name or any(ord(c) < 32 for c in name):
        raise ValueError('Invalid archive path')
    if '\\' in name or ':' in name or name.startswith('/'):
        raise ValueError('Nonportable archive path: ' + name)
    if any(p in ('', '.', '..') or p.endswith((' ', '.')) for p in name.split('/')):
        raise ValueError('Noncanonical archive path: ' + name)
    if PurePosixPath(name).as_posix() != name:
        raise ValueError('Noncanonical archive path: ' + name)
    return name


def files(root=ROOT):
    """Repository/delivery inventory, excluding outputs and external archives."""
    root = Path(root)
    for base, dirs, names in os.walk(root):
        for directory in dirs:
            if directory not in EXCLUDE and (Path(base) / directory).is_symlink():
                raise ValueError('Symlink is not a source delivery directory: ' + str(Path(base) / directory))
        dirs[:] = sorted(d for d in dirs if d not in EXCLUDE)
        for name in sorted(names):
            path = Path(base) / name
            if name in EXCLUDE or name == 'SHA256SUMS' or path.suffix in ('.pyc', '.zip', '.bundle'):
                continue
            if path.is_symlink():
                raise ValueError('Symlink is not a source delivery file: ' + str(path))
            yield path


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for part in iter(lambda: handle.read(1024 * 1024), b''):
            h.update(part)
    return h.hexdigest()


def read_sums(path):
    entries = {}
    folded = set()
    for line in Path(path).read_text(encoding='utf-8').splitlines():
        sha, name = line.split('  ', 1)
        safe_name(name)
        if not HEX.fullmatch(sha) or name.casefold() in folded:
            raise ValueError('Invalid or duplicate checksum entry: ' + name)
        entries[name] = sha
        folded.add(name.casefold())
    if not entries:
        raise ValueError('Empty checksum manifest')
    return entries


def check(root=ROOT, companions=None):
    root = Path(root).resolve()
    entries = read_sums(root / 'SHA256SUMS')
    failures = []
    package = None
    if (root / 'PACKAGE.json').is_file():
        package = json.loads((root / 'PACKAGE.json').read_text(encoding='utf-8'))
        if package.get('format') != 'zil-source/1':
            raise ValueError('Unknown source package format')
    # A restored audit adds the inventory bound by the source package. It is
    # not an unrestricted permission to ignore arbitrary extra files.
    restored = root / 'AUDIT-MANIFEST.json'
    if restored.exists():
        from evidence_archive import parse_manifest
        if not package or digest(restored) != package['companions']['audit']['manifest_sha256']:
            raise ValueError('Restored audit manifest does not match source package')
        audit = parse_manifest(restored.read_bytes())
        for record in audit['files']:
            name, sha = record['path'], record['sha256']
            if name in entries and entries[name] != sha:
                raise ValueError('Source/audit content conflict: ' + name)
            entries[name] = sha
        entries['AUDIT-MANIFEST.json'] = digest(restored)
    for name, expected in entries.items():
        path = root / name
        current = root
        for part in PurePosixPath(name).parts:
            current = current / part
            if current.is_symlink():
                failures.append(name + ': symlink')
                break
        else:
            if not path.is_file() or digest(path) != expected:
                failures.append(name)
    actual = {p.relative_to(root).as_posix() for p in files(root)}
    unlisted = sorted(actual - set(entries))
    companion_results = []
    if companions is not None:
        if not package:
            raise ValueError('PACKAGE.json required for companion verification')
        for kind, record in package['companions'].items():
            if record.get('status') != 'included':
                companion_results.append({'kind': kind, 'status': 'not-included'})
                continue
            name = safe_name(record['file'])
            if '/' in name:
                raise ValueError('Companion reference must be a filename')
            path = Path(companions) / name
            passed = path.is_file() and not path.is_symlink() and digest(path) == record['sha256']
            if not passed:
                failures.append('companion: ' + name)
            companion_results.append({'kind': kind, 'file': name, 'ok': passed})
    return {'ok': not failures and not unlisted, 'scope': 'file integrity only',
            'entries': len(entries), 'failures': failures, 'unlisted': unlisted,
            'companions': companion_results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--write', action='store_true', help='write a fresh inventory for this tree')
    parser.add_argument('--companions', type=Path, help='directory containing the audit ZIP and Git bundle')
    args = parser.parse_args()
    try:
        root = args.root.resolve()
        if args.write:
            if (root / 'PACKAGE.json').exists():
                raise ValueError('Do not rewrite a delivered manifest; rebuild the delivery with package.py')
            (root / 'SHA256SUMS').write_text(''.join(
                f'{digest(p)}  {p.relative_to(root).as_posix()}\n' for p in sorted(files(root))), encoding='utf-8')
        result = check(root, args.companions)
    except (OSError, ValueError, KeyError, TypeError) as error:
        result = {'ok': False, 'error': str(error)}
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
