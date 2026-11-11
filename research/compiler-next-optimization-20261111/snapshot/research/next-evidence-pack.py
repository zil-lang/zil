#!/usr/bin/env python3
"""Package existing evidence only; no compiler/tool/native invocation.

Default payload: all out/next-optimization, structured corpus, default ThinLTO
pilots/argv0/process controls/decoded, strict decoded, and exactly five release
ZIL executables. --plugin explicitly names each new plugin binary; --add names
additional evidence paths under --root. Full research source belongs in Git;
no repository, SDK, LLVM installation, or target build tree is implicitly added.
--tool hashes an invocation/resolved tool identity but DOES NOT copy tool bytes.

No I/O-heavy scan without --execute. --out is a NEW directory (never reused),
containing zil-compiler-next-evidence-20261111.tar.gz, receipt.json and final SHA256SUMS. A failure retains
partial artifacts plus Failed receipt; retry requires another unique --out.
Symlinks are rejected unless their exact root-relative path is allowlisted;
allowed links must be relative, resolve inside the selected payload and never
serve as a traversed directory. Tar names/links are independently read back.
Over 50 MiB keeps the entire evidence and marks prepared-upload-required.
"""
import argparse
import datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import tarfile
import traceback

DEFAULTS = [
    'out/next-optimization',
    'research/structured-emission/corpus',
    'research/thinlto-prototype/pilot-2unit-v1',
    'research/thinlto-prototype/pilot-2unit-v2',
    'research/thinlto-prototype/argv0-probes',
    'research/thinlto-prototype/process-scope-pilot',
    'research/thinlto-prototype/decoded-default-v1',
    'research/thinlto-complete-visibility/decoded-strict-v1',
    'targets/main990/release/zil',
    'targets/structured-ee0/release/zil',
    'targets/builder-9f0/release/zil',
    'targets/thinlto-eb5/release/zil',
    'targets/thin-complete-2549/release/zil',
]
META = '__evidence_pack__/MANIFEST.json'
CHUNK = 1024 * 1024

def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()

def safe_name(name):
    p = PurePosixPath(name)
    if not name or p.is_absolute() or any(v in ('', '.', '..') for v in name.split('/')) or '\\' in name or '\x00' in name:
        raise ValueError('unsafe archive path: ' + repr(name))
    return name

def stamp(s):
    return (s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns)

def digest_file(path):
    # O_NOFOLLOW also closes the final-component symlink swap race.
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode):
            raise ValueError('not regular file: ' + str(path))
        h = hashlib.sha256()
        with os.fdopen(fd, 'rb', closefd=False) as f:
            for block in iter(lambda: f.read(CHUNK), b''):
                h.update(block)
        after = os.fstat(fd)
        if stamp(before) != stamp(after) or stamp(after) != stamp(path.lstat()):
            raise ValueError('file changed while hashing: ' + str(path))
        return h.hexdigest(), before
    finally:
        os.close(fd)

def write_receipt(path, value):
    # Replaces only this invocation's own receipt inside its exclusive directory.
    temporary = path.with_suffix('.json.tmp')
    with temporary.open('xb') as f:
        f.write(encoded(value))
    os.replace(temporary, path)

def relative_input(root, raw):
    p = Path(os.path.abspath(raw if Path(raw).is_absolute() else root / raw))
    name = p.relative_to(root).as_posix()
    safe_name(name)
    # Never traverse a symlink directory, even one pointing back under root.
    for parent in p.parents:
        if parent == root:
            break
        if parent.is_symlink():
            raise ValueError('symlink ancestor: ' + str(parent))
    return name

def inventory(root, selections, allowlinks):
    found = {}
    def visit(name):
        safe_name(name)
        if name in found:
            return
        if name == '__evidence_pack__' or name.startswith('__evidence_pack__/'):
            raise ValueError('reserved archive metadata path')
        p = root / name
        s = p.lstat()
        row = {'path': name, 'mode': stat.S_IMODE(s.st_mode), 'source_mtime_ns': s.st_mtime_ns,
               'source_ctime_ns': s.st_ctime_ns, 'source_inode': s.st_ino, 'source_device': s.st_dev}
        if stat.S_ISLNK(s.st_mode):
            if name not in allowlinks:
                raise ValueError('unlisted symlink: ' + name)
            target = os.readlink(p)
            if Path(target).is_absolute() or '\\' in target or '\x00' in target:
                raise ValueError('unsafe symlink target: ' + name)
            resolved = p.resolve(strict=True)
            resolved.relative_to(root)
            row.update(type='symlink', bytes=len(os.fsencode(target)), target=target,
                       sha256=hashlib.sha256(os.fsencode(target)).hexdigest(), resolved_path=resolved.relative_to(root).as_posix())
        elif stat.S_ISDIR(s.st_mode):
            row.update(type='directory', bytes=0)
        elif stat.S_ISREG(s.st_mode):
            h, observed = digest_file(p)
            if stamp(s) != stamp(observed):
                raise ValueError('file changed during enumeration: ' + name)
            row.update(type='file', bytes=s.st_size, sha256=h)
        else:
            raise ValueError('special file not permitted: ' + name)
        found[name] = row
        if row['type'] == 'directory':
            for child in sorted(p.iterdir()):
                visit(child.relative_to(root).as_posix())
    for name in selections:
        visit(name)
    unused = allowlinks - set(found)
    if unused:
        raise ValueError('allowlisted symlinks absent from payload: ' + repr(sorted(unused)))
    for name, row in found.items():
        if row['type'] == 'symlink' and row['resolved_path'] not in found:
            raise ValueError('symlink target not selected: ' + name)
    return [found[name] for name in sorted(found)]

