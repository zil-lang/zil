#!/usr/bin/env python3
"""Preserve exact current CI inputs; full Git history stays at the recorded commit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import stat
import tarfile


OUTPUTS = ('commit.txt', 'source-tree.txt', 'source-inputs.tar.gz',
           'source-inputs.json', 'source-restore.txt')


def require(condition, message):
    if not condition:
        raise ValueError(message)



def verify_checkout(repo, included, object_format):
    """Read actual inputs independently of index flags and core.filemode."""
    checked_dirs = {repo}
    modes = {}
    for name, entry in included.items():
        path = repo / name
        for parent in path.parents:
            if parent == repo:
                break
            if parent not in checked_dirs:
                require(stat.S_ISDIR(parent.lstat().st_mode),
                        'Input parent is not a real directory: ' + str(parent))
                checked_dirs.add(parent)
        before = path.lstat()
        if entry['git_mode'] == '120000':
            require(stat.S_ISLNK(before.st_mode), 'Expected checkout symlink: ' + name)
            data = os.readlink(path).encode('utf-8')
        else:
            require(stat.S_ISREG(before.st_mode), 'Expected checkout regular file: ' + name)
            actual_mode = '100755' if before.st_mode & stat.S_IXUSR else '100644'
            require(actual_mode == entry['git_mode'], 'Checkout executable mode differs: ' + name)
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
            with os.fdopen(descriptor, 'rb') as source:
                opened = os.fstat(source.fileno())
                require((opened.st_dev, opened.st_ino) == (before.st_dev, before.st_ino),
                        'Checkout file replaced before read: ' + name)
                data = source.read()
                closed = os.fstat(source.fileno())
            require((opened.st_mode, opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns)
                    == (closed.st_mode, closed.st_size, closed.st_mtime_ns, closed.st_ctime_ns),
                    'Checkout input changed during read: ' + name)
        after = path.lstat()
        require((before.st_dev, before.st_ino, before.st_mode, before.st_size,
                 before.st_mtime_ns, before.st_ctime_ns)
                == (after.st_dev, after.st_ino, after.st_mode, after.st_size,
                    after.st_mtime_ns, after.st_ctime_ns),
                'Checkout path changed during read: ' + name)
        oid = hashlib.new(object_format, b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        require(oid == entry['git_oid'] and len(data) == entry['bytes'],
                'Actual checkout bytes differ from HEAD: ' + name)
        modes[name] = stat.S_IMODE(after.st_mode)
    return modes


def capture(repo, output):
    repo = repo.resolve(strict=True)
    require(output.is_absolute() and output == output.resolve(),
            'Receipt directory must be absolute and canonical')
    output.mkdir(parents=True, exist_ok=True)
    require(output.is_dir() and not output.is_symlink(), 'Invalid receipt directory')
    for name in OUTPUTS:
        require(not os.path.lexists(output / name), 'Refusing existing output: ' + name)

    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args])

    commit = git('rev-parse', 'HEAD').decode().strip()
    require(not git('diff', '--name-only', 'HEAD', '--'),
            'Tracked checkout differs from HEAD')
    require(not git('diff', '--cached', '--name-only', 'HEAD', '--'),
            'Index differs from HEAD')
    untracked = git('ls-files', '--others', '--exclude-standard', '-z', '--',
                    '.', ':(exclude)evidence')
    require(not untracked, 'Untracked files outside historical evidence/ are not captured')
    tree = git('ls-tree', '-rlz', commit)
    included = {}
    excluded_count = excluded_bytes = 0
    tops = set()
    for row in tree.split(b'\0'):
        if not row:
            continue
        metadata, raw_path = row.split(b'\t', 1)
        mode, kind, oid, size = metadata.split()
        path = raw_path.decode('utf-8')
        if path.startswith('evidence/'):
            excluded_count += 1
            excluded_bytes += int(size) if size != b'-' else 0
            continue
        require(kind == b'blob' and mode in (b'100644', b'100755', b'120000'),
                'Unsupported input type: ' + path)
        require(not path.startswith('/') and all(x not in ('', '.', '..') for x in path.split('/')),
                'Invalid tracked path: ' + path)
        included[path] = {'path': path, 'git_mode': mode.decode(),
                          'git_oid': oid.decode(), 'bytes': int(size)}
        tops.add(path.split('/')[0])
    require(included, 'No current input files')
    object_format = git('rev-parse', '--show-object-format').decode().strip()
    require(object_format in ('sha1', 'sha256'), 'Unsupported Git object format')
    checkout_modes = verify_checkout(repo, included, object_format)
    report = {'status': 'Preparing', 'commit': commit, 'git_object_format': object_format,
              'scope': 'All tracked non-evidence/ current files; full history and historical evidence require exact GitHub commit',
              'excluded_prefix': 'evidence/', 'excluded_entries': excluded_count,
              'excluded_blob_bytes': excluded_bytes, 'files': [],
              'checkout_before': 'ActualBytesTypesAndGitModesMatchHEAD'}
    (output / 'commit.txt').write_text(commit + '\n')
    (output / 'source-tree.txt').write_bytes(git('ls-tree', '-rl', commit))
    (output / 'source-restore.txt').write_text(
        'Current CI input bytes only; this is not a Git history bundle or full historical/handoff recovery.\n'
        'For the complete Git repository, historical evidence and Git-dependent commands, '
        'fetch https://github.com/zil-lang/zil and checkout exact commit ' + commit + '.\n'
        'source-tree.txt inventories the entire recorded Git tree, including excluded evidence/.\n'
        'Pinned external SDK and Rust dependency installation is still required.\n')
    try:
        archive = output / 'source-inputs.tar.gz'
        subprocess.run(['git', '-C', str(repo), '-c', 'tar.umask=0022', 'archive', '--format=tar.gz',
                        '--output=' + str(archive), commit, '--', *sorted(tops)], check=True)
        seen = set()
        with tarfile.open(archive, 'r:gz') as stream:
            for member in stream:
                if member.isdir():
                    continue
                require(member.name in included and member.name not in seen,
                        'Unexpected or duplicate archive member: ' + member.name)
                entry = included[member.name]
                seen.add(member.name)
                if entry['git_mode'] == '120000':
                    require(member.issym(), 'Expected symbolic link: ' + member.name)
                    data = member.linkname.encode('utf-8')
                else:
                    require(member.isfile(), 'Expected regular input: ' + member.name)
                    require(member.mode == (0o755 if entry['git_mode'] == '100755' else 0o644),
                            'Executable mode mismatch: ' + member.name)
                    data = stream.extractfile(member).read()
                digest = hashlib.new(object_format, b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
                require(digest == entry['git_oid'] and len(data) == entry['bytes'],
                        'Archive differs from Git blob: ' + member.name)
                report['files'].append({**entry, 'sha256': hashlib.sha256(data).hexdigest(),
                                        'worktree_mode': checkout_modes[member.name]})
        require(seen == set(included), 'Archive is missing tracked current inputs')
        require(git('rev-parse', 'HEAD').decode().strip() == commit,
                'HEAD changed during snapshot')
        require(not git('diff', '--name-only', 'HEAD', '--')
                and not git('diff', '--cached', '--name-only', 'HEAD', '--')
                and not git('ls-files', '--others', '--exclude-standard', '-z', '--', '.', ':(exclude)evidence'),
                'Checkout changed during snapshot')
        require(verify_checkout(repo, included, object_format) == checkout_modes,
                'Actual checkout permission modes changed during capture')
        report['checkout_after'] = 'ActualBytesTypesAndGitModesMatchHEAD'
        with archive.open('rb') as source:
            archive_sha = hashlib.file_digest(source, 'sha256').hexdigest()
        report.update(status='Pass', archive_bytes=archive.stat().st_size,
                      archive_sha256=archive_sha, input_files=len(included),
                      input_blob_bytes=sum(x['bytes'] for x in included.values()))
    except Exception as error:
        report.update(status='Fail', error=str(error))
        raise
    finally:
        with (output / 'source-inputs.json').open('x') as stream:
            json.dump(report, stream, indent=2)
            stream.write('\n')
    print(json.dumps({key: value for key, value in report.items() if key != 'files'}))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    capture(args.repo, args.out)


if __name__ == '__main__':
    main()
