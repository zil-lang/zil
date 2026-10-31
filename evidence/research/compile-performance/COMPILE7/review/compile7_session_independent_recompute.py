#!/usr/bin/env python3
"""Read-only independent audit/statistics for completed CP29/CP30 session windows.

Never imports or executes a compiler binary. Original failed windows are retained
and are never pooled with recovery windows. Standard library only.
"""
import argparse
import collections
import copy
import datetime
import hashlib
import importlib.util
import itertools
import json
import math
from pathlib import Path
import statistics
import sys

sys.dont_write_bytecode = True
ROOT = Path('/workspace/scratch/c3607737969d')
CASES = {'tree4x2': .05, 'tree32x8': .15, 'tree256x8': .30,
         'tree1024x8': .30, 'retained': .20}
CORE_REQUESTS = {'cold': .10, 'unchanged': .35, 'private': .40, 'public': .15}
REQUESTS = ['cold', 'unchanged', 'private', 'public', 'restore', 'private_again', 'restore_again']
SNAPSHOTS = ['base', 'base', 'private', 'public', 'base', 'private', 'base']
CAPACITIES = ['default', '8192']
HISTORICAL_BASELINE = 'ee72fd6f701efa5f7a75ea3f13d4effc20d5c4b8'
CANDIDATES = {'cp29': 'a0f38d2bca8f01fc19ff1f2fa0a42c3983c223b8',
              'cp30': '1b86175c8728834c20d1bb18ce13a329764e315f'}
RUNNER = ROOT / 'zil/tools/research/compile-performance/compile7_session.py'
sha_cache = {}


def sha(path):
    path = Path(path)
    stat = path.stat()
    key = (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    if key not in sha_cache:
        with path.open('rb') as stream:
            sha_cache[key] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return sha_cache[key]


def read(path):
    return json.loads(Path(path).read_text())


def near(actual, expected):
    assert type(actual) in (int, float) and type(expected) in (int, float)
    assert math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), (actual, expected)


def quantile(values, probability):
    values = sorted(values)
    position = (len(values) - 1) * probability
    low, high = math.floor(position), math.ceil(position)
    return values[low] + (values[high] - values[low]) * (position - low)


BOOTSTRAP = []
for sample in itertools.combinations_with_replacement(range(5), 5):
    multiplicity = math.factorial(5)
    for count in collections.Counter(sample).values():
        multiplicity //= math.factorial(count)
    BOOTSTRAP.append((sample, multiplicity))
assert sum(multiplicity for _, multiplicity in BOOTSTRAP) == 3125


def weighted_result(components, unit):
    """Component = (fixed absolute-cost weight, five A block means, five B means)."""
    totals = {arm: math.fsum(weight * statistics.median(values[index])
                            for weight, *values in components)
              for index, arm in enumerate('AB')}
    paired = []
    for block in range(5):
        values = {arm: math.fsum(weight * values[index][block]
                                for weight, *values in components)
                  for index, arm in enumerate('AB')}
        paired.append({'block': block, **values, 'B_over_A': values['B'] / values['A']})
    resampled = []
    for sample, multiplicity in BOOTSTRAP:
        values = {arm: math.fsum(weight * statistics.median(values[index][b] for b in sample)
                                for weight, *values in components)
                  for index, arm in enumerate('AB')}
        resampled.extend([values['B'] / values['A']] * multiplicity)
    return {'unit': unit, **totals, 'B_over_A': totals['B'] / totals['A'],
            'B_change_percent': (totals['B'] / totals['A'] - 1) * 100,
            'paired_blocks': paired,
            'paired_block_ratios_B_over_A': [p['B_over_A'] for p in paired],
            'median_paired_block_ratio': statistics.median(p['B_over_A'] for p in paired),
            'exploratory_paired_block_bootstrap95': [quantile(resampled, .025), quantile(resampled, .975)],
            'bootstrap_ordered_resamples': 3125}


def value(row, metric):
    if metric == 'process_wall':
        return row['process_wall_ns'] / 1e6
    if metric == 'process_cpu':
        return row['process_cpu_s'] * 1000
    if metric == 'process_maxrss':
        return row['process_maxrss_kib']
    return sum(request['compile_ns'] for request in row['requests']
               if metric == 'sequence' or request['request'] == metric) / 1e6


