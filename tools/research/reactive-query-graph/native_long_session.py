#!/usr/bin/env python3
"""Measure uninterrupted paired scalar checks with genuinely unique private edits.

One binary, independent legacy/reactive resident processes, no resets or disk
caches. --edits N means one cold request plus N unique edits per process. Every
request is archived before validation; retry a failed run with a new --out.
This runs checks only, never generated programs or native compilation.
"""
import argparse
import difflib
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

from native_measure import Worker, digest, pair_validation, report_summary, validate_response
from native_workloads import snapshot, write_snapshot


GRAPH_FIELDS = (
    'executed', 'edge_reads', 'notifications', 'enqueues', 'deduplicated',
    'reused_bonds', 'new_bonds', 'removed_bonds', 'rank_visits', 'input_updates',
    'retained_nodes', 'retained_edges', 'inventory_visits', 'equal_results',
    'changed_results', 'cycle_check_edges', 'validation_edge_reads', 'rank_edge_reads',
)
TYPE_FIELDS = (
    'executed_functions', 'reused_functions', 'body_key_executions',
    'body_key_reuses', 'core_function_eq_comparisons', 'body_key_visited_nodes',
    'context_modules_visited', 'context_declarations_hashed', 'context_imports_hashed',
    'graph_edges',
)
SEMANTIC_FIELDS = (
    'status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces',
)
ANCHOR = '    if y % 3 == 0 { y = (y * 2 + 1) % 251 } else { y = (y + 2) % 251 }'


