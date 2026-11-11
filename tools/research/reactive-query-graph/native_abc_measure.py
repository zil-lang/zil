#!/usr/bin/env python3
"""Matched real CLI A/B/C requests using one frozen compiler binary.

A: existing caches; B: producer versions and direct reuse; C: B plus the graph.
Only run in the root-assigned single heavy window. Profiled cost diagnostics and
unprofiled wall measurements are separate runs, never pooled by the summarizer.
"""
import argparse
import itertools
import json
import os
from pathlib import Path
import re

from native_measure import (Worker, command, digest, dump, native_outcomes,
                            pair_validation, report_summary, transition)
from native_workloads import EXISTING, write_snapshot
from native_abc_workloads import oracle, requests, snapshot

ARMS = {'A': [], 'B': ['--producer-reuse'],
        'C': ['--producer-reuse', '--reactive-queries']}
# Every arm occupies every position twice; every directed adjacency occurs.
ORDERS = [('A', 'B', 'C'), ('B', 'C', 'A'), ('C', 'A', 'B'),
          ('C', 'B', 'A'), ('A', 'C', 'B'), ('B', 'A', 'C')]
CONTRASTS = [('A', 'B'), ('B', 'C'), ('A', 'C')]


def validate_selection(a, selected):
    if a.blocks < 1 or a.sample_ms <= 0 or a.timeout <= 0 or a.native_timeout <= 0:
        raise ValueError('blocks, sample-ms and timeouts must be positive')
    if not re.fullmatch('[0-9a-f]{40}', a.source):
        raise ValueError('source must be one complete frozen git commit SHA')
    if not selected or len(set(selected)) != len(selected):
        raise ValueError('request selection must be nonempty and unique')
    if any(label not in requests(a.case) for label in selected):
        raise ValueError('request is not defined by this workload')
    if 'cold' in selected and selected[0] != 'cold':
        raise ValueError('cold must be first in a fresh block')
    if 'multi' in selected and a.modules < 2:
        raise ValueError('multi requires two actually changed modules')
    if 3 * len(selected) >= 4096:
        raise ValueError('sequence would approach the resident reset boundary')
    reserved = {'--producer-reuse', '--reactive-queries', '--profile-compilation',
                '--resident', '--resident-capacity', '--resident-reset-after',
                '--semantic-cache', '--object-cache', '--cache-dir'}
    if any(value.split('=', 1)[0] in reserved for value in a.extra_arg):
        raise ValueError('extra-arg cannot alter arm, profile, session or cache policy')


def row_valid(report, label):
    if label == 'error':
        diagnostics = report['diagnostics']
        return (report['status'] == 'SafeReject' and report['phase'] == 'types'
                and report['verification_runs'] == 0 and len(diagnostics) == 1
                and diagnostics[0]['code'] == 'E-UNKNOWN-NAME'
                and diagnostics[0].get('span') is not None)
    return (report['status'] == 'Success' and report['verification_runs'] == 1
            and report['diagnostics'] == []
            and sum(event == {'phase': 'verify', 'disposition': 'executed'}
                    for event in report['query_events']) == 1)