def tools_identity(paths):
    result = []
    for raw in paths:
        invocation = Path(os.path.abspath(raw))
        target = invocation.resolve(strict=True)
        h, s = digest_file(target)
        result.append({'invocation_path': str(invocation), 'resolved_path': str(target),
                       'sha256': h, 'bytes': s.st_size, 'mode': stat.S_IMODE(s.st_mode),
                       'invocation_symlink': os.readlink(invocation) if invocation.is_symlink() else None,
                       'tool_binary_included': False})
    return result

def tar_header(name, mode, kind=tarfile.REGTYPE, size=0, target=''):
    t = tarfile.TarInfo(safe_name(name))
    t.mode, t.type, t.size, t.linkname = mode, kind, size, target
    t.uid = t.gid = t.mtime = 0
    t.uname = t.gname = ''
    return t

def package(path, root, rows, manifest_bytes):
    identities = {}
    regular_count = hardlinks = 0
    with path.open('xb') as raw:
        with gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0, compresslevel=6) as zipped:
            with tarfile.open(fileobj=zipped, mode='w|', format=tarfile.PAX_FORMAT) as tar:
                tar.addfile(tar_header(META, 0o644, size=len(manifest_bytes)), io.BytesIO(manifest_bytes))
                for row in rows:
                    name, mode = row['path'], row['mode']
                    if row['type'] == 'directory':
                        tar.addfile(tar_header(name, mode, tarfile.DIRTYPE))
                    elif row['type'] == 'symlink':
                        tar.addfile(tar_header(name, mode, tarfile.SYMTYPE, target=row['target']))
                    else:
                        key = row['sha256'], mode, row['bytes']
                        if key in identities:
                            tar.addfile(tar_header(name, mode, tarfile.LNKTYPE, target=identities[key]))
                            hardlinks += 1
                        else:
                            fd = os.open(root / name, os.O_RDONLY | os.O_NOFOLLOW)
                            with os.fdopen(fd, 'rb') as f:
                                tar.addfile(tar_header(name, mode, size=row['bytes']), f)
                            identities[key] = name
                            regular_count += 1
    return {'regular_payload_members': regular_count, 'hardlink_payload_members': hardlinks}

def readback(path, rows, manifest_bytes):
    expected = {row['path']: row for row in rows}
    seen, verified_files = set(), {}
    with tarfile.open(path, mode='r|gz') as tar:
        for member in tar:
            name = safe_name(member.name)
            if name in seen:
                raise ValueError('duplicate tar member: ' + name)
            seen.add(name)
            if name == META:
                if not member.isfile() or member.mode != 0o644 or tar.extractfile(member).read() != manifest_bytes:
                    raise ValueError('embedded manifest changed')
                continue
            row = expected[name]
            if member.mode != row['mode']:
                raise ValueError('tar mode differs: ' + name)
            if row['type'] == 'directory':
                if not member.isdir():
                    raise ValueError('tar directory type differs')
            elif row['type'] == 'symlink':
                if not member.issym() or member.linkname != row['target']:
                    raise ValueError('tar symlink differs')
            else:
                if member.islnk():
                    safe_name(member.linkname)
                    actual = verified_files[member.linkname]
                elif member.isfile():
                    h, count = hashlib.sha256(), 0
                    if member.size != row['bytes']:
                        raise ValueError('tar declared size differs: ' + name)
                    with tar.extractfile(member) as stream:
                        for block in iter(lambda: stream.read(CHUNK), b''):
                            h.update(block)
                            count += len(block)
                    actual = (h.hexdigest(), member.mode, count)
                else:
                    raise ValueError('unexpected tar file type')
                if actual != (row['sha256'], row['mode'], row['bytes']):
                    raise ValueError('tar bytes differ: ' + name)
                verified_files[name] = actual
    if seen != set(expected) | {META}:
        raise ValueError('tar members missing')
    return len(seen)

