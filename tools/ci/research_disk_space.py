#!/usr/bin/env python3
"""Release only unused preinstalled toolchains on a GitHub-hosted Linux runner.

Dry-run is the default. There is no command-line deletion path. Project sources,
compiler targets, test outputs, and evidence are outside the fixed allowlist.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess


ALLOWLIST = (
    Path('/usr/local/lib/android'),
    Path('/usr/share/dotnet'),
    Path('/opt/ghc'),
    Path('/usr/local/.ghcup'),
)


def validate_environment(env):
    required = {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted',
                'RUNNER_OS': 'Linux'}
    if platform.system() != 'Linux' or any(env.get(k) != v for k, v in required.items()):
        raise ValueError('This operation requires a GitHub-hosted Linux runner')
    protected = []
    for name in ('GITHUB_WORKSPACE', 'RUNNER_TEMP'):
        path = Path(env.get(name, ''))
        if not path.is_absolute() or path != path.resolve() or not path.is_dir():
            raise ValueError(name + ' must name an existing canonical directory')
        protected.append(path)
    return protected


def mount_points():
    # Kernel mountinfo escapes whitespace and backslashes using octal sequences.
    result = []
    for line in Path('/proc/self/mountinfo').read_text().splitlines():
        fields = line.split()
        if len(fields) < 6:
            raise ValueError('Malformed mountinfo')
        name = re.sub(r'\\([0-7]{3})', lambda m: chr(int(m.group(1), 8)), fields[4])
        result.append(Path(name))
    return result


def checked_candidate(path, protected, mounts):
    if path not in ALLOWLIST or not path.is_absolute():
        raise ValueError('Path is outside the fixed preinstalled-toolchain allowlist')
    if path != path.resolve() or path.is_symlink():
        raise ValueError('Toolchain path must be canonical and not a symlink: ' + str(path))
    if any(path.is_relative_to(p) or p.is_relative_to(path) for p in protected):
        raise ValueError('Toolchain path overlaps protected workspace or runner temp: ' + str(path))
    if any(m == path or m.is_relative_to(path) for m in mounts):
        raise ValueError('Toolchain path contains a mount point: ' + str(path))
    if not path.exists():
        return None
    value = path.lstat()
    if not stat.S_ISDIR(value.st_mode):
        raise ValueError('Toolchain path is not a directory: ' + str(path))
    return {'device': value.st_dev, 'inode': value.st_ino}


def allocated_bytes(path):
    result = subprocess.run(['du', '--summarize', '--block-size=1', '--one-file-system',
                             '--', str(path)], capture_output=True, text=True, check=True)
    return int(result.stdout.split()[0])


def capacity(path):
    value = shutil.disk_usage(path)
    return dict(total_bytes=value.total, used_bytes=value.used, free_bytes=value.free)


def reclaim(env, apply=False):
    workspace, temporary = validate_environment(env)
    receipt = temporary / 'receipts' / 'disk-space-before-build.json'
    if receipt != receipt.resolve() or receipt.parent.is_symlink() or receipt.exists():
        raise ValueError('Receipt must be a fresh canonical runner-temporary file')
    receipt.parent.mkdir(parents=True, exist_ok=True)
    record = {'status': 'Preparing', 'apply': apply,
              'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'scope': 'Only fixed unused preinstalled toolchains; no project, test, target or evidence deletion',
              'runner': {name: env[name] for name in ('GITHUB_ACTIONS', 'RUNNER_ENVIRONMENT', 'RUNNER_OS')},
              'protected': [str(workspace), str(temporary)],
              'before': capacity(temporary), 'paths': []}

    def save():
        receipt.write_text(json.dumps(record, indent=2) + '\n')
        receipt.chmod(0o644)

    save()
    try:
        if apply and not shutil.rmtree.avoids_symlink_attacks:
            raise ValueError('Apply requires fd-based symlink-safe rmtree')
        mounts = mount_points()
        # Validate the complete plan before deleting the first directory.
        for path in ALLOWLIST:
            identity = checked_candidate(path, [workspace, temporary], mounts)
            row = {'path': str(path), 'identity': identity,
                   'status': 'Missing' if identity is None else 'Planned'}
            if identity is not None:
                row['allocated_bytes_before'] = allocated_bytes(path)
            record['paths'].append(row)
            save()
        for row in record['paths']:
            if row['identity'] is None:
                continue
            path = Path(row['path'])
            current = checked_candidate(path, [workspace, temporary], mount_points())
            if current != row['identity']:
                raise ValueError('Toolchain directory changed before deletion: ' + str(path))
            if apply:
                row['status'] = 'Deleting'
                save()
                shutil.rmtree(path)
                row['status'] = 'Removed'
            else:
                row['status'] = 'DryRun'
            row['free_bytes_after'] = capacity(temporary)['free_bytes']
            save()
        record['status'] = 'Pass'
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        record['status'] = 'Fail'
        record['error'] = str(error)
        raise
    finally:
        record['after'] = capacity(temporary)
        record['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save()
        print(json.dumps({'status': record['status'], 'receipt': str(receipt),
                          'free_bytes_before': record['before']['free_bytes'],
                          'free_bytes_after': record['after']['free_bytes']}))
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='remove the validated fixed directories')
    args = parser.parse_args()
    try:
        reclaim(os.environ, args.apply)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()