def changed_control(case, label, changed, a):
    if label == 'unchanged' and changed:
        raise ValueError('unchanged changed source bytes')
    if label not in ['cold', 'unchanged'] and not changed:
        raise ValueError(f'{label} did not change source bytes')
    if case in ['scalar', 'mixed']:
        expected = {'private': ['m0.zil'], 'private_equal_width': ['m0.zil'],
                    'multi': sorted(f'm{i}.zil' for i in {0, a.modules // 2, a.modules - 1}),
                    'broad': sorted(f'm{i}.zil' for i in range(a.modules))}.get(label)
        if expected is not None and changed != expected:
            raise ValueError(f'{label} changed an unexpected source set')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source', required=True, help='complete frozen compiler git SHA')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', choices=['scalar', 'mixed', *EXISTING], default='scalar')
    parser.add_argument('--modules', type=int, default=20)
    parser.add_argument('--functions', type=int, default=20)
    parser.add_argument('--steps', type=int, default=16)
    parser.add_argument('--blocks', type=int, default=1)
    parser.add_argument('--request', action='append')
    parser.add_argument('--mode', choices=['check', 'O0', 'O2'], default='check')
    parser.add_argument('--backend', choices=['clang', 'llvm-api'], default='llvm-api')
    parser.add_argument('--cache', choices=['none', 'semantic', 'object', 'all'], default='all')
    parser.add_argument('--source-cache', action='store_true')
    parser.add_argument('--closed-executable', action='store_true')
    parser.add_argument('--codegen-units', type=int, default=1)
    parser.add_argument('--codegen-jobs', type=int, default=1)
    parser.add_argument('--capacity', type=int, default=256)
    parser.add_argument('--timeout', type=float, default=180)
    parser.add_argument('--native-timeout', type=float, default=10)
    parser.add_argument('--sample-ms', type=float, default=10)
    parser.add_argument('--profile-compilation', action='store_true', help='cost diagnostic only')
    parser.add_argument('--extra-arg', action='append', default=[])
    a = parser.parse_args()
    selected = a.request or ([x for x in ['cold', 'private_equal_width', 'private', 'multi', 'broad', 'restore']
                             if x in requests(a.case)])
    try:
        validate_selection(a, selected)
    except ValueError as error:
        parser.error(str(error))
    a.binary = a.binary.resolve()
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    binary_hash = digest(a.binary)
    scripts = ['native_abc_measure.py', 'native_abc_summarize.py', 'native_abc_workloads.py', 'native_measure.py',
               'native_workloads.py', 'native_summarize.py']
    hashes = {}
    for name in scripts:
        original = Path(__file__).with_name(name)
        (a.out / name).write_bytes(original.read_bytes())
        hashes[name] = digest(original)
    manifest = {'schema': 'native-abc-v1',
                'arguments': {k: str(v) if isinstance(v, Path) else v for k, v in vars(a).items()},
                'source_sha': a.source, 'binary': {'path': str(a.binary), 'sha256': binary_hash},
                'arms': ARMS, 'requests': selected, 'script_sha256': hashes,
                'purpose': 'profiled_cost_diagnostic' if a.profile_compilation else 'unprofiled_wall',
                'orders': ORDERS, 'affinity': sorted(os.sched_getaffinity(0)),
                'load_average': list(os.getloadavg()),
                'environment': {key: os.environ.get(key) for key in
                                ['ZIL_CLANG', 'ZIL_LLVM_CONFIG', 'LLVM_CONFIG', 'LD_LIBRARY_PATH']},
                'cpu_clock_ticks_per_second': os.sysconf('SC_CLK_TCK'),
                'transition_policy': 'Each edit has a validated unmeasured named parent; restoration also requires an earlier successful cold snapshot. Each block has independent A/B/C resident and application caches, with shared identical absolute source paths. OS page cache is not flushed.',
                'measurement_boundary': 'Wall is send compile through complete response; startup, source writes and native oracle runs are separate. CPU includes waited children at /proc tick precision. Sampled live tree RSS sums can double-count shared pages and are not child high-water measurements.',
                'correctness_boundary': 'CLI public report/diagnostic/span/identity and native independent oracles. Full TypedProgram equality is covered by separate Rust differential tests. Unsupported, transport failure and missing fields are invalid evidence; no performance acceptance is inferred.'}
    dump(a.out / 'manifest.json', manifest)
    for arm, flags in ARMS.items():
        setattr(a, arm + '_extra_arg', flags + (['--profile-compilation'] if a.profile_compilation else []))
    rows = []
    fatal = None
    try:
        for block in range(a.blocks):
            source = a.out / f'source-{block}'
            write_snapshot(source, snapshot(a.case, 'cold', a.modules, a.functions, a.steps))
            workers, previous, successful = {}, {}, set()
            try:
                for arm in ORDERS[block % len(ORDERS)]:
                    directory = a.out / f'block-{block}-{arm}'
                    directory.mkdir()
                    workers[arm] = Worker(command(a.binary, source, directory, a, arm),
                                          directory, a.timeout, a.sample_ms)

                def run(index, label, measured, parent):
                    nonlocal previous
                    files = snapshot(a.case, label, a.modules, a.functions, a.steps)
                    changed = sorted(k for k in set(previous) | set(files) if previous.get(k) != files.get(k))
                    identity = write_snapshot(source, files)
                    previous = files
                    identity['changed_files'] = changed
                    archived = write_snapshot(a.out / 'snapshots' / label, files)
                    if archived['identity_sha256'] != identity['identity_sha256']:
                        raise RuntimeError('archived/current source identity differs')
                    if measured:
                        changed_control(a.case, label, changed, a)
                    name = f'{index}-' + ('' if measured else 'prepare-') + label
                    dump(a.out / f'source-{block}-{name}.json', identity)
                    group = []
                    for position, arm in enumerate(ORDERS[(block + index) % len(ORDERS)]):
                        worker = workers[arm]
                        program = worker.directory / 'program'
                        program.unlink(missing_ok=True)
                        row = {'block': block, 'request_index': index, 'request': label,
                               'measured': measured, 'transition_from': parent, 'arm': arm,
                               'position': position, 'case': a.case, 'mode': a.mode,
                               'source_identity_sha256': identity['identity_sha256'], 'changed_files': changed}
                        try:
                            if worker.failed:
                                raise RuntimeError('resident transport previously failed')
                            row.update(worker.request(name))
                            row.update(report_summary(row['resident']))
                            report = row['resident']['report']
                            if set(report['source_files'].values()) != {str(source / name) for name in files}:
                                raise ValueError('source map does not cover the exact input snapshot')
                            row['valid'] = row_valid(report, label)
                            if row['valid'] and label != 'error' and a.mode != 'check':
                                if report['artifacts']['executable'] != str(program):
                                    raise ValueError('unexpected executable publication')
                                row['native'] = native_outcomes(program, a.case, label, a, oracle_fn=oracle)
                                row['valid'] = bool(row['native']) and all(item['valid'] for item in row['native'])
                                row['executable_sha256'] = digest(program)
                                row['llvm_text_sha256'] = digest(Path(report['artifacts']['llvm']))
                        except (OSError, ValueError, RuntimeError, TimeoutError, KeyError) as error:
                            row.update(valid=False, measurement_error=str(error))
                            worker.failed = True
                            worker.close()
                        group.append(row)
                    equal = all(pair_validation(list(pair)) for pair in itertools.combinations(group, 2))
                    executable_hashes = [row.get('executable_sha256') for row in group]
                    for row in group:
                        row['paired_semantics_equal'] = equal
                        row['paired_executable_equal'] = (bool(executable_hashes[0]) and len(set(executable_hashes)) == 1) if a.mode != 'check' else None
                        row['valid'] &= equal
                        rows.append(row)
                        with (a.out / 'rows.jsonl').open('a') as handle:
                            handle.write(json.dumps(row, ensure_ascii=False) + '\n')
                    valid = all(row['valid'] for row in group)
                    if valid and all(row['status'] == 'Success' for row in group):
                        successful.add(label)
                    print(json.dumps({'block': block, 'request': label, 'measured': measured,
                                      'triplet_valid': valid, 'wall_ms': {row['arm']: row.get('wall_ms') for row in group}}), flush=True)
                    return valid

                for index, label in enumerate(selected):
                    parent = transition(a.case, label)
                    if label == 'restore' and 'cold' not in successful and not run(index, 'cold', False, None):
                        raise RuntimeError('initial successful snapshot for restoration failed')
                    if parent is not None and not run(index, parent, False, None):
                        raise RuntimeError(f'parent preparation failed for {label}')
                    if not run(index, label, True, parent):
                        raise RuntimeError(f'invalid A/B/C triplet for {label}')
            finally:
                for worker in workers.values():
                    worker.close()
    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
        fatal = str(error)
    final_hash = digest(a.binary)
    valid = fatal is None and binary_hash == final_hash and all(row['valid'] for row in rows)
    dump(a.out / 'summary.json', {'complete': valid, 'fatal_error': fatal,
                                 'binary_sha256_after': final_hash, 'rows': len(rows),
                                 'measured_rows': sum(row['measured'] for row in rows),
                                 'invalid_rows': sum(not row['valid'] for row in rows),
                                 'native_executions': sum(len(row.get('native', [])) for row in rows)})
    return int(not valid)


if __name__ == '__main__':
    raise SystemExit(main())
