#!/usr/bin/env python3
"""Post-process saved traces, without invoking any probe or authorizing reuse."""
import argparse
import ast
import collections
import hashlib
import json
from pathlib import Path
import re

from trace_probes import DECODE


def stitch(text):
    pending = {}
    for number, line in enumerate(text.splitlines(), 1):
        m = re.match(r'^\s*(\d+)\s+(.*)$', line)
        if not m:
            raise ValueError(f'unrecognized trace prefix at {number}')
        pid, tail = m.groups()
        if '<unfinished ...>' in tail:
            if pid in pending:
                raise ValueError('two unfinished calls for one pid')
            pending[pid] = (number, tail.replace('<unfinished ...>', ''))
            continue
        resumed = re.match(r'<\.\.\. (\w+) resumed>(.*)', tail)
        if resumed:
            start, previous = pending.pop(pid)
            if not previous.startswith(resumed[1] + '('):
                raise ValueError('resumed syscall does not match')
            yield start, int(pid), previous + resumed[2]
        elif tail.startswith(('--- ', '+++ ')):
            continue
        else:
            yield number, int(pid), tail
    if pending:
        raise ValueError('incomplete trace')


PATH_CALLS = set('open openat openat2 stat lstat newfstatat statx access faccessat faccessat2 readlink readlinkat chdir execve execveat'.split())


def analyze(text):
    counts = collections.Counter()
    errors = collections.Counter()
    events = []
    fd_paths = {}
    enumerations = []
    execs = []
    raw_other = set()
    for line, pid, call in stitch(text):
        m = re.match(r'(\w+)\(.*\)\s+=\s+(.*)$', call)
        if not m:
            raise ValueError(f'unrecognized complete syscall at line {line}')
        name, result = m.groups()
        counts[name] += 1
        err = re.match(r'-1 (\w+)', result)
        if err:
            errors[err[1]] += 1
        quoted = re.findall(r'"(?:[^"\\]|\\.)*"', call)
        if name in PATH_CALLS:
            if not quoted:
                raise ValueError(f'undecoded pathname at line {line}')
            path = ast.literal_eval(quoted[0])
            events.append(dict(line=line, pid=pid, syscall=name, path=path, result=result))
            if name.startswith('open') and not err and re.match(r'\d+$', result):
                fd_paths[pid, int(result)] = path
            if name in ('execve', 'execveat'):
                execs.append(dict(line=line, path=path, result=result, call=call))
        elif name in ('getdents', 'getdents64'):
            fd = int(re.match(r'\w+\((\d+),', call)[1])
            enumerations.append(dict(line=line, pid=pid, fd=fd,
                                     directory=fd_paths.get((pid, fd)), result=result,
                                     names_recorded=False))
        elif name == 'close':
            fd = int(re.match(r'close\((\d+)\s*\)', call)[1])
            fd_paths.pop((pid, fd), None)
        if name not in DECODE.split(','):
            raw_other.add(name)
    paths = collections.defaultdict(lambda: dict(calls=collections.Counter(), results=collections.Counter()))
    for event in events:
        item = paths[event['path']]
        item['calls'][event['syscall']] += 1
        item['results']['failure:' + event['result'].split()[1] if event['result'].startswith('-1 ') else 'success'] += 1
    return dict(syscalls=dict(counts), errors=dict(errors), events=events,
                paths={k: dict(calls=dict(v['calls']), results=dict(v['results']))
                       for k, v in sorted(paths.items())},
                enumerations=enumerations, execs=execs,
                raw_argument_syscalls=sorted(raw_other), closure_proven=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    out = args.evidence
    results = []
    for i in range(4):
        trace = out / f'probe-{i}.strace'
        result = analyze(trace.read_text())
        result['trace_sha256'] = hashlib.sha256(trace.read_bytes()).hexdigest()
        results.append(result)
        (out / f'probe-{i}.analysis.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    # This is a later observation, not what getdents returned transactionally.
    directories = sorted({e['directory'] for r in results for e in r['enumerations'] if e['directory']})
    snapshots = []
    for directory in directories:
        p = Path(directory)
        names = sorted(x.name for x in p.iterdir())
        snapshots.append(dict(path=directory, names=names, stat=list(p.stat()),
                              note='post-trace names; not an atomic acquisition-time dependency proof'))
    (out / 'post-trace-directory-snapshots.json').write_text(json.dumps(snapshots, indent=2, sort_keys=True) + '\n')
    summary = [dict(probe=i, syscalls=sum(r['syscalls'].values()),
                    path_events=len(r['events']), unique_pathnames=len(r['paths']),
                    successful_path_events=sum(not e['result'].startswith('-1') for e in r['events']),
                    failed_path_events=sum(e['result'].startswith('-1') for e in r['events']),
                    ENOENT_events=r['errors'].get('ENOENT', 0),
                    getdents_calls=len(r['enumerations']), execve_calls=len(r['execs']),
                    raw_argument_syscalls=r['raw_argument_syscalls'])
               for i, r in enumerate(results)]
    (out / 'analysis-summary.json').write_text(json.dumps(dict(
        probes=summary, closure_proven=False, complete_raw_trace=True,
        directory_membership_at_syscall_time_captured=False), indent=2, sort_keys=True) + '\n')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
