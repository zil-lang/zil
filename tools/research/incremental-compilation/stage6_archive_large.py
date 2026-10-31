#!/usr/bin/env python3
"""Stream large stage6 evidence into deduplicated blobs and bounded gzip manifests.

This is an independent schema-2 collector. Existing schema-1 packages are untouched.
Every selected path remains in a SHA-bound retained or omitted JSONL manifest.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import tempfile

PART_LIMIT = 40 * 1024 * 1024
BINARY_SUFFIXES = {'.o', '.a', '.bc', '.bin', '.rlib', '.rmeta', '.so', '.dylib',
                   '.dll', '.exe', '.zobj', '.pyc', '.zsem', '.zsemantic', '.ztyped'}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write_json(path, value):
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def roots_for(kind, repo, research):
    stage = research / 'stage6'
    extra_sources = []
    external_sources = []
    if kind == 'session':
        roots = sorted(p for p in stage.iterdir() if p.name.startswith('session-'))
        roots.append(stage / 'final_vs_3d6-prepare.log')
        roots.append(stage / 'ee_vs_old_db-prepare.log')
        roots.append(stage / 'reactive-key-attribution')
        tools = ['stage6_session_bench.py', 'stage6_session_probe.rs']
    elif kind == 'session-review':
        roots = [research / 'stage6-session-review']
        tools = ['stage6_session_bench.py', 'stage6_session_probe.rs']
    elif kind == 'syntax-session':
        roots = sorted(p for p in stage.iterdir() if p.name.startswith('session-syntax-'))
        roots += [stage / 'project-syntax-baseline' / name for name in ['SOURCE.json', 'release.log']]
        roots += [research / 'stage6-project-syntax' / name for name in ['root-identity.json', 'release.log']]
        tools = ['stage6_session_bench.py', 'stage6_session_probe.rs']
    elif kind == 'syntax-session-review':
        roots = [research / 'stage6-syntax-session-review']
        tools = ['stage6_session_bench.py', 'stage6_session_probe.rs']
    elif kind == 'integration-correctness':
        roots = [stage / 'integration-checks']
        review = research / 'stage6-integration-review'
        roots += sorted(p for p in review.iterdir() if p.is_file()
                        or p.is_dir() and p.name.startswith('controls'))
        roots += [research / 'upstream-integration' / name for name in [
            'STATIC.md', 'STATIC.json', 'resolve-expand.patch', 'resolved-expand-lib.rs',
        ]]
        roots += sorted(stage.glob('old-rust-cache-cleanup-2-*'))
        roots.append(stage / 'remaining-old-rust-cache-scan.json')
        tools = []
    elif kind == 'ci-disk':
        roots = [stage / 'ci-disk-recovery', stage / 'ci-disk-recovery-review']
        roots += [stage / ('github-41207249551-' + name) for name in [
            'annotations.json', 'checks.json', 'jobs.json', 'failed.log', 'logs.zip',
        ]]
        roots += [stage / ('ci-disk-recovery-41209327141.' + suffix) for suffix in ['json', 'zip']]
        tools = []
        extra_sources = [repo / name for name in [
            '.github/workflows/verify-main.yml', 'tools/ci/research_disk_space.py',
            'changes/research-ci-disk-recovery.md',
        ]]
    elif kind == 'screen-cache-cleanup':
        roots = sorted(stage.glob('archived-rust-incremental-cleanup-*'))
        tools = []
    elif kind == 'ci-main-412093':
        roots = [stage / ('github-41209327141-' + name) for name in [
            'run.json', 'jobs.json', 'artifacts.json', 'logs.zip',
        ]]
        tools = []
    elif kind in ['llvm-entry', 'llvm-lvi', 'llvm-cfg', 'llvm-ssa', 'llvm-combined', 'research-ci-tools', 'benchmark-tools', 'runtime-final-v1', 'confirmation-default', 'confirmation-default-cache-cleanup', 'confirmation-closed', 'confirmation-closed-cache-cleanup', 'ci-dual-df04', 'confirmation-real', 'quality-closure-v1', 'confirmation-isolation', 'quality-final-a1ab']:
        plan_path = repo / ('evidence/research/incremental-compilation/stage6/' + kind + '-plan.json')
        plan = json.loads(plan_path.read_text())
        roots = [research / name for name in plan['research_roots']]
        tools = plan.get('tools', ['jump_threading_plugin'])
        extra_sources = [plan_path] + [repo / name for name in plan['repo_extra_sources']]
        external_sources = [(row['logical_path'], Path(row['path'])) for row in plan['external_sources']]
        for row in plan['tool_source_inventory']:
            path = repo / row['path']
            require(path.is_file() and path.stat().st_size == row['bytes']
                    and sha(path) == row['sha256'], 'frozen tool source differs: ' + row['path'])
    elif kind == 'scalar-screens':
        roots = sorted(p for p in stage.iterdir() if p.name.startswith('scalar-screen-'))
        roots.append(stage / 'scalar-screens.log')
        tools = ['stage6_bench.py']
    elif kind == 'scalar-correctness':
        review = research / 'stage6-scalar-review'
        roots = sorted(p for p in review.iterdir() if p.is_file()) + [review / 'differential']
        roots += sorted(p for p in stage.iterdir() if p.name.startswith('scalar-')
                        and not p.name.startswith('scalar-screen') and p.is_file())
        tools = ['stage6_bench.py']
    elif kind == 'diagnostics':
        plan_path = repo / 'evidence/research/incremental-compilation/stage6/diagnostics-plan.json'
        plan = json.loads(plan_path.read_text())
        roots = [stage / name for name in plan['directories'] + plan['files']]
        roots += [stage / name for name in [
            'ca79-pass-profile', 'run-ca79-pass-profile.py', 'ca79-pass-profile.log',
            'ca79-opaque8192-default-early', 'ca79-opaque8192-default-early.stdout',
            'ca79-opaque8192-default-early.stderr',
        ]]
        tools = ['pass_timing', 'slp_ablation']
        extra_sources = [plan_path] + [repo / 'changes' / name for name in [
            'research-api-pass-timing.md', 'research-o2-slp-ablation.md',
            'research-effective-slp-control.md',
        ]]
    elif kind == 'syntax-correctness':
        author = research / 'stage6-project-syntax'
        review = research / 'stage6-syntax-review'
        roots = sorted(p for p in author.iterdir() if p.is_file())
        roots += sorted(p for p in review.iterdir() if p.is_file())
        roots += [stage / 'project-syntax-baseline' / name for name in ['SOURCE.json', 'release.log']]
        tools = ['stage6_session_bench.py', 'stage6_session_probe.rs']
    else:
        raise ValueError('unknown package kind')
    require(all(p.exists() for p in roots), 'missing selected root')
    result = [('research/' + str(p.relative_to(research)), p) for p in roots]
    result += [('collection-source/tools/research/incremental-compilation/' + name,
                repo / 'tools/research/incremental-compilation' / name) for name in tools]
    result += [('collection-source/' + str(path.relative_to(repo)), path) for path in extra_sources]
    result += external_sources
    result.append(('collection-source/tools/research/incremental-compilation/' + Path(__file__).name,
                   Path(__file__).resolve()))
    require(all(path.exists() for _, path in result), 'missing selected source tool')
    return result


def walk(root):
    if root.is_file() or root.is_symlink():
        yield root
        return
    for parent, directories, files in os.walk(root, followlinks=False):
        directories.sort()
        for name in list(directories):
            path = Path(parent) / name
            if path.is_symlink():
                yield path
                directories.remove(name)
        for name in sorted(files):
            yield Path(parent) / name


def reason_for(path, header, size):
    if path.is_symlink():
        return 'symlink-target-recorded-without-dereference'
    # Native stdin and actual perf samples are evidence, regardless of suffix.
    if path.name in ['input.bin', 'perf.data'] or path.name.endswith('expected.bin'):
        return None
    if '__pycache__' in path.parts or path.suffix == '.pyc':
        return 'generated-python-bytecode'
    if path.suffix in BINARY_SUFFIXES or '.so.' in path.name:
        return 'compiled-artifact-or-binary-cache'
    if path.name.endswith('.lock') and 'incremental' in path.parts:
        return 'ephemeral-rust-incremental-lock'
    if path.name == 'llvmcache.timestamp':
        return 'ephemeral-thin-cache-timestamp'
    if path.suffix == '.candidate' and size > 256 * 1024 and 'stage6-semantic-review' not in path.parts:
        return 'large-semantic-cache-candidate'
    if header.startswith(b'\x7fELF'):
        return 'elf-executable-or-object'
    if header.startswith((b'BC\xc0\xde', b'\xde\xc0\x17\x0b', b'!<arch>\n')):
        return 'llvm-bitcode-or-static-archive'
    return None


class JsonlWriter:
    def __init__(self, path):
        self.path = path
        self.stream = gzip.open(path, 'wb', compresslevel=1)
        self.hasher = hashlib.sha256()
        self.bytes = 0
        self.rows = 0

    def write(self, row):
        data = (json.dumps(row, separators=(',', ':')) + '\n').encode()
        self.stream.write(data)
        self.hasher.update(data)
        self.bytes += len(data)
        self.rows += 1

    def finish(self):
        self.stream.close()
        return {'encoding': 'gzip-jsonl', 'rows': self.rows,
                'logical_bytes': self.bytes, 'logical_sha256': self.hasher.hexdigest(),
                'storage': store_file(self.path)}


def store_file(path):
    descriptor = {'logical_path': path.name, 'bytes': path.stat().st_size, 'sha256': sha(path)}
    if descriptor['bytes'] <= PART_LIMIT:
        descriptor['parts'] = [{'path': path.name, 'bytes': descriptor['bytes'], 'sha256': descriptor['sha256']}]
        return descriptor
    parts = []
    combined = hashlib.sha256()
    with path.open('rb') as stream:
        while data := stream.read(PART_LIMIT):
            part = path.with_name(f'{path.name}.part-{len(parts) + 1:03d}')
            require(not part.exists(), 'part already exists')
            part.write_bytes(data)
            key = digest(data)
            require(sha(part) == key, 'written part differs')
            parts.append({'path': part.name, 'bytes': len(data), 'sha256': key})
            combined.update(data)
    require(combined.hexdigest() == descriptor['sha256'], 'segmented stream differs')
    descriptor['parts'] = parts
    path.unlink()
    return descriptor


def open_stored(root, descriptor):
    temporary = tempfile.TemporaryFile()
    combined = hashlib.sha256()
    total = 0
    for part in descriptor['parts']:
        path = root / part['path']
        require(path.stat().st_size == part['bytes'], 'part size differs')
        part_hash = hashlib.sha256()
        with path.open('rb') as stream:
            while data := stream.read(1024 * 1024):
                combined.update(data)
                part_hash.update(data)
                total += len(data)
                temporary.write(data)
        require(part_hash.hexdigest() == part['sha256'], 'part SHA differs')
    require(total == descriptor['bytes'] and combined.hexdigest() == descriptor['sha256'],
            'stored stream differs')
    temporary.seek(0)
    return temporary


def read_manifest(root, descriptor):
    logical = hashlib.sha256()
    size = count = 0
    with open_stored(root, descriptor['storage']) as stored:
        with gzip.GzipFile(fileobj=stored, mode='rb') as stream:
            for line in stream:
                logical.update(line)
                size += len(line)
                count += 1
                yield json.loads(line)
    require(size == descriptor['logical_bytes'] and count == descriptor['rows'] and
            logical.hexdigest() == descriptor['logical_sha256'], 'logical manifest differs')


def add_bytes(bundle, name, data):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = 0o644
    info.mtime = 0
    bundle.addfile(info, io.BytesIO(data))


def add_path(bundle, name, path, size, key):
    class HashingReader:
        def __init__(self, stream):
            self.stream = stream
            self.hasher = hashlib.sha256()
            self.bytes = 0

        def read(self, count=-1):
            data = self.stream.read(count)
            self.hasher.update(data)
            self.bytes += len(data)
            return data

    require(path.stat().st_size == size, 'source size changed before archive')
    info = tarfile.TarInfo(name)
    info.size = size
    info.mode = 0o644
    info.mtime = 0
    with path.open('rb') as stream:
        reader = HashingReader(stream)
        bundle.addfile(info, reader)
        require(reader.bytes == size and reader.hasher.hexdigest() == key,
                'source changed while archiving')


def collect(args):
    require(not args.out.exists(), 'output directory already exists')
    roots = roots_for(args.kind, args.repo, args.research)
    planned_omissions = {}
    planned_preservation = set()
    if args.kind in ['llvm-entry', 'llvm-lvi', 'llvm-cfg', 'llvm-ssa', 'llvm-combined', 'research-ci-tools', 'benchmark-tools', 'runtime-final-v1', 'confirmation-default', 'confirmation-default-cache-cleanup', 'confirmation-closed', 'confirmation-closed-cache-cleanup', 'ci-dual-df04', 'confirmation-real', 'quality-closure-v1', 'confirmation-isolation', 'quality-final-a1ab']:
        plan_path = args.repo / ('evidence/research/incremental-compilation/stage6/' + args.kind + '-plan.json')
        plan = json.loads(plan_path.read_text())
        planned_omissions = plan.get('omission_paths', {})
        planned_preservation = set(plan.get('preserve_paths', []))
        require(not planned_preservation.intersection(planned_omissions), 'conflicting planned dispositions')
    args.out.mkdir(parents=True)
    print(json.dumps({'phase': 'start', 'kind': args.kind, 'pid': os.getpid()}), flush=True)
    write_json(args.out / 'ROOTS.json', [{'prefix': name, 'source_path': str(path)} for name, path in roots])
    retained = JsonlWriter(args.out / 'FILES.jsonl.gz')
    omitted = JsonlWriter(args.out / 'OMITTED.jsonl.gz')
    blobs, seen, counts, sizes = {}, set(), Counter(), Counter()
    total = 0
    for label, root in roots:
        print(json.dumps({'phase': 'scan-and-hash', 'root': label}), flush=True)
        for path in walk(root):
            name = label if root.is_file() or root.is_symlink() else label + '/' + str(path.relative_to(root))
            require(name not in seen, 'duplicate selected path')
            seen.add(name)
            before = path.lstat()
            if path.is_symlink():
                payload = os.readlink(path).encode()
                header, key = b'', digest(payload)
            else:
                hasher = hashlib.sha256()
                with path.open('rb') as stream:
                    header = stream.read(8)
                    hasher.update(header)
                    while data := stream.read(1024 * 1024):
                        hasher.update(data)
                key = hasher.hexdigest()
            after = path.lstat()
            require((before.st_size, before.st_mtime_ns, before.st_mode) ==
                    (after.st_size, after.st_mtime_ns, after.st_mode), 'input changed during capture')
            reason = reason_for(path, header, before.st_size)
            if name in planned_omissions:
                reason = planned_omissions[name]
            if name in planned_preservation:
                require(not path.is_symlink(), 'planned payload preservation requires a regular file')
                reason = None
            row = {'path': name, 'source_path': str(path), 'bytes': before.st_size,
                   'mode': stat.S_IMODE(before.st_mode), 'sha256': key,
                   'sha256_scope': 'symlink-target-text' if path.is_symlink() else 'file-bytes'}
            if reason:
                row.update(disposition='omit', reason=reason)
                if path.is_symlink():
                    row['link_target'] = os.readlink(path)
                omitted.write(row)
            else:
                row.update(disposition='retain', archive_member='blobs/sha256/' + key)
                retained.write(row)
                blobs.setdefault(key, (path, before.st_size))
            counts[reason or 'retained-raw-evidence'] += 1
            sizes[reason or 'retained-raw-evidence'] += before.st_size
            total += 1
            if total % 100000 == 0:
                print(json.dumps({'phase': 'scan-and-hash', 'paths': total}), flush=True)
    manifests = {'FILES': retained.finish(), 'OMITTED': omitted.finish()}
    summary = [{'class': key, 'files': counts[key], 'bytes': sizes[key]} for key in sorted(counts)]
    write_json(args.out / 'PLAN.json', {'kind': args.kind, 'summary': summary,
                                      'selected_paths': total, 'unique_blobs': len(blobs)})
    metadata = {}
    for manifest in manifests.values():
        for part in manifest['storage']['parts']:
            metadata[part['path']] = {'bytes': part['bytes'], 'sha256': part['sha256']}
    for name in ['ROOTS.json', 'PLAN.json']:
        path = args.out / name
        metadata[name] = {'bytes': path.stat().st_size, 'sha256': sha(path)}
    print(json.dumps({'phase': 'compressing', 'paths': total, 'unique_blobs': len(blobs),
                      'unique_bytes': sum(size for _, size in blobs.values())}), flush=True)
    archive_path = args.out / 'raw.tar.gz'
    with tarfile.open(archive_path, 'w:gz', compresslevel=1) as bundle:
        for key, (path, size) in sorted(blobs.items()):
            add_path(bundle, 'blobs/sha256/' + key, path, size, key)
        for name in sorted(metadata):
            add_path(bundle, name, args.out / name, metadata[name]['bytes'], metadata[name]['sha256'])
    identity = {
        'schema': 2, 'kind': args.kind, 'collected_utc': datetime.now(timezone.utc).isoformat(),
        'head_at_collection': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.repo, text=True).strip(),
        'collector_sha256': sha(Path(__file__)), 'archive': store_file(archive_path),
        'manifests': manifests, 'metadata': metadata, 'summary': summary,
        'retained_paths': retained.rows, 'omitted_paths': omitted.rows, 'unique_blobs': len(blobs),
        'boundary': 'Original raw bytes and source identities are retained, with each original path mapped to one content blob. JSONL manifests are gzip-compressed externally and internally; every stream and each <=40MiB segment is SHA-bound. Omitted binaries/cache bytes are individually inventoried. Source and request identities in original receipts remain authoritative; collection HEAD is not a measured compiler identity. Only retained files and ordinary permissions are reconstructible; not omitted files, symlinks, hardlink topology or timestamps.',
        'scope': (plan['archive_scope'] if args.kind in ['llvm-combined', 'research-ci-tools', 'benchmark-tools', 'runtime-final-v1', 'confirmation-default', 'confirmation-default-cache-cleanup', 'confirmation-closed', 'confirmation-closed-cache-cleanup', 'ci-dual-df04', 'confirmation-real', 'quality-closure-v1', 'confirmation-isolation', 'quality-final-a1ab'] else
                  'Two completed formal Session roots plus preparation/control evidence. session-final-vs-3d6 is SupersededPrepared only and contributes no measured requests.' if args.kind == 'session' else
                  'Independent Session review, including its full large inventory and recomputation.' if args.kind == 'session-review' else
                  'Two completed six-block AST Session windows: ccf1 versus ffe4 isolation and 3d6 versus ffe4 combination. Frozen raw controls, requests, exclusive-window receipts and original build/source identities. Collection HEAD is a later compiler and does not relabel these measurements.' if args.kind == 'syntax-session' else
                  'Independent audit of both frozen AST Session windows, complete source/request inventory, fixed paired bootstrap, Parse/Verify/object/native controls and preserved reviewer schema-error attempt.' if args.kind == 'syntax-session-review' else
                  'Frozen ac4 upstream integration scoped correctness and 888 matching DB unit-test repair, independent review and retained fixture/CI failures. Exact review/worktree/target trees and active API/plugin/runtime/SSA work are excluded. The later in-progress CI run is not a pass or an archived completed run. Includes identity-bound old Rust-cache cleanup receipts.' if args.kind == 'integration-correctness' else
                  'CI disk recovery only: original 41207249551 ENOSPC failure, empty downloaded logs, author/reviewer controls, root-discovered sudo-parent and umask corrections, and actual uploaded cleanup receipt from still-in-progress 41209327141. Receipt Pass is not a whole-CI or compiler-test Pass.' if args.kind == 'ci-disk' else
                  'Frozen independent entry-preserving JumpThreading v1: original/entry clone, native DSOs, ac4 API pair and two independent reviews with all failure epochs. Preserves cache ZIP controls, native input/oracle bytes and official-baseline perf; these are not final combined or formal end-to-end performance results. SDK/archive and reproducible source trees have explicit SHA-bound receipt exclusions; active SSA/LVI/CFG/runtime work is excluded.' if args.kind == 'llvm-entry' else
                  'Exact 36 old-screen Rust incremental-cache cleanup after full SHA comparison with frozen archive identities. Preserves initial schema-check failure before any deletion and complete verification/deletion receipts. No source/test/target or old evidence package deletion.' if args.kind == 'screen-cache-cleanup' else
                  'Completed successful official CI run 41209327141 on exact head 2e8f8c1f6e4bcb92d829cfa91cc4582c230f1e3e: raw run/jobs/artifact-list metadata and full downloaded logs ZIP. This is not CI acceptance of later combined toolchains, new research workflow or current HEAD. Prior in-progress receipts remain frozen separately.' if args.kind == 'ci-main-412093' else
                  'Frozen isolated ' + args.kind + ' candidate sources, licenses, functional diagnostics, failure epochs and independent reviews. No combined-toolchain, final runtime or formal performance acceptance. Selected compiled artifacts are identity-inventoried; original input/oracle/perf bytes are retained.' if args.kind in ['llvm-lvi', 'llvm-cfg', 'llvm-ssa'] else
                  'Two complete scalar screening windows, 160 requests each; two-block screening only.' if args.kind == 'scalar-screens' else
                  'Frozen pass/SLP/perf diagnostics and ca79 profiles. Initial instrumentation failures, invalid SLP setter ablation, adaptive NotRun cases and perf permission failures remain distinct; no formal exclusive timing claim. Active jump-threading plugin candidates and SDK/build targets are outside scope.' if args.kind == 'diagnostics' else
                  'Syntax author/reviewer correctness, failed initial fixtures, fixed controls and root build identities. Exact source checkouts/shared targets are outside scope; commit/source receipts retain their identities. Formal syntax Session windows are separate.' if args.kind == 'syntax-correctness' else
                  'Scalar author/reviewer controls and release identity. Reviewer exact worktree is outside scope; its commit and source hashes are retained.'),
    }
    write_json(args.out / 'IDENTITY.json', identity)
    print(json.dumps({'out': str(args.out), 'retained_paths': retained.rows, 'omitted_paths': omitted.rows,
                      'unique_blobs': len(blobs), 'archive_bytes': identity['archive']['bytes'],
                      'archive_parts': identity['archive']['parts']}, indent=2), flush=True)


def verify(root, destination=None):
    identity = json.loads((root / 'IDENTITY.json').read_text())
    require(identity['schema'] == 2, 'use original collector for schema 1 packages')
    blobs, paths = {}, set()
    retained_count = omitted_count = 0
    for row in read_manifest(root, identity['manifests']['FILES']):
        require(row['path'] not in paths, 'duplicate path')
        paths.add(row['path'])
        require(row['archive_member'] == 'blobs/sha256/' + row['sha256'], 'blob identity differs')
        item = (row['bytes'], row['sha256'])
        require(row['archive_member'] not in blobs or blobs[row['archive_member']] == item,
                'conflicting blob metadata')
        blobs[row['archive_member']] = item
        retained_count += 1
    for row in read_manifest(root, identity['manifests']['OMITTED']):
        require(row['path'] not in paths, 'duplicate omitted path')
        paths.add(row['path'])
        require(row['disposition'] == 'omit' and row['reason'] and len(row['sha256']) == 64,
                'incomplete omission identity')
        omitted_count += 1
    require(retained_count == identity['retained_paths'] and omitted_count == identity['omitted_paths'],
            'path counts differ')
    require(len(blobs) == identity['unique_blobs'], 'unique blob count differs')
    expected = set(blobs) | set(identity['metadata'])
    if destination:
        require(not destination.exists(), 'destination already exists')
        destination.mkdir(parents=True)
    with tempfile.TemporaryDirectory() as temporary:
        blob_root = Path(temporary)
        with open_stored(root, identity['archive']) as stored:
            with tarfile.open(fileobj=stored, mode='r|gz') as bundle:
                observed = set()
                for member in bundle:
                    require(member.isfile() and member.name in expected and member.name not in observed,
                            'unexpected/duplicate archive member')
                    observed.add(member.name)
                    if member.name in blobs:
                        size, key = blobs[member.name]
                    else:
                        meta = identity['metadata'][member.name]
                        size, key = meta['bytes'], meta['sha256']
                        require(sha(root / member.name) == key, 'external metadata differs')
                    require(member.size == size, 'member size differs')
                    member_hash = hashlib.sha256()
                    output = (blob_root / key).open('wb') if destination and member.name in blobs else None
                    with bundle.extractfile(member) as member_stream:
                        while data := member_stream.read(1024 * 1024):
                            member_hash.update(data)
                            if output:
                                output.write(data)
                    if output:
                        output.close()
                    require(member_hash.hexdigest() == key, 'member bytes differ')
                require(observed == expected, 'archive members missing')
        if destination:
            for row in read_manifest(root, identity['manifests']['FILES']):
                relative = Path(row['path'])
                require(not relative.is_absolute() and '..' not in relative.parts, 'unsafe output path')
                target = destination / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                with (blob_root / row['sha256']).open('rb') as source, target.open('wb') as output:
                    while data := source.read(1024 * 1024):
                        output.write(data)
                target.chmod(row['mode'] & 0o777)
    result = {'kind': identity['kind'], 'status': 'AllBlobsManifestsAndSegmentsMatch',
              'retained_paths': retained_count, 'omitted_paths': omitted_count,
              'unique_blobs': len(blobs), 'identity_sha256': sha(root / 'IDENTITY.json'),
              'verified_utc': datetime.now(timezone.utc).isoformat()}
    if not destination:
        write_json(root / 'VERIFY.json', result)
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=['session', 'session-review', 'syntax-session', 'syntax-session-review', 'integration-correctness', 'ci-disk', 'ci-main-412093', 'screen-cache-cleanup', 'llvm-entry', 'llvm-lvi', 'llvm-cfg', 'llvm-ssa', 'llvm-combined', 'research-ci-tools', 'benchmark-tools', 'runtime-final-v1', 'confirmation-default', 'confirmation-default-cache-cleanup', 'confirmation-closed', 'confirmation-closed-cache-cleanup', 'ci-dual-df04', 'confirmation-real', 'quality-closure-v1', 'confirmation-isolation', 'quality-final-a1ab', 'scalar-screens', 'scalar-correctness', 'diagnostics', 'syntax-correctness'])
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--research', type=Path, default=Path('/workspace/.zil-env/research'))
    parser.add_argument('--out', type=Path)
    parser.add_argument('--verify', type=Path)
    parser.add_argument('--extract', type=Path)
    parser.add_argument('--dest', type=Path)
    args = parser.parse_args()
    if args.verify or args.extract:
        require(not args.extract or args.dest, '--extract requires --dest')
        verify(args.verify or args.extract, args.dest if args.extract else None)
    else:
        require(args.kind and args.out, '--kind and --out are required')
        args.repo = args.repo.resolve()
        args.research = args.research.resolve()
        collect(args)


if __name__ == '__main__':
    main()
