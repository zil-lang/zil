#!/usr/bin/env python3
"""Package completed multi-edit evidence; no build, upload, or source deletion.

--root is the existing out directory. Only invocation of this script packages;
it was prepared without running it. Source/scripts/patches belong to Git archive.
A completed failed runtime gate is required evidence, never treated as incomplete.
"""
import argparse
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tarfile
import tempfile

MAIN = '99035931d358add0eaed6d4535035c38413701dd'
LAYOUT = 'a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf'
BOUNDED = '1679decf177c9a6eda5a64063c525f06e44e56a2'
ROOTS = ['multi-edit', 'bounded-imports']
PRUNE = {'objects', 'semantic', 'cache', 'caches', 'incremental', 'target',
         'target-reference', '__pycache__', 'node_modules', '.git'}
RAW_SUFFIXES = {'.json', '.jsonl', '.log', '.txt', '.stdout', '.stderr', '.stdin'}
RAW_NAMES = {'stdout', 'stderr', 'stdin'}
SECRETS = [
    ('private-key', re.compile(rb'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----')),
    ('github-token', re.compile(rb'(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,})')),
    ('aws-access-key', re.compile(rb'\b(?:AKIA|ASIA)[A-Z0-9]{16}\b')),
    ('bearer-credential', re.compile(rb'(?i)\bBearer\s+[A-Za-z0-9_.~+/=-]{20,}')),
    ('credential-assignment', re.compile(
        rb'''(?i)(?:^|[\s"'])(?:GH_TOKEN|GITHUB_TOKEN|OPENAI_API_KEY|AWS_SECRET_ACCESS_KEY|AZURE_CLIENT_SECRET)\s*["']?\s*[:=]\s*["']?[A-Za-z0-9_./+=-]{16,}''')),
]
CHUNK = 1024 * 1024


def metadata(value):
    return (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns, value.st_mode)


def open_regular(path):
    handle = os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), 'rb')
    if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
        handle.close()
        raise ValueError('Not a regular file: ' + str(path))
    return handle


def snapshot(path, read_bytes=False):
    """Read without following file symlinks; reject any observed concurrent edit."""
    before = path.lstat()
    with open_regular(path) as stream:
        opened = os.fstat(stream.fileno())
        if metadata(before) != metadata(opened):
            raise ValueError('File changed before read: ' + str(path))
        digest, tail, data = hashlib.sha256(), b'', []
        while chunk := stream.read(CHUNK):
            digest.update(chunk)
            window = tail + chunk
            for label, pattern in SECRETS:
                if pattern.search(window):
                    # Report the label/path only, never the suspected value.
                    raise ValueError('Suspected secret (' + label + '), refusing package: ' + str(path))
            tail = window[-512:]
            if read_bytes:
                data.append(chunk)
        after = os.fstat(stream.fileno())
    if metadata(opened) != metadata(after) or metadata(after) != metadata(path.lstat()):
        raise ValueError('File changed while reading: ' + str(path))
    record = {'sha256': digest.hexdigest(), 'size': after.st_size, 'mtime_ns': after.st_mtime_ns,
              'state': metadata(after)}
    return (record, b''.join(data)) if read_bytes else record


def excluded_directory(name):
    lower = name.lower()
    return lower in PRUNE or lower.endswith(('-cache', '_cache')) or lower == '.cache'


def raw_paths(root):
    found = []
    for top in ROOTS:
        start = root / top
        if not start.is_dir() or start.is_symlink():
            raise ValueError('Missing regular evidence directory: ' + str(start))
        for directory, dirs, names in os.walk(start, followlinks=False):
            current = Path(directory)
            for name in list(dirs):
                child = current / name
                if child.is_symlink():
                    raise ValueError('Symlink directory refused: ' + str(child))
            dirs[:] = sorted(name for name in dirs if not excluded_directory(name))
            frozen = any(part.startswith('frozen-tools') for part in current.relative_to(root).parts)
            for name in sorted(names):
                path = current / name
                disassembly_text = path.suffix.lower() in {'.nm', '.objdump'} or (not frozen and name in {'nm', 'objdump'})
                if name in RAW_NAMES or path.suffix.lower() in RAW_SUFFIXES or disassembly_text:
                    if path.is_symlink():
                        raise ValueError('Symlink evidence file refused: ' + str(path))
                    found.append(path)
    return sorted(found)


