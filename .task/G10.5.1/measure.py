#!/usr/bin/env python3
"""Same-host complete CLI experiment; never report model checks as speedups."""
from __future__ import annotations
import argparse
import collections
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import time

BASE = '327eccfaf3169d1404534ddc16359310a21a40ad'
REQUESTS = [('cold', 'base'), ('warm', 'base'), ('private', 'private'),
            ('edit-warm', 'private'), ('public', 'public'), ('restore', 'base')]


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def dump(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def timed(argv, log, timeout=180):
    log.mkdir(parents=True, exist_ok=False)
    def alarm(_s, _f):
        raise TimeoutError('compiler deadline')
    previous = signal.signal(signal.SIGALRM, alarm)
    with (log/'stdout').open('wb') as stdout, (log/'stderr').open('wb') as stderr:
        start = time.perf_counter_ns()
        proc = subprocess.Popen(list(map(str, argv)), stdout=stdout, stderr=stderr, start_new_session=True)
        signal.setitimer(signal.ITIMER_REAL, timeout)
        expired = False
        try:
            _, status, usage = os.wait4(proc.pid, 0)
        except TimeoutError:
            expired = True
            os.killpg(proc.pid, signal.SIGKILL)
            _, status, usage = os.wait4(proc.pid, 0)
        finally:
            stop = time.perf_counter_ns()
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous)
        proc.returncode = os.waitstatus_to_exitcode(status)
    return {'argv': list(map(str, argv)), 'wall_ms': (stop-start)/1e6,
            'cpu_ms': 1000*(usage.ru_utime+usage.ru_stime), 'rss_kib': usage.ru_maxrss,
            'exit': proc.returncode, 'timeout': expired}