def main():
    q = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    q.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    q.add_argument('--out', type=Path, required=True, help='exclusive NEW output directory, outside every selected root')
    q.add_argument('--plugin', action='append', default=[], help='explicit plugin binary under root; at least one required for execution')
    q.add_argument('--add', action='append', default=[], help='additional exact evidence path under root')
    q.add_argument('--tool', action='append', default=[], help='identity only, including external SDK tools; bytes NOT copied')
    q.add_argument('--allow-symlink', action='append', default=[], help='exact root-relative symlink path; never a wildcard')
    q.add_argument('--execute', action='store_true')
    a = q.parse_args()
    root = a.root.resolve(strict=True)
    output = Path(os.path.abspath(a.out))
    for parent in output.parents:
        if parent.is_symlink():
            raise ValueError('symlink output ancestor: ' + str(parent))
    if not a.execute:
        selections = sorted(set(relative_input(root, raw) for raw in DEFAULTS + a.plugin + a.add))
        print(json.dumps({'status': 'not-executed-no-source-scan', 'root': str(root), 'selected_roots': selections,
                          'explicit_plugin_paths': a.plugin, 'tools_identity_only': a.tool,
                          'exclusions': ['full git repositories', 'SDK/tool binaries', 'target build trees except five named executables'],
                          'out': str(output)}, indent=2))
        return
    output.mkdir(parents=True, exist_ok=False)
    receipt = {'format': 'zil-next-evidence-pack-receipt/1', 'status': 'Running', 'started_utc': now(),
               'root': str(root), 'requested_roots': DEFAULTS + a.plugin + a.add, 'script_sha256': digest_file(Path(__file__).resolve())[0],
               'phase': 'inventory', 'compiler_llvm_native_executed': False}
    write_receipt(output / 'receipt.json', receipt)
    try:
        selections = sorted(set(relative_input(root, raw) for raw in DEFAULTS + a.plugin + a.add))
        if any(output == root / name or (root / name) in output.parents for name in selections):
            raise ValueError('--out would be inside selected evidence')
        receipt['selected_roots'] = selections
        if not a.plugin:
            raise ValueError('explicit --plugin binary required; do not silently omit new plugin')
        links = set(relative_input(root, name) for name in a.allow_symlink)
        tools = tools_identity(a.tool)
        rows = inventory(root, selections, links)
        manifest = {'format': 'zil-next-raw-evidence-manifest/1', 'created_utc': now(),
                    'source_root': str(root), 'selected_roots': selections, 'allowed_symlinks': sorted(links),
                    'external_tools_identity_only': tools, 'entries': rows,
                    'source_path_count': len(rows), 'logical_file_bytes': sum(v['bytes'] for v in rows if v['type'] == 'file'),
                    'source_script_sha256': receipt['script_sha256'],
                    'deduplication': 'hardlink only identical SHA256+mode+bytes; every original path retained',
                    'research_code': 'separate Git archive; no complete repository/SDK/build-tree implicit inclusion'}
        manifest_bytes = encoded(manifest)
        with (output / 'MANIFEST.json').open('xb') as f:
            f.write(manifest_bytes)
        receipt.update(phase='tar-write', source_path_count=len(rows), manifest_sha256=hashlib.sha256(manifest_bytes).hexdigest())
        write_receipt(output / 'receipt.json', receipt)
        target = output / 'zil-compiler-next-evidence-20261111.tar.gz'
        receipt.update(package(target, root, rows, manifest_bytes))
        receipt['phase'] = 'source-after-hash'
        write_receipt(output / 'receipt.json', receipt)
        if inventory(root, selections, links) != rows or tools_identity(a.tool) != tools:
            raise ValueError('source inventory/hash/mode/identity changed during packaging')
        receipt['phase'] = 'tar-full-readback'
        write_receipt(output / 'receipt.json', receipt)
        member_count = readback(target, rows, manifest_bytes)
        h, s = digest_file(target)
        receipt.update(status='Pass', phase='complete', completed_utc=now(), archive_path=str(target),
                       archive_sha256=h, archive_bytes=s.st_size, tar_member_count=member_count,
                       all_sources_before_after_equal=True, all_tar_members_verified=True,
                       upload_route='direct-upload-eligible-size-only' if s.st_size < 50 * 1024 * 1024 else 'prepared-upload-required',
                       size_limit_bytes=50 * 1024 * 1024, evidence_trimmed=False)
        write_receipt(output / 'receipt.json', receipt)
        with (output / 'SHA256SUMS').open('x') as f:
            for name in ['zil-compiler-next-evidence-20261111.tar.gz', 'MANIFEST.json', 'receipt.json']:
                f.write(digest_file(output / name)[0] + '  ' + name + '\n')
        print(json.dumps(receipt, indent=2))
    except BaseException as error:
        receipt.update(status='Failed', completed_utc=now(), exception=repr(error), traceback=traceback.format_exc(),
                       partial_artifacts_retained=True, retry_requires_new_output=True)
        write_receipt(output / 'receipt.json', receipt)
        raise

if __name__ == '__main__':
    main()
