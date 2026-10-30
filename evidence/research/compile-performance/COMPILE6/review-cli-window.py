#!/usr/bin/env python3
"""Read-only independent frozen-window matrix and estimator audit."""
import collections
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import random
import statistics
import subprocess
import sys

ROOT = Path('/workspace/scratch/4ab361722127/zil')
WINDOW = Path(sys.argv[1])
OUTPUT = Path(sys.argv[2])
SPEC = importlib.util.spec_from_file_location('reviewed_bench', ROOT / 'tools/research/compile-performance/bench.py')
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)
PLAN, _ = BENCH.load(WINDOW / 'plan.json', check_environment=False)
SUMMARY = json.loads((WINDOW / 'summary.json').read_text())
ROWS = [json.loads(line) for line in (WINDOW / 'measure/records.jsonl').read_text().splitlines()]
PREFLIGHT = [json.loads(line) for line in (WINDOW / 'preflight/records.jsonl').read_text().splitlines()]
MEASURE_RECEIPT = json.loads((WINDOW / 'measure/complete.json').read_text())
PREFLIGHT_RECEIPT = json.loads((WINDOW / 'preflight/result.json').read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()

assert SUMMARY['plan_sha256'] == sha(WINDOW / 'plan.json')
assert SUMMARY['records_sha256'] == sha(WINDOW / 'measure/records.jsonl')
assert MEASURE_RECEIPT == {'plan_sha256': sha(WINDOW / 'plan.json'), 'rows': len(ROWS), 'failures': [],
                         'pass': True, 'records_sha256': sha(WINDOW / 'measure/records.jsonl'),
                         'preflight_sha256': sha(WINDOW / 'preflight/result.json')}
assert PREFLIGHT_RECEIPT == {'plan_sha256': sha(WINDOW / 'plan.json'), 'rows': len(PREFLIGHT), 'failures': [],
                           'pass': True, 'records_sha256': sha(WINDOW / 'preflight/records.jsonl')}
BENCH.checked_preflight(PLAN, WINDOW, WINDOW / 'preflight/result.json')
assert BENCH.validate_rows(PLAN, WINDOW, WINDOW / 'measure/records.jsonl', ROWS, PLAN['order']) == (True, [])
for spec in PLAN['source_identity'].values():
    tree = subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', spec['commit'] + ':crates'], text=True).strip()
    assert tree == spec['crates_tree']

# Recreate scheduling directly from the frozen plan, independently of execute.
rng = random.Random(PLAN['seed'])
order = []
for block in range(PLAN['blocks']):
    pairs = [(phase, case['id']) for phase in PLAN['phases'] for case in PLAN['cases']]
    rng.shuffle(pairs)
    for phase, case in pairs:
        for position, arm in enumerate('ABBA' if block % 2 == 0 else 'BAAB'):
            order.append({'block': block, 'phase': phase, 'case': case, 'position': position, 'arm': arm})
assert order == PLAN['order']
assert PLAN['request_weights'] == {'cold_process': .10, 'unchanged': .35, 'private_edit': .40, 'public_edit': .15}
assert math.isclose(sum(c['weight'] for c in PLAN['cases']), 1.0)
assert len(ROWS) == PLAN['expected_rows'] == len(order) * 4
expected = [(o['block'], o['phase'], o['case'], o['position'], o['arm'], req) for o in order for req in PLAN['request_weights']]
observed = [(r['block'], r['phase'], r['case'], r['position'], r['arm'], r['request']) for r in ROWS]
assert expected == observed
counts = collections.Counter((r['block'], r['phase'], r['case'], r['request'], r['arm']) for r in ROWS)
assert set(counts.values()) == {2}

# Recompute medians of within-block paired means without calling legacy.aggregate.
def estimate(rows, metric, blocks):
    per_cell = collections.defaultdict(list)
    for row in rows:
        value = row['wall_seconds'] if metric == 'wall' else row['cpu_user'] + row['cpu_system']
        per_cell[row['block'], row['case'], row['request'], row['arm']].append(value)
    medians = {}
    for case in PLAN['cases']:
        for req in PLAN['request_weights']:
            for arm in 'AB':
                values = []
                for block in blocks:
                    sample = per_cell[block, case['id'], req, arm]
                    values.append(sum(sample) / len(sample))
                medians[case['id'], req, arm] = statistics.median(values)
    totals = {arm: sum(c['weight'] * rw * medians[c['id'], req, arm]
                       for c in PLAN['cases'] for req, rw in PLAN['request_weights'].items()) for arm in 'AB'}
    totals['B_over_A'] = totals['B'] / totals['A']
    totals['by_case'] = {}
    for c in PLAN['cases']:
        values = {arm: sum(rw * medians[c['id'], req, arm] for req, rw in PLAN['request_weights'].items()) for arm in 'AB'}
        values['B_over_A'] = values['B'] / values['A']
        totals['by_case'][c['id']] = values
    totals['by_request'] = {}
    for req in PLAN['request_weights']:
        values = {arm: sum(c['weight'] * medians[c['id'], req, arm] for c in PLAN['cases']) for arm in 'AB'}
        values['B_over_A'] = values['B'] / values['A']
        totals['by_request'][req] = values
    return totals, medians

def equal_numbers(a, b):
    if isinstance(a, dict):
        assert a.keys() == b.keys()
        for key in a:
            equal_numbers(a[key], b[key])
    elif isinstance(a, list):
        assert len(a) == len(b)
        for x, y in zip(a, b):
            equal_numbers(x, y)
    else:
        assert math.isclose(a, b, rel_tol=1e-13, abs_tol=1e-13), (a, b)

arithmetic = {}
for phase in PLAN['phases']:
    rows = [r for r in ROWS if r['phase'] == phase]
    phase_summary = SUMMARY['by_phase'][phase]
    arithmetic[phase] = {}
    for metric in ('wall', 'cpu'):
        values, medians = estimate(rows, metric, list(range(PLAN['blocks'])))
        equal_numbers(values, phase_summary[metric])
        arithmetic[phase][metric] = values
        if metric == 'wall':
            cells = {}
            for c in PLAN['cases']:
                for req in PLAN['request_weights']:
                    cell = {arm: medians[c['id'], req, arm] for arm in 'AB'}
                    cell['B_over_A'] = cell['B'] / cell['A']
                    cells[c['id'] + '/' + req] = cell
            equal_numbers(cells, phase_summary['by_case_request'])
            arithmetic[phase]['slower_case_requests'] = {k: v for k, v in cells.items() if v['B_over_A'] > 1}
    rng = random.Random(PLAN['seed'])
    boots = sorted(estimate(rows, 'wall', rng.choices(list(range(PLAN['blocks'])), k=PLAN['blocks']))[0]['B_over_A'] for _ in range(2000))
    interval = [boots[49], boots[1949]]
    equal_numbers(interval, phase_summary['exploratory_paired_block_bootstrap95'])
    arithmetic[phase]['exploratory_paired_block_bootstrap95'] = interval

native = sum(len(row['checks']) for row in ROWS)
assert native == SUMMARY['native_checks']
raw_stats = {'verification_runs': collections.Counter(), 'query_state': collections.Counter(), 'parse_report_present': 0}
digest_sets = collections.defaultdict(set)
for row in ROWS:
    raw = json.loads((WINDOW / 'measure/logs' / row['stdout']).read_text())
    raw_stats['verification_runs'][str(raw['verification_runs'])] += 1
    raw_stats['query_state'][raw['query_cache']['state']] += 1
    raw_stats['parse_report_present'] += int('parse_cache' in raw)
    digest_sets[row['case'], row['snapshot']].add(row['executable_sha256'])
assert set(raw_stats['verification_runs']) == {'1'}
assert all(len(group) == 1 for group in digest_sets.values())
report = {'reviewer': '/root/incremental_review', 'verdict': 'Pass',
          'scope': 'Retained full-CLI raw evidence, frozen identity, complete matrix, oracle and arithmetic audit; no compiler re-execution',
          'window': str(WINDOW), 'source_identity': PLAN['source_identity'],
          'files_sha256': {str(p.relative_to(WINDOW)): sha(p) for p in [WINDOW/'plan.json', WINDOW/'preflight/result.json', WINDOW/'preflight/records.jsonl', WINDOW/'measure/complete.json', WINDOW/'measure/records.jsonl', WINDOW/'summary.json']},
          'rows': len(ROWS), 'preflight_rows': len(PREFLIGHT), 'native_checks': native,
          'all_row_contracts_and_raw_oracles_pass': True, 'matrix_recreated_from_seed': True,
          'binary_and_helper_hashes_rechecked': True, 'fixtures_rechecked': len(PLAN['fixtures_sha256']),
          'independently_recomputed_estimator': arithmetic, 'raw_report_counts': raw_stats,
          'same_binary_by_case_snapshot': True,
          'limits': ['Caller-declared commit-to-binary mapping is frozen and hashes match; this audit does not independently reproduce the full compiler build.',
                     'The original timing processes were not re-executed; observed wall/CPU times are retained measurements.',
                     'Only the frozen phases/profile/options/cases are covered. Three paired blocks on a shared host provide descriptive evidence, not a universal or Rust comparison.',
                     'An arithmetic/data Pass does not mean a performance adoption Pass. Slower cells and intervals spanning 1 remain material.']}
OUTPUT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'pass': True, 'rows': len(ROWS), 'native_checks': native,
                  'ratios': {p: {m: v[m]['B_over_A'] for m in ('wall', 'cpu')} for p,v in arithmetic.items()}, 'output': str(OUTPUT)}))