def key_paths(root):
    found = []
    for n in [64, 256]:
        for arm in ['A', 'B', 'R']:
            base = root / 'bounded-imports/runs/runtime-1679' / f'n{n}/base' / ('compile-' + arm)
            found.append((base / 'program', f'runtime-1679/n{n}/base/{arm}/ELF'))
            if arm != 'R':
                found.append((base / 'program.ll', f'runtime-1679/n{n}/base/{arm}/diagnostic-IR'))
    return found


def load_json(root, relative, records):
    path = root / relative
    record, data = snapshot(path, True)
    if record != records[str(path.relative_to(root))]:
        raise ValueError('Preflight receipt changed: ' + relative)
    return json.loads(data)


def prerequisites(root, records):
    main_root = 'multi-edit/runs/main-concurrency-990/'
    completion = load_json(root, main_root + 'completion.json', records)
    protocol = load_json(root, main_root + 'protocol.json', records)
    if completion.get('complete') is not True or completion.get('batches') != 36:
        raise ValueError('main-concurrency must be completed with all 36 batches')
    if completion.get('protocol_sha256') != records[main_root + 'protocol.json']['sha256']:
        raise ValueError('Main concurrency completion/protocol hash mismatch')
    if protocol.get('source_identities') != {'A': MAIN, 'B': MAIN}:
        raise ValueError('Main A/A concurrency source identity mismatch')
    if (protocol.get('files') != [256] or protocol.get('edits') != [16, 64]
            or protocol.get('concurrency') != [1, 2, 4] or protocol.get('blocks') != 1
            or protocol.get('order') != ['A', 'R', 'B', 'B', 'R', 'A']
            or protocol.get('cpus') != [0, 1, 2, 3]):
        raise ValueError('Main concurrency scope differs from frozen four-CPU protocol')
    path = root / (main_root + 'batches.jsonl')
    row_record, data = snapshot(path, True)
    if row_record != records[main_root + 'batches.jsonl']:
        raise ValueError('Main batch rows changed')
    rows = [json.loads(line) for line in data.splitlines()]
    if len(rows) != 36 or sum(len(row['candidates']) for row in rows) != 84:
        raise ValueError('Main concurrency actual batch/child counts incomplete')
    if any(child.get('exit') != 0 or child.get('errors') != []
           for row in rows for child in row['candidates']):
        raise ValueError('Main concurrency contains invalid final children')
    runtime_root = 'bounded-imports/runs/runtime-1679/'
    runtime = load_json(root, runtime_root + 'summary.json', records)
    runtime_protocol = load_json(root, runtime_root + 'protocol.json', records)
    execution = load_json(root, runtime_root + 'execution-manifest.json', records)
    expected_cases = {f'n{n}/' + ('base' if k == 0 else f'edit-{k}')
                      for n in [64, 256] for k in [0, 1, 16, 64]}
    if (runtime.get('format') != 'zil-bounded-imports-runtime-result/1'
            or runtime.get('status') != 'Failed' or runtime.get('gate_pass') is not False
            or runtime.get('counts') != {'clean_compilations': 24, 'native_total': 216, 'native_formal': 144}
            or runtime.get('all_native_outputs_valid') is not True
            or len(runtime.get('cases', [])) != 8
            or {case['case'] for case in runtime['cases']} != expected_cases
            or runtime.get('source_claims') != {'A': MAIN, 'B': BOUNDED}
            or execution.get('completed') is not True or execution.get('status') != 'Failed'):
        raise ValueError('1679 must be the complete eight-case failed-quality gate, not an incomplete run')
    if (runtime['protocol_sha256'] != records[runtime_root + 'protocol.json']['sha256']
            or runtime_protocol.get('source_claims') != runtime['source_claims']):
        raise ValueError('1679 source/protocol binding mismatch')
    identities = {}
    for label, relative, expected in [
        ('layout', 'multi-edit/frozen-tools-a60/identity.json', LAYOUT),
        ('bounded', 'bounded-imports/frozen-tools-1679/identity.json', BOUNDED),
    ]:
        identity = load_json(root, relative, records)
        if (identity['tools']['baseline']['source'] != MAIN
                or identity['tools']['candidate']['source'] != expected):
            raise ValueError('Frozen binary/source identity mismatch: ' + label)
        identities[label] = identity
    bounded_tools = identities['bounded']['tools']
    for arm, label in [('A', 'baseline'), ('B', 'candidate')]:
        if runtime['binaries'][arm]['sha256'] != bounded_tools[label]['sha256']:
            raise ValueError('1679 runtime binary receipt does not match source freeze')
    if any(protocol['tools'][arm]['sha256'] != bounded_tools['baseline']['sha256'] for arm in ['A', 'B']):
        raise ValueError('Main A/A concurrency binary receipt does not match main freeze')
    return {'main_concurrency': {'complete': True, 'batches': 36, 'final_children': 84,
                                 'source_identities': protocol['source_identities'],
                                 'meaning': 'Main A/A reference only; no candidate improvement claim'},
            'runtime_1679': {'complete': True, 'status': 'Failed', 'decision': 'NoGo',
                             'counts': runtime['counts'], 'source_claims': runtime['source_claims'],
                             'meaning': 'All functional outputs valid; runtime quality gate failed; not incomplete'},
            'binary_source_identities': identities}


