#!/usr/bin/env python3
"""Describe complete paired native runs; never decide performance acceptance.

Uses only the standard library. Each run stays separate, including different
project sizes. Bootstrap intervals describe exploratory means, not a stable p95.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import random
import re
import statistics
from native_measure import validate_response


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def json_safe(value):
    """Keep malformed numeric evidence visible without emitting nonstandard JSON."""
    if isinstance(value, dict):
        return {key: json_safe(item) for key, item in value.items()}
    if isinstance(value, list):
        return [json_safe(item) for item in value]
    if type(value) is float and not math.isfinite(value):
        return {'invalid_nonfinite_number': repr(value)}
    return value


def describe(values):
    values = list(values)
    if not values:
        return {'n': 0}
    return {'n': len(values), 'mean': statistics.mean(values),
            'median': statistics.median(values), 'min': min(values),
            'max': max(values), 'sum': sum(values), 'values': values}


def percentile(ordered, fraction):
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def interval(values, draws):
    means = sorted(statistics.mean(values[i] for i in draw) for draw in draws)
    return {'confidence': 0.95, 'method': 'paired block percentile bootstrap of arithmetic mean',
            'lower': percentile(means, 0.025), 'upper': percentile(means, 0.975),
            'replicates': len(draws), 'exploratory_only': True}


def flatten(value, prefix=''):
    result = {}
    if isinstance(value, dict):
        for key, item in value.items():
            result.update(flatten(item, f'{prefix}.{key}' if prefix else key))
    elif number(value):
        result[prefix] = value
    return result


def metrics(rows, getter):
    records = [flatten(getter(row)) for row in rows]
    keys = sorted({key for record in records for key in record})
    return {key: {**describe(record[key] for record in records if key in record),
                  'missing_or_null': sum(key not in record for record in records)}
            for key in keys}


def arm_details(rows):
    return {
        'cpu_seconds': metrics(rows, lambda r: {k: r.get(k) for k in ['cpu_seconds', 'own_cpu_seconds']}),
        'memory_kib': metrics(rows, lambda r: {
            'sampled_live_tree_peak_rss_sum': r.get('tree_peak_sampled_rss_sum_kib'),
            'resident_rss_at_response': r.get('resident', {}).get('rss_kib'),
            'session_cumulative_hwm': r.get('resident', {}).get('peak_kib')}),
        'memory_sample_count': describe(r['memory_sample_count'] for r in rows if number(r.get('memory_sample_count'))),
        'actual_frontend_and_graph_counts': metrics(rows, lambda r: r.get('actual_work_fields', {})),
        'rows_without_actual_work_counters': sum(not flatten(r.get('actual_work_fields', {})) for r in rows),
        'phase_ms': metrics(rows, lambda r: {k: v / 1e6 for k, v in (r.get('phase_timings_ns') or {}).items() if number(v)}),
        'object_cache_counts_and_timings': metrics(rows, lambda r: (r.get('native_codegen') or {}).get('object_cache', {})),
        'disk_cache_states': dict(Counter(str(r.get('disk_cache')) for r in rows)),
        'native_executions': sum(len(r.get('native', [])) for r in rows),
    }


def weights_from_policy(policy):
    declared = policy['weights']
    if isinstance(declared, dict):
        groups = declared
    else:
        groups = {}
        for name, body in re.findall(r'([A-Za-z][A-Za-z-]*)\[([^\]]+)\]', declared):
            pairs = re.findall(r'([a-z_]+)\s+(\d+(?:\.\d*)?|\.\d+)', body)
            groups[name] = {label: float(weight) for label, weight in pairs}
    if not groups:
        raise ValueError('policy has no machine-readable hypothetical weight groups')
    for name, group in groups.items():
        if not group or any(not number(v) or v < 0 for v in group.values()) or not math.isclose(sum(group.values()), 1, abs_tol=1e-9):
            raise ValueError(f'invalid hypothetical weights: {name}')
    return groups


def summarize(path, groups, seed, repetitions):
    errors = []
    result = {'run': str(path.resolve()), 'errors': errors,
              'performance_acceptance': 'not_decided_by_tool'}
    try:
        manifest = json.loads((path / 'manifest.json').read_text())
        rows = [json.loads(line) for line in (path / 'rows.jsonl').read_text().splitlines() if line.strip()]
        summary = json.loads((path / 'summary.json').read_text())
    except (OSError, ValueError) as error:
        errors.append(f'incomplete/unreadable run: {error}')
        result['complete_valid_evidence'] = False
        return result
    args = manifest['arguments']
    labels = manifest['requests']
    blocks = args['blocks']
    result['identity'] = {key: manifest.get(key) for key in ['source_shas', 'binaries', 'runner_sha256', 'generator_sha256']}
    result['configuration'] = args
    result['environment'] = {key: manifest.get(key) for key in ['environment', 'affinity', 'load_average', 'cpu_clock_ticks_per_second']}
    if type(blocks) is not int or blocks < 1 or not labels or len(set(labels)) != len(labels):
        raise ValueError('invalid manifest block/request declaration')
    for filename, key in [('native_measure.py', 'runner_sha256'), ('native_workloads.py', 'generator_sha256')]:
        try:
            if hashlib.sha256((path / filename).read_bytes()).hexdigest() != manifest.get(key):
                errors.append(f'archived script hash mismatch: {filename}')
        except OSError as error:
            errors.append(str(error))
    measured = [row for row in rows if row.get('measured') is True]
    preparations = [row for row in rows if row.get('measured') is False]
    rejected_rows = []
    request_numbers = Counter()
    for row in rows:
        prior_errors = len(errors)
        if type(row.get('measured')) is not bool:
            errors.append('row missing measured boolean')
        if row.get('valid') is not True or row.get('paired_semantics_equal') is not True:
            errors.append(f"invalid row: block={row.get('block')} request={row.get('request')} arm={row.get('arm')}")
        response = row.get('resident', {})
        report = response.get('report', {})
        identity = (row.get('block'), row.get('arm'))
        request_numbers[identity] += 1
        try:
            validate_response(response, request_numbers[identity])
        except (KeyError, TypeError, ValueError) as error:
            errors.append(f'invalid full resident response: {error}')
        if response.get('active_queries') != 0 or response.get('session_cleared') is not False or response.get('session_epoch') != 0:
            errors.append('missing/invalid resident cleanup or epoch')
        if row.get('request') == 'error':
            diagnostics = report.get('diagnostics', [])
            if (report.get('status') != 'SafeReject' or report.get('verification_runs') != 0
                    or len(diagnostics) != 1 or diagnostics[0].get('code') != 'E-UNKNOWN-NAME'):
                errors.append('unexpected error-control result')
        else:
            if report.get('status') != 'Success' or type(report.get('verification_runs')) is not int or report.get('verification_runs') != 1:
                errors.append('missing current successful Verify')
            native = row.get('native', [])
            if args['mode'] != 'check' and (not native or any(n.get('valid') is not True or n.get('expected') != n.get('actual') for n in native)):
                errors.append('missing/invalid native oracle result')
        for key in ['wall_ms', 'cpu_seconds', 'own_cpu_seconds', 'tree_peak_sampled_rss_sum_kib', 'memory_sample_count']:
            if not number(row.get(key)) or row[key] < 0:
                errors.append(f'missing/invalid measurement: {key}')
        if row.get('memory_sample_count', 0) <= 0 or row.get('tree_peak_sampled_rss_sum_kib', 0) <= 0:
            errors.append('missing sampled memory evidence')
        if len(errors) > prior_errors:
            rejected_rows.append({'errors': errors[prior_errors:], 'row': row})
    observed = {'rows': len(rows), 'measured_rows': len(measured),
                'invalid_rows': sum(row.get('valid') is not True for row in rows),
                'native_executions': sum(len(row.get('native', [])) for row in rows)}
    for key, value in observed.items():
        if summary.get(key) != value:
            errors.append(f'summary/raw count mismatch: {key}')
    if len(measured) != 2 * blocks * len(labels):
        errors.append('measured row count differs from complete expected matrix')
    cells = summary.get('cells', [])
    if len(cells) != len(labels) or {cell.get('request') for cell in cells} != set(labels):
        errors.append('summary cells differ from expected scenarios')
    matrix = {}
    for label in labels:
        matrix[label] = []
        for block in range(blocks):
            pair = [row for row in measured if row.get('request') == label and row.get('block') == block]
            if len(pair) != 2 or {row.get('arm') for row in pair} != {'baseline', 'candidate'}:
                errors.append(f'missing/duplicate measured pair: {label} block={block}')
                continue
            indexed = {row['arm']: row for row in pair}
            left, right = indexed['baseline'], indexed['candidate']
            if left.get('source_identity_sha256') != right.get('source_identity_sha256'):
                errors.append(f'pair source mismatch: {label} block={block}')
            for field in ['status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces']:
                a = left.get('resident', {}).get('report', {})
                b = right.get('resident', {}).get('report', {})
                if field not in a or field not in b or a[field] != b[field]:
                    errors.append(f'pair semantic mismatch/missing field: {label} block={block} {field}')
            if [n.get('stdin_hex') for n in left.get('native', [])] != [n.get('stdin_hex') for n in right.get('native', [])]:
                errors.append(f'pair native input mismatch: {label} block={block}')
            if not number(left.get('wall_ms')) or not number(right.get('wall_ms')):
                continue
            matrix[label].append((block, left, right))
        found = [cell for cell in cells if cell.get('request') == label]
        if len(found) != 1 or found[0].get('expected_pairs') != blocks or found[0].get('valid_pairs') != blocks:
            errors.append(f'summary lacks complete valid pairs: {label}')
        elif len(matrix[label]) == blocks:
            expected = {b: (l['wall_ms'], r['wall_ms']) for b, l, r in matrix[label]}
            saved = found[0].get('pairs', [])
            if len(saved) != blocks or any(p.get('block') not in expected or (p.get('baseline_ms'), p.get('candidate_ms')) != expected.get(p.get('block')) for p in saved) or len({p.get('block') for p in saved}) != blocks:
                errors.append(f'summary/raw pair mismatch: {label}')
            for pair in saved:
                if pair.get('block') in expected:
                    baseline, candidate = expected[pair['block']]
                    if baseline <= 0 or pair.get('saved_ms') != baseline - candidate or pair.get('candidate_over_baseline') != candidate / baseline:
                        errors.append(f'summary/raw paired arithmetic mismatch: {label}')
    result['counts'] = {**observed, 'preparation_rows': len(preparations), 'expected_blocks': blocks}
    result['preparations'] = {arm: arm_details([row for row in preparations if row.get('arm') == arm]) for arm in ['baseline', 'candidate']}
    result['complete_valid_evidence'] = not errors
    # Retain invalid raw rows explicitly; never bootstrap a silently filtered set.
    result['invalid_rows'] = [row for row in rows if row.get('valid') is not True]
    result['rejected_rows'] = rejected_rows
    if errors:
        result['scenarios'] = {label: {'observed_pairs': len(pairs), 'expected_pairs': blocks} for label, pairs in matrix.items()}
        result['hypothetical_wait_per_100_requests'] = {'available': False, 'reason': 'invalid/incomplete evidence'}
        return result
    rng = random.Random(seed)
    draws = [[rng.randrange(blocks) for _ in range(blocks)] for _ in range(repetitions)]
    result['bootstrap'] = {'seed': seed, 'replicates': repetitions, 'sampling_unit': 'matched block',
                           'caution': f'Exploratory mean intervals with n={blocks}; six samples cannot estimate a stable p95. No performance acceptance is inferred.'}
    result['scenarios'] = {}
    for label, pairs in matrix.items():
        baseline = [left['wall_ms'] for _, left, _ in pairs]
        candidate = [right['wall_ms'] for _, _, right in pairs]
        saved = [left - right for left, right in zip(baseline, candidate)]
        result['scenarios'][label] = {
            'blocks': [block for block, _, _ in pairs], 'baseline_wall_ms': describe(baseline),
            'candidate_wall_ms': describe(candidate), 'paired_saved_ms': describe(saved),
            'paired_mean_saved_ms_ci': interval(saved, draws),
            'paired_candidate_over_baseline': describe(right / left for left, right in zip(baseline, candidate) if left > 0),
            'details': {arm: arm_details([pair[index] for pair in pairs]) for arm, index in [('baseline', 1), ('candidate', 2)]}}
    weighted = {}
    for name, weights in groups.items():
        missing = sorted(set(weights) - set(matrix))
        if missing:
            weighted[name] = {'available': False, 'missing_scenarios': missing, 'weights': weights}
            continue
        baseline = [100 * sum(weight * matrix[label][b][1]['wall_ms'] for label, weight in weights.items()) for b in range(blocks)]
        candidate = [100 * sum(weight * matrix[label][b][2]['wall_ms'] for label, weight in weights.items()) for b in range(blocks)]
        saved = [left - right for left, right in zip(baseline, candidate)]
        weighted[name] = {'available': True, 'hypothetical_only': True, 'weights': weights,
                          'baseline_total_ms': describe(baseline), 'candidate_total_ms': describe(candidate),
                          'paired_saved_ms': describe(saved), 'paired_mean_saved_ms_ci': interval(saved, draws),
                          'interpretation': '100 hypothetical requests; common block resampling preserves cross-scenario correlation. Not an observed user distribution.'}
    result['hypothetical_wait_per_100_requests'] = weighted
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('runs', nargs='+', type=Path)
    parser.add_argument('--policy', required=True, type=Path)
    parser.add_argument('--seed', default=20261111, type=int)
    parser.add_argument('--bootstrap', default=20000, type=int)
    args = parser.parse_args()
    if args.bootstrap < 1:
        parser.error('bootstrap must be positive')
    policy_bytes = args.policy.read_bytes()
    policy = json.loads(policy_bytes)
    groups = weights_from_policy(policy)
    reports = []
    for path in args.runs:
        try:
            report = summarize(path, groups, args.seed, args.bootstrap)
            report['matches_policy_primary_configuration'] = all(
                report.get('configuration', {}).get(k) == v
                for k, v in policy.get('primary', {}).items() if k != 'comparator')
            reports.append(report)
        except (OSError, ValueError, KeyError, TypeError, ZeroDivisionError) as error:
            reports.append({'run': str(path.resolve()), 'complete_valid_evidence': False,
                            'errors': [f'malformed/incomplete evidence: {error}'],
                            'performance_acceptance': 'not_decided_by_tool'})
    print(json.dumps(json_safe({'policy_path': str(args.policy.resolve()),
                      'policy_sha256': hashlib.sha256(policy_bytes).hexdigest(), 'policy': policy,
                      'runs_kept_separate': True, 'reports': reports}), indent=2, allow_nan=False))
    return int(any(not report['complete_valid_evidence'] for report in reports))


if __name__ == '__main__':
    raise SystemExit(main())
