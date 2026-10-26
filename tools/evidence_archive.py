#!/usr/bin/env python3
"""Verify or restore the optional standard-ZIP raw evidence companion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import zipfile

from package_check import HEX, digest, safe_name

PREFIX = 'zil-rust/'
MANIFEST = 'AUDIT-MANIFEST.json'


def raw_path(name):
    return name.startswith('evidence/') or name.startswith('out/runs/')


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')


def parse_manifest(data):
    value = json.loads(data)
    if value.get('format') != 'zil-evidence-audit/1' or not isinstance(value.get('files'), list):
        raise ValueError('Unknown audit manifest format')
    seen = set()
    for row in value['files']:
        name = safe_name(row['path'])
        if not raw_path(name) or name.casefold() in seen:
            raise ValueError('Invalid or duplicate evidence path: ' + name)
        if not isinstance(row['sha256'], str) or not HEX.fullmatch(row['sha256']):
            raise ValueError('Invalid digest: ' + name)
        if type(row['size']) is not int or row['size'] < 0 or row['mode'] not in (0o644, 0o755):
            raise ValueError('Invalid evidence metadata: ' + name)
        seen.add(name.casefold())
    if not value['files']:
        raise ValueError('Empty audit archive')
    return value


def create(root, paths, output, source):
    rows = []
    seen = set()
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(paths):
            name = safe_name(path.relative_to(root).as_posix())
            if not raw_path(name) or name.casefold() in seen or path.is_symlink():
                raise ValueError('Invalid or duplicate evidence input: ' + name)
            seen.add(name.casefold())
            data = path.read_bytes()
            mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            info = zipfile.ZipInfo(PREFIX + name)
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
            rows.append({'path': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'mode': mode})
        manifest = encoded({'format': 'zil-evidence-audit/1', 'source': source,
                            'scope': 'Original raw evidence, including failures; no new test execution', 'files': rows})
        parse_manifest(manifest)
        archive.writestr(PREFIX + MANIFEST, manifest)
    return {'files': len(rows), 'raw_bytes': sum(r['size'] for r in rows),
            'manifest_sha256': hashlib.sha256(manifest).hexdigest(), 'rows': rows}


def inspect(archive_path, expected_sha=None):
    archive_path = Path(archive_path)
    if expected_sha is not None and (not HEX.fullmatch(expected_sha) or digest(archive_path) != expected_sha):
        raise ValueError('Audit archive SHA-256 mismatch')
    with zipfile.ZipFile(archive_path) as archive:
        names = {}
        folded = set()
        for item in archive.infolist():
            safe_name(item.filename)
            if not item.filename.startswith(PREFIX) or item.filename.casefold() in folded:
                raise ValueError('Unexpected or duplicate ZIP entry: ' + item.filename)
            mode = item.external_attr >> 16
            if item.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG):
                raise ValueError('Only ordinary files may be restored: ' + item.filename)
            names[item.filename] = item
            folded.add(item.filename.casefold())
        manifest_bytes = archive.read(PREFIX + MANIFEST)
        manifest = parse_manifest(manifest_bytes)
        expected = {PREFIX + row['path'] for row in manifest['files']} | {PREFIX + MANIFEST}
        if set(names) != expected:
            raise ValueError('ZIP entries do not exactly match the audit manifest')
        for row in manifest['files']:
            item = names[PREFIX + row['path']]
            if item.file_size != row['size']:
                raise ValueError('Evidence size mismatch: ' + row['path'])
            data = archive.read(item)
            if hashlib.sha256(data).hexdigest() != row['sha256']:
                raise ValueError('Evidence hash mismatch: ' + row['path'])
    return manifest, manifest_bytes


def restore(archive_path, destination, expected_sha=None):
    destination = Path(destination).absolute()
    # Check the supplied lexical path before reading PACKAGE.json or writing.
    # Checking only the final directory misses safe/link/new-directory, where
    # link already redirects all later operations outside the reviewed path.
    current = Path(destination.anchor)
    for part in destination.parts[1:]:
        current = current / part
        if current.is_symlink() or (hasattr(current, 'is_junction') and current.is_junction()):
            raise ValueError('Symlink/junction in destination ancestry; choose an explicit real directory path: ' + str(current))
        if current.exists() and not current.is_dir():
            raise ValueError('Non-directory in destination ancestry: ' + str(current))
    package_path = destination / 'PACKAGE.json'
    if package_path.is_file():
        package = json.loads(package_path.read_text(encoding='utf-8'))
        if package.get('format') != 'zil-source/1':
            raise ValueError('Unknown source package format')
        linked = package['companions']['audit']
        if expected_sha is not None and expected_sha != linked['sha256']:
            raise ValueError('Expected digest conflicts with source package')
        expected_sha = linked['sha256']
    else:
        linked = None
    manifest, manifest_bytes = inspect(archive_path, expected_sha)
    if linked and hashlib.sha256(manifest_bytes).hexdigest() != linked['manifest_sha256']:
        raise ValueError('Audit manifest conflicts with source package')
    entries = [(row['path'], row['sha256'], row['mode']) for row in manifest['files']]
    entries.append((MANIFEST, hashlib.sha256(manifest_bytes).hexdigest(), 0o644))
    # Validate everything before writing; never overwrite different bytes.
    for name, sha, _ in entries:
        current = destination
        for part in Path(name).parts:
            current = current / part
            if current.is_symlink():
                raise ValueError('Symlink in restore destination: ' + name)
            if current.exists() and current != destination / name and not current.is_dir():
                raise ValueError('Non-directory restore parent: ' + name)
        target = destination / name
        if target.exists() and (not target.is_file() or digest(target) != sha):
            raise ValueError('Restore would replace different existing content: ' + name)
    written = 0
    with zipfile.ZipFile(archive_path) as archive:
        for name, _, mode in entries:
            target = destination / name
            if target.exists():
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as handle:
                handle.write(archive.read(PREFIX + name))
            os.chmod(target, mode)
            written += 1
    return {'ok': True, 'files_verified': len(manifest['files']), 'files_written': written,
            'scope': 'Original evidence restored; no test result recomputed'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['verify', 'restore'])
    parser.add_argument('archive', type=Path)
    parser.add_argument('--destination', type=Path)
    parser.add_argument('--expected-sha256')
    args = parser.parse_args()
    try:
        if args.action == 'restore':
            if args.destination is None:
                raise ValueError('restore requires --destination')
            result = restore(args.archive, args.destination, args.expected_sha256)
        else:
            manifest, _ = inspect(args.archive, args.expected_sha256)
            result = {'ok': True, 'files_verified': len(manifest['files']),
                      'sha256': digest(args.archive), 'scope': 'Archive integrity only; no new test execution'}
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as error:
        result = {'ok': False, 'error': str(error)}
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