def save(path, value):
    """Atomic durable metadata checkpoints; previous raw evidence is immutable."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    with temporary.open('w') as output:
        json.dump(value, output, indent=2, ensure_ascii=False, allow_nan=False)
        output.write('\n')
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)


def append(path, row):
    with path.open('a') as output:
        output.write(json.dumps(row, ensure_ascii=False, allow_nan=False) + '\n')
        output.flush()
        os.fsync(output.fileno())


def number(value, name, positive=False):
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0 or (positive and value == 0):
        raise ValueError(f'missing/invalid numeric field: {name}')
    return value


def integer(value, name, positive=False):
    if type(value) is not int:
        raise ValueError(f'missing/noninteger field: {name}')
    return number(value, name, positive)


def validate_report(report, source, files):
    if not isinstance(report, dict) or any(field not in report for field in SEMANTIC_FIELDS):
        raise ValueError('missing compiler report or semantic fields')
    if (report['status'] != 'Success' or report['phase'] != 'verify'
            or report['diagnostics'] != [] or type(report.get('verification_runs')) is not int
            or report['verification_runs'] != 1):
        raise ValueError('check did not complete exactly one current independent Verify')
    events = report.get('query_events')
    if (not isinstance(events, list) or not events
            or sum(event == {'phase': 'verify', 'disposition': 'executed'} for event in events) != 1):
        raise ValueError('missing/nonexecuted current Verify event')
    if not isinstance(report['scalar_interfaces'], list) or not report['scalar_interfaces']:
        raise ValueError('missing/nonexecuted scalar interface observation')
    if (not isinstance(report['source_files'], dict)
            or set(report['source_files'].values()) != {str(source / name) for name in files}):
        raise ValueError('compiler source inventory differs from current shared snapshot')
    if not isinstance(report['input_fingerprint'], str) or not re.fullmatch('[0-9a-f]{64}', report['input_fingerprint']):
        raise ValueError('missing/invalid current input fingerprint')
    if report.get('native_codegen') is not None or report.get('commands') != []:
        raise ValueError('check unexpectedly performed native code generation or commands')
    if report.get('artifacts') != {}:
        raise ValueError('check unexpectedly published artifacts')
    number(report.get('seconds'), 'report.seconds', positive=True)
    timings = report.get('phase_timings_ns')
    if not isinstance(timings, dict) or not timings:
        raise ValueError('missing actual phase timings')
    for name, value in timings.items():
        integer(value, f'phase_timings_ns.{name}')
    stats = report.get('frontend_stats')
    if not isinstance(stats, dict) or not isinstance(stats.get('function_types'), dict):
        raise ValueError('missing actual Types work')
    types = stats['function_types']
    for field in TYPE_FIELDS:
        integer(types.get(field), f'function_types.{field}')
    if types['executed_functions'] + types['reused_functions'] == 0:
        raise ValueError('zero actual Types function work')
    if types['body_key_executions'] + types['body_key_reuses'] == 0:
        raise ValueError('zero actual BodyKey work')
    if not isinstance(types.get('graph'), dict):
        raise ValueError('missing graph statistics')
    for field in GRAPH_FIELDS:
        integer(types['graph'].get(field), f'graph.{field}')
    return types


def unique_snapshot(cold, index):
    files = dict(cold)
    # The first h0 branch is the first matching statement. Exactly one literal
    # changes; later h functions may contain the same arithmetic text.
    if not files['m0.zil'].startswith('fn h0(x: i32) -> i32 {\n'):
        raise ValueError('generator no longer starts m0 with h0')
    if files['m0.zil'].splitlines()[2] != ANCHOR:
        raise ValueError('generator first-branch anchor changed')
    replacement = ANCHOR.replace('y * 2 + 1)', f'y * 2 + {1 + index})', 1)
    files['m0.zil'] = files['m0.zil'].replace(ANCHOR, replacement, 1)
    return files


def fresh_check(binary, source, directory, timeout, files, expected):
    command = [str(binary), 'check', str(source / 'main.zil'), '--json']
    start = time.perf_counter_ns()
    try:
        result = subprocess.run(command, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        (directory / 'stdout.json').write_bytes(error.stdout or b'')
        (directory / 'stderr.log').write_bytes(error.stderr or b'')
        save(directory / 'execution.json', {'command': command, 'exit_code': None, 'timeout': True,
             'valid': False, 'wall_ms_excluded_from_resident_latency': (time.perf_counter_ns() - start) / 1e6})
        raise
    (directory / 'stdout.json').write_bytes(result.stdout)
    (directory / 'stderr.log').write_bytes(result.stderr)
    execution = {'command': command, 'exit_code': result.returncode,
                 'wall_ms_excluded_from_resident_latency': (time.perf_counter_ns() - start) / 1e6}
    save(directory / 'execution.json', execution)
    if result.returncode != 0:
        raise ValueError('fresh legacy check exited unsuccessfully')
    report = json.loads(result.stdout)
    validate_report(report, source, files)
    if any(report[field] != expected[field] for field in SEMANTIC_FIELDS):
        raise ValueError('fresh legacy diagnostics/interfaces/current source disagree')
    execution['valid'] = True
    execution['comparison_boundary'] = 'Public JSON only; hidden TypedProgram payload not compared.'
    save(directory / 'execution.json', execution)
    return execution


def memory_summary(rows):
    half = rows[len(rows) // 2:]
    xs = [row['index'] for row in half]
    ys = [row['resident']['rss_kib'] for row in half]
    xmean, ymean = statistics.mean(xs), statistics.mean(ys)
    denominator = sum((x - xmean) ** 2 for x in xs)
    return {
        'requests': len(rows),
        'max_resident_rss_kib': max(row['resident']['rss_kib'] for row in rows),
        'max_session_cumulative_hwm_kib': max(row['resident']['peak_kib'] for row in rows),
        'max_sampled_live_process_tree_rss_sum_kib': max(row['tree_peak_sampled_rss_sum_kib'] for row in rows),
        'initial_resident_rss_kib': rows[0]['resident']['rss_kib'],
        'final_resident_rss_kib': rows[-1]['resident']['rss_kib'],
        'second_half_start_index': half[0]['index'],
        'second_half_final_minus_start_rss_kib': ys[-1] - ys[0],
        'second_half_max_minus_min_rss_kib': max(ys) - min(ys),
        'second_half_rss_slope_kib_per_edit': sum((x - xmean) * (y - ymean) for x, y in zip(xs, ys)) / denominator if denominator else 0,
        'second_half_hwm_growth_kib': half[-1]['resident']['peak_kib'] - half[0]['resident']['peak_kib'],
        'max_cached_queries': max(row['resident']['cached_queries'] for row in rows),
        'max_graph_retained_nodes': max(row['graph']['retained_nodes'] for row in rows),
        'max_graph_retained_edges': max(row['graph']['retained_edges'] for row in rows),
    }


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--producer-reuse', action='store_true',
                        help='enable identical sealed producer reuse in both arms (B versus C)')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--modules', type=int, default=20)
    parser.add_argument('--functions', type=int, default=20)
    parser.add_argument('--steps', type=int, default=16)
    parser.add_argument('--edits', type=int, default=50)
    parser.add_argument('--capacity', type=int, default=32)
    parser.add_argument('--timeout', type=float, default=180)
    parser.add_argument('--sample-ms', type=float, default=10)
    parser.add_argument('--max-generated-statements', type=int, default=2_000_000)
    parser.add_argument('--max-source-bytes', type=int, default=16 * 1024 * 1024)
    parser.add_argument('--max-graph-nodes', type=int)
    parser.add_argument('--max-graph-edges', type=int)
    args = parser.parse_args()
    if not re.fullmatch('[0-9a-f]{40}', args.source_sha):
        parser.error('--source-sha must be the exact 40-character lowercase Git SHA')
    if min(args.modules, args.functions, args.steps, args.edits, args.max_generated_statements, args.max_source_bytes) < 1:
        parser.error('dimensions, edits and input limits must be positive')
    if args.modules * args.functions * args.steps > args.max_generated_statements:
        parser.error('input exceeds --max-generated-statements')
    if not 8 <= args.capacity <= 256 or args.edits + 1 >= 4096:
        parser.error('capacity must be 8..256 and cold+edits must remain below reset_after=4096')
    if not math.isfinite(args.timeout) or not math.isfinite(args.sample_ms) or min(args.timeout, args.sample_ms) <= 0:
        parser.error('timeout/sample-ms must be finite and positive')
    if any(value is not None and value < 1 for value in (args.max_graph_nodes, args.max_graph_edges)):
        parser.error('graph upper bounds must be positive')
    args.binary = args.binary.resolve(strict=True)
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error('--binary must be an executable file')
    args.out = args.out.resolve()
    return args


def main():
    args = arguments()
    args.out.mkdir(parents=True, exist_ok=False)
    source = args.out / 'source'
    workers, rows, checks, identities = {}, [], [], set()
    errors = []
    cold = snapshot('scalar', 'cold', args.modules, args.functions, args.steps)
    declared = sum(len(re.findall(r'\bfn\s+[A-Za-z_]\w*\s*\(', text)) for text in cold.values())
    calls = sum(len(re.findall(r'\b[A-Za-z_]\w*\s*\(', text)) for text in cold.values()) - declared
    node_bound = args.max_graph_nodes or (1 + 3 * declared)
    edge_bound = args.max_graph_edges or (3 * declared + calls)
    binary_hash = digest(args.binary)
    for name in ('native_long_session.py', 'native_measure.py', 'native_workloads.py'):
        (args.out / name).write_bytes(Path(__file__).with_name(name).read_bytes())
    manifest = {
        'arguments': {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()},
        'binary_sha256': binary_hash, 'source_sha_supplied_by_runner': args.source_sha,
        'script_sha256': digest(Path(__file__)),
        'generator_sha256': digest(Path(__file__).with_name('native_workloads.py')),
        'worker_sha256': digest(Path(__file__).with_name('native_measure.py')),
        'requests_per_arm': args.edits + 1, 'unique_private_edits': args.edits,
        'arms': {'baseline': ['--producer-reuse'] if args.producer_reuse else [],
                 'candidate': (['--producer-reuse'] if args.producer_reuse else []) + ['--reactive-queries']},
        'reset_after': 4096, 'fresh_legacy_indices': sorted(set(range(10, args.edits + 1, 10)) | {args.edits}),
        'declared_functions_including_externs': declared, 'syntactic_call_occurrences': calls,
        'graph_bounds': {'nodes': node_bound, 'edges': edge_bound,
                         'derivation': 'At most one context + signature/raw/body query per declared function; at most three fixed bonds per query plus all syntactic call occurrences. Includes externs conservatively; no generated declarations in scalar input.'},
        'affinity': sorted(os.sched_getaffinity(0)), 'load_average': list(os.getloadavg()),
        'environment': {key: os.environ.get(key) for key in ('ZIL_CLANG', 'ZIL_LLVM_CONFIG', 'LLVM_CONFIG', 'LD_LIBRARY_PATH')},
        'policy': 'Same current files, one binary, two independent resident sessions without disk caches. Arm request order alternates each index. Constant is 1+index; no earlier snapshot is restored. Cold index 0 is separate from edit latency.',
        'recovery': 'Output must be new. Each raw response/checkpoint and source patch is retained on failure; rerun from cold with a new output directory. Process caches are never claimed recoverable across processes.',
        'comparison_boundary': 'Public diagnostics, input/source identities, scalar interfaces, and exactly one current independent Verify. Hidden TypedProgram payload is not compared; the Rust long-session differential is the full TypedProgram control.',
        'coverage_boundary': 'check only; zero native builds/runs. Native coverage belongs to separate real O2 matrices.',
        'memory_boundary': 'RSS/HWM are session process gauges; sampled live process-tree RSS may double count shared pages. Entry/node/edge bounds are not byte bounds. HWM is cumulative, not per-request peak.',
        'timing_boundary': 'Worker wall is send compile through complete response; CPU is resident /proc delta including waited children, subject to clock tick quantization. Startup, source writes and fresh legacy checks excluded.',
    }
    save(args.out / 'manifest.json', manifest)
    previous = None
    previous_fingerprints, previous_hwm = {}, {}
    try:
        write_snapshot(source, cold)
        write_snapshot(args.out / 'snapshots' / 'initial', cold)
        for arm in ('baseline', 'candidate'):
            directory = args.out / arm
            directory.mkdir()
            command = [str(args.binary), 'check', str(source / 'main.zil'), '--json', '--resident',
                       '--resident-capacity', str(args.capacity), '--resident-reset-after', '4096']
            if args.producer_reuse:
                command.append('--producer-reuse')
            if arm == 'candidate':
                command.append('--reactive-queries')
            workers[arm] = Worker(command, directory, args.timeout, args.sample_ms)
            if workers[arm].ready.get('capacity_entries') != args.capacity or workers[arm].ready.get('reset_after') != 4096:
                raise ValueError('resident startup did not honor capacity/reset configuration')
        for index in range(args.edits + 1):
            files = unique_snapshot(cold, index)
            identity = write_snapshot(source, files)
            if identity['bytes'] > args.max_source_bytes:
                raise ValueError('input exceeds --max-source-bytes')
            if identity['identity_sha256'] in identities:
                raise ValueError('source snapshot repeated; not a unique-edit long session')
            identities.add(identity['identity_sha256'])
            expected_changes = sorted(files) if previous is None else ['m0.zil']
            actual_changes = sorted(name for name in files if previous is None or previous[name] != files[name])
            if actual_changes != expected_changes:
                raise ValueError('private edit changed an unexpected source set')
            name = f'{index:04d}'
            evidence = args.out / 'edits' / name
            evidence.mkdir(parents=True)
            patch = ''.join(difflib.unified_diff(
                [] if previous is None else previous['m0.zil'].splitlines(keepends=True),
                files['m0.zil'].splitlines(keepends=True), fromfile='before/m0.zil', tofile='after/m0.zil'))
            (evidence / 'm0.patch').write_text(patch)
            identity.update({'index': index, 'constant': 1 + index, 'changed_from_previous': actual_changes,
                             'parent_identity_sha256': None if previous is None else prior_identity,
                             'patch_sha256': hashlib.sha256(patch.encode()).hexdigest()})
            save(evidence / 'source-identity.json', identity)
            pair = []
            for position, arm in enumerate(('baseline', 'candidate') if index % 2 == 0 else ('candidate', 'baseline')):
                row = {'index': index, 'request': 'cold' if index == 0 else 'unique-private',
                       'arm': arm, 'position': position, 'source_identity_sha256': identity['identity_sha256'],
                       'constant': 1 + index, 'valid': False}
                try:
                    row.update(workers[arm].request(name))
                    for field in ('wall_ms', 'tree_peak_sampled_rss_sum_kib', 'memory_sample_count'):
                        number(row.get(field), field, positive=True)
                    for field in ('cpu_seconds', 'own_cpu_seconds'):
                        number(row.get(field), field)
                    response = row['resident']
                    validate_response(response, index + 1)
                    types = validate_report(response['report'], source, files)
                    for field in ('rss_kib', 'peak_kib'):
                        number(response[field], field, positive=True)
                    integer(response['cached_queries'], 'cached_queries')
                    if response['cached_queries'] > args.capacity or response['peak_kib'] < response['rss_kib']:
                        raise ValueError('capacity exceeded or invalid cumulative memory HWM')
                    if response['peak_kib'] < previous_hwm.get(arm, 0):
                        raise ValueError('session HWM decreased without a permitted reset')
                    previous_hwm[arm] = response['peak_kib']
                    graph = types['graph']
                    if graph['retained_nodes'] > node_bound or graph['retained_edges'] > edge_bound:
                        raise ValueError('graph exceeds source-inventory bound')
                    if arm == 'candidate' and (not graph['retained_nodes'] or not graph['retained_edges'] or not graph['executed']):
                        raise ValueError('candidate has no actual current reactive graph execution')
                    if arm == 'baseline' and any(graph.values()):
                        raise ValueError('legacy arm unexpectedly used reactive graph')
                    fingerprint = response['report']['input_fingerprint']
                    if fingerprint == previous_fingerprints.get(arm):
                        raise ValueError('compiler input fingerprint did not change with source')
                    previous_fingerprints[arm] = fingerprint
                    row.update(report_summary(response))
                    row['graph'] = graph
                    row['valid'] = True
                except (OSError, ValueError, RuntimeError, TimeoutError, KeyError, TypeError) as error:
                    row['error'] = str(error)
                pair.append(row)
                rows.append(row)
                save(evidence / f'{arm}.row.json', row)
                append(args.out / 'rows.jsonl', row)
                if not row['valid']:
                    raise ValueError(f'{arm} request {index}: {row["error"]}')
            if not pair_validation(pair):
                raise ValueError(f'paired diagnostics/interfaces/source mismatch at index {index}')
            if index in manifest['fresh_legacy_indices']:
                directory = args.out / 'fresh-legacy' / name
                directory.mkdir(parents=True)
                check = fresh_check(args.binary, source, directory, args.timeout, files, pair[0]['resident']['report'])
                checks.append({'index': index, **check})
            save(evidence / 'pair-validation.json', {'valid': True, 'index': index, 'fresh_legacy_checked': index in manifest['fresh_legacy_indices']})
            save(args.out / 'checkpoint.json', {'last_completed_index': index, 'valid_resident_rows': len(rows),
                                               'completed_fresh_checks': len(checks), 'source_identity_sha256': identity['identity_sha256']})
            print(json.dumps({'index': index, 'valid_pair': True, 'arms': {row['arm']: {'wall_ms': row['wall_ms'],
                  'rss_kib': row['resident']['rss_kib'], 'hwm_kib': row['resident']['peak_kib'],
                  'cached_queries': row['resident']['cached_queries'], 'graph': row['graph']} for row in pair}}), flush=True)
            previous, prior_identity = files, identity['identity_sha256']
        if digest(args.binary) != binary_hash:
            raise ValueError('binary changed during measurement')
    except (OSError, ValueError, RuntimeError, TimeoutError, KeyError, TypeError, subprocess.TimeoutExpired, KeyboardInterrupt) as error:
        errors.append(f'{type(error).__name__}: {error}')
        save(args.out / 'failure.json', {'errors': errors, 'resident_rows_preserved': len(rows),
                                        'fresh_checks_preserved': len(checks)})
    finally:
        if source.exists():
            final_files = {str(path.relative_to(source)): path.read_text() for path in sorted(source.rglob('*.zil'))}
            write_snapshot(args.out / 'snapshots' / 'final', final_files)
        for arm, worker in workers.items():
            worker.close()
            if worker.p.returncode != 0:
                errors.append(f'{arm} resident exit {worker.p.returncode}')
    expected_rows = 2 * (args.edits + 1)
    complete = (not errors and len(rows) == expected_rows and all(row['valid'] for row in rows)
                and len(identities) == args.edits + 1 and len(checks) == len(manifest['fresh_legacy_indices']))
    summary = {'complete_valid_evidence': complete, 'errors': errors,
               'expected_resident_rows': expected_rows, 'resident_rows': len(rows),
               'unique_source_snapshots': len(identities), 'fresh_legacy_checks': checks,
               'native_compilations': 0, 'native_executions': 0,
               'comparison_boundary': manifest['comparison_boundary'], 'coverage_boundary': manifest['coverage_boundary'],
               'performance_acceptance': 'not_decided_by_tool', 'arms': {}, 'edit_latency': None}
    if complete:
        for arm in ('baseline', 'candidate'):
            arm_rows = [row for row in rows if row['arm'] == arm]
            summary['arms'][arm] = {**memory_summary(arm_rows),
                'cache_within_capacity_every_request': all(row['resident']['cached_queries'] <= args.capacity for row in arm_rows),
                'graph_within_source_bounds_every_request': all(row['graph']['retained_nodes'] <= node_bound and row['graph']['retained_edges'] <= edge_bound for row in arm_rows),
                'cold_wall_ms': arm_rows[0]['wall_ms'],
                'edit_wall_ms_sum': sum(row['wall_ms'] for row in arm_rows[1:]),
                'edit_cpu_seconds_sum': sum(row['cpu_seconds'] for row in arm_rows[1:])}
        baseline = [row for row in rows if row['arm'] == 'baseline' and row['index'] > 0]
        candidate = [row for row in rows if row['arm'] == 'candidate' and row['index'] > 0]
        ratios = [c['wall_ms'] / b['wall_ms'] for b, c in zip(baseline, candidate)]
        summary['edit_latency'] = {'pairs': len(ratios), 'median_candidate_over_baseline': statistics.median(ratios),
                                   'total_candidate_over_baseline': sum(row['wall_ms'] for row in candidate) / sum(row['wall_ms'] for row in baseline),
                                   'cold_excluded': True, 'fresh_legacy_time_excluded': True}
    save(args.out / 'summary.json', summary)
    return 0 if complete else 1


if __name__ == '__main__':
    raise SystemExit(main())
