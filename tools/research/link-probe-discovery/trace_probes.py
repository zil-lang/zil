#!/usr/bin/env python3
"""Trace four existing discovery commands; never compile, link, or run a product.

This is evidence acquisition, NOT a dependency-cache implementation. Full syscall
names/results are recorded. Only pathname/process/metadata calls are decoded;
all other arguments (including read/write buffers) stay numeric. No -v, environment
dump, shell wrapper, or timing comparison is used.
"""
import argparse
import ast
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess


def sha(data):
    return hashlib.sha256(data).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def identity(path):
    path = Path(path)
    resolved = path.resolve(strict=True)
    s = resolved.stat()
    return dict(path=str(path), resolved=str(resolved), sha256=sha(resolved.read_bytes()),
                bytes=s.st_size, mode=s.st_mode, device=s.st_dev, inode=s.st_ino)


# execve's default (non -v) decoder emits an environment pointer/count, not values.
# getdents names, symlink targets, stat fields, and argv are intentional evidence.
DECODE = ','.join('''execve execveat open openat openat2 stat lstat fstat newfstatat
statx access faccessat faccessat2 readlink readlinkat getdents getdents64 chdir
fchdir getcwd close dup dup2 dup3 fcntl clone clone3 fork vfork wait4 waitid exit
exit_group lseek mmap mprotect munmap brk rt_sigaction rt_sigprocmask rt_sigreturn
arch_prctl set_tid_address set_robust_list rseq prlimit64'''.split())