spec = importlib.util.spec_from_file_location('review_compile7_session_v3', RUNNER)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def audit_window(name, root):
    plan_path = root / 'plan.json'
    plan = read(plan_path)
    assert plan['schema'] == 'compile7-session-v3'
    assert plan['cases'] == list(CASES) and plan['capacities'] == CAPACITIES
    assert plan['blocks'] == 5 and plan['requests'] == REQUESTS
    assert plan['binaries']['A']['declared_source'] == HISTORICAL_BASELINE
    assert plan['binaries']['B']['declared_source'] == CANDIDATES[name]
    expected_order = [{'block': block, 'capacity': capacity, 'case': case,
                       'position': position, 'arm': arm}
                      for block in range(5) for capacity in CAPACITIES for case in CASES
                      for position, arm in enumerate('ABBA' if block % 2 == 0 else 'BAAB')]
    assert plan['order'] == expected_order and len(expected_order) == 200
    runner.validate(plan)
    control_sha, control_observables = runner.verify_control(root, plan, plan_path)
    control = read(root / 'control/result.json')
    assert control['rows'] == 20 and len(control_observables) == 2 * 5 * 7
    path = root / 'measure/rows.jsonl'
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    result = read(root / 'measure/result.json')
    assert result['pass'] is True and result['quiet_window'] is True
    assert result['rows'] == len(rows) == 200
    assert result['plan_sha256'] == sha(plan_path)
    assert result['rows_sha256'] == sha(path)
    assert result['control_result_sha256'] == control_sha
    manifest = {}
    for index, (row, sequence) in enumerate(zip(rows, expected_order)):
        assert all(row[key] == expected for key, expected in sequence.items()), index
        assert row['pass'] is True and row['exit'] == 0 and row['timeout'] is False
        argv = [plan['binaries'][row['arm']]['path'], str(Path(plan['fixtures']) / row['case']),
                str(root / 'live'), row['capacity'], 'measure']
        assert row['argv'] == argv, index
        for kind, suffix in [('stdout', '.stdout.jsonl'), ('stderr', '.stderr.txt')]:
            filename = f'{index:04d}' + suffix
            assert row[kind] == filename
            digest = sha(root / 'measure' / filename)
            assert digest == row[kind + '_sha256']
            manifest[filename] = digest
        assert (root / 'measure' / row['stderr']).stat().st_size == 0
        lines = [json.loads(line) for line in (root / 'measure' / row['stdout']).read_text().splitlines()]
        checked = copy.deepcopy(row)
        runner.inspect_output(checked, lines, sequence, 'measure')
        assert checked['pass'] is True and checked['requests'] == row['requests']
        assert checked['initialization_ns'] == row['initialization_ns']
        assert [r['request'] for r in row['requests']] == REQUESTS
        assert [r['snapshot'] for r in row['requests']] == SNAPSHOTS
        for request in row['requests']:
            assert request['fresh_equal'] is None
            assert request['verification_runs'] == 1 and request['active_query_count'] == 0
            assert request['phases']['verify'] == {'executed': 1, 'reused': 0}
            assert request['observable_digest'] == control_observables[(row['case'], row['capacity'], request['request'])]
    assert manifest == result['logs_sha256']
    assert runner.observables(rows) == control_observables
    control_rows = [json.loads(line) for line in (root / 'control/rows.jsonl').read_text().splitlines()]
    assert all(r['fresh_equal'] is True for row in control_rows for r in row['requests'])

    metrics = REQUESTS + ['sequence', 'process_wall', 'process_cpu', 'process_maxrss']
    cells = {}
    for capacity, case, metric, arm, block in itertools.product(CAPACITIES, CASES, metrics, 'AB', range(5)):
        observations = [value(row, metric) for row in rows
                        if (row['capacity'], row['case'], row['arm'], row['block']) == (capacity, case, arm, block)]
        assert len(observations) == 2 and all(math.isfinite(v) and v > 0 for v in observations)
        cells[(capacity, case, metric, arm, block)] = statistics.mean(observations)

    # Independently recheck all 110 cells in the producer's summary, including all five block pairs.
    published = {(s['capacity'], s['case'], s['request']): s for s in result['summaries']}
    assert len(published) == len(result['summaries']) == 110
    for capacity, case, metric in itertools.product(CAPACITIES, CASES, metrics):
        summary = published[(capacity, case, metric)]
        pairs = [{arm: cells[(capacity, case, metric, arm, block)] for arm in 'AB'} for block in range(5)]
        ratios = [p['B'] / p['A'] for p in pairs]
        medians = {arm: statistics.median(p[arm] for p in pairs) for arm in 'AB'}
        assert summary['unit'] == ('KiB' if metric == 'process_maxrss' else 'ms')
        assert len(summary['block_values']) == len(summary['block_ratios_B_over_A']) == 5
        for block in range(5):
            for arm in 'AB':
                near(summary['block_values'][block][arm], pairs[block][arm])
            near(summary['block_ratios_B_over_A'][block], ratios[block])
        for arm in 'AB':
            near(summary['median_values'][arm], medians[arm])
        near(summary['median_ratio'], statistics.median(ratios))
        near(summary['ratio_of_medians'], medians['B'] / medians['A'])

    def component(capacity, case, metric, weight):
        return (weight, *[[cells[(capacity, case, metric, arm, block)] for block in range(5)] for arm in 'AB'])

    capacities = {}
    for capacity in CAPACITIES:
        primary = weighted_result([component(capacity, case, request, cw * rw)
                                   for case, cw in CASES.items() for request, rw in CORE_REQUESTS.items()], 'ms')
        requests = {request: weighted_result([component(capacity, case, request, weight)
                                               for case, weight in CASES.items()], 'ms')
                    for request in CORE_REQUESTS}
        sequence = {metric: weighted_result([component(capacity, case, metric, weight)
                                              for case, weight in CASES.items()],
                                             'KiB' if metric == 'process_maxrss' else 'ms')
                    for metric in ['sequence', 'process_wall', 'process_cpu', 'process_maxrss']}
        per_case = {}
        for case in CASES:
            per_case[case] = {
                'four_request_mix': weighted_result([component(capacity, case, request, weight)
                                                     for request, weight in CORE_REQUESTS.items()], 'ms'),
                'request_median_compile_ms': {
                    request: {arm: statistics.median(cells[(capacity, case, request, arm, b)] for b in range(5))
                              for arm in 'AB'} for request in REQUESTS},
                'seven_request_process_median': {
                    metric: {arm: statistics.median(cells[(capacity, case, metric, arm, b)] for b in range(5))
                             for arm in 'AB'} for metric in ['sequence', 'process_wall', 'process_cpu', 'process_maxrss']},
            }
        rss_max = {arm: max(row['process_maxrss_kib'] for row in rows
                           if row['capacity'] == capacity and row['arm'] == arm) for arm in 'AB'}
        capacities[capacity] = {'four_request_mix_compile': primary,
                                'case_weighted_core_requests': requests,
                                'case_weighted_seven_request_sequence': sequence,
                                'worst_observed_single_process_rss_kib': rss_max,
                                'by_case': per_case}

    memo_counts = collections.Counter()
    for row in rows:
        for request in row['requests']:
            memo = request['type_memo']
            if memo is not None:
                types = request['phases'].get('types', {})
                key = (row['capacity'], row['case'], row['arm'], request['request'],
                       types.get('executed', 0), types.get('reused', 0), memo['eligible'],
                       memo['reused_functions'], memo['checked_functions'],
                       memo['retained_modules'], memo['retained_weight'])
                memo_counts[key] += 1
    memo_keys = ['capacity', 'case', 'arm', 'request', 'types_executed', 'types_reused', 'eligible',
                 'reused_functions', 'checked_functions', 'retained_modules', 'retained_weight']
    return {
        'candidate': name, 'source_commit': CANDIDATES[name], 'verdict': 'Pass',
        'root': str(root), 'plan_sha256': sha(plan_path), 'result_sha256': sha(root / 'measure/result.json'),
        'records_sha256': sha(path), 'control_result_sha256': control_sha,
        'measurement_sequences': 200, 'measurement_compile_requests': 1400,
        'unscored_control_sequences': 20, 'unscored_persistent_control_requests': 140,
        'unscored_fresh_control_comparisons': 140,
        'all_raw_logs_commands_Verify_observables_and_receipts_checked': True,
        'nonempty_stderr_logs': 0, 'published_summary_cells_independently_recomputed': 110,
        'capacities': capacities,
        'memo_observations': [{**dict(zip(memo_keys, key)), 'observations': count}
                              for key, count in sorted(memo_counts.items())],
        'memo_interpretation': 'eligible is current-request eligibility, while retained counts describe the committed epoch. A whole Types query hit can report current eligible=false/checked=0 without rejecting or dropping a retained memo. No disabled reason is inferred from eligible=false alone.',
    }


