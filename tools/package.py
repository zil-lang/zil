#!/usr/bin/env python3
"""Produce a lean source ZIP, a raw-evidence ZIP, and a separate Git bundle."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import zipfile

import evidence_archive
from package_check import ROOT, digest, files, safe_name

MAX_COMPACT = 256 * 1024


def git(root, *arguments):
    result = subprocess.run(['git', '-C', str(root), *arguments], capture_output=True, text=True, check=False)
    if result.returncode:
        raise ValueError('git ' + ' '.join(arguments) + ': ' + result.stderr.strip())
    return result.stdout.strip()


def provenance(root):
    try:
        if Path(git(root, 'rev-parse', '--show-toplevel')).resolve() != root:
            raise ValueError('Not the root of a Git worktree')
        return {'commit': git(root, 'rev-parse', 'HEAD'),
                'branch': git(root, 'branch', '--show-current'),
                'dirty': bool(git(root, 'status', '--porcelain', '--untracked-files=normal'))}
    except (OSError, ValueError):
        return {'commit': None, 'branch': None, 'dirty': None}


def native_artifact(path):
    with path.open('rb') as handle:
        magic = handle.read(4)
    return native_header(path.as_posix(), magic)


def native_header(name, magic):
    if Path(name).suffix.lower() in {'.bin', '.exe', '.o', '.obj', '.a', '.lib', '.so', '.dll', '.dylib', '.wasm'}:
        return True
    return magic.startswith((b'\x7fELF', b'MZ')) or magic in {
        b'\xfe\xed\xfa\xce', b'\xce\xfa\xed\xfe', b'\xfe\xed\xfa\xcf',
        b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca'}


def selected_evidence(root, paths):
    available = {p.relative_to(root).as_posix(): p for p in paths}
    selected = set()

    def require(name):
        safe_name(name)
        if not name.startswith('evidence/') or name not in available:
            raise ValueError('Required compact evidence is missing: ' + name)
        if native_artifact(available[name]):
            raise ValueError('Task evidence must not be a native artifact: ' + name)
        selected.add(name)

    for task in sorted((root / '.task').rglob('*.md')):
        for block in re.findall(r'```json\s*(.*?)```', task.read_text(encoding='utf-8'), re.S):
            record = json.loads(block)
            for ref in record.get('evidence', []) + (record.get('review') or {}).get('evidence', []):
                require(ref if isinstance(ref, str) else ref['result_path'])
    for name, path in available.items():
        parts = path.relative_to(root).parts
        if path.stat().st_size > MAX_COMPACT or native_artifact(path):
            continue
        ordinary = (len(parts) == 2 or parts[1] == 'reviews'
                    or (parts[1] == 'impl' and len(parts) == 4)
                    or (parts[1] == 'integration' and len(parts) == 3))
        failure = path.name in {'failure.json', 'mutation-detection.json', 'cleanup-original-diagnostic.json'}
        if 'corpus' in parts:
            offset = parts.index('corpus')
            failure |= len(parts) == offset + 3 and (path.name == 'case.json' or path.name.startswith('reduced-'))
        if ordinary or failure:
            selected.add(name)
        if len(parts) == 4 and path.name == 'result.json':
            report = json.loads(path.read_text(encoding='utf-8'))
            if (report.get('status') in ('Fail', 'InfraError', 'Error', 'fail', 'error')
                    or report.get('draft_gate') == 'Fail' or report.get('ok') is False):
                selected.add(name)
    # Keep the current aggregate and its immediate reports, without recursively
    # pulling thousands of case-level results and stage artifacts.
    summary_path = root / 'evidence/final-summary.json'
    if summary_path.exists():
        # The active entry point is required even when the aggregate grows past
        # the optional evidence size cap. Its referenced report is already required.
        require('evidence/final-summary.json')
        summary = json.loads(summary_path.read_text(encoding='utf-8'))
        if summary.get('result_path'):
            require(summary['result_path'])
        for record in summary.get('records', []):
            report = record.get('reported')
            if isinstance(report, dict) and report.get('result_path', '').startswith('evidence/'):
                require(report['result_path'])
    return selected


def publish(source, destination):
    # Exclusive creation never replaces an older release if publishers race.
    with destination.open('xb') as handle, source.open('rb') as origin:
        shutil.copyfileobj(origin, handle)
    sha = digest(destination)
    with Path(str(destination) + '.sha256').open('x', encoding='utf-8') as handle:
        handle.write(sha + '  ' + destination.name + '\n')
    return {'file': destination.name, 'bytes': destination.stat().st_size, 'sha256': sha}


def build(root, output, audit_output=None, history_output=None, without_history=False, raw_runs=(), audit_input=None):
    root, output = Path(root).resolve(), Path(output).resolve()
    if (root / 'PACKAGE.json').exists():
        raise ValueError('Repackage from a source checkout, not a compact delivery; recover the Git bundle to retain the full evidence boundary')
    audit_output = Path(audit_output).resolve() if audit_output else output.with_name(output.stem + '-audit.zip')
    history_output = Path(history_output).resolve() if history_output else output.with_name(output.stem + '-history.bundle')
    destinations = [output, audit_output] + ([] if without_history else [history_output])
    if len(set(destinations)) != len(destinations):
        raise ValueError('Source, audit and history outputs must be distinct')
    for destination in destinations:
        if destination.is_relative_to(root) and not destination.is_relative_to(root / 'out'):
            raise ValueError('Write deliveries outside the source tree or under out/')
        if destination.exists() or Path(str(destination) + '.sha256').exists():
            raise ValueError('Delivery already exists; choose a new version: ' + str(destination))
        safe_name(destination.name)
        destination.parent.mkdir(parents=True, exist_ok=True)
    source = provenance(root)
    if not without_history and (source['branch'] != 'main' or source['dirty'] is not False):
        raise ValueError('History delivery requires a clean main checkout; use --without-history for an explicit development snapshot')
    inventory = sorted(files(root))
    evidence_paths = [p for p in inventory if p.relative_to(root).parts[0] == 'evidence']
    raw_paths, raw_roots = [], []
    for raw_run in raw_runs:
        path = (root / raw_run).absolute()
        if path.is_symlink() or not path.is_dir() or not path.resolve().is_relative_to(root / 'out/runs') or path.resolve() == root / 'out/runs':
            raise ValueError('--raw-run must name an existing run directory beneath out/runs')
        name = safe_name(path.relative_to(root).as_posix())
        raw_roots.append(name)
        for base, directories, names in os.walk(path):
            for item in directories + names:
                entry = Path(base) / item
                if entry.is_symlink():
                    raise ValueError('Symlinks are not raw-run evidence: ' + str(entry))
            raw_paths.extend(Path(base) / item for item in names)
    kept = selected_evidence(root, evidence_paths)
    selected = [p for p in inventory if p.name not in ('PACKAGE.json', 'AUDIT-MANIFEST.json')
                and (p.relative_to(root).parts[0] != 'evidence' or p.relative_to(root).as_posix() in kept)]
    for path in selected:
        if native_artifact(path):
            raise ValueError('Unexpected native binary outside raw evidence: ' + str(path.relative_to(root)))
    with tempfile.TemporaryDirectory(prefix='zil-package-') as temporary:
        temp = Path(temporary)
        audit_path = temp / 'audit.zip'
        if audit_input is None:
            audit = evidence_archive.create(root, evidence_paths + raw_paths, audit_path, source)
            evidence_archive.inspect(audit_path)
        else:
            prior, manifest_bytes = evidence_archive.inspect(audit_input)
            prior_hashes = {row['path']: row['sha256'] for row in prior['files']}
            for path in evidence_paths + raw_paths:
                name = path.relative_to(root).as_posix()
                if prior_hashes.get(name) != digest(path):
                    raise ValueError('Current evidence is not identical to --audit-input: ' + name)
            shutil.copyfile(audit_input, audit_path)
            audit = {'files': len(prior['files']), 'raw_bytes': sum(row['size'] for row in prior['files']),
                     'manifest_sha256': hashlib.sha256(manifest_bytes).hexdigest(), 'rows': prior['files']}
        audit_record = {'status': 'included', 'file': audit_output.name, 'sha256': digest(audit_path),
                        'bytes': audit_path.stat().st_size, 'files': audit['files'],
                        'raw_bytes': audit['raw_bytes'], 'manifest_sha256': audit['manifest_sha256']}
        history_record = {'status': 'not-included', 'reason': 'Explicit --without-history; historical handoff is unavailable from the source ZIP alone'}
        history_path = temp / 'history.bundle'
        if not without_history:
            if git(root, 'rev-parse', 'refs/heads/main') != source['commit']:
                raise ValueError('main changed during packaging')
            git(root, 'bundle', 'create', str(history_path), 'refs/heads/main')
            git(root, 'bundle', 'verify', str(history_path))
            history_record = {'status': 'included', 'file': history_output.name, 'sha256': digest(history_path),
                              'bytes': history_path.stat().st_size, 'branch': 'main', 'commit': source['commit']}
        audit_hashes = {r['path']: r['sha256'] for r in audit['rows']}
        duplicate_groups = Counter(row['sha256'] for row in audit['rows'])
        unique_sizes = {row['sha256']: row['size'] for row in audit['rows']}
        native = []
        with zipfile.ZipFile(audit_path) as archived:
            for row in audit['rows']:
                with archived.open(evidence_archive.PREFIX + row['path']) as handle:
                    if native_header(row['path'], handle.read(4)):
                        native.append(row)
        package = {'format': 'zil-source/1', 'source': source,
                   'scope': 'Complete project source and compact evidence; raw evidence and Git history are separate companions',
                   'historical_handoff_requires_git': True,
                   'retention': {'policy': 'compact-receipts-and-counterexamples/1',
                                 'evidence_files_kept': len(kept),
                                 'evidence_files_external': sum(row['path'].startswith('evidence/') for row in audit['rows']) - len(kept),
                                 'entire_evidence_tree_archived': True,
                                 'audit_input_reused_byte_for_byte': audit_input is not None,
                                 'raw_run_roots': raw_roots,
                                 'other_ignored_runs_archived': False,
                                 'all_case_results_in_source': False},
                   'audit_statistics': {'native_files': len(native), 'native_bytes': sum(r['size'] for r in native),
                                        'duplicate_extra_files': sum(n - 1 for n in duplicate_groups.values()),
                                        'duplicate_extra_bytes': sum((n - 1) * unique_sizes[sha] for sha, n in duplicate_groups.items())},
                   'companions': {'audit': audit_record, 'history': history_record}}
        source_path = temp / 'source.zip'
        checksums, folded = [], set()
        raw_bytes = 0
        with zipfile.ZipFile(source_path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path in selected:
                name = safe_name(path.relative_to(root).as_posix())
                if name.casefold() in folded:
                    raise ValueError('Case-colliding source path: ' + name)
                folded.add(name.casefold())
                data = path.read_bytes()
                sha = hashlib.sha256(data).hexdigest()
                if name in kept and sha != audit_hashes[name]:
                    raise ValueError('Evidence changed during packaging: ' + name)
                info = zipfile.ZipInfo(evidence_archive.PREFIX + name)
                info.external_attr = (stat.S_IFREG | (0o755 if path.stat().st_mode & 0o111 else 0o644)) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data, compresslevel=9)
                checksums.append(sha + '  ' + name + '\n')
                raw_bytes += len(data)
            data = evidence_archive.encoded(package)
            archive.writestr(evidence_archive.PREFIX + 'PACKAGE.json', data)
            checksums.append(hashlib.sha256(data).hexdigest() + '  PACKAGE.json\n')
            sums = ''.join(checksums).encode('utf-8')
            archive.writestr(evidence_archive.PREFIX + 'SHA256SUMS', sums)
            raw_bytes += len(data) + len(sums)
        if provenance(root) != source:
            raise ValueError('Source identity or dirty state changed during packaging')
        with zipfile.ZipFile(source_path) as archive:
            if archive.testzip() is not None:
                raise ValueError('Source ZIP CRC failure')
        audit_published = publish(audit_path, audit_output)
        history_published = history_record if without_history else publish(history_path, history_output)
        result = {'ok': True, 'scope': 'Packaging and byte integrity; no new language test execution',
                  'source': publish(source_path, output), 'audit': audit_published,
                  'source_files': len(selected) + 2, 'source_raw_bytes': raw_bytes,
                  'retention': package['retention'], 'audit_statistics': package['audit_statistics']}
        result['history'] = history_published
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--audit-output', type=Path)
    parser.add_argument('--history-output', type=Path)
    parser.add_argument('--without-history', action='store_true')
    parser.add_argument('--raw-run', action='append', default=[], type=Path,
                        help='explicit out/runs subdirectory to add to the audit companion; repeatable')
    parser.add_argument('--audit-input', type=Path,
                        help='reuse a verified complete archive after pruning; all current evidence must match it exactly')
    args = parser.parse_args()
    try:
        result = build(args.root, args.output, args.audit_output, args.history_output, args.without_history, args.raw_run, args.audit_input)
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as error:
        result = {'ok': False, 'error': str(error)}
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
