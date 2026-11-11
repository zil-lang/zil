#!/usr/bin/env python3
"""Alternate paired real CLI requests; preserve raw evidence, never infer work.

Run only during an assigned single heavy window. Both binaries must be built
from the recorded source SHAs with the same Rust/LLVM toolchain and profile.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import select
import signal
import statistics
import subprocess
import threading
import time

from native_workloads import EXISTING, REQUESTS, ROOT, inputs, oracle, requests, snapshot, write_snapshot


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def proc_stat(pid):
    try:
        # comm may contain whitespace or parentheses; the final ')' delimits it.
        tail = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        ticks = os.sysconf('SC_CLK_TCK')
        return {'cpu_seconds': sum(int(tail[i]) for i in [11, 12, 13, 14]) / ticks,
                'own_cpu_seconds': sum(int(tail[i]) for i in [11, 12]) / ticks,
                'rss_kib': int(tail[21]) * os.sysconf('SC_PAGE_SIZE') / 1024}
    except (FileNotFoundError, ProcessLookupError, ValueError, IndexError):
        return None


def descendants(pid):
    found = [pid]
    for current in found:
        try:
            children = Path(f'/proc/{current}/task/{current}/children').read_text().split()
            found.extend(int(p) for p in children if int(p) not in found)
        except (FileNotFoundError, ProcessLookupError):
            pass
    return found


class Worker:
    def __init__(self, command, directory, timeout, sample_ms):
        self.command, self.directory, self.timeout = command, directory, timeout
        self.sample_ms = sample_ms
        self.failed = False
        self.requests = 0
        self.err = (directory / 'stderr.log').open('wb')
        start = time.perf_counter_ns()
        self.p = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=self.err, start_new_session=True, bufsize=0)
        self.buffer = b''
        try:
            self.ready = self.read_json()
        except (OSError, ValueError, RuntimeError, TimeoutError):
            self.close()
            raise
        self.startup_wall_ms = (time.perf_counter_ns() - start) / 1e6
        if self.ready.get('ready') is not True or self.ready.get('protocol') != 1:
            raise RuntimeError(f'invalid resident ready: {self.ready}')
        dump(directory / 'startup.json', {'command': command, 'ready': self.ready,
                                        'wall_ms': self.startup_wall_ms})

    def read_json(self):
        deadline = time.monotonic() + self.timeout
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.p.stdout], [], [], remaining)[0]:
                raise TimeoutError('resident response timeout')
            block = os.read(self.p.stdout.fileno(), 65536)
            if not block:
                raise RuntimeError(f'resident exited before response: {self.p.poll()}')
            self.buffer += block
        line, self.buffer = self.buffer.split(b'\n', 1)
        return json.loads(line)

    def request(self, name):
        stop = threading.Event()
        samples = []

        def sample():
            while not stop.is_set():
                pids = descendants(self.p.pid)
                stats = [s for pid in pids if (s := proc_stat(pid)) is not None]
                samples.append({'offset_ms': (time.perf_counter_ns() - start) / 1e6,
                                'processes': len(stats),
                                'tree_rss_sum_kib': sum(s['rss_kib'] for s in stats)})
                stop.wait(self.sample_ms / 1000)

        before = proc_stat(self.p.pid)
        start = time.perf_counter_ns()
        thread = threading.Thread(target=sample, daemon=True)
        thread.start()
        try:
            self.p.stdin.write(b'compile\n')
            self.p.stdin.flush()
            response = self.read_json()
            wall_ms = (time.perf_counter_ns() - start) / 1e6
        finally:
            stop.set()
            thread.join()
        after = proc_stat(self.p.pid)
        dump(self.directory / f'{name}.response.json', response)
        dump(self.directory / f'{name}.memory-samples.json', samples)
        self.requests += 1
        validate_response(response, self.requests)
        if (before is None or after is None or not samples
                or any(not math.isfinite(after[k] - before[k]) or after[k] < before[k]
                       for k in ['cpu_seconds', 'own_cpu_seconds'])
                or any(not math.isfinite(s['tree_rss_sum_kib']) or s['tree_rss_sum_kib'] <= 0 for s in samples)):
            raise ValueError('missing or invalid CPU/memory measurement')
        return {'wall_ms': wall_ms, 'cpu_seconds': after['cpu_seconds'] - before['cpu_seconds'] if before and after else None,
                'own_cpu_seconds': after['own_cpu_seconds'] - before['own_cpu_seconds'] if before and after else None,
                'tree_peak_sampled_rss_sum_kib': max((s['tree_rss_sum_kib'] for s in samples), default=None),
                'memory_sample_count': len(samples), 'resident': response}

    def close(self):
        if self.p.poll() is None:
            try:
                self.p.stdin.write(b'quit\n')
                self.p.stdin.flush()
                self.p.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                os.killpg(self.p.pid, signal.SIGKILL)
                self.p.wait()
        self.err.close()


def native_outcomes(program, case, label, a, oracle_fn=oracle):
    out = []
    for data in inputs(case):
        expected = oracle_fn(case, data, label, a.modules, a.functions, a.steps)
        try:
            p = subprocess.run([str(program)], input=data, capture_output=True, timeout=a.native_timeout)
            actual = {'exit': p.returncode, 'stdout_hex': p.stdout.hex(), 'stderr_hex': p.stderr.hex()}
            out.append({'stdin_hex': data.hex(), 'expected': expected, 'actual': actual, 'valid': expected == actual})
        except (OSError, subprocess.TimeoutExpired) as error:
            out.append({'stdin_hex': data.hex(), 'expected': expected, 'error': str(error), 'valid': False})
    return out


def default_requests(case):
    return requests(case)


def validate_response(response, request):
    """Missing fields, stale responses and reset epochs are failures, never hits."""
    required = {'request': int, 'session_epoch': int, 'session_cleared': bool,
                'active_queries': int, 'cached_queries': int, 'report': dict,
                'elapsed_ms': (int, float), 'rss_kib': (int, float), 'peak_kib': (int, float)}
    for field, kind in required.items():
        if field not in response or type(response[field]) not in (kind if isinstance(kind, tuple) else (kind,)):
            raise ValueError(f'missing/invalid resident field: {field}')
    if (response['request'] != request or response['session_epoch'] != 0
            or response['session_cleared'] is not False or response['active_queries'] != 0):
        raise ValueError('stale request, reset session or unfinished query')
    report = response['report']
    fields = {'status': str, 'phase': str, 'diagnostics': list, 'input_fingerprint': str,
              'source_files': dict, 'scalar_interfaces': list, 'query_events': list,
              'verification_runs': int, 'seconds': (int, float), 'artifacts': dict}
    for field, kind in fields.items():
        if field not in report or type(report[field]) not in (kind if isinstance(kind, tuple) else (kind,)):
            raise ValueError(f'missing/invalid compiler field: {field}')
    fingerprint = report['input_fingerprint']
    if len(fingerprint) != 64 or any(c not in '0123456789abcdef' for c in fingerprint):
        raise ValueError('invalid current input fingerprint')
    if not report['query_events'] or not report['source_files']:
        raise ValueError('empty query execution or source identity')
    if any(not math.isfinite(response[k]) or response[k] < 0 for k in ['elapsed_ms', 'rss_kib', 'peak_kib']):
        raise ValueError('invalid timing or resident memory')
    for event in report['query_events']:
        if set(event) != {'phase', 'disposition'} or event['disposition'] not in ['executed', 'reused']:
            raise ValueError('invalid query event')


def transition(case, label):
    if label == 'cold':
        return None
    if label == 'recovery':
        return 'error'
    if label == 'restore':
        return 'private' if case in ['mixed', 'scalar'] else 'positions'
    return 'cold'


def report_summary(response):
    report = response.get('report', {})
    # Stage labels are context only. Real work fields are retained by exact name.
    queries = Counter(f"{e.get('phase')}:{e.get('disposition')}" for e in report.get('query_events', []))
    work = {key: value for key, value in report.items()
            if any(s in key for s in ['stats', 'work', 'execut', 'reactive', 'incremental'])}
    return {'status': report.get('status'), 'phase': report.get('phase'),
            'verification_runs': report.get('verification_runs'),
            'query_event_counts': dict(queries), 'actual_work_fields': work,
            'native_codegen': report.get('native_codegen'), 'disk_cache': report.get('disk_cache'),
            'phase_timings_ns': report.get('phase_timings_ns')}


def command(binary, source, directory, a, arm):
    cmd = [str(binary.resolve()), 'check' if a.mode == 'check' else 'build',
           str(source / 'main.zil'), '--json', '--resident',
           '--resident-capacity', str(a.capacity), '--resident-reset-after', '4096']
    if a.mode != 'check':
        cmd += ['--backend', a.backend, '--opt', a.mode, '--codegen-units', str(a.codegen_units),
                '--codegen-jobs', str(a.codegen_jobs), '-o', str(directory / 'program')]
        if a.closed_executable:
            cmd += ['--closed-executable']
    if a.cache in ['semantic', 'all']:
        cmd += ['--semantic-cache', str(directory / 'semantic')]
    if a.mode != 'check' and a.cache in ['object', 'all']:
        cmd += ['--object-cache', str(directory / 'objects')]
    if a.source_cache:
        cmd += ['--cache-dir', str(directory / 'source-cache')]
    return cmd + a.extra_arg + getattr(a, arm + '_extra_arg')


def pair_validation(rows):
    if len(rows) != 2 or any('resident' not in r for r in rows):
        return False
    left, right = [r['resident']['report'] for r in rows]
    # Paths are shared by both arms, so diagnostic spans/IDs must also agree.
    fields = ['status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces']
    return all(k in left and k in right and left[k] == right[k] for k in fields)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--baseline', required=True, type=Path)
    p.add_argument('--candidate', required=True, type=Path)
    p.add_argument('--baseline-source', required=True, help='exact baseline git SHA')
    p.add_argument('--candidate-source', required=True, help='exact candidate git SHA or tree identity')
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--case', choices=['mixed', 'scalar', *EXISTING], default='mixed')
    p.add_argument('--modules', type=int, default=8)
    p.add_argument('--functions', type=int, default=8)
    p.add_argument('--steps', type=int, default=16)
    p.add_argument('--blocks', type=int, default=5)
    p.add_argument('--request', choices=REQUESTS, action='append')
    p.add_argument('--mode', choices=['check', 'O0', 'O2'], default='O2')
    p.add_argument('--backend', choices=['clang', 'llvm-api'], default='llvm-api')
    p.add_argument('--cache', choices=['none', 'semantic', 'object', 'all'], default='all')
    p.add_argument('--source-cache', action='store_true')
    p.add_argument('--closed-executable', action='store_true')
    p.add_argument('--codegen-units', type=int, default=1)
    p.add_argument('--codegen-jobs', type=int, default=1)
    p.add_argument('--capacity', type=int, default=256)
    p.add_argument('--timeout', type=float, default=180)
    p.add_argument('--native-timeout', type=float, default=10)
    p.add_argument('--sample-ms', type=float, default=10)
    p.add_argument('--extra-arg', action='append', default=[])
    p.add_argument('--baseline-extra-arg', action='append', default=[])
    p.add_argument('--candidate-extra-arg', action='append', default=[])
    a = p.parse_args()
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    if a.blocks < 1 or a.sample_ms <= 0:
        p.error('blocks and sample-ms must be positive')
    selected = a.request or default_requests(a.case)
    if len(selected) != len(set(selected)):
        p.error('request labels must be unique within one sequence')
    if any(r not in default_requests(a.case) for r in selected):
        p.error('requested edit is not defined for this case')
    if 'multi' in selected and a.modules < 2:
        p.error('multi requires at least two actually changed source modules')
    if 'cold' in selected and selected[0] != 'cold':
        p.error('cold must be the first request of a fresh resident block')
    if 2 * len(selected) >= 4096:
        p.error('request count must remain below reset boundary')
    binaries = {'baseline': a.baseline, 'candidate': a.candidate}
    for name in ['native_measure.py', 'native_workloads.py']:
        (a.out / name).write_bytes(Path(__file__).with_name(name).read_bytes())
    manifest = {'arguments': {k: str(v) if isinstance(v, Path) else v for k, v in vars(a).items()},
                'requests': selected, 'binaries': {arm: {'path': str(path.resolve()), 'sha256': digest(path)} for arm, path in binaries.items()},
                'source_shas': {'baseline': a.baseline_source, 'candidate': a.candidate_source},
                'runner_sha256': digest(Path(__file__)), 'generator_sha256': digest(Path(__file__).with_name('native_workloads.py')),
                'environment': {k: os.environ.get(k) for k in ['ZIL_CLANG', 'ZIL_LLVM_CONFIG', 'LLVM_CONFIG', 'LD_LIBRARY_PATH']},
                'affinity': sorted(os.sched_getaffinity(0)),
                'cpu_clock_ticks_per_second': os.sysconf('SC_CLK_TCK'),
                'load_average': list(os.getloadavg()),
                'transition_policy': 'Each measured edit is preceded by a paired unmeasured compile of its named parent snapshot. Preparations are archived and validated. No application caches cross blocks; OS page cache is not flushed. Recovery produces a new successful body; restore returns to an old successful snapshot.',
                'measurement_boundary': 'Wall: send compile through complete response; startup separate. CPU: /proc resident utime+stime+cutime+cstime delta, includes waited child CPU; tick quantization. RSS: resident response and sampled live tree sum (shared pages can double count; not exact child high-water). Native runs and source writes outside compilation timer.',
                'coverage_boundary': 'Real Source/Types/Analyze/current independent Verify/native build. Mixed generator combines bounded arithmetic with live stdin, shared generic/trait calls, direct recursion, CT branch and owned closure. Scalar generator covers bounded arithmetic only. Existing consumers are small coverage controls, not scale evidence. No claims for full SCC, blanket traits, general resource composition or budget/cancel/eviction until separately exercised.'}
    dump(a.out / 'manifest.json', manifest)
    rows = []
    for block in range(a.blocks):
        source = a.out / f'source-{block}'
        write_snapshot(source, snapshot(a.case, 'cold', a.modules, a.functions, a.steps))
        workers = {}
        try:
            for arm in (['baseline', 'candidate'] if block % 2 == 0 else ['candidate', 'baseline']):
                directory = a.out / f'block-{block}-{arm}'
                directory.mkdir()
                workers[arm] = Worker(command(binaries[arm], source, directory, a, arm), directory, a.timeout, a.sample_ms)
            previous_files = None
            successful_snapshots = set()

            def run_pair(index, label, measured, parent):
                nonlocal previous_files
                files = snapshot(a.case, label, a.modules, a.functions, a.steps)
                before = previous_files or {}
                changed = sorted(k for k in set(before) | set(files) if before.get(k) != files.get(k))
                identity = write_snapshot(source, files)
                previous_files = files
                identity['changed_files'] = changed
                archive = a.out / 'snapshots' / label
                archived = write_snapshot(archive, files)
                if archived['identity_sha256'] != identity['identity_sha256']:
                    raise RuntimeError('snapshot identity mismatch')
                if measured:
                    if label == 'unchanged' and changed:
                        raise RuntimeError('unchanged request changed source bytes')
                    if label not in ['cold', 'unchanged'] and not changed:
                        raise RuntimeError(f'{label} request did not change source bytes')
                    if label == 'private' and changed != ['m0.zil']:
                        raise RuntimeError('private edit changed an unexpected source set')
                    if label == 'multi':
                        expected = sorted(f'm{i}.zil' for i in {0, a.modules // 2, a.modules - 1})
                        if changed != expected:
                            raise RuntimeError('multi edit changed an unexpected source set')
                    if label == 'broad' and changed != sorted(f'm{i}.zil' for i in range(a.modules)):
                        raise RuntimeError('broad edit changed an unexpected source set')
                name = f'{index}-{label}' if measured else f'{index}-prepare-{label}'
                dump(a.out / f'source-{block}-{name}.json', identity)
                pair = []
                order = ['baseline', 'candidate'] if (block + index) % 2 == 0 else ['candidate', 'baseline']
                for position, arm in enumerate(order):
                    worker = workers[arm]
                    program = worker.directory / 'program'
                    program.unlink(missing_ok=True)
                    row = {'block': block, 'request_index': index, 'request': label,
                           'measured': measured, 'transition_from': parent,
                           'position': position, 'arm': arm, 'case': a.case, 'mode': a.mode,
                           'source_identity_sha256': identity['identity_sha256'], 'changed_files': changed}
                    try:
                        if worker.failed:
                            raise RuntimeError('resident unavailable after previous transport failure')
                        row.update(worker.request(name))
                        row.update(report_summary(row['resident']))
                        report = row['resident']['report']
                        if set(report['source_files'].values()) != {str(source / name) for name in files}:
                            raise ValueError('source map does not cover this exact input snapshot')
                        if label == 'error':
                            row['valid'] = (report['status'] == 'SafeReject' and report['phase'] == 'types'
                                            and report['verification_runs'] == 0 and len(report['diagnostics']) == 1
                                            and report['diagnostics'][0]['code'] == 'E-UNKNOWN-NAME'
                                            and report['diagnostics'][0]['span'] is not None)
                        else:
                            row['valid'] = (report['status'] == 'Success' and report['verification_runs'] == 1
                                            and report['diagnostics'] == []
                                            and sum(e == {'phase': 'verify', 'disposition': 'executed'}
                                                    for e in report['query_events']) == 1)
                            if row['valid'] and a.mode != 'check':
                                if report['artifacts']['executable'] != str(program):
                                    raise ValueError('unexpected executable publication path')
                                row['native'] = native_outcomes(program, a.case, label, a)
                                row['valid'] = bool(row['native']) and all(x['valid'] for x in row['native'])
                                row['executable_sha256'] = digest(program)
                                llvm = Path(report['artifacts']['llvm'])
                                row['llvm_text_sha256'] = digest(llvm)
                    except (OSError, ValueError, RuntimeError, TimeoutError, KeyError) as error:
                        row['valid'] = False
                        row['measurement_error'] = str(error)
                        worker.failed = True
                        worker.close()
                    pair.append(row)
                equal = pair_validation(pair)
                binary_equal = (len(pair) == 2 and 'executable_sha256' in pair[0]
                                and pair[0].get('executable_sha256') == pair[1].get('executable_sha256'))
                for row in pair:
                    row['paired_semantics_equal'] = equal
                    row['paired_executable_equal'] = binary_equal if a.mode != 'check' else None
                    row['valid'] &= equal
                    rows.append(row)
                    with (a.out / 'rows.jsonl').open('a') as f:
                        f.write(json.dumps(row, ensure_ascii=False) + '\n')
                print(json.dumps({'block': block, 'request': label, 'measured': measured,
                                  'pair_valid': all(r['valid'] for r in pair),
                                  'arms': {r['arm']: {'wall_ms': r.get('wall_ms'), 'status': r.get('status')} for r in pair}}), flush=True)
                if all(r['valid'] and r['status'] == 'Success' for r in pair):
                    successful_snapshots.add(label)
                return all(r['valid'] for r in pair)

            for index, label in enumerate(selected):
                parent = transition(a.case, label)
                if label == 'restore' and 'cold' not in successful_snapshots:
                    if not run_pair(index, 'cold', False, None):
                        raise RuntimeError('initial successful snapshot for restoration failed')
                if parent is not None and not run_pair(index, parent, False, None):
                    raise RuntimeError(f'preparation for {label} failed; no valid measured pair')
                run_pair(index, label, True, parent)
        finally:
            for worker in workers.values():
                worker.close()
    cells = []
    for label in selected:
        pairs = []
        for block in range(a.blocks):
            selected_rows = {r['arm']: r for r in rows if r['block'] == block and r['request'] == label and r['measured']}
            if len(selected_rows) == 2 and all(r['valid'] for r in selected_rows.values()):
                b, c = selected_rows['baseline'], selected_rows['candidate']
                pairs.append({'block': block, 'baseline_ms': b['wall_ms'], 'candidate_ms': c['wall_ms'],
                              'candidate_over_baseline': c['wall_ms'] / b['wall_ms'],
                              'saved_ms': b['wall_ms'] - c['wall_ms']})
        cells.append({'request': label, 'valid_pairs': len(pairs), 'expected_pairs': a.blocks, 'pairs': pairs,
                      'median_baseline_ms': statistics.median(r['baseline_ms'] for r in pairs) if len(pairs) == a.blocks else None,
                      'median_candidate_ms': statistics.median(r['candidate_ms'] for r in pairs) if len(pairs) == a.blocks else None,
                      'baseline_range_ms': [min(r['baseline_ms'] for r in pairs), max(r['baseline_ms'] for r in pairs)] if pairs else None,
                      'candidate_range_ms': [min(r['candidate_ms'] for r in pairs), max(r['candidate_ms'] for r in pairs)] if pairs else None,
                      'median_candidate_over_baseline': statistics.median(r['candidate_over_baseline'] for r in pairs) if len(pairs) == a.blocks else None,
                      'median_saved_ms': statistics.median(r['saved_ms'] for r in pairs) if len(pairs) == a.blocks else None})
    dump(a.out / 'summary.json', {'rows': len(rows), 'measured_rows': sum(r['measured'] for r in rows), 'invalid_rows': sum(not r['valid'] for r in rows),
                                 'native_executions': sum(len(r.get('native', [])) for r in rows), 'cells': cells})
    return int(any(not r['valid'] for r in rows))


if __name__ == '__main__':
    raise SystemExit(main())