def summarize(trace):
    counts = collections.Counter()
    failures = collections.Counter()
    absolute = collections.defaultdict(set)
    execs = []
    unfinished = 0
    for line in trace.splitlines():
        m = re.match(r'^\s*\d+\s+(\w+)\(', line)
        if not m:
            unfinished += int('<unfinished ...>' in line or 'resumed>' in line)
            continue
        name = m[1]
        counts[name] += 1
        failure = re.search(r'= -1 (\w+)', line)
        if failure:
            failures[name + ':' + failure[1]] += 1
        for quoted in re.findall(r'"(?:[^"\\]|\\.)*"', line):
            value = ast.literal_eval(quoted)
            if value.startswith('/'):
                absolute[value].add('failure:' + failure[1] if failure else 'non-error')
        if name in ('execve', 'execveat'):
            execs.append(line)
    return dict(syscall_counts=dict(counts), failures=dict(failures),
                absolute_strings={k: sorted(v) for k, v in sorted(absolute.items())},
                exec_lines=execs, unfinished_or_resumed_lines=unfinished,
                warning='absolute_strings includes argv/targets; not a resolved dependency set')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--run-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--execute-four-probes', action='store_true', required=True)
    args = parser.parse_args()
    source = args.source_root.resolve(strict=True)
    run = args.run_root.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if (out / 'receipt.json').exists() or (out / 'slots').exists():
        raise RuntimeError('refuse to overwrite an existing acquisition')
    rows_file = run / 'rows.jsonl'
    row = next(r for r in map(json.loads, rows_file.open())
               if r['arm'] == 'candidate' and r['request'] == 'private')
    report = row['resident']['report']
    original = report['commands'][:4]
    clang = row['runtime']['environment']['ZIL_CLANG']
    assert original[0][:4] == [clang, '-###', '--target=x86_64-unknown-linux-gnu', '-O2']
    assert original[1] == ['/usr/bin/ldd', str(Path(clang).resolve(strict=True))]
    assert original[2] == ['/usr/bin/ldd', '/usr/bin/ld']
    assert original[3] == ['/usr/bin/ld', '--verbose']
    assert len(original[0]) == 22 and original[0][-2] == '-o'
    slots = out / 'slots'
    slots.mkdir()
    cache = run / 'block-0-candidate' / 'objects' / 'objects-v1'
    namespaces = list(cache.iterdir())
    assert len(namespaces) == 1 and re.fullmatch('[a-f0-9]{64}', namespaces[0].name)
    assert namespaces[0].is_dir() and not namespaces[0].is_symlink()
    objects = report['native_codegen']['object_reports']
    assert len(objects) == 16
    inventory = []
    for index, obj in enumerate(objects):
        key = obj['cache']['action_key']
        assert re.fullmatch('[a-f0-9]{64}', key)
        p = namespaces[0] / (key + '.zobj')
        assert not p.is_symlink()
        data = p.read_bytes()
        assert data[:8] == b'ZILOBJ01' and data[8:40].hex() == key
        assert struct.unpack('<Q', data[72:80])[0] == len(data) - 80
        payload = data[80:]
        assert data[40:72].hex() == sha(payload)
        assert payload[:6] == b'\x7fELF\x02\x01' and payload[16:18] == b'\x01\x00'
        dest = slots / f'unit-{index}.o'
        dest.write_bytes(payload)
        assert original[0][index + 4].endswith(f'/unit-{index}.o')
        inventory.append(dict(index=index, container=str(p), container_sha256=sha(data),
                              action_key=key, payload=identity(dest)))
    commands = [original[0][:4] + [str(slots / f'unit-{i}.o') for i in range(16)]
                + ['-o', str(slots / 'program')]] + original[1:]
    env = os.environ.copy()
    for key, value in row['runtime']['environment'].items():
        if value is None:
            env.pop(key, None)
        else:
            env[key] = value
    assert not env.get('LD_PRELOAD') and not env.get('LD_AUDIT')
    # Bind all inherited values without disclosing any. Names alone are not a cache key.
    environment_digest = sha(json.dumps(sorted(env.items()), separators=(',', ':')).encode())
    strace = shutil.which('strace')
    assert strace
    receipt = dict(schema='discovery-syscall-evidence-v1', source_root=str(source),
                   source_commit=subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip(),
                   measured_profile_source='5bf55f35ccdad6ca9eee3200a063115602d575ea',
                   rows=dict(path=str(rows_file), sha256=sha(rows_file.read_bytes())),
                   row_selector=dict(arm='candidate', request='private', block=0),
                   original_commands=original, commands=commands, cwd=str(source),
                   environment_sha256=environment_digest, environment_names=sorted(env),
                   environment_note='Inherited now, with exact archived arm runtime overrides; not a reconstruction of every historical environment value.',
                   runtime_overrides=row['runtime']['environment'],
                   objects=inventory, namespace=namespaces[0].name,
                   tools=[identity(p) for p in [strace, clang, '/usr/bin/ldd', '/usr/bin/ld']],
                   recorder_sha256=sha(Path(__file__).read_bytes()),
                   trace_decoded_syscalls=DECODE.split(','), probes=[])
    shutil.copyfile(__file__, out / 'trace_probes.py')
    for index, command in enumerate(commands):
        prefix = out / f'probe-{index}'
        trace = prefix.with_suffix('.strace')
        trace_command = [strace, '-f', '-qq', '-s', '4096', '-e', 'trace=all',
                         '-e', 'raw=!' + DECODE, '-o', str(trace), '--'] + command
        # No shell, native product, actual linker invocation with objects, or timing loop.
        result = subprocess.run(trace_command, cwd=source, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        prefix.with_suffix('.stdout').write_bytes(result.stdout)
        prefix.with_suffix('.stderr').write_bytes(result.stderr)
        text = trace.read_text()
        summary = summarize(text)
        dump(prefix.with_suffix('.summary.json'), summary)
        receipt['probes'].append(dict(index=index, argv=command, strace_argv=trace_command,
                                      exit_code=result.returncode, stdout_sha256=sha(result.stdout),
                                      stderr_sha256=sha(result.stderr), trace_sha256=sha(trace.read_bytes()),
                                      summary_sha256=sha(prefix.with_suffix('.summary.json').read_bytes())))
        assert result.returncode == 0, 'probe failed; preserve evidence and investigate'
    assert not (slots / 'program').exists(), 'dry-run unexpectedly created an output'
    for item in inventory:
        assert identity(item['payload']['path']) == item['payload'], 'object changed'
    # Record all actually executed regular files, without invoking them again.
    executed = set()
    for index in range(4):
        for line in json.loads((out / f'probe-{index}.summary.json').read_text())['exec_lines']:
            m = re.search(r'execve\("([^"\\]+)"', line)
            if m and '= 0' in line:
                executed.add(m[1])
    receipt['executed_files'] = [identity(p) for p in sorted(executed)]
    receipt['output_absent'] = True
    receipt['closure_proven'] = False
    dump(out / 'receipt.json', receipt)
    print(json.dumps(dict(output=str(out), exit_codes=[p['exit_code'] for p in receipt['probes']],
                          object_count=len(inventory), executed_files=sorted(executed))))


if __name__ == '__main__':
    main()