def verify_build_artifacts():
    result = {'cp29': [], 'cp30': [], 'baseline': [], 'cargo_fingerprints': []}
    cp29 = read(ROOT / 'recovery/cp29-release-a0f38d2b/BUILD-RECEIPT.json')
    for artifact in cp29['artifacts'] + cp29['tools']:
        path = Path(artifact['path'])
        assert path.stat().st_size == artifact['size'] and sha(path) == artifact['sha256'], str(path)
        result['cp29'].append({'path': str(path), 'sha256': sha(path)})
    cp30 = read(ROOT / 'recovery/cp30-release-1b86175c/BUILD-RECEIPT.json')
    for path, digest in cp30['rlibs'].items():
        assert sha(path) == digest, path
        result['cp30'].append({'path': path, 'sha256': digest})
    assert sha(cp30['binary']) == cp30['binary_sha256']
    result['cp30'].append({'path': cp30['binary'], 'sha256': cp30['binary_sha256']})
    baseline = read(ROOT / 'recovery/baseline-build.json')
    assert sha(baseline['compiler']['path']) == baseline['compiler']['sha256']
    assert sha(ROOT / 'recovery/baseline-build.log') == baseline['build_log_sha256']
    result['baseline'].append(baseline['compiler'])
    static = read(ROOT / 'review/SESSION-HARNESS-STATIC-BUILD-REVIEW.json')
    for path, digest in static['fingerprint_files_sha256'].items():
        assert sha(path) == digest
        result['cargo_fingerprints'].append({'path': path, 'sha256': digest})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cp29', type=Path, default=ROOT / 'measurements/cp29-session')
    parser.add_argument('--cp30', type=Path, default=Path('/tmp/zil-compile7-cp30-session-recovery'))
    parser.add_argument('--failed-cp30', type=Path, default=ROOT / 'measurements/cp30-session')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    original = read(args.failed_cp30 / 'plan.json')
    recovered = read(args.cp30 / 'plan.json')
    assert original == recovered, 'Recovery must preserve the frozen protocol/identities'
    incomplete_path = args.failed_cp30 / 'measure/rows.jsonl'
    incomplete = [json.loads(line) for line in incomplete_path.read_text().splitlines()]
    assert len(incomplete) == 114 and not (args.failed_cp30 / 'measure/result.json').exists()
    failed_window = {'root': str(args.failed_cp30), 'rows': len(incomplete), 'expected_rows': 200,
                     'records_sha256': sha(incomplete_path), 'plan_sha256': sha(args.failed_cp30 / 'plan.json'),
                     'complete_result_exists': False, 'verdict': 'Incomplete_NoScore',
                     'excluded_from_all_statistics': True,
                     'reason_provenance': 'Coordinator reports ENOTEMPTY during live-directory cleanup; this audit independently confirms the incomplete prefix and missing completion result, without rerunning cleanup.'}
    artifacts = verify_build_artifacts()
    windows = {name: audit_window(name, getattr(args, name)) for name in ('cp29', 'cp30')}
    output = {
        'reviewer': 'cp25_evidence', 'verdict': 'Pass',
        'at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'scope': 'Read-only independent data recomputation and artifact identity review; no compilation/native execution or new performance sample',
        'case_weights': CASES, 'four_request_weights': CORE_REQUESTS,
        'aggregation': 'Each arm/case/request has two observations per paired block. Take their arithmetic mean, then the median across the five blocks for each absolute cost cell. Apply fixed case/request weights and divide total B by total A only afterward. Capacities are never pooled.',
        'bootstrap': 'Enumerate all 5^5=3125 ordered paired whole-block resamples (126 unique multisets with multiplicity). Recompute the same weighted absolute-cost statistic for each sample, using the same sampled blocks for both arms and all cases/requests. Linear 2.5/97.5 percentiles; descriptive/exploratory on this shared host, not a product gate or population guarantee.',
        'seven_request_scope': 'Whole-process wall/CPU/maxrss include initialization, edit copying, seven fixed-order compiler requests and reporting. CPU/RSS cannot be reassigned to the four-request weighted mix from these process-only records. Case-weighted RSS is a descriptive weighted mean of per-case process-peak medians, not simultaneous process-tree memory; worst observed single-process RSS also reported.',
        'runner_sha256': sha(RUNNER), 'audit_script_sha256': sha(__file__),
        'build_artifact_verification': artifacts, 'original_failed_cp30_window': failed_window,
        'windows': windows,
        'limitations': ['Compile-through-Verify sessions, no LLVM emission/link timing',
                        'Report/interface observables versus fresh controls, not a new whole-TypedIR/native oracle',
                        'Original CP30 failed 114-sequence prefix excluded, not resumed or pooled',
                        'Two separately paired baseline comparisons, not a direct CP29-versus-CP30 contest',
                        'Shared host, fixed affinity/quotas, unflushed OS caches, five blocks',
                        'No full language/product-gate conclusion'],
    }
    (args.out / 'RESULT.json').write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n')
    compact = {}
    for name, window in windows.items():
        compact[name] = {capacity: {
            'four_request_mix_B_over_A': data['four_request_mix_compile']['B_over_A'],
            'four_request_mix_CI95': data['four_request_mix_compile']['exploratory_paired_block_bootstrap95'],
            'seven_request_process_wall_B_over_A': data['case_weighted_seven_request_sequence']['process_wall']['B_over_A'],
            'seven_request_process_cpu_B_over_A': data['case_weighted_seven_request_sequence']['process_cpu']['B_over_A'],
            'seven_request_case_weighted_peak_RSS_B_over_A': data['case_weighted_seven_request_sequence']['process_maxrss']['B_over_A'],
        } for capacity, data in window['capacities'].items()}
    print(json.dumps({'verdict': 'Pass', 'summary': compact, 'result': str(args.out / 'RESULT.json')}, indent=2))


if __name__ == '__main__':
    main()