class HashingReader:
    def __init__(self, stream):
        self.stream, self.digest = stream, hashlib.sha256()

    def read(self, size=-1):
        data = self.stream.read(size)
        self.digest.update(data)
        return data


def add_original(archive, path, archive_path, expected, executable=False):
    with open_regular(path) as stream:
        before = os.fstat(stream.fileno())
        if metadata(before) != expected['state']:
            raise ValueError('File changed before archiving: ' + str(path))
        header = tarfile.TarInfo(archive_path)
        header.size, header.mtime = expected['size'], before.st_mtime_ns // 1000000000
        header.mode = 0o755 if executable else 0o644
        reader = HashingReader(stream)
        archive.addfile(header, reader)
        if (reader.digest.hexdigest() != expected['sha256']
                or metadata(os.fstat(stream.fileno())) != expected['state']
                or metadata(path.lstat()) != expected['state']):
            raise ValueError('File changed while archiving: ' + str(path))


def add_json(archive, path, value):
    data = (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode()
    info = tarfile.TarInfo(path)
    info.size, info.mode = len(data), 0o644
    archive.addfile(info, io.BytesIO(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent,
                        help='Existing out directory containing multi-edit and bounded-imports')
    parser.add_argument('--out', type=Path, help='Final .tar.gz; replaced atomically only on complete success')
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    output = (args.out or root / 'multi-edit/zil-multi-edit-raw-evidence-20261111.tar.gz').resolve()
    if not str(output).endswith('.tar.gz') or not output.parent.is_dir():
        raise ValueError('Use an existing output parent and a .tar.gz target')
    paths = raw_paths(root)
    records = {str(path.relative_to(root)): snapshot(path) for path in paths}
    gates = prerequisites(root, records)
    keys, key_records, unique = [], {}, {}
    for path, role in key_paths(root):
        record = snapshot(path)
        if role.endswith('/ELF'):
            with open_regular(path) as stream:
                if stream.read(4) != b'\x7fELF':
                    raise ValueError('Key program is not ELF: ' + str(path))
        relative = str(path.relative_to(root))
        key_records[relative] = record
        archive_path = 'key-artifacts/' + record['sha256']
        keys.append({'original_relative_path': relative, 'archive_path': archive_path,
                     'sha256': record['sha256'], 'size': record['size'], 'role': role})
        unique.setdefault(record['sha256'], (path, role, record))
    raw_manifest = [{'original_relative_path': relative, 'archive_path': 'raw/' + relative,
                     'sha256': record['sha256'], 'size': record['size'], 'mtime_ns': record['mtime_ns']}
                    for relative, record in records.items()]
    protocols, observed = [], []
    for relative, record in records.items():
        name = Path(relative).name
        if name == 'protocol.json' or name.endswith('plan.json'):
            try:
                value = load_json(root, relative, records)
            except json.JSONDecodeError as error:
                value = {'json_parse_error': {'type': type(error).__name__, 'line': error.lineno, 'column': error.colno}}
            protocols.append({'path': relative, 'sha256': record['sha256'], 'size': record['size'],
                              'interpretation': 'Frozen preparation only; execution requires its separate raw/completion receipt',
                              **({key: value[key] for key in ['format', 'schema', 'status', 'policy',
                                 'files', 'edits', 'concurrency', 'blocks', 'order', 'states', 'counts',
                                 'source_identities', 'source_claims', 'json_parse_error'] if key in value} if isinstance(value, dict) else {})})
        if name in ['summary.json', 'completion.json', 'decision.json', 'fatal-failure.json']:
            try:
                value = load_json(root, relative, records)
            except json.JSONDecodeError as error:
                value = {'json_parse_error': {'type': type(error).__name__, 'line': error.lineno, 'column': error.colno}}
            if isinstance(value, dict):
                observed.append({'path': relative, 'sha256': record['sha256'],
                                 **{key: value[key] for key in ['format', 'status', 'complete', 'decision',
                                    'compile_requests', 'formal_requests', 'counts', 'batches', 'gate_pass', 'json_parse_error'] if key in value}})
    index = {'format': 'zil-multi-edit-raw-evidence/1',
             'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
             'gates': gates, 'raw_files': len(raw_manifest), 'logical_key_artifacts': len(keys),
             'unique_key_artifacts': len(unique), 'protocols': protocols, 'observed_scope_receipts': observed,
             'not_executed_scopes': [
                 'a60 prepared off-policy and candidate-concurrency matrices: not executed after mixed screening',
                 '1679 post-runtime compilation and concurrency screens: forbidden after runtime NoGo',
                 'available_externally replacement: history audit only, no new implementation or measurement',
                 'Rejected candidates original eight-case full weighted acceptance rerun',
                 'Rejected candidates full workspace/reference/text/remote-CI product acceptance'],
             'receipt_interpretation': 'Observed summaries/failures are preserved source claims, not promoted to new acceptance results; prepared, copied and rerun plans alone never count as executed scope. Only the two explicit prerequisite scopes are checked here.',
             'storage_boundary': 'Production source histories, deterministic fixture generators, Python/protocol tools, Markdown and patches are preserved separately in Git archive. Generated .zil/.rs trees can be reconstructed from those pinned generators and protocol parameters; their per-file hashes remain in raw protocols. No final archive SHA invented here.',
             'exclusions': 'No object/semantic/cache/incremental data, frozen compiler binaries, or other ELF/IR; no environment dump collected, no upload, no source deletion',
             'integrity': 'Per-file SHA256 plus before/after fstat/lstat, hash while tar reads, final full rehash and raw inventory rescan; any observed change rejects replacement'}
    fd, temporary = tempfile.mkstemp(prefix=output.name + '.tmp-', suffix='.tar', dir=output.parent)
    os.close(fd)
    temporary = Path(temporary)
    try:
        with tarfile.open(temporary, 'w:gz', compresslevel=6) as archive:
            for path in paths:
                relative = str(path.relative_to(root))
                add_original(archive, path, 'raw/' + relative, records[relative])
            for digest, (path, role, record) in unique.items():
                add_original(archive, path, 'key-artifacts/' + digest, record, role.endswith('/ELF'))
            add_json(archive, 'index.json', index)
            add_json(archive, 'raw-manifest.json', raw_manifest)
            add_json(archive, 'key-artifacts/index.json', keys)
        if paths != raw_paths(root):
            raise ValueError('Evidence file inventory changed during packaging')
        for relative, record in {**records, **key_records}.items():
            if snapshot(root / relative) != record:
                raise ValueError('Evidence changed after archiving: ' + relative)
        if prerequisites(root, records) != gates:
            raise ValueError('Completion/runtime identities changed during packaging')
        with temporary.open('rb') as stream:
            os.fsync(stream.fileno())
        result = snapshot(temporary)
        os.replace(temporary, output)
        directory = os.open(output.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        print(json.dumps({'output': str(output), 'sha256': result['sha256'], 'bytes': result['size'],
                          'raw_count': len(raw_manifest), 'key_logical_count': len(keys),
                          'key_unique_count': len(unique), 'tar_file_count': len(raw_manifest) + len(unique) + 3}))
    except BaseException:
        # Preserve the unique partial temporary archive and the prior final file.
        print(json.dumps({'status': 'Refused; prior final archive and every source retained',
                          'partial_temporary': str(temporary)}))
        raise


if __name__ == '__main__':
    main()