def summarize(rows):
    cells = collections.defaultdict(list)
    for row in rows:
        if not row['warmup'] and row['valid']:
            cells[(row['case'], row['request'], row['arm'])].append(row)
    summary = []
    for (case, request, arm), group in sorted(cells.items()):
        vals = [r['wall_ms'] for r in group]
        summary.append({'case': case, 'request': request, 'arm': arm, 'n': len(vals),
                        'median_ms': statistics.median(vals), 'min_ms': min(vals), 'max_ms': max(vals),
                        'cpu_median_ms': statistics.median(r['cpu_ms'] for r in group),
                        'rss_median_kib': statistics.median(r['rss_kib'] for r in group),
                        'actual_units': sorted({r.get('native_codegen', {}).get('actual_units', 0) for r in group}),
                        'all_values_ms': vals})
    return {'status': 'Complete limited experiment' if rows and all(r['valid'] for r in rows) else 'Incomplete or failed',
            'compile_requests': len(rows), 'formal_requests': sum(not r['warmup'] for r in rows),
            'native_checks': sum(len(r.get('native_checks', [])) for r in rows), 'cells': summary,
            'independent_review': 'NotRun', 'merge_allowed': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('repo', 'base', 'candidate', 'rustc', 'out'):
        parser.add_argument('--'+flag, required=True, type=Path)
    parser.add_argument('--rounds', type=int, default=7)
    args = parser.parse_args()
    if args.rounds < 7:
        parser.error('at least seven independent measured histories required')
    repo, out = args.repo.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    if len(cpus) != 2:
        raise RuntimeError('two available CPUs required')
    os.sched_setaffinity(0, cpus)
    generator = repo/'tools/research/compiler-differential/corpus.py'
    spec = importlib.util.spec_from_file_location('frozen_corpus', generator)
    corpus = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(corpus)
    tools = {'base': args.base.resolve(), 'cand': args.candidate.resolve(), 'rust': args.rustc.resolve()}
    manifest = {
        'baseline_commit': BASE,
        'candidate_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
        'machine_source_sha256': sha(repo/'crates/zil-backend-llvm/src/machine_cgu.rs'),
        'generator_sha256': sha(generator), 'runner_sha256': sha(__file__), 'cpus': cpus,
        'tools': {k: {'path': str(v), 'sha256': sha(v)} for k, v in tools.items()},
        'llvm_sha256': sha(Path(os.environ['ZIL_LLVM_LIB_DIR'])/'libLLVM-23.so'),
        'clang_sha256': sha(os.environ['ZIL_CLANG']),
        'rustc': subprocess.check_output([str(tools['rust']), '-vV'], text=True),
        'cpuinfo': Path('/proc/cpuinfo').read_text().split('\n\n')[0],
        'boundary': 'Popen to wait4, fresh compiler process including link; input edits and native checks excluded',
        'cache': 'independent per arm and block; OS cache not flushed; output recreated, not Cargo Fresh',
        'profile': 'ZIL O2 incremental-o2 function-merging=after CGU8/jobs2, official LLVM, semantic+object cache; Rust O2 default incremental CGU count, overflow checks, panic abort, debuginfo0',
        'rounds': args.rounds, 'warmup_histories_per_arm_case': 1,
        'memory': 'wait4 maximum individual process RSS, not simultaneous process-tree sum',
        'not_claimed': ['full-language performance', 'independent review', 'runtime quality acceptance', 'front-end regression attribution'],
    }
    for modules in (32, 256):
        for language in ('zil', 'rust'):
            directory = out/'corpus'/str(modules)/language
            corpus.tree_sources(directory, language, modules, 8)
    manifest['source_files'] = {str(p.relative_to(out)): sha(p) for p in sorted((out/'corpus').rglob('*')) if p.is_file()}
    dump(out/'manifest.json', manifest)
    rows = []
    try:
        for modules in (32, 256):
            for block in range(-1, args.rounds):
                order = ['base', 'cand', 'rust']
                shift = (block+1) % 3
                order = order[shift:] + order[:shift]
                if block % 2:
                    order.reverse()
                for request, snapshot in REQUESTS:
                    for arm in order:
                        lang, ext = ('rust', 'rs') if arm == 'rust' else ('zil', 'zil')
                        work = out/'work'/str(modules)/str(block)/arm
                        src, exe = work/'src', work/'program'
                        src.mkdir(parents=True, exist_ok=True)
                        for source in (out/'corpus'/str(modules)/lang).iterdir():
                            text = source.read_text()
                            if source.stem == 'effect' and snapshot != 'base':
                                if text.count('y+i+7') != 1:
                                    raise ValueError('private edit anchor mismatch')
                                text = text.replace('y+i+7', 'y+i+11')
                                if snapshot == 'public':
                                    text = text.replace('adjust(x:i64)', 'adjust(x:i64,extra:i64)').replace('y+i+11', 'y+i+11+extra')
                            if source.stem == 'main' and snapshot == 'public':
                                a, b = ('effect::adjust(i64::from(x))', 'effect::adjust(i64::from(x),3)') if arm == 'rust' else ('effect.adjust(i64(x))', 'effect.adjust(i64(x),3)')
                                if text.count(a) != 1:
                                    raise ValueError('public edit anchor mismatch')
                                text = text.replace(a, b)
                            target = src/source.name
                            if not target.exists() or target.read_text() != text:
                                target.write_text(text)
                        exe.unlink(missing_ok=True)
                        Path(str(exe)+'.ll').unlink(missing_ok=True)
                        if arm == 'rust':
                            argv = [tools[arm], src/'main.rs', '--edition=2024', '--crate-name', 'compiler_cost', '-A', 'warnings',
                                    '-C', 'opt-level=2', '-C', 'debuginfo=0', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                                    '-C', 'embed-bitcode=no', '-C', 'incremental='+str(work/'incremental'), '-o', exe]
                        else:
                            argv = [tools[arm], 'build', src/'main.zil', '--json', '--opt', 'O2', '--backend', 'llvm-api',
                                    '--codegen-units', '8', '--codegen-jobs', '2', '--incremental-o2', '--function-merging', 'after',
                                    '--semantic-cache', work/'semantic', '--object-cache', work/'objects', '--clang', os.environ['ZIL_CLANG'], '-o', exe]
                        log = out/'logs'/f'{len(rows):05d}'
                        row = timed(argv, log)
                        row.update({'case': f'tree{modules}x8', 'block': block, 'warmup': block < 0,
                                    'arm': arm, 'request': request, 'snapshot': snapshot, 'native_checks': [],
                                    'log': str(log.relative_to(out))})
                        valid = row['exit'] == 0 and not row['timeout'] and exe.is_file()
                        if arm != 'rust':
                            try:
                                report = json.loads((log/'stdout').read_text())
                                valid = valid and report['status'] == 'Success' and report['verification_runs'] == 1
                                valid = valid and report['native_codegen']['backend'] == 'llvm-api'
                                row['native_codegen'] = report.get('native_codegen')
                                row['verification_runs'] = report.get('verification_runs')
                            except (ValueError, KeyError, TypeError) as error:
                                row['report_error'] = str(error)
                                valid = False
                        if valid:
                            row['executable_sha256'], row['executable_bytes'] = sha(exe), exe.stat().st_size
                            for size in (0, 7, 256):
                                data = bytes((i*73+19)%256 for i in range(size))
                                expected = corpus.tree_expected(modules, 8, snapshot, data)
                                proc = subprocess.run([str(exe)], input=data, capture_output=True, timeout=30)
                                okay = proc.returncode == 0 and proc.stdout == expected and not proc.stderr
                                row['native_checks'].append({'size': size, 'pass': okay, 'exit': proc.returncode,
                                    'expected_sha256': hashlib.sha256(expected).hexdigest(),
                                    'stdout_sha256': hashlib.sha256(proc.stdout).hexdigest(), 'stderr_hex': proc.stderr[:256].hex()})
                                valid = valid and okay
                        row['valid'] = bool(valid)
                        rows.append(row)
                        with (out/'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(row)+'\n')
                        dump(log/'record.json', row)
                        print(json.dumps({k: row[k] for k in ('case', 'block', 'request', 'arm', 'wall_ms', 'valid')}), flush=True)
                        if not valid:
                            raise RuntimeError(f'first failure retained at {log}')
    finally:
        result = summarize(rows)
        result['expected_compile_requests'] = 2*(args.rounds+1)*3*len(REQUESTS)
        if len(rows) != result['expected_compile_requests']:
            result['status'] = 'Incomplete or failed'
        dump(out/'summary.json', result)
        print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
